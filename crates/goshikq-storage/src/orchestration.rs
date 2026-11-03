//! Parent/child tasks and versioned shared context. Project-scoped like every other query.

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    Db, DbResult, RunRow, RunTaskRow, SharedContextHistoryRow, SharedContextRow, StorageError,
};

impl Db {
    pub async fn insert_run_task(&self, row: &RunTaskRow) -> DbResult<RunTaskRow> {
        Ok(sqlx::query_as::<_, RunTaskRow>(
            r#"INSERT INTO run_tasks
                 (id, project_id, parent_run_id, child_run_id, task_id, correlation_id, role, status,
                  verification_status, requires_approval, approval_id, depth, metadata)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
               RETURNING *"#,
        )
        .bind(row.id)
        .bind(row.project_id)
        .bind(row.parent_run_id)
        .bind(row.child_run_id)
        .bind(row.task_id)
        .bind(&row.correlation_id)
        .bind(&row.role)
        .bind(&row.status)
        .bind(&row.verification_status)
        .bind(row.requires_approval)
        .bind(row.approval_id)
        .bind(row.depth)
        .bind(&row.metadata)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn list_child_tasks(
        &self,
        project_id: Uuid,
        parent_run_id: Uuid,
    ) -> DbResult<Vec<RunTaskRow>> {
        Ok(sqlx::query_as::<_, RunTaskRow>(
            "SELECT * FROM run_tasks WHERE project_id = $1 AND parent_run_id = $2 ORDER BY created_at",
        )
        .bind(project_id)
        .bind(parent_run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_run_task_by_child(
        &self,
        project_id: Uuid,
        child_run_id: Uuid,
    ) -> DbResult<Option<RunTaskRow>> {
        Ok(sqlx::query_as::<_, RunTaskRow>(
            "SELECT * FROM run_tasks WHERE project_id = $1 AND child_run_id = $2",
        )
        .bind(project_id)
        .bind(child_run_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_descendant_runs(
        &self,
        project_id: Uuid,
        parent_run_id: Uuid,
    ) -> DbResult<Vec<RunRow>> {
        Ok(sqlx::query_as::<_, RunRow>(
            r#"WITH RECURSIVE tree AS (
                   SELECT * FROM runs WHERE project_id = $1 AND parent_run_id = $2
                   UNION ALL
                   SELECT r.* FROM runs r JOIN tree t ON r.parent_run_id = t.id AND r.project_id = $1
               )
               SELECT * FROM tree"#,
        )
        .bind(project_id)
        .bind(parent_run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_child_runs(
        &self,
        project_id: Uuid,
        parent_run_id: Uuid,
    ) -> DbResult<Vec<RunRow>> {
        Ok(sqlx::query_as::<_, RunRow>(
            "SELECT * FROM runs WHERE project_id = $1 AND parent_run_id = $2 ORDER BY created_at",
        )
        .bind(project_id)
        .bind(parent_run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn count_child_runs(&self, project_id: Uuid, parent_run_id: Uuid) -> DbResult<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM runs WHERE project_id = $1 AND parent_run_id = $2",
        )
        .bind(project_id)
        .bind(parent_run_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }

    pub async fn update_task_status(
        &self,
        project_id: Uuid,
        child_run_id: Uuid,
        status: &str,
        verification_status: &str,
    ) -> DbResult<()> {
        sqlx::query(
            r#"UPDATE run_tasks SET status = $3, verification_status = $4, updated_at = now()
               WHERE project_id = $1 AND child_run_id = $2"#,
        )
        .bind(project_id)
        .bind(child_run_id)
        .bind(status)
        .bind(verification_status)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_task_approval(
        &self,
        project_id: Uuid,
        child_run_id: Uuid,
        approval_id: Uuid,
    ) -> DbResult<()> {
        sqlx::query("UPDATE run_tasks SET approval_id = $3, updated_at = now() WHERE project_id = $1 AND child_run_id = $2")
            .bind(project_id)
            .bind(child_run_id)
            .bind(approval_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Walk `parent_run_id` until the root. Returns (depth of `run_id` from root, ancestor run ids including self, ancestor agent names).
    pub async fn ancestor_chain(
        &self,
        project_id: Uuid,
        mut run_id: Uuid,
    ) -> DbResult<(u32, Vec<Uuid>, Vec<String>)> {
        let mut ids = Vec::new();
        let mut names = Vec::new();
        let mut guard = 0u32;
        loop {
            let row: Option<(Option<Uuid>, String)> = sqlx::query_as(
                "SELECT parent_run_id, agent_name FROM runs WHERE project_id = $1 AND id = $2",
            )
            .bind(project_id)
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await?;
            let Some((parent, name)) = row else {
                break;
            };
            ids.push(run_id);
            names.push(name);
            match parent {
                Some(p) => {
                    if ids.contains(&p) {
                        break;
                    }
                    run_id = p;
                }
                None => break,
            }
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        Ok((ids.len().saturating_sub(1) as u32, ids, names))
    }

    pub async fn get_shared_context(
        &self,
        project_id: Uuid,
        scope_id: Uuid,
        key: &str,
    ) -> DbResult<Option<SharedContextRow>> {
        Ok(sqlx::query_as::<_, SharedContextRow>(
            "SELECT * FROM shared_context WHERE project_id = $1 AND scope_id = $2 AND key = $3",
        )
        .bind(project_id)
        .bind(scope_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_shared_context(
        &self,
        project_id: Uuid,
        scope_id: Uuid,
    ) -> DbResult<Vec<SharedContextRow>> {
        Ok(sqlx::query_as::<_, SharedContextRow>(
            "SELECT * FROM shared_context WHERE project_id = $1 AND scope_id = $2 ORDER BY key",
        )
        .bind(project_id)
        .bind(scope_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_shared_history(
        &self,
        project_id: Uuid,
        scope_id: Uuid,
        key: Option<&str>,
    ) -> DbResult<Vec<SharedContextHistoryRow>> {
        Ok(sqlx::query_as::<_, SharedContextHistoryRow>(
            r#"SELECT * FROM shared_context_history
               WHERE project_id = $1 AND scope_id = $2 AND ($3::text IS NULL OR key = $3)
               ORDER BY version"#,
        )
        .bind(project_id)
        .bind(scope_id)
        .bind(key)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Optimistic put. `expected_version = None` inserts; `Some(n)` updates only if version matches.
    pub async fn put_shared_context(
        &self,
        project_id: Uuid,
        scope_id: Uuid,
        key: &str,
        value: &Value,
        expected_version: Option<i64>,
        owner_agent: &str,
        visibility: &str,
        evidence_refs: &Value,
        actor: &str,
    ) -> DbResult<SharedContextRow> {
        let mut tx = self.pool.begin().await?;
        let existing: Option<SharedContextRow> = sqlx::query_as(
            "SELECT * FROM shared_context WHERE project_id = $1 AND scope_id = $2 AND key = $3 FOR UPDATE",
        )
        .bind(project_id)
        .bind(scope_id)
        .bind(key)
        .fetch_optional(&mut *tx)
        .await?;

        let row = match (existing, expected_version) {
            (None, Some(_)) => {
                return Err(StorageError::Conflict(format!(
                    "shared context `{key}` does not exist (expected version {expected_version:?})"
                )));
            }
            (Some(cur), Some(exp)) if cur.version != exp => {
                return Err(StorageError::Conflict(format!(
                    "shared context `{key}` version conflict: stored {} expected {exp}",
                    cur.version
                )));
            }
            (Some(cur), _) => {
                let updated = sqlx::query_as::<_, SharedContextRow>(
                    r#"UPDATE shared_context
                       SET value = $2, version = version + 1, owner_agent = $3, visibility = $4,
                           evidence_refs = $5, updated_at = $6
                       WHERE id = $1
                       RETURNING *"#,
                )
                .bind(cur.id)
                .bind(value)
                .bind(owner_agent)
                .bind(visibility)
                .bind(evidence_refs)
                .bind(Utc::now())
                .fetch_one(&mut *tx)
                .await?;
                sqlx::query(
                    r#"INSERT INTO shared_context_history
                         (id, context_id, project_id, scope_id, key, value, version, owner_agent, visibility, evidence_refs, actor)
                       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
                )
                .bind(Uuid::now_v7())
                .bind(updated.id)
                .bind(project_id)
                .bind(scope_id)
                .bind(key)
                .bind(value)
                .bind(updated.version)
                .bind(owner_agent)
                .bind(visibility)
                .bind(evidence_refs)
                .bind(actor)
                .execute(&mut *tx)
                .await?;
                updated
            }
            (None, None) => {
                let id = Uuid::now_v7();
                let inserted = sqlx::query_as::<_, SharedContextRow>(
                    r#"INSERT INTO shared_context
                         (id, project_id, scope_id, key, value, version, owner_agent, visibility, evidence_refs)
                       VALUES ($1,$2,$3,$4,$5,1,$6,$7,$8)
                       RETURNING *"#,
                )
                .bind(id)
                .bind(project_id)
                .bind(scope_id)
                .bind(key)
                .bind(value)
                .bind(owner_agent)
                .bind(visibility)
                .bind(evidence_refs)
                .fetch_one(&mut *tx)
                .await?;
                sqlx::query(
                    r#"INSERT INTO shared_context_history
                         (id, context_id, project_id, scope_id, key, value, version, owner_agent, visibility, evidence_refs, actor)
                       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
                )
                .bind(Uuid::now_v7())
                .bind(inserted.id)
                .bind(project_id)
                .bind(scope_id)
                .bind(key)
                .bind(value)
                .bind(inserted.version)
                .bind(owner_agent)
                .bind(visibility)
                .bind(evidence_refs)
                .bind(actor)
                .execute(&mut *tx)
                .await?;
                inserted
            }
        };
        tx.commit().await?;
        Ok(row)
    }
}
