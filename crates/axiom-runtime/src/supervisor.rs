//! Recovery supervisor: claim recoverable runs, reconstruct checkpoints, resume safely.

use std::sync::Arc;
use std::time::Duration;

use axiom_core::{contract_key, recovery_action_from_journal, RecoveryAction};
use axiom_protocol::{CreateRunRequest, DispatchState, EffectCategory, EventStatus, EventType, ExecutionEvent, RunStatus, ToolStatus};
use axiom_storage::{leases::LEASE_TTL_SECS, RunRow};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::runtime::Runtime;

impl Runtime {
    /// Scan non-terminal native runs after a process start and resume those that are safe.
    pub async fn recover_on_startup(self: &Arc<Self>) -> Result<usize, crate::runtime::RuntimeError> {
        let orphaned = self.db.mark_orphaned_runs().await?;
        if !orphaned.is_empty() {
            tracing::warn!(count = orphaned.len(), "marked orphaned runs RECOVERY_REQUIRED");
            self.wake_publisher();
        }
        let resumed = self.resume_recoverable_runs().await?;
        Ok(orphaned.len() + resumed)
    }

    /// Discover recoverable runs and spawn executors. Skips runs already owned in-process.
    pub async fn resume_recoverable_runs(self: &Arc<Self>) -> Result<usize, crate::runtime::RuntimeError> {
        let rows = self.db.list_recoverable_runs().await?;
        let mut n = 0;
        for run in rows {
            if self.handle(run.id).is_some() {
                continue;
            }
            match self.resume_run(run).await {
                Ok(true) => n += 1,
                Ok(false) => {}
                Err(e) => tracing::error!(error = %e, "supervisor failed to resume a run"),
            }
        }
        Ok(n)
    }

    /// Resume one run if a worker can reattach. Returns true when an executor was spawned.
    pub async fn resume_run(self: &Arc<Self>, run: RunRow) -> Result<bool, crate::runtime::RuntimeError> {
        if self.handle(run.id).is_some() {
            return Ok(false);
        }
        let status = RunStatus::parse(&run.status).unwrap_or(RunStatus::Failed);
        if status.is_terminal() {
            return Ok(false);
        }
        let req: CreateRunRequest = serde_json::from_value(run.definition.clone())
            .map_err(|e| crate::runtime::RuntimeError::Invalid(format!("stored run definition is not a CreateRunRequest: {e}")))?;
        if req.defer_start && status == RunStatus::Created {
            return Ok(false);
        }

        if let Err(reason) = compatibility_ok(&run, &req) {
            let gen = self.db.bump_recovery_generation(run.id).await.ok();
            let _ = self.db.record_recovery_attempt(
                run.project_id,
                run.id,
                "compatibility",
                "hold",
                "intervention_required",
                Some(&reason),
                None,
                json!({"recovery_generation": gen}),
            )
            .await;
            if status != RunStatus::RecoveryRequired {
                let ev = ExecutionEvent::native(run.id, run.trace_id.clone(), EventType::RecoveryStarted)
                    .with_status(EventStatus::Pending)
                    .with_payload(json!({"reason": reason, "note": "workflow or agent definition changed; operator intervention required"}));
                self.db.transition_run(run.project_id, run.id, RunStatus::RecoveryRequired, Some(&reason), vec![ev]).await?;
                self.wake_publisher();
            }
            return Ok(false);
        }

        let attempts = self.db.list_recovery_attempts(run.id).await.unwrap_or_default();
        let max_auto = std::env::var("AXIOM_MAX_RECOVERY_ATTEMPTS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(8)
            .max(1);
        if !status.is_durable_wait() {
            let auto = attempts.iter().filter(|a| a.result == "resumed").count();
            if auto >= max_auto {
                let reason = format!("recovery attempt budget exceeded ({auto} >= {max_auto})");
                let _ = self.db.record_recovery_attempt(
                    run.project_id,
                    run.id,
                    "supervisor",
                    "hold",
                    "intervention_required",
                    Some(&reason),
                    None,
                    json!({"max_recovery_attempts": max_auto}),
                )
                .await;
                if status != RunStatus::RecoveryRequired {
                    let ev = ExecutionEvent::native(run.id, run.trace_id.clone(), EventType::RecoveryStarted)
                        .with_status(EventStatus::Pending)
                        .with_payload(json!({"reason": reason, "note": "automatic resume stopped; operator intervention required"}));
                    self.db.transition_run(run.project_id, run.id, RunStatus::RecoveryRequired, Some(&reason), vec![ev]).await?;
                    self.wake_publisher();
                }
                return Ok(false);
            }
        }

        let write = self.db.latest_write_invocation(run.id).await?;
        let effect = write
            .as_ref()
            .and_then(|t| EffectCategory::parse(&t.effect))
            .unwrap_or(EffectCategory::ReadOnly);
        let dispatch = write.as_ref().and_then(|t| DispatchState::parse(&t.dispatch_state));
        let tool_status = write.as_ref().and_then(|t| ToolStatus::parse(&t.status));
        let action = recovery_action_from_journal(effect, dispatch, status, tool_status);

        let gen = self.db.bump_recovery_generation(run.id).await.unwrap_or(run.recovery_generation);
        match action {
            RecoveryAction::ReconcileDoNotRetry => {
                let _ = self.db.record_recovery_attempt(
                    run.project_id,
                    run.id,
                    "supervisor",
                    "hold_uncertain_write",
                    "intervention_required",
                    Some("DISPATCH_UNCERTAIN non-idempotent write; not retried"),
                    None,
                    json!({"recovery_generation": gen, "dispatch_state": dispatch.map(|d| d.as_str())}),
                )
                .await;
                if let Some(inv) = &write {
                    let _ = self.db.set_dispatch_state(inv.id, run.id, DispatchState::RecoveryRequired, Some("uncertain_non_idempotent"), None, None, None).await;
                }
                if status != RunStatus::RecoveryRequired {
                    let ev = ExecutionEvent::native(run.id, run.trace_id.clone(), EventType::RecoveryStarted)
                        .with_status(EventStatus::Pending)
                        .with_payload(json!({"reason": "uncertain_non_idempotent_write", "action": "hold"}));
                    self.db.transition_run(run.project_id, run.id, RunStatus::RecoveryRequired, Some("uncertain_non_idempotent_write"), vec![ev]).await?;
                }
                self.wake_publisher();
                return Ok(false);
            }
            RecoveryAction::WaitForExternalState | RecoveryAction::ResumeApprovalWait | RecoveryAction::HoldWithoutDispatch | RecoveryAction::RetrySameIdempotencyKey | RecoveryAction::ResumePlanning => {
                let ev = ExecutionEvent::native(run.id, run.trace_id.clone(), EventType::RecoveryCompleted)
                    .with_status(EventStatus::Info)
                    .with_payload(json!({
                        "action": format!("{action:?}"),
                        "recovery_generation": gen,
                        "dispatch_state": dispatch.map(|d| d.as_str()),
                        "note": "executor reattached; the original write is not blindly re-dispatched",
                    }));
                let _ = self.db.append_events(run.project_id, vec![ev]).await;
                let _ = self.db.record_recovery_attempt(
                    run.project_id,
                    run.id,
                    "supervisor",
                    &format!("{action:?}"),
                    "resumed",
                    None,
                    None,
                    json!({"recovery_generation": gen}),
                )
                .await;
                self.wake_publisher();
                let mut run = run;
                run.recovery_generation = gen;
                self.spawn_executor(run.project_id, run, req);
                Ok(true)
            }
        }
    }
}

fn compatibility_ok(run: &RunRow, req: &CreateRunRequest) -> Result<(), String> {
    if req.agent.name != run.agent_name {
        return Err(format!("agent name changed since the run was created ({} vs {})", run.agent_name, req.agent.name));
    }
    let stored: CreateRunRequest = serde_json::from_value(run.definition.clone())
        .map_err(|e| format!("stored run definition cannot be deserialized: {e}"))?;
    if contract_key(&stored) != contract_key(req) {
        return Err("agent tool contract or goal predicate changed since the run was created".into());
    }
    Ok(())
}

pub fn lease_ttl() -> chrono::Duration {
    let secs = std::env::var("AXIOM_LEASE_TTL_SECS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(LEASE_TTL_SECS)
        .max(5);
    chrono::Duration::seconds(secs)
}

pub fn heartbeat_interval() -> Duration {
    Duration::from_secs((lease_ttl().num_seconds() / 3).max(2) as u64)
}

/// Stable per-process owner id used as the lease holder.
pub fn runtime_instance_id() -> String {
    format!("runtime-{}-{}", std::process::id(), Uuid::now_v7())
}

pub fn now_utc() -> chrono::DateTime<Utc> {
    Utc::now()
}
