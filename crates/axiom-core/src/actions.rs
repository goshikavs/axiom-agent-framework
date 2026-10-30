//! Action-contract enforcement: the model proposes, the runtime validates.

use std::collections::HashMap;

use axiom_protocol::{EffectCategory, ToolCall, ToolSpec, WorkerError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ActionValidationError {
    #[error("tool `{name}` is not registered for this agent")]
    UnknownTool { name: String },
    #[error("tool `{name}` arguments failed schema validation: {errors:?}")]
    InvalidArguments { name: String, errors: Vec<String> },
    #[error("tool `{name}` requires permissions {missing:?} not granted to the agent")]
    MissingPermissions { name: String, missing: Vec<String> },
    #[error("tool `{name}` has an invalid input schema: {reason}")]
    InvalidSchema { name: String, reason: String },
    #[error("tool `{name}` arguments must be a JSON object")]
    ArgumentsNotObject { name: String },
}

/// Registry of tools available to one run, with compiled argument validators.
pub struct ActionRegistry {
    specs: HashMap<String, ToolSpec>,
    validators: HashMap<String, jsonschema::Validator>,
    agent_permissions: Vec<String>,
}

impl ActionRegistry {
    pub fn new(specs: &[ToolSpec], agent_permissions: &[String]) -> Result<Self, ActionValidationError> {
        let mut map = HashMap::new();
        let mut validators = HashMap::new();
        for spec in specs {
            let validator = jsonschema::validator_for(&spec.input_schema).map_err(|e| {
                ActionValidationError::InvalidSchema { name: spec.name.clone(), reason: e.to_string() }
            })?;
            validators.insert(spec.name.clone(), validator);
            map.insert(spec.name.clone(), spec.clone());
        }
        Ok(ActionRegistry { specs: map, validators, agent_permissions: agent_permissions.to_vec() })
    }

    pub fn spec(&self, name: &str) -> Option<&ToolSpec> {
        self.specs.get(name)
    }

    pub fn specs(&self) -> impl Iterator<Item = &ToolSpec> {
        self.specs.values()
    }

    /// Validate a model-proposed tool call: registration, argument shape, schema, permissions.
    /// Returns the spec on success. Never mutates arguments: the runtime executes exactly
    /// what was validated.
    pub fn validate_call(&self, call: &ToolCall) -> Result<&ToolSpec, ActionValidationError> {
        let spec = self
            .specs
            .get(&call.name)
            .ok_or_else(|| ActionValidationError::UnknownTool { name: call.name.clone() })?;
        if !call.arguments.is_object() {
            return Err(ActionValidationError::ArgumentsNotObject { name: call.name.clone() });
        }
        let validator = self.validators.get(&call.name).expect("validator registered with spec");
        let errors: Vec<String> = validator
            .iter_errors(&call.arguments)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();
        if !errors.is_empty() {
            return Err(ActionValidationError::InvalidArguments { name: call.name.clone(), errors });
        }
        let missing: Vec<String> = spec
            .required_permissions
            .iter()
            .filter(|p| !self.agent_permissions.iter().any(|g| g == *p || g == "*"))
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(ActionValidationError::MissingPermissions { name: call.name.clone(), missing });
        }
        Ok(spec)
    }
}

/// What the runtime should do after a failed or ambiguous tool attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum RetryDecision {
    /// Re-run the tool with the same idempotency key after `delay_ms`.
    Retry { attempt: u32, delay_ms: u64 },
    /// The side effect may or may not have happened; reconcile against authoritative state.
    Reconcile { reason: String },
    /// Give up on this invocation and report the error to the model / operator.
    Fail { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// Worker returned a structured error.
    Error { retryable: bool },
    /// The runtime stopped waiting; the tool may still have executed.
    TimedOut,
}

/// Retry policy by effect category (ADR-003). `attempt` is the attempt that just failed (1-based).
pub fn decide_retry(spec: &ToolSpec, attempt: u32, outcome: AttemptOutcome) -> RetryDecision {
    let max = spec.retry_policy.max_attempts.max(1);
    let delay_ms = (spec.retry_policy.initial_backoff_ms as f64
        * spec.retry_policy.backoff_multiplier.powi(attempt.saturating_sub(1) as i32)) as u64;
    let ambiguous_safe = spec.effect.safe_to_retry_when_ambiguous() || spec.idempotent;

    match outcome {
        AttemptOutcome::Error { retryable } => {
            if spec.effect == EffectCategory::Irreversible {
                return RetryDecision::Fail { reason: "irreversible actions are never retried automatically".into() };
            }
            if !retryable {
                return RetryDecision::Fail { reason: "tool reported a non-retryable error".into() };
            }
            if attempt >= max {
                return RetryDecision::Fail { reason: format!("retry budget exhausted after {attempt} attempt(s)") };
            }
            // A retryable error is a worker assertion that the call did not take effect.
            RetryDecision::Retry { attempt: attempt + 1, delay_ms }
        }
        AttemptOutcome::TimedOut => {
            if !ambiguous_safe {
                return RetryDecision::Reconcile {
                    reason: format!(
                        "timeout on {} action; side effect is ambiguous and must be reconciled before any retry",
                        spec.effect.as_str()
                    ),
                };
            }
            if attempt >= max {
                return RetryDecision::Fail { reason: format!("timed out after {attempt} attempt(s)") };
            }
            RetryDecision::Retry { attempt: attempt + 1, delay_ms }
        }
    }
}

/// Convert a validation failure into the structured error fed back to the model.
pub fn validation_error_to_worker_error(err: &ActionValidationError) -> WorkerError {
    let code = match err {
        ActionValidationError::UnknownTool { .. } => "unknown_tool",
        ActionValidationError::InvalidArguments { .. } => "invalid_arguments",
        ActionValidationError::MissingPermissions { .. } => "permission_denied",
        ActionValidationError::InvalidSchema { .. } => "invalid_schema",
        ActionValidationError::ArgumentsNotObject { .. } => "invalid_arguments",
    };
    WorkerError { code: code.into(), message: err.to_string(), retryable: false, details: serde_json::to_value(err).ok() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_protocol::{RetryPolicy, RiskLevel};
    use serde_json::json;

    fn spec(name: &str, effect: EffectCategory) -> ToolSpec {
        ToolSpec {
            name: name.into(),
            description: String::new(),
            input_schema: json!({
                "type": "object",
                "properties": {"order_id": {"type": "string"}, "amount": {"type": "number", "minimum": 0}},
                "required": ["order_id"],
                "additionalProperties": false
            }),
            output_schema: None,
            effect,
            risk: RiskLevel::Low,
            timeout_ms: 1000,
            retry_policy: RetryPolicy { max_attempts: 3, initial_backoff_ms: 10, backoff_multiplier: 2.0 },
            idempotent: false,
            requires_approval: false,
            required_permissions: vec![],
            preconditions: vec![],
            has_verifier: false,
            revalidation_policy: Default::default(),
            evidence_coverage: None,
        }
    }

    #[test]
    fn validates_arguments_against_schema() {
        let reg = ActionRegistry::new(&[spec("lookup", EffectCategory::ReadOnly)], &[]).unwrap();
        let ok = ToolCall { id: "1".into(), name: "lookup".into(), arguments: json!({"order_id": "o1"}) };
        assert!(reg.validate_call(&ok).is_ok());
        let bad = ToolCall { id: "2".into(), name: "lookup".into(), arguments: json!({"amount": -5}) };
        match reg.validate_call(&bad) {
            Err(ActionValidationError::InvalidArguments { errors, .. }) => assert_eq!(errors.len(), 2),
            other => panic!("unexpected {other:?}"),
        }
        let unknown = ToolCall { id: "3".into(), name: "delete_everything".into(), arguments: json!({}) };
        assert!(matches!(reg.validate_call(&unknown), Err(ActionValidationError::UnknownTool { .. })));
    }

    #[test]
    fn enforces_permissions() {
        let mut s = spec("refund", EffectCategory::NonIdempotentWrite);
        s.required_permissions = vec!["payments:write".into()];
        let reg = ActionRegistry::new(&[s.clone()], &[]).unwrap();
        let call = ToolCall { id: "1".into(), name: "refund".into(), arguments: json!({"order_id": "o1"}) };
        assert!(matches!(reg.validate_call(&call), Err(ActionValidationError::MissingPermissions { .. })));
        let reg = ActionRegistry::new(&[s], &["payments:write".into()]).unwrap();
        assert!(reg.validate_call(&call).is_ok());
    }

    #[test]
    fn timeout_on_non_idempotent_write_requires_reconciliation() {
        let s = spec("refund", EffectCategory::NonIdempotentWrite);
        assert!(matches!(decide_retry(&s, 1, AttemptOutcome::TimedOut), RetryDecision::Reconcile { .. }));
        let s = spec("lookup", EffectCategory::ReadOnly);
        assert_eq!(decide_retry(&s, 1, AttemptOutcome::TimedOut), RetryDecision::Retry { attempt: 2, delay_ms: 10 });
        assert!(matches!(decide_retry(&s, 3, AttemptOutcome::TimedOut), RetryDecision::Fail { .. }));
    }

    #[test]
    fn idempotent_write_may_retry_on_timeout() {
        let mut s = spec("refund", EffectCategory::IdempotentWrite);
        s.idempotent = true;
        assert!(matches!(decide_retry(&s, 1, AttemptOutcome::TimedOut), RetryDecision::Retry { .. }));
    }

    #[test]
    fn irreversible_never_retries() {
        let s = spec("wire", EffectCategory::Irreversible);
        assert!(matches!(decide_retry(&s, 1, AttemptOutcome::Error { retryable: true }), RetryDecision::Fail { .. }));
        assert!(matches!(decide_retry(&s, 1, AttemptOutcome::TimedOut), RetryDecision::Reconcile { .. }));
    }

    #[test]
    fn non_retryable_error_fails_fast() {
        let s = spec("lookup", EffectCategory::ReadOnly);
        assert!(matches!(decide_retry(&s, 1, AttemptOutcome::Error { retryable: false }), RetryDecision::Fail { .. }));
    }
}
