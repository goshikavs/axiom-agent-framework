use chrono::Utc;
use goshikq_protocol::{DispatchState, RecoveryAttemptView, RecoveryDisposition, RecoveryReport};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{ActionJournalRow, Db, DbResult, RecoveryAttemptRow, ToolInvocationRow};

impl Db {
    pub async fn append_journal(
        &self,
        invocation_id: Uuid,
        run_id: Uuid,
        state: DispatchState,
        detail: Value,
    ) -> DbResult<ActionJournalRow> {
        Ok(sqlx::query_as::<_, ActionJournalRow>(
            r#"INSERT INTO action_journal (id, invocation_id, run_id, dispatch_state, detail)
               VALUES ($1,$2,$3,$4,$5) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(invocation_id)
        .bind(run_id)
        .bind(state.as_str())
        .bind(detail)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn set_dispatch_state(
        &self,
        invocation_id: Uuid,
        run_id: Uuid,
        state: DispatchState,
        extra: Option<&str>,
        business: Option<&str>,
        external_id: Option<&str>,
        ack: Option<&Value>,
    ) -> DbResult<()> {
        sqlx::query(
            r#"UPDATE tool_invocations SET
                 dispatch_state = $2,
                 business_operation_status = COALESCE($3, business_operation_status),
                 external_operation_id = COALESCE($4, external_operation_id),
                 last_provider_ack = COALESCE($5, last_provider_ack),
                 reconciliation_at = CASE WHEN $2 IN ('OUTCOME_CONFIRMED','REJECTED','RECOVERY_REQUIRED') THEN now() ELSE reconciliation_at END
               WHERE id = $1"#,
        )
        .bind(invocation_id)
        .bind(state.as_str())
        .bind(business)
        .bind(external_id)
        .bind(ack)
        .execute(&self.pool)
        .await?;
        let mut detail = json!({});
        if let Some(s) = extra {
            detail["note"] = json!(s);
        }
        if let Some(b) = business {
            detail["business_operation_status"] = json!(b);
        }
        if let Some(e) = external_id {
            detail["external_operation_id"] = json!(e);
        }
        self.append_journal(invocation_id, run_id, state, detail)
            .await?;
        Ok(())
    }

    /// Independent of dispatch_state. Appends a journal row with the *current* dispatch
    /// so historical ACCEPTED/OUTCOME_PENDING acknowledgements are not overwritten.
    pub async fn set_recovery_disposition(
        &self,
        invocation_id: Uuid,
        run_id: Uuid,
        disposition: RecoveryDisposition,
        note: Option<&str>,
    ) -> DbResult<()> {
        sqlx::query("UPDATE tool_invocations SET recovery_disposition = $2 WHERE id = $1")
            .bind(invocation_id)
            .bind(disposition.as_str())
            .execute(&self.pool)
            .await?;
        let current: String =
            sqlx::query_scalar("SELECT dispatch_state FROM tool_invocations WHERE id = $1")
                .bind(invocation_id)
                .fetch_one(&self.pool)
                .await?;
        let state = DispatchState::parse(&current).unwrap_or(DispatchState::OutcomePending);
        let mut detail = serde_json::json!({"recovery_disposition": disposition.as_str()});
        if let Some(n) = note {
            detail["note"] = serde_json::json!(n);
        }
        self.append_journal(invocation_id, run_id, state, detail)
            .await?;
        Ok(())
    }

    pub async fn list_journal(&self, run_id: Uuid) -> DbResult<Vec<ActionJournalRow>> {
        Ok(sqlx::query_as::<_, ActionJournalRow>(
            "SELECT * FROM action_journal WHERE run_id = $1 ORDER BY created_at",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn latest_write_invocation(
        &self,
        run_id: Uuid,
    ) -> DbResult<Option<ToolInvocationRow>> {
        Ok(sqlx::query_as::<_, ToolInvocationRow>(
            r#"SELECT * FROM tool_invocations WHERE run_id = $1 AND effect <> 'READ_ONLY'
               ORDER BY created_at DESC LIMIT 1"#,
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn record_recovery_attempt(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        phase: &str,
        action_taken: &str,
        result: &str,
        reason: Option<&str>,
        lease_generation: Option<i64>,
        detail: Value,
    ) -> DbResult<RecoveryAttemptRow> {
        let attempt: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(attempt), 0) + 1 FROM recovery_attempts WHERE run_id = $1",
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(sqlx::query_as::<_, RecoveryAttemptRow>(
            r#"INSERT INTO recovery_attempts
                 (id, run_id, project_id, attempt, phase, reason, action_taken, lease_generation, result, detail)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(project_id)
        .bind(attempt)
        .bind(phase)
        .bind(reason)
        .bind(action_taken)
        .bind(lease_generation)
        .bind(result)
        .bind(detail)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn list_recovery_attempts(&self, run_id: Uuid) -> DbResult<Vec<RecoveryAttemptRow>> {
        Ok(sqlx::query_as::<_, RecoveryAttemptRow>(
            "SELECT * FROM recovery_attempts WHERE run_id = $1 ORDER BY attempt",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn recovery_report(
        &self,
        project_id: Uuid,
        run_id: Uuid,
    ) -> DbResult<RecoveryReport> {
        let run = self.get_run(project_id, run_id).await?;
        let attempts = self.list_recovery_attempts(run_id).await?;
        let cp = self.latest_checkpoint(run_id).await?;
        let write = self.latest_write_invocation(run_id).await?;
        let approval = self.latest_approval_for_run(run_id).await?;
        let dispatch = write.as_ref().and_then(|t| {
            if t.dispatch_state.is_empty() {
                None
            } else {
                Some(t.dispatch_state.clone())
            }
        });
        let disposition = write.as_ref().and_then(|t| {
            if t.recovery_disposition.is_empty() || t.recovery_disposition == "NONE" {
                None
            } else {
                Some(t.recovery_disposition.clone())
            }
        });
        // Dispatch RECOVERY_REQUIRED is *not* the wait-deadline signal for an accepted write.
        let uncertain = matches!(dispatch.as_deref(), Some("DISPATCH_UNCERTAIN"));
        let warning = if uncertain {
            Some("This action may have reached the external system; the local outcome is unconfirmed. Do not resubmit a non-idempotent write.".into())
        } else if disposition.as_deref() == Some("MANUAL_RECONCILIATION_REQUIRED") {
            Some("The processor acknowledged this write; settlement was not confirmed before the wait deadline. Dispatch is not uncertain. Manual reconciliation may be required.".into())
        } else if disposition.as_deref() == Some("REVALIDATION_REQUIRED") {
            Some("Authoritative evidence changed after a prior decision. Sensitive writes are blocked until revalidation.".into())
        } else {
            None
        };
        Ok(RecoveryReport {
            run_id,
            status: run.status,
            verification_status: run.verification_status,
            tool_status: write.as_ref().map(|t| t.status.clone()),
            dispatch_state: dispatch,
            business_operation_status: write
                .as_ref()
                .and_then(|t| t.business_operation_status.clone()),
            recovery_disposition: disposition,
            lease_generation: Some(run.lease_generation),
            lease_owner: run.lease_owner,
            recovery_generation: Some(run.recovery_generation),
            last_checkpoint_at: cp.as_ref().map(|c| c.created_at),
            last_checkpoint_phase: cp.as_ref().map(|c| c.status.clone()),
            pending_approval_id: approval.filter(|a| a.status == "pending").map(|a| a.id),
            external_operation_id: write
                .as_ref()
                .and_then(|t| t.external_operation_id.clone())
                .or_else(|| {
                    write
                        .as_ref()
                        .and_then(|t| t.result.as_ref())
                        .and_then(|r| {
                            r.get("refund")
                                .and_then(|rf| rf.get("id"))
                                .and_then(|id| id.as_str())
                                .map(str::to_string)
                        })
                }),
            uncertain_write: Some(uncertain),
            recovery_attempts: attempts
                .into_iter()
                .map(|a| RecoveryAttemptView {
                    attempt: a.attempt,
                    phase: a.phase,
                    action_taken: a.action_taken,
                    result: a.result,
                    reason: a.reason,
                    created_at: a.created_at,
                })
                .collect(),
            warning,
        })
    }

    pub async fn bump_recovery_generation(&self, run_id: Uuid) -> DbResult<i32> {
        let g: (i32,) = sqlx::query_as("UPDATE runs SET recovery_generation = recovery_generation + 1, updated_at = $2 WHERE id = $1 RETURNING recovery_generation")
            .bind(run_id)
            .bind(Utc::now())
            .fetch_one(&self.pool)
            .await?;
        Ok(g.0)
    }
}
