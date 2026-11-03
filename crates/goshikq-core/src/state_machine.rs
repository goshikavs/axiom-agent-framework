//! Run lifecycle transitions.
//!
//! ```text
//! CREATED → PLANNING → VALIDATING_ACTION → (WAITING_FOR_APPROVAL) → EXECUTING
//!         → RECONCILING_STATE → VERIFYING_GOAL → COMPLETED | UNVERIFIED | REPLANNING | FAILED
//! ```
//! plus `PAUSED`, `RECOVERY_REQUIRED`, `WAITING_FOR_EXTERNAL_STATE`,
//! `CANCELLED`, `TIMED_OUT`, `BUDGET_EXCEEDED`, `STEP_LIMIT_EXCEEDED`, `MODEL_BUDGET_EXCEEDED`.

use goshikq_protocol::RunStatus;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid run transition {from} -> {to}")]
pub struct TransitionError {
    pub from: RunStatus,
    pub to: RunStatus,
}

/// Returns `Ok(())` when `from -> to` is a legal transition.
pub fn validate_transition(from: RunStatus, to: RunStatus) -> Result<(), TransitionError> {
    use RunStatus::*;
    if from == to {
        return Ok(());
    }
    if from.is_terminal() {
        return Err(TransitionError { from, to });
    }
    // Any non-terminal state may be cancelled, time out, exceed a budget, or require recovery.
    if matches!(
        to,
        Cancelled
            | TimedOut
            | BudgetExceeded
            | StepLimitExceeded
            | ModelBudgetExceeded
            | RecoveryRequired
    ) {
        return Ok(());
    }
    let ok = match from {
        Created => matches!(to, Planning | Failed),
        Planning => matches!(to, ValidatingAction | VerifyingGoal | Failed | Paused),
        ValidatingAction => matches!(to, WaitingForApproval | Executing | Replanning | Failed),
        WaitingForApproval => matches!(to, Executing | Replanning | Paused | Failed),
        Executing => matches!(
            to,
            ReconcilingState | VerifyingGoal | Replanning | WaitingForExternalState | Failed
        ),
        ReconcilingState => matches!(
            to,
            VerifyingGoal | ValidatingAction | Replanning | WaitingForExternalState | Failed
        ),
        VerifyingGoal => matches!(
            to,
            Completed | Unverified | Replanning | WaitingForExternalState | Failed
        ),
        Replanning => matches!(to, Planning | Failed),
        Paused => matches!(
            to,
            Planning
                | ValidatingAction
                | WaitingForApproval
                | Executing
                | WaitingForExternalState
                | Failed
        ),
        RecoveryRequired => matches!(
            to,
            Planning
                | ValidatingAction
                | WaitingForApproval
                | Executing
                | ReconcilingState
                | VerifyingGoal
                | WaitingForExternalState
                | Failed
        ),
        WaitingForExternalState => matches!(
            to,
            VerifyingGoal
                | ReconcilingState
                | Completed
                | Unverified
                | Replanning
                | Failed
                | Paused
        ),
        Completed | Unverified | Failed | Cancelled | TimedOut | BudgetExceeded
        | StepLimitExceeded | ModelBudgetExceeded => false,
    };
    if ok {
        Ok(())
    } else {
        Err(TransitionError { from, to })
    }
}

/// Tracks the current status and enforces transitions.
#[derive(Debug, Clone)]
pub struct RunStateMachine {
    status: RunStatus,
    history: Vec<RunStatus>,
}

impl RunStateMachine {
    pub fn new() -> Self {
        RunStateMachine {
            status: RunStatus::Created,
            history: vec![RunStatus::Created],
        }
    }
    pub fn resume_from(status: RunStatus) -> Self {
        RunStateMachine {
            status,
            history: vec![status],
        }
    }
    pub fn status(&self) -> RunStatus {
        self.status
    }
    pub fn history(&self) -> &[RunStatus] {
        &self.history
    }
    pub fn transition(&mut self, to: RunStatus) -> Result<RunStatus, TransitionError> {
        validate_transition(self.status, to)?;
        let from = self.status;
        self.status = to;
        self.history.push(to);
        Ok(from)
    }
}

impl Default for RunStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use RunStatus::*;

    #[test]
    fn happy_path() {
        let mut sm = RunStateMachine::new();
        for s in [
            Planning,
            ValidatingAction,
            Executing,
            ReconcilingState,
            VerifyingGoal,
            Completed,
        ] {
            sm.transition(s).unwrap();
        }
        assert!(sm.status().is_terminal());
    }

    #[test]
    fn approval_path() {
        let mut sm = RunStateMachine::new();
        for s in [Planning, ValidatingAction, WaitingForApproval, Executing] {
            sm.transition(s).unwrap();
        }
    }

    #[test]
    fn terminal_states_are_sticky() {
        let mut sm = RunStateMachine::new();
        sm.transition(Planning).unwrap();
        sm.transition(Cancelled).unwrap();
        assert_eq!(
            sm.transition(Planning),
            Err(TransitionError {
                from: Cancelled,
                to: Planning
            })
        );
        assert_eq!(
            sm.transition(Failed),
            Err(TransitionError {
                from: Cancelled,
                to: Failed
            })
        );
    }

    #[test]
    fn cannot_skip_to_completed_from_planning() {
        assert!(validate_transition(Planning, Completed).is_err());
        assert!(validate_transition(Created, Executing).is_err());
        assert!(validate_transition(Executing, Completed).is_err());
    }

    #[test]
    fn unverified_only_from_verifying_goal() {
        assert!(validate_transition(VerifyingGoal, Unverified).is_ok());
        assert!(validate_transition(Executing, Unverified).is_err());
    }

    #[test]
    fn any_active_state_can_be_cancelled_or_time_out() {
        for s in [
            Created,
            Planning,
            ValidatingAction,
            WaitingForApproval,
            Executing,
            ReconcilingState,
            VerifyingGoal,
            Replanning,
            Paused,
            RecoveryRequired,
            WaitingForExternalState,
        ] {
            assert!(validate_transition(s, Cancelled).is_ok());
            assert!(validate_transition(s, TimedOut).is_ok());
            assert!(validate_transition(s, BudgetExceeded).is_ok());
            assert!(validate_transition(s, StepLimitExceeded).is_ok());
            assert!(validate_transition(s, ModelBudgetExceeded).is_ok());
        }
    }

    #[test]
    fn wait_for_external_state_from_executing_and_verifying() {
        assert!(validate_transition(Executing, WaitingForExternalState).is_ok());
        assert!(validate_transition(VerifyingGoal, WaitingForExternalState).is_ok());
        assert!(validate_transition(WaitingForExternalState, Unverified).is_ok());
        assert!(validate_transition(WaitingForExternalState, Completed).is_ok());
        assert!(validate_transition(WaitingForExternalState, VerifyingGoal).is_ok());
    }

    #[test]
    fn recovery_required_can_reattach_to_durable_waits() {
        assert!(validate_transition(RecoveryRequired, WaitingForApproval).is_ok());
        assert!(validate_transition(RecoveryRequired, WaitingForExternalState).is_ok());
        assert!(validate_transition(RecoveryRequired, Executing).is_ok());
        assert!(validate_transition(RecoveryRequired, Planning).is_ok());
    }
}
