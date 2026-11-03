use chrono::{DateTime, Utc};
use goshikq_protocol::ApprovalStatus;
use serde_json::Value;
use uuid::Uuid;

use crate::{ApprovalRow, Db, DbResult, Page, StorageError};

pub struct DecisionOutcome {
    pub approval: ApprovalRow,
    /// False when the approval had already been decided (idempotent replay).
    pub applied: bool,
}

impl Db {
    #[allow(clippy::too_many_arguments)]
    pub async fn create_approval(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        tool_invocation_id: Uuid,
        agent_name: &str,
        action_name: &str,
        arguments: &Value,
        risk: &str,
        reason: &str,
        requested_permissions: &[String],
        expires_at: DateTime<Utc>,
    ) -> DbResult<ApprovalRow> {
        Ok(sqlx::query_as::<_, ApprovalRow>(
            r#"INSERT INTO approvals (id, run_id, project_id, tool_invocation_id, status, agent_name, action_name, arguments, risk, reason, requested_permissions, expires_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(project_id)
        .bind(tool_invocation_id)
        .bind(ApprovalStatus::Pending.as_str())
        .bind(agent_name)
        .bind(action_name)
        .bind(arguments)
        .bind(risk)
        .bind(reason)
        .bind(serde_json::json!(requested_permissions))
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn get_approval(&self, project_id: Uuid, id: Uuid) -> DbResult<ApprovalRow> {
        sqlx::query_as::<_, ApprovalRow>(
            "SELECT * FROM approvals WHERE project_id = $1 AND id = $2",
        )
        .bind(project_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("approval {id}")))
    }

    pub async fn list_approvals(
        &self,
        project_id: Uuid,
        status: Option<&str>,
        run_id: Option<Uuid>,
        page: Page,
    ) -> DbResult<Vec<ApprovalRow>> {
        Ok(sqlx::query_as::<_, ApprovalRow>(
            r#"SELECT * FROM approvals WHERE project_id = $1
                 AND ($2::text IS NULL OR status = $2)
                 AND ($3::uuid IS NULL OR run_id = $3)
               ORDER BY requested_at DESC LIMIT $4 OFFSET $5"#,
        )
        .bind(project_id)
        .bind(status)
        .bind(run_id)
        .bind(page.limit)
        .bind(page.offset)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Idempotent decision: only a `pending`, unexpired approval can be decided. A repeat
    /// call returns the stored row with `applied = false`. Expired approvals are marked
    /// `expired` and rejected with `Conflict`.
    pub async fn decide_approval(
        &self,
        project_id: Uuid,
        id: Uuid,
        approve: bool,
        decided_by: &str,
        reason: Option<&str>,
    ) -> DbResult<DecisionOutcome> {
        let mut tx = self.pool.begin().await?;
        let current = sqlx::query_as::<_, ApprovalRow>(
            "SELECT * FROM approvals WHERE project_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(project_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("approval {id}")))?;
        if current.status != ApprovalStatus::Pending.as_str() {
            tx.commit().await?;
            return Ok(DecisionOutcome {
                approval: current,
                applied: false,
            });
        }
        if current.expires_at <= Utc::now() {
            let expired = sqlx::query_as::<_, ApprovalRow>(
                "UPDATE approvals SET status = $2, decided_at = now(), decision_reason = 'expired before decision' WHERE id = $1 RETURNING *",
            )
            .bind(id)
            .bind(ApprovalStatus::Expired.as_str())
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            let _ = expired;
            return Err(StorageError::Conflict("approval expired".into()));
        }
        let status = if approve {
            ApprovalStatus::Approved
        } else {
            ApprovalStatus::Denied
        };
        let updated = sqlx::query_as::<_, ApprovalRow>(
            "UPDATE approvals SET status = $2, decided_at = now(), decided_by = $3, decision_reason = $4 WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(status.as_str())
        .bind(decided_by)
        .bind(reason)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO audit_logs (id, project_id, actor, action, subject_type, subject_id, details) VALUES ($1,$2,$3,$4,'approval',$5,$6)")
            .bind(Uuid::now_v7())
            .bind(project_id)
            .bind(decided_by)
            .bind(if approve { "approval.granted" } else { "approval.denied" })
            .bind(id.to_string())
            .bind(serde_json::json!({"run_id": updated.run_id, "action": updated.action_name, "reason": reason}))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(DecisionOutcome {
            approval: updated,
            applied: true,
        })
    }

    pub async fn latest_approval_for_run(&self, run_id: Uuid) -> DbResult<Option<ApprovalRow>> {
        Ok(sqlx::query_as::<_, ApprovalRow>(
            "SELECT * FROM approvals WHERE run_id = $1 ORDER BY requested_at DESC LIMIT 1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Mark a previously granted (or still pending) approval as unusable because
    /// material prerequisites changed. Appends an audit row; does not rewrite history.
    pub async fn invalidate_approval(
        &self,
        project_id: Uuid,
        id: Uuid,
        reason: &str,
    ) -> DbResult<ApprovalRow> {
        let updated = sqlx::query_as::<_, ApprovalRow>(
            r#"UPDATE approvals SET status = $2, decided_at = COALESCE(decided_at, now()), decision_reason = $3
               WHERE id = $1 AND status IN ('pending','approved') RETURNING *"#,
        )
        .bind(id)
        .bind(ApprovalStatus::Invalidated.as_str())
        .bind(reason)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("invalidatable approval {id}")))?;
        sqlx::query("INSERT INTO audit_logs (id, project_id, actor, action, subject_type, subject_id, details) VALUES ($1,$2,$3,$4,'approval',$5,$6)")
            .bind(Uuid::now_v7())
            .bind(project_id)
            .bind("goshikq")
            .bind("approval.invalidated")
            .bind(id.to_string())
            .bind(serde_json::json!({"run_id": updated.run_id, "reason": reason}))
            .execute(&self.pool)
            .await?;
        Ok(updated)
    }

    pub async fn expire_approval(&self, id: Uuid) -> DbResult<ApprovalRow> {
        Ok(sqlx::query_as::<_, ApprovalRow>(
            "UPDATE approvals SET status = $2, decided_at = now(), decision_reason = 'expired' WHERE id = $1 AND status = 'pending' RETURNING *",
        )
        .bind(id)
        .bind(ApprovalStatus::Expired.as_str())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("pending approval {id}")))?)
    }

    pub async fn count_pending_approvals(&self, project_id: Uuid) -> DbResult<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM approvals WHERE project_id = $1 AND status = 'pending' AND expires_at > now()")
            .bind(project_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(n)
    }
}
