//! Conservative recovery policy for process interruption (ADR-003).
//!
//! These rules decide what the runtime is *allowed* to do after a crash. They do not
//! execute tools. Exactly-once external I/O is not claimed: idempotency keys are hints
//! the simulated (or real) processor must honor.

use axiom_protocol::{CreateRunRequest, DispatchState, EffectCategory, RunStatus, ToolStatus};

/// Stable fingerprint of the agent/tool/goal contract stored with a checkpoint.
/// Used to refuse restore when the definition changed since the checkpoint was written.
pub fn contract_key(req: &CreateRunRequest) -> String {
    let mut tools: Vec<String> = req
        .agent
        .tools
        .iter()
        .map(|t| format!("{}:{}:{}", t.name, t.effect.as_str(), t.requires_approval))
        .collect();
    tools.sort();
    format!(
        "{}|{}|{}",
        req.agent.name,
        tools.join(","),
        req.goal.success_predicate.as_deref().unwrap_or("")
    )
}

/// Where the process stopped relative to a single tool-mediated action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptionBoundary {
    /// Crash before the runtime accepted the model's proposed call.
    BeforeToolExecution,
    /// Invocation row exists (`REQUESTED` / `VALIDATED`) but the worker was not asked.
    AfterIntentBeforeDispatch,
    /// Worker was executing (`RUNNING`) when the process died.
    DuringToolExecution,
    /// External system accepted the request; completion/settlement was not recorded.
    AfterExternalAcceptBeforeCompletion,
    /// Crash while evaluating a verifier or success predicate.
    DuringVerification,
    /// Human approval was requested and is still pending.
    WaitingForApproval,
}

/// What recovery is allowed to do. None of these re-dispatch an uncertain write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    /// No side effect was started; the run may re-enter planning.
    ResumePlanning,
    /// Do not dispatch. Operator or resume path must inspect the invocation row.
    HoldWithoutDispatch,
    /// Ambiguous write. Reconcile against authoritative state; never auto-retry.
    ReconcileDoNotRetry,
    /// The write was accepted. Wait for settlement; do not submit again.
    WaitForExternalState,
    /// Approval row is durable. Do not recreate it or dispatch the tool.
    ResumeApprovalWait,
    /// Safe to retry with the same idempotency key (read-only / declared-idempotent).
    RetrySameIdempotencyKey,
}

/// Map an interruption onto a recovery action. `tool_status` is the last persisted
/// invocation status, if any.
pub fn recovery_action(
    effect: EffectCategory,
    boundary: InterruptionBoundary,
    tool_status: Option<ToolStatus>,
) -> RecoveryAction {
    use InterruptionBoundary::*;
    use RecoveryAction::*;
    match boundary {
        BeforeToolExecution => ResumePlanning,
        AfterIntentBeforeDispatch => HoldWithoutDispatch,
        WaitingForApproval => ResumeApprovalWait,
        AfterExternalAcceptBeforeCompletion => WaitForExternalState,
        DuringVerification => match tool_status {
            Some(ToolStatus::Succeeded) => WaitForExternalState,
            _ => ResumePlanning,
        },
        DuringToolExecution => match (effect, tool_status) {
            (_, Some(ToolStatus::Succeeded)) => WaitForExternalState,
            (EffectCategory::ReadOnly | EffectCategory::Reversible, _) => RetrySameIdempotencyKey,
            (EffectCategory::IdempotentWrite, _) => RetrySameIdempotencyKey,
            (EffectCategory::NonIdempotentWrite | EffectCategory::Irreversible, _) => ReconcileDoNotRetry,
        },
    }
}

/// Sequence used to resume an SSE client after a history fetch.
pub fn stream_after(last_event_id: Option<i64>, query_after: Option<i64>) -> i64 {
    last_event_id.or(query_after).unwrap_or(0)
}

/// Whether startup recovery should rewrite this status to `RECOVERY_REQUIRED`.
pub fn should_mark_orphaned(status: axiom_protocol::RunStatus) -> bool {
    !status.is_terminal() && !status.is_durable_wait() && status != axiom_protocol::RunStatus::RecoveryRequired
}

/// Map persisted journal + run status onto a recovery action. Used by the supervisor.
pub fn recovery_action_from_journal(
    effect: EffectCategory,
    dispatch: Option<DispatchState>,
    run_status: RunStatus,
    _tool_status: Option<ToolStatus>,
) -> RecoveryAction {
    if run_status == RunStatus::WaitingForApproval {
        return RecoveryAction::ResumeApprovalWait;
    }
    if run_status == RunStatus::WaitingForExternalState {
        return RecoveryAction::WaitForExternalState;
    }
    match dispatch {
        None => RecoveryAction::ResumePlanning,
        Some(DispatchState::Prepared) => RecoveryAction::HoldWithoutDispatch,
        Some(DispatchState::DispatchUncertain) => match effect {
            EffectCategory::ReadOnly | EffectCategory::Reversible | EffectCategory::IdempotentWrite => {
                RecoveryAction::RetrySameIdempotencyKey
            }
            EffectCategory::NonIdempotentWrite | EffectCategory::Irreversible => RecoveryAction::ReconcileDoNotRetry,
        },
        Some(DispatchState::Accepted | DispatchState::OutcomePending | DispatchState::OutcomeConfirmed) => {
            RecoveryAction::WaitForExternalState
        }
        Some(DispatchState::Rejected) => RecoveryAction::ResumePlanning,
        Some(DispatchState::RecoveryRequired) => RecoveryAction::ReconcileDoNotRetry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_boundaries_never_blindly_retry_non_idempotent() {
        let effect = EffectCategory::NonIdempotentWrite;
        assert_eq!(recovery_action(effect, InterruptionBoundary::BeforeToolExecution, None), RecoveryAction::ResumePlanning);
        assert_eq!(recovery_action(effect, InterruptionBoundary::AfterIntentBeforeDispatch, Some(ToolStatus::Validated)), RecoveryAction::HoldWithoutDispatch);
        assert_eq!(recovery_action(effect, InterruptionBoundary::DuringToolExecution, Some(ToolStatus::Running)), RecoveryAction::ReconcileDoNotRetry);
        assert_eq!(recovery_action(effect, InterruptionBoundary::AfterExternalAcceptBeforeCompletion, Some(ToolStatus::Succeeded)), RecoveryAction::WaitForExternalState);
        assert_eq!(recovery_action(effect, InterruptionBoundary::DuringVerification, Some(ToolStatus::Succeeded)), RecoveryAction::WaitForExternalState);
        assert_eq!(recovery_action(effect, InterruptionBoundary::WaitingForApproval, Some(ToolStatus::ApprovalRequired)), RecoveryAction::ResumeApprovalWait);
    }

    #[test]
    fn idempotent_write_may_retry_same_key_after_in_flight_timeout() {
        let action = recovery_action(EffectCategory::IdempotentWrite, InterruptionBoundary::DuringToolExecution, Some(ToolStatus::Running));
        assert_eq!(action, RecoveryAction::RetrySameIdempotencyKey);
    }

    #[test]
    fn accepted_pending_settlement_never_resubmits() {
        let action = recovery_action(EffectCategory::IdempotentWrite, InterruptionBoundary::AfterExternalAcceptBeforeCompletion, Some(ToolStatus::Succeeded));
        assert_eq!(action, RecoveryAction::WaitForExternalState);
        assert_ne!(action, RecoveryAction::RetrySameIdempotencyKey);
    }

    #[test]
    fn sse_resume_prefers_last_event_id() {
        assert_eq!(stream_after(Some(12), Some(3)), 12);
        assert_eq!(stream_after(None, Some(3)), 3);
        assert_eq!(stream_after(None, None), 0);
    }

    #[test]
    fn restart_does_not_orphan_durable_waits() {
        use axiom_protocol::RunStatus::*;
        assert!(!should_mark_orphaned(WaitingForApproval));
        assert!(!should_mark_orphaned(WaitingForExternalState));
        assert!(!should_mark_orphaned(Paused));
        assert!(!should_mark_orphaned(Completed));
        assert!(should_mark_orphaned(Executing));
        assert!(should_mark_orphaned(Planning));
        assert!(should_mark_orphaned(ValidatingAction));
        assert!(should_mark_orphaned(ReconcilingState));
        assert!(should_mark_orphaned(VerifyingGoal));
    }

    #[test]
    fn journal_uncertain_non_idempotent_is_not_retried() {
        let action = recovery_action_from_journal(
            EffectCategory::NonIdempotentWrite,
            Some(DispatchState::DispatchUncertain),
            RunStatus::RecoveryRequired,
            Some(ToolStatus::Running),
        );
        assert_eq!(action, RecoveryAction::ReconcileDoNotRetry);
    }

    #[test]
    fn journal_prepared_is_hold_not_uncertain() {
        let action = recovery_action_from_journal(
            EffectCategory::NonIdempotentWrite,
            Some(DispatchState::Prepared),
            RunStatus::RecoveryRequired,
            Some(ToolStatus::Validated),
        );
        assert_eq!(action, RecoveryAction::HoldWithoutDispatch);
        let missing_status = recovery_action_from_journal(
            EffectCategory::NonIdempotentWrite,
            Some(DispatchState::Prepared),
            RunStatus::RecoveryRequired,
            None,
        );
        assert_eq!(missing_status, RecoveryAction::HoldWithoutDispatch);
    }

    #[test]
    fn journal_accepted_waits_even_for_idempotent_writes() {
        let action = recovery_action_from_journal(
            EffectCategory::IdempotentWrite,
            Some(DispatchState::Accepted),
            RunStatus::Executing,
            Some(ToolStatus::Succeeded),
        );
        assert_eq!(action, RecoveryAction::WaitForExternalState);
        assert_ne!(action, RecoveryAction::RetrySameIdempotencyKey);
    }
}
