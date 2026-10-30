//! Projects, agents, integrations, workflows, API keys, audit log.

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    sha256_hex, AgentRow, AgentVersionRow, ApiKeyRow, Db, DbResult, IntegrationRow, Page, ProjectMemberRow, ProjectRow,
    SecurityEventRow, StorageError, WorkflowRow, WorkflowVersionRow,
};

impl Db {
    pub async fn list_projects(&self) -> DbResult<Vec<ProjectRow>> {
        Ok(sqlx::query_as::<_, ProjectRow>("SELECT * FROM projects ORDER BY created_at").fetch_all(&self.pool).await?)
    }

    pub async fn list_projects_for_subject(&self, subject: &str) -> DbResult<Vec<ProjectRow>> {
        Ok(sqlx::query_as::<_, ProjectRow>(
            "SELECT p.* FROM projects p JOIN project_members m ON m.project_id = p.id WHERE m.subject = $1 ORDER BY p.created_at",
        )
        .bind(subject)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_project(&self, id: Uuid) -> DbResult<ProjectRow> {
        sqlx::query_as::<_, ProjectRow>("SELECT * FROM projects WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("project {id}")))
    }

    pub async fn create_project(&self, organization_id: Uuid, name: &str, slug: &str) -> DbResult<ProjectRow> {
        Ok(sqlx::query_as::<_, ProjectRow>(
            "INSERT INTO projects (id, organization_id, name, slug) VALUES ($1,$2,$3,$4) RETURNING *",
        )
        .bind(Uuid::now_v7())
        .bind(organization_id)
        .bind(name)
        .bind(slug)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn list_agents(&self, project_id: Uuid, page: Page) -> DbResult<Vec<AgentRow>> {
        Ok(sqlx::query_as::<_, AgentRow>("SELECT * FROM agents WHERE project_id = $1 ORDER BY updated_at DESC LIMIT $2 OFFSET $3")
            .bind(project_id)
            .bind(page.limit)
            .bind(page.offset)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn get_agent(&self, project_id: Uuid, id: Uuid) -> DbResult<AgentRow> {
        sqlx::query_as::<_, AgentRow>("SELECT * FROM agents WHERE project_id = $1 AND id = $2")
            .bind(project_id)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("agent {id}")))
    }

    /// Create or update an agent and store a new content-addressed version.
    pub async fn save_agent_definition(&self, project_id: Uuid, name: &str, description: &str, source_framework: &str, definition: &Value, model: &str) -> DbResult<(AgentRow, AgentVersionRow)> {
        let mut tx = self.pool.begin().await?;
        let agent = sqlx::query_as::<_, AgentRow>(
            r#"INSERT INTO agents (id, project_id, name, description, source_framework) VALUES ($1,$2,$3,$4,$5)
               ON CONFLICT (project_id, name) DO UPDATE SET description = EXCLUDED.description, updated_at = now() RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(project_id)
        .bind(name)
        .bind(description)
        .bind(source_framework)
        .fetch_one(&mut *tx)
        .await?;
        let hash = sha256_hex(&serde_json::to_vec(definition)?);
        let existing = sqlx::query_as::<_, AgentVersionRow>("SELECT * FROM agent_versions WHERE agent_id = $1 AND config_hash = $2")
            .bind(agent.id)
            .bind(&hash)
            .fetch_optional(&mut *tx)
            .await?;
        let version = match existing {
            Some(v) => v,
            None => {
                let (next,): (i32,) = sqlx::query_as("SELECT COALESCE(MAX(version), 0) + 1 FROM agent_versions WHERE agent_id = $1")
                    .bind(agent.id)
                    .fetch_one(&mut *tx)
                    .await?;
                sqlx::query_as::<_, AgentVersionRow>(
                    "INSERT INTO agent_versions (id, agent_id, version, definition, config_hash, model) VALUES ($1,$2,$3,$4,$5,$6) RETURNING *",
                )
                .bind(Uuid::now_v7())
                .bind(agent.id)
                .bind(next)
                .bind(definition)
                .bind(&hash)
                .bind(model)
                .fetch_one(&mut *tx)
                .await?
            }
        };
        tx.commit().await?;
        Ok((agent, version))
    }

    pub async fn list_agent_versions(&self, agent_id: Uuid) -> DbResult<Vec<AgentVersionRow>> {
        Ok(sqlx::query_as::<_, AgentVersionRow>("SELECT * FROM agent_versions WHERE agent_id = $1 ORDER BY version DESC")
            .bind(agent_id)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn agent_run_stats(&self, project_id: Uuid, agent_id: Uuid) -> DbResult<(i64, i64, i64, Option<chrono::DateTime<Utc>>)> {
        let row: (i64, i64, i64, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
            r#"SELECT COUNT(*), COUNT(*) FILTER (WHERE verification_status = 'VERIFIED'),
                      COUNT(*) FILTER (WHERE status IN ('FAILED','TIMED_OUT','BUDGET_EXCEEDED','STEP_LIMIT_EXCEEDED','MODEL_BUDGET_EXCEEDED')), MAX(created_at)
               FROM runs WHERE project_id = $1 AND agent_id = $2"#,
        )
        .bind(project_id)
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_integrations(&self, project_id: Uuid) -> DbResult<Vec<IntegrationRow>> {
        Ok(sqlx::query_as::<_, IntegrationRow>("SELECT * FROM framework_integrations WHERE project_id = $1 ORDER BY framework")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_integration(&self, project_id: Uuid, framework: &str, adapter_version: &str, framework_version: Option<&str>, status: &str, capabilities: &Value, unsupported: &Value, config: &Value, last_error: Option<&str>) -> DbResult<IntegrationRow> {
        Ok(sqlx::query_as::<_, IntegrationRow>(
            r#"INSERT INTO framework_integrations (id, project_id, framework, adapter_version, framework_version, status, capabilities, unsupported_capabilities, config, last_error)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
               ON CONFLICT (project_id, framework) DO UPDATE SET adapter_version = EXCLUDED.adapter_version,
                 framework_version = EXCLUDED.framework_version, status = EXCLUDED.status, capabilities = EXCLUDED.capabilities,
                 unsupported_capabilities = EXCLUDED.unsupported_capabilities, config = EXCLUDED.config,
                 last_error = EXCLUDED.last_error, updated_at = now()
               RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(project_id)
        .bind(framework)
        .bind(adapter_version)
        .bind(framework_version)
        .bind(status)
        .bind(capabilities)
        .bind(unsupported)
        .bind(config)
        .bind(last_error)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn list_workflows(&self, project_id: Uuid, page: Page) -> DbResult<Vec<WorkflowRow>> {
        Ok(sqlx::query_as::<_, WorkflowRow>("SELECT * FROM workflows WHERE project_id = $1 ORDER BY updated_at DESC LIMIT $2 OFFSET $3")
            .bind(project_id)
            .bind(page.limit)
            .bind(page.offset)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn get_workflow(&self, project_id: Uuid, id: Uuid) -> DbResult<WorkflowRow> {
        sqlx::query_as::<_, WorkflowRow>("SELECT * FROM workflows WHERE project_id = $1 AND id = $2")
            .bind(project_id)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("workflow {id}")))
    }

    pub async fn create_workflow(&self, project_id: Uuid, name: &str, version: i32, definition: &Value) -> DbResult<WorkflowRow> {
        self.create_workflow_full(project_id, name, version, "", "draft", definition).await
    }

    pub async fn create_workflow_full(
        &self,
        project_id: Uuid,
        name: &str,
        version: i32,
        description: &str,
        status: &str,
        definition: &Value,
    ) -> DbResult<WorkflowRow> {
        Ok(sqlx::query_as::<_, WorkflowRow>(
            "INSERT INTO workflows (id, project_id, name, version, definition, description, status) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING *",
        )
        .bind(Uuid::now_v7())
        .bind(project_id)
        .bind(name)
        .bind(version)
        .bind(definition)
        .bind(description)
        .bind(status)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn update_workflow_draft(&self, project_id: Uuid, id: Uuid, name: &str, description: &str, definition: &Value) -> DbResult<WorkflowRow> {
        sqlx::query_as::<_, WorkflowRow>(
            "UPDATE workflows SET name = $3, description = $4, definition = $5, status = 'draft', updated_at = now() WHERE project_id = $1 AND id = $2 RETURNING *",
        )
        .bind(project_id)
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(definition)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("workflow {id}")))
    }

    pub async fn publish_workflow(&self, project_id: Uuid, id: Uuid, actor: &str) -> DbResult<(WorkflowRow, WorkflowVersionRow)> {
        let wf = self.get_workflow(project_id, id).await?;
        let version = wf.published_version.unwrap_or(0) + 1;
        let snap = sqlx::query_as::<_, WorkflowVersionRow>(
            "INSERT INTO workflow_versions (id, workflow_id, project_id, version, definition, published_by) VALUES ($1,$2,$3,$4,$5,$6) RETURNING *",
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(project_id)
        .bind(version)
        .bind(&wf.definition)
        .bind(actor)
        .fetch_one(&self.pool)
        .await?;
        let row = sqlx::query_as::<_, WorkflowRow>(
            "UPDATE workflows SET status = 'published', version = $3, published_version = $3, updated_at = now() WHERE project_id = $1 AND id = $2 RETURNING *",
        )
        .bind(project_id)
        .bind(id)
        .bind(version)
        .fetch_one(&self.pool)
        .await?;
        Ok((row, snap))
    }

    pub async fn list_workflow_versions(&self, project_id: Uuid, workflow_id: Uuid) -> DbResult<Vec<WorkflowVersionRow>> {
        Ok(sqlx::query_as::<_, WorkflowVersionRow>(
            "SELECT * FROM workflow_versions WHERE project_id = $1 AND workflow_id = $2 ORDER BY version DESC",
        )
        .bind(project_id)
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_workflow_version(&self, project_id: Uuid, workflow_id: Uuid, version: i32) -> DbResult<WorkflowVersionRow> {
        sqlx::query_as::<_, WorkflowVersionRow>(
            "SELECT * FROM workflow_versions WHERE project_id = $1 AND workflow_id = $2 AND version = $3",
        )
        .bind(project_id)
        .bind(workflow_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("workflow {workflow_id} version {version}")))
    }

    pub async fn duplicate_workflow(&self, project_id: Uuid, id: Uuid, name: &str) -> DbResult<WorkflowRow> {
        let src = self.get_workflow(project_id, id).await?;
        self.create_workflow_full(project_id, name, 1, &src.description, "draft", &src.definition).await
    }

    /// Returns the project for a presented API key, updating `last_used_at`.
    pub async fn authenticate_api_key(&self, presented: &str) -> DbResult<Option<ApiKeyRow>> {
        let hash = sha256_hex(presented.as_bytes());
        let row = sqlx::query_as::<_, ApiKeyRow>(
            "UPDATE api_keys SET last_used_at = now() WHERE key_hash = $1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now()) RETURNING *",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn create_api_key(&self, project_id: Uuid, name: &str, plaintext: &str) -> DbResult<ApiKeyRow> {
        self.create_api_key_full(project_id, name, plaintext, "operator", "service", None).await
    }

    pub async fn create_api_key_full(
        &self,
        project_id: Uuid,
        name: &str,
        plaintext: &str,
        role: &str,
        kind: &str,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> DbResult<ApiKeyRow> {
        Ok(sqlx::query_as::<_, ApiKeyRow>(
            "INSERT INTO api_keys (id, project_id, name, key_hash, role, kind, expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING *",
        )
        .bind(Uuid::now_v7())
        .bind(project_id)
        .bind(name)
        .bind(sha256_hex(plaintext.as_bytes()))
        .bind(role)
        .bind(kind)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn list_api_keys(&self, project_id: Uuid) -> DbResult<Vec<ApiKeyRow>> {
        Ok(sqlx::query_as::<_, ApiKeyRow>("SELECT * FROM api_keys WHERE project_id = $1 ORDER BY created_at")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn revoke_api_key(&self, project_id: Uuid, id: Uuid) -> DbResult<ApiKeyRow> {
        sqlx::query_as::<_, ApiKeyRow>(
            "UPDATE api_keys SET revoked_at = now() WHERE id = $1 AND project_id = $2 AND revoked_at IS NULL RETURNING *",
        )
        .bind(id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("api key {id}")))
    }

    pub async fn rotate_api_key(&self, project_id: Uuid, id: Uuid, plaintext: &str) -> DbResult<ApiKeyRow> {
        let old = self.revoke_api_key(project_id, id).await?;
        Ok(sqlx::query_as::<_, ApiKeyRow>(
            "INSERT INTO api_keys (id, project_id, name, key_hash, role, kind, rotated_from) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING *",
        )
        .bind(Uuid::now_v7())
        .bind(project_id)
        .bind(&old.name)
        .bind(sha256_hex(plaintext.as_bytes()))
        .bind(&old.role)
        .bind(&old.kind)
        .bind(old.id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn upsert_member(&self, project_id: Uuid, subject: &str, role: &str) -> DbResult<ProjectMemberRow> {
        Ok(sqlx::query_as::<_, ProjectMemberRow>(
            r#"INSERT INTO project_members (project_id, subject, role) VALUES ($1,$2,$3)
               ON CONFLICT (project_id, subject) DO UPDATE SET role = EXCLUDED.role
               RETURNING *"#,
        )
        .bind(project_id)
        .bind(subject)
        .bind(role)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn get_member(&self, project_id: Uuid, subject: &str) -> DbResult<Option<ProjectMemberRow>> {
        Ok(sqlx::query_as::<_, ProjectMemberRow>("SELECT * FROM project_members WHERE project_id = $1 AND subject = $2")
            .bind(project_id)
            .bind(subject)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn list_members(&self, project_id: Uuid) -> DbResult<Vec<ProjectMemberRow>> {
        Ok(sqlx::query_as::<_, ProjectMemberRow>("SELECT * FROM project_members WHERE project_id = $1 ORDER BY subject")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn delete_member(&self, project_id: Uuid, subject: &str) -> DbResult<()> {
        let n = sqlx::query("DELETE FROM project_members WHERE project_id = $1 AND subject = $2")
            .bind(project_id)
            .bind(subject)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n == 0 {
            return Err(StorageError::NotFound(format!("member {subject}")));
        }
        Ok(())
    }

    pub async fn record_security_event(
        &self,
        project_id: Option<Uuid>,
        tenant_id: Option<Uuid>,
        actor: &str,
        action: &str,
        outcome: &str,
        details: Value,
    ) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO security_events (id, project_id, tenant_id, actor, action, outcome, details) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(Uuid::now_v7())
        .bind(project_id)
        .bind(tenant_id)
        .bind(actor)
        .bind(action)
        .bind(outcome)
        .bind(details)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_security_events(&self, project_id: Uuid, limit: i64) -> DbResult<Vec<SecurityEventRow>> {
        Ok(sqlx::query_as::<_, SecurityEventRow>(
            "SELECT * FROM security_events WHERE project_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(project_id)
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn audit(&self, project_id: Uuid, actor: &str, action: &str, subject_type: &str, subject_id: &str, details: Value) -> DbResult<()> {
        sqlx::query("INSERT INTO audit_logs (id, project_id, actor, action, subject_type, subject_id, details) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(Uuid::now_v7())
            .bind(project_id)
            .bind(actor)
            .bind(action)
            .bind(subject_type)
            .bind(subject_id)
            .bind(details)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
