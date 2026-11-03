use chrono::{DateTime, Utc};
use goshikq_protocol::{DispatchState, ToolSpec, ToolStatus, VerificationStatus};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{sha256_hex, Db, DbResult, ToolInvocationRow};

impl Db {
    pub async fn create_tool_invocation(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        step_id: &str,
        tool_call_id: &str,
        spec: &ToolSpec,
        arguments: &Value,
        idempotency_key: &str,
    ) -> DbResult<ToolInvocationRow> {
        let fingerprint = sha256_hex(arguments.to_string().as_bytes());
        let inv = sqlx::query_as::<_, ToolInvocationRow>(
            r#"INSERT INTO tool_invocations
                 (id, run_id, project_id, step_id, tool_call_id, tool_name, status, effect, risk, arguments, idempotency_key, requires_approval, dispatch_state, input_fingerprint, recovery_policy)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(project_id)
        .bind(step_id)
        .bind(tool_call_id)
        .bind(&spec.name)
        .bind(ToolStatus::Requested.as_str())
        .bind(spec.effect.as_str())
        .bind(spec.risk.as_str())
        .bind(arguments)
        .bind(idempotency_key)
        .bind(spec.requires_approval)
        .bind(DispatchState::Prepared.as_str())
        .bind(&fingerprint)
        .bind(spec.effect.as_str())
        .fetch_one(&self.pool)
        .await?;
        let _ = self
            .append_journal(
                inv.id,
                run_id,
                DispatchState::Prepared,
                json!({"input_fingerprint": fingerprint}),
            )
            .await;
        Ok(inv)
    }

    /// Record a rejected (unknown / invalid) tool call so Studio can show it.
    pub async fn create_rejected_tool_invocation(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        step_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        arguments: &Value,
        error: &Value,
    ) -> DbResult<ToolInvocationRow> {
        Ok(sqlx::query_as::<_, ToolInvocationRow>(
            r#"INSERT INTO tool_invocations
                 (id, run_id, project_id, step_id, tool_call_id, tool_name, status, effect, risk, arguments, error, idempotency_key, ended_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,'UNKNOWN','unknown',$8,$9,'',now()) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(project_id)
        .bind(step_id)
        .bind(tool_call_id)
        .bind(tool_name)
        .bind(ToolStatus::Failed.as_str())
        .bind(arguments)
        .bind(error)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn set_tool_status(&self, id: Uuid, status: ToolStatus) -> DbResult<()> {
        sqlx::query("UPDATE tool_invocations SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(status.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_tool_approval(&self, id: Uuid, approval_id: Uuid) -> DbResult<()> {
        sqlx::query("UPDATE tool_invocations SET approval_id = $2, status = $3 WHERE id = $1")
            .bind(id)
            .bind(approval_id)
            .bind(ToolStatus::ApprovalRequired.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_tool_started(
        &self,
        id: Uuid,
        attempt: i32,
        started_at: DateTime<Utc>,
    ) -> DbResult<()> {
        sqlx::query("UPDATE tool_invocations SET status = $2, attempts = $3, started_at = COALESCE(started_at, $4) WHERE id = $1")
            .bind(id)
            .bind(ToolStatus::Running.as_str())
            .bind(attempt)
            .bind(started_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn finish_tool(
        &self,
        id: Uuid,
        status: ToolStatus,
        result: Option<&Value>,
        error: Option<&Value>,
        ended_at: DateTime<Utc>,
    ) -> DbResult<()> {
        sqlx::query(
            r#"UPDATE tool_invocations SET status = $2, result = $3, error = $4, ended_at = $5,
                 duration_ms = EXTRACT(EPOCH FROM ($5 - COALESCE(started_at, created_at))) * 1000
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(status.as_str())
        .bind(result)
        .bind(error)
        .bind(ended_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_tool_verification(
        &self,
        id: Uuid,
        status: VerificationStatus,
    ) -> DbResult<()> {
        sqlx::query("UPDATE tool_invocations SET verification_status = $2 WHERE id = $1")
            .bind(id)
            .bind(status.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_tool_invocations(
        &self,
        project_id: Uuid,
        run_id: Uuid,
    ) -> DbResult<Vec<ToolInvocationRow>> {
        Ok(sqlx::query_as::<_, ToolInvocationRow>(
            "SELECT * FROM tool_invocations WHERE project_id = $1 AND run_id = $2 ORDER BY created_at",
        )
        .bind(project_id)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_tool_invocation(&self, id: Uuid) -> DbResult<Option<ToolInvocationRow>> {
        Ok(
            sqlx::query_as::<_, ToolInvocationRow>("SELECT * FROM tool_invocations WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }
}
