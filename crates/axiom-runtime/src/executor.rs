//! The bounded execution loop (spec §4.6). One `RunExecutor` task per run.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axiom_core::actions::{decide_retry, validation_error_to_worker_error, ActionRegistry, AttemptOutcome, RetryDecision};
use axiom_core::baselines::{run_strategy_with_context, RevalidationStrategy, StrategyContext, StrategyOutput};
use axiom_core::coverage::{evaluate_coverage, resolve_contract_templates, CoverageReport};
use axiom_core::gates::{evaluate_action_gate, prediction_blocks_action};
use axiom_core::impact::{observed_resources, parse_decision_hint, parse_evidence_hints};
use axiom_core::redaction::redact_value;
use axiom_core::revalidation::{apply_rule, decision_after_rule, impact_and_plan, RevalidationAction};
use axiom_core::contract_key;
use axiom_core::recovery_action_from_journal;
use axiom_core::state_diff;
use axiom_core::validate_transition;
use axiom_core::RecoveryAction;
use axiom_protocol::*;
use axiom_storage::evidence::VerificationInsert;
use axiom_storage::{ApprovalRow, RunRow, StateSnapshotRow, StorageError, ToolInvocationRow};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::bridge::{AskError, RunHandle};
use crate::crash::{crash_at, crash_at_write};
use crate::runtime::{CaptureMode, Runtime};
use crate::supervisor::{heartbeat_interval, lease_ttl};

/// How a run ends (or stops).
#[derive(Debug, Clone)]
pub(crate) enum RunEnd {
    Completed,
    Unverified(String),
    Failed(String),
    Cancelled,
    TimedOut,
    StepLimitExceeded(String),
    ModelBudgetExceeded(String),
    RecoveryRequired(String),
}

pub(crate) enum GoalOutcome {
    Verified,
    NotSatisfied(String),
    NotEvaluated(String),
    InvariantViolated(String),
}

#[derive(Clone)]
pub(crate) struct WaitSpec {
    poll_after_ms: u64,
    invocation_id: Uuid,
    tool_name: String,
}

pub(crate) struct ToolOutcome {
    pub(crate) message: ModelMessage,
    /// Set when the run cannot continue (cancel, ambiguous side effect, ...).
    pub(crate) end: Option<RunEnd>,
    /// Set when the model must replan before any further action in this step.
    pub(crate) stop_step: bool,
    /// Accepted write whose settlement is still pending.
    pub(crate) wait: Option<WaitSpec>,
}

pub struct RunExecutor {
    pub(crate) rt: Arc<Runtime>,
    pub(crate) handle: Arc<RunHandle>,
    pub(crate) project_id: Uuid,
    pub run_id: Uuid,
    pub(crate) trace_id: String,
    pub(crate) root_span: String,
    pub(crate) req: CreateRunRequest,
    pub(crate) registry: Option<ActionRegistry>,
    pub(crate) limits: ExecutionLimits,
    pub(crate) started: Instant,
    pub(crate) status: RunStatus,
    // mutable loop state
    pub(crate) messages: Vec<ModelMessage>,
    pub(crate) step: u32,
    pub(crate) step_span: String,
    pub(crate) replans: u32,
    pub(crate) total_tokens: u64,
    pub(crate) total_cost: f64,
    pub(crate) cost_known: bool,
    pub(crate) last_snapshot: Option<StateSnapshotRow>,
    pub(crate) final_output: Option<String>,
    pub(crate) pending_wait: Option<WaitSpec>,
    pub(crate) lease_owner: String,
    pub(crate) lease_generation: i64,
    pub(crate) recovery_generation: i32,
    pub(crate) execution_deadline_at: DateTime<Utc>,
    pub(crate) wait_deadline_at: Option<DateTime<Utc>>,
    pub(crate) pending_invocation_id: Option<Uuid>,
    pub(crate) pending_approval_id: Option<Uuid>,
    pub(crate) pending_tool_calls: Vec<ToolCall>,
    pub(crate) agent_version_id: Option<Uuid>,
    pub(crate) workflow_id: Option<Uuid>,
    pub(crate) checkpoint_incompatible: Option<String>,
}

impl RunExecutor {
    pub fn new(rt: Arc<Runtime>, handle: Arc<RunHandle>, project_id: Uuid, run: RunRow, req: CreateRunRequest) -> Self {
        let mut limits = req.agent.execution_limits.clone();
        if let Some(s) = req.goal.max_steps {
            limits.max_steps = limits.max_steps.min(s);
        }
        if let Some(d) = req.goal.deadline_seconds {
            limits.deadline_seconds = limits.deadline_seconds.min(d);
        }
        if let Some(b) = &req.goal.budget {
            limits.max_total_tokens = match (limits.max_total_tokens, b.max_total_tokens) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            limits.max_cost_usd = match (limits.max_cost_usd, b.max_cost_usd) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        if let Some(w) = req.goal.external_wait_seconds {
            limits.external_wait_seconds = limits.external_wait_seconds.min(w);
        }
        let root_span = run.metadata.get("root_span_id").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(new_span_id);
        RunExecutor {
            rt,
            handle,
            project_id,
            run_id: run.id,
            trace_id: run.trace_id.clone(),
            root_span,
            registry: None,
            started: Instant::now(),
            status: RunStatus::parse(&run.status).unwrap_or(RunStatus::Created),
            messages: vec![],
            step: 0,
            step_span: String::new(),
            replans: 0,
            total_tokens: 0,
            total_cost: 0.0,
            cost_known: true,
            last_snapshot: None,
            final_output: None,
            pending_wait: None,
            lease_owner: String::new(),
            lease_generation: 0,
            recovery_generation: run.recovery_generation,
            execution_deadline_at: Utc::now() + chrono::Duration::seconds(limits.deadline_seconds as i64),
            wait_deadline_at: None,
            pending_invocation_id: None,
            pending_approval_id: None,
            pending_tool_calls: vec![],
            agent_version_id: run.agent_version_id,
            workflow_id: run.workflow_id,
            checkpoint_incompatible: None,
            limits,
            req,
        }
    }

    // ----------------------------------------------------------------------------------
    // Event / transition helpers
    // ----------------------------------------------------------------------------------

    pub(crate) fn event(&self, t: EventType) -> ExecutionEvent {
        let parent = if self.step_span.is_empty() { self.root_span.clone() } else { self.step_span.clone() };
        let mut e = ExecutionEvent::native(self.run_id, self.trace_id.clone(), t)
            .with_agent(self.req.agent.name.clone())
            .with_parent(parent);
        if self.step > 0 {
            e = e.with_step(format!("step-{}", self.step));
        }
        e
    }

    pub(crate) async fn emit(&self, e: ExecutionEvent) {
        if self.lease_generation != 0 {
            match self.rt.db.lease_is_valid(self.run_id, &self.lease_owner, self.lease_generation).await {
                Ok(true) => {}
                _ => {
                    tracing::warn!(run_id = %self.run_id, "dropping event; lease is fenced or expired");
                    return;
                }
            }
        }
        if let Err(err) = self.rt.db.append_events(self.project_id, vec![e]).await {
            tracing::error!(run_id = %self.run_id, error = %err, "failed to persist event");
        }
        self.rt.wake_publisher();
    }

    pub(crate) async fn transition(&mut self, to: RunStatus, reason: Option<&str>, events: Vec<ExecutionEvent>) -> bool {
        if let Err(e) = validate_transition(self.status, to) {
            tracing::error!(run_id = %self.run_id, error = %e, "illegal transition requested by executor");
            return false;
        }
        if self.status == to && events.is_empty() {
            return true;
        }
        let persist = if self.lease_generation != 0 {
            self.rt
                .db
                .transition_run_leased(self.project_id, self.run_id, to, reason, events, &self.lease_owner, self.lease_generation)
                .await
        } else {
            self.rt.db.transition_run(self.project_id, self.run_id, to, reason, events).await
        };
        match persist {
            Ok(_) => {
                self.status = to;
                self.handle.set_status(to).await;
                self.rt.wake_publisher();
                true
            }
            Err(e) => {
                if matches!(e, StorageError::Conflict(_)) {
                    tracing::error!(run_id = %self.run_id, error = %e, "transition rejected; lease fenced");
                    self.handle.cancel.cancel();
                } else {
                    tracing::error!(run_id = %self.run_id, error = %e, "failed to persist transition");
                }
                false
            }
        }
    }

    fn capture(&self, v: &Value) -> Value {
        match self.rt.cfg.capture_model_io {
            CaptureMode::Full => v.clone(),
            CaptureMode::Redacted => redact_value(v),
            CaptureMode::None => json!({"captured": false, "reason": "AXIOM_CAPTURE_MODEL_IO=none"}),
        }
    }

    fn capture_text(&self, s: &Option<String>) -> Value {
        match s {
            None => Value::Null,
            Some(t) => match self.rt.cfg.capture_model_io {
                CaptureMode::Full => json!(t),
                CaptureMode::Redacted => json!(axiom_core::redaction::redact_str(t)),
                CaptureMode::None => json!({"captured": false, "length": t.len()}),
            },
        }
    }

    fn elapsed_ms(&self) -> i64 {
        self.started.elapsed().as_millis() as i64
    }

    fn deadline_exceeded(&self) -> bool {
        Utc::now() >= self.execution_deadline_at
    }

    fn remaining(&self) -> Duration {
        (self.execution_deadline_at - Utc::now()).to_std().unwrap_or(Duration::from_secs(0))
    }

    // ----------------------------------------------------------------------------------
    // Entry point
    // ----------------------------------------------------------------------------------

    pub async fn run(mut self) {
        if let Err(e) = self.claim_lease().await {
            tracing::error!(run_id = %self.run_id, error = %e, "could not claim run lease");
            return;
        }
        self.spawn_heartbeat();
        let end = match self.execute().await {
            Ok(end) => end,
            Err(end) => end,
        };
        self.finish(end).await;
        if self.lease_generation != 0 {
            let _ = self.rt.db.release_lease(self.run_id, &self.lease_owner, self.lease_generation).await;
        }
    }

    async fn claim_lease(&mut self) -> Result<(), String> {
        self.lease_owner = self.rt.instance_id.clone();
        let gen = self.rt.db.steal_run_lease(self.run_id, &self.lease_owner, lease_ttl()).await.map_err(|e| e.to_string())?;
        self.lease_generation = gen;
        Ok(())
    }

    fn spawn_heartbeat(&self) {
        if self.lease_generation == 0 {
            return;
        }
        let db = self.rt.db.clone();
        let run_id = self.run_id;
        let owner = self.lease_owner.clone();
        let gen = self.lease_generation;
        let cancel = self.handle.cancel.clone();
        let interval = heartbeat_interval();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(interval) => {
                        match db.heartbeat_lease(run_id, &owner, gen, lease_ttl()).await {
                            Ok(true) => {}
                            Ok(false) => {
                                tracing::warn!(%run_id, "lease heartbeat rejected (fenced); cancelling executor");
                                cancel.cancel();
                                break;
                            }
                            Err(e) => tracing::warn!(%run_id, error = %e, "lease heartbeat failed"),
                        }
                    }
                }
            }
        });
    }

    async fn finish(&mut self, end: RunEnd) {
        let duration = self.elapsed_ms();
        let summary = json!({"steps": self.step, "replans": self.replans, "duration_ms": duration, "total_tokens": self.total_tokens});
        if let Err(e) = self.rt.db.set_step_count(self.run_id, self.step as i32).await {
            tracing::error!(error = %e, "set_step_count");
        }
        let _ = self.rt.db.set_final_output(self.run_id, self.final_output.as_deref()).await;
        self.step_span.clear();
        self.step = 0;
        match end {
            RunEnd::Completed => {
                let _ = self.rt.db.set_run_verification(self.run_id, VerificationStatus::Verified).await;
                let ev = self.event(EventType::RunCompleted).with_status(EventStatus::Succeeded).with_duration(duration)
                    .with_payload(json!({"outcome": "VERIFIED", "final_output": self.capture_text(&self.final_output), "summary": summary}));
                self.transition(RunStatus::Completed, None, vec![ev]).await;
            }
            RunEnd::Unverified(reason) => {
                let ev = self.event(EventType::RunCompleted).with_status(EventStatus::Unverified).with_duration(duration)
                    .with_payload(json!({"outcome": "UNVERIFIED", "reason": reason, "final_output": self.capture_text(&self.final_output), "summary": summary}));
                self.transition(RunStatus::Unverified, Some(&reason), vec![ev]).await;
            }
            RunEnd::Failed(reason) => {
                let ev = self.event(EventType::RunFailed).with_status(EventStatus::Failed).with_duration(duration)
                    .with_payload(json!({"reason": reason, "summary": summary}));
                self.transition(RunStatus::Failed, Some(&reason), vec![ev]).await;
            }
            RunEnd::Cancelled => {
                let ev = self.event(EventType::RunCancelled).with_status(EventStatus::Cancelled).with_duration(duration)
                    .with_payload(json!({"summary": summary}));
                self.transition(RunStatus::Cancelled, Some("cancelled"), vec![ev]).await;
            }
            RunEnd::TimedOut => {
                let ev = self.event(EventType::RunFailed).with_status(EventStatus::TimedOut).with_duration(duration)
                    .with_payload(json!({"reason": "deadline_exceeded", "deadline_seconds": self.limits.deadline_seconds, "conceptual_status": "EXECUTION_TIMED_OUT", "summary": summary}));
                self.transition(RunStatus::TimedOut, Some("deadline_exceeded"), vec![ev]).await;
            }
            RunEnd::StepLimitExceeded(reason) => {
                let ev = self.event(EventType::RunFailed).with_status(EventStatus::Failed).with_duration(duration)
                    .with_payload(json!({"reason": reason, "summary": summary}));
                self.transition(RunStatus::StepLimitExceeded, Some(&reason), vec![ev]).await;
            }
            RunEnd::ModelBudgetExceeded(reason) => {
                let ev = self.event(EventType::RunFailed).with_status(EventStatus::Failed).with_duration(duration)
                    .with_payload(json!({"reason": reason, "summary": summary}));
                self.transition(RunStatus::ModelBudgetExceeded, Some(&reason), vec![ev]).await;
            }
            RunEnd::RecoveryRequired(reason) => {
                let ev = self.event(EventType::RecoveryStarted).with_status(EventStatus::Pending).with_duration(duration)
                    .with_payload(json!({"reason": reason, "note": "operator action required; nothing will be retried automatically", "summary": summary}));
                self.transition(RunStatus::RecoveryRequired, Some(&reason), vec![ev]).await;
            }
        }
        let verify = if self.status == RunStatus::Completed {
            "VERIFIED"
        } else if self.status == RunStatus::Failed {
            "FAILED"
        } else {
            "NOT_EVALUATED"
        };
        let _ = self.rt.db.update_task_status(self.project_id, self.run_id, self.status.as_str(), verify).await;
        if let Err(e) = self.rt.db.save_checkpoint(self.run_id, self.step as i32, self.status, self.checkpoint_data()).await {
            tracing::error!(error = %e, "final checkpoint");
        }
    }

    fn checkpoint_data(&self) -> Value {
        json!({
            "schema_version": CheckpointV2::SCHEMA_VERSION,
            "run_id": self.run_id,
            "workflow_version": self.workflow_id,
            "agent_version_id": self.agent_version_id,
            "agent_config_hash": contract_key(&self.req),
            "goal_predicate": self.req.goal.success_predicate,
            "phase": self.status.as_str(),
            "step": self.step,
            "step_count": self.step,
            "limits": self.limits,
            "deadline_at": self.execution_deadline_at,
            "recovery_generation": self.recovery_generation,
            "last_event_sequence": 0,
            "pending_invocation_id": self.pending_invocation_id,
            "pending_approval_id": self.pending_approval_id,
            "external_operation_id": self.pending_wait.as_ref().map(|w| w.invocation_id.to_string()),
            "wait_deadline_at": self.wait_deadline_at,
            "last_state_version": self.last_snapshot.as_ref().and_then(|s| s.version.clone()),
            "last_verified_evidence_versions": self.last_snapshot.as_ref().and_then(|s| s.version.clone()),
            "last_state_snapshot_id": self.last_snapshot.as_ref().map(|s| s.id),
            "final_output": self.final_output,
            "created_at": Utc::now(),
            "replans": self.replans,
            "messages": self.capture(&serde_json::to_value(&self.messages).unwrap_or(Value::Null)),
            "total_tokens": self.total_tokens,
            "total_cost_usd": if self.cost_known { json!(self.total_cost) } else { Value::Null },
            "external_wait_seconds": self.limits.external_wait_seconds,
            "resumption_policy": "poll_authoritative_state_do_not_redispatch",
            "wait": self.pending_wait.as_ref().map(|w| json!({
                "poll_after_ms": w.poll_after_ms,
                "invocation_id": w.invocation_id,
                "tool_name": w.tool_name,
            })),
            "pending_tool_calls": self.pending_tool_calls,
        })
    }

    // ----------------------------------------------------------------------------------
    // Main loop
    // ----------------------------------------------------------------------------------

    async fn execute(&mut self) -> Result<RunEnd, RunEnd> {
        let registry = ActionRegistry::new(&self.req.agent.tools, &self.req.agent.permissions)
            .map_err(|e| RunEnd::Failed(format!("invalid tool contract: {e}")))?;
        self.registry = Some(registry);

        self.restore_from_checkpoint().await;
        if let Some(reason) = self.checkpoint_incompatible.clone() {
            return Err(RunEnd::RecoveryRequired(reason));
        }

        if let Some(def) = crate::workflow_exec::pinned_definition(&self.req.metadata) {
            return self.execute_workflow(def).await;
        }

        if let Some(end) = self.resume_if_needed().await? {
            return end;
        }

        if self.status != RunStatus::Created && self.status != RunStatus::Planning {
            // Resumed mid-loop after a tool; continue the planning loop without a second run.started.
        } else {
        let started = self.event(EventType::RunStarted).with_status(EventStatus::Started)
            .with_payload(json!({"limits": self.limits, "tool_count": self.req.agent.tools.len(), "recovery_generation": self.recovery_generation}));
        if !self.transition(RunStatus::Planning, None, vec![started]).await {
            return Err(RunEnd::Failed("could not persist run start".into()));
        }
        }

        // Initial authoritative state + invariants.
        if self.step == 0 && self.req.has_state_provider {
            if let Some(snap) = self.take_snapshot("initial").await? {
                self.last_snapshot = Some(snap);
                if !self.req.goal.invariants.is_empty() {
                    if let GoalOutcome::InvariantViolated(reason) = self.evaluate_goal("initial", false).await? {
                        return Err(RunEnd::Failed(format!("invariant violated before execution: {reason}")));
                    }
                }
            }
        }

        if self.messages.is_empty() {
            self.messages = vec![ModelMessage::system(self.system_prompt()), ModelMessage::user(self.user_prompt())];
        }

        loop {
            if self.handle.cancel.is_cancelled() {
                return Err(RunEnd::Cancelled);
            }
            if self.deadline_exceeded() {
                return Err(RunEnd::TimedOut);
            }
            if self.step >= self.limits.max_steps {
                return Err(RunEnd::StepLimitExceeded(format!("max_steps ({}) exhausted without a verified outcome", self.limits.max_steps)));
            }
            if let Some(max) = self.limits.max_total_tokens {
                if self.total_tokens > max {
                    return Err(RunEnd::ModelBudgetExceeded(format!("token budget exceeded: {} > {}", self.total_tokens, max)));
                }
            }
            if let (Some(max), true) = (self.limits.max_cost_usd, self.cost_known) {
                if self.total_cost > max {
                    return Err(RunEnd::ModelBudgetExceeded(format!("cost budget exceeded: {:.4} > {:.4} USD", self.total_cost, max)));
                }
            }

            let continuing = !self.pending_tool_calls.is_empty();
            if !continuing {
                self.step += 1;
            }
            self.step_span = new_span_id();
            let step_started = Utc::now();
            let _ = self.rt.db.upsert_step(self.run_id, self.step as i32, "planning", "running", json!({}), step_started, None).await;
            if self.status != RunStatus::Planning && !self.transition(RunStatus::Planning, None, vec![]).await {
                return Err(RunEnd::Failed("could not enter PLANNING".into()));
            }

            // 4–5. Ask the model for the next action and validate its shape.
            let model = if continuing {
                ModelGenerateResult {
                    content: None,
                    tool_calls: std::mem::take(&mut self.pending_tool_calls),
                    finish_reason: Some("tool_calls".into()),
                    usage: None,
                    provider: None,
                    model: None,
                    provider_response_id: None,
                    latency_ms: None,
                }
            } else {
                self.call_model().await?
            };
            if !continuing {
                self.messages.push(ModelMessage::assistant(model.content.clone(), model.tool_calls.clone()));
            }
            self.pending_tool_calls = model.tool_calls.clone();
            self.checkpoint().await;

            let mut step_summary = json!({"tool_calls": model.tool_calls.len(), "finish_reason": model.finish_reason});

            if model.tool_calls.is_empty() {
                // The model claims it is done. Only authoritative verification can confirm.
                self.final_output = model.content.clone();
                return match self.evaluate_goal("final", true).await? {
                    GoalOutcome::Verified => Ok(RunEnd::Completed),
                    GoalOutcome::NotEvaluated(reason) => Ok(RunEnd::Unverified(reason)),
                    GoalOutcome::InvariantViolated(reason) => Err(RunEnd::Failed(format!("invariant violated: {reason}"))),
                    GoalOutcome::NotSatisfied(reason) => {
                        if self.replans < self.limits.max_replans && self.step < self.limits.max_steps {
                            self.replans += 1;
                            self.messages.push(ModelMessage::user(format!(
                                "The goal is NOT yet satisfied according to authoritative state: {reason}. Continue working or explain what is blocking."
                            )));
                            step_summary["replan"] = json!(true);
                            let _ = self.rt.db.upsert_step(self.run_id, self.step as i32, "replanning", "completed", step_summary, step_started, Some(Utc::now())).await;
                            self.checkpoint().await;
                            if !self.transition(RunStatus::Replanning, None, vec![]).await {
                                return Err(RunEnd::Failed("could not enter REPLANNING".into()));
                            }
                            continue;
                        }
                        let _ = self.rt.db.set_run_verification(self.run_id, VerificationStatus::Failed).await;
                        Err(RunEnd::Failed(format!("goal not satisfied: {reason}")))
                    }
                };
            }

            // 6–11. Validate, (approve), execute, record, reconcile each proposed action.
            let calls = model.tool_calls.clone();
            let mut tool_messages = Vec::with_capacity(calls.len());
            let mut stop_step = false;
            for call in &calls {
                if stop_step {
                    tool_messages.push(ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"error": {"code": "skipped", "message": "skipped because a previous action in this step was denied or failed validation"}}).to_string()));
                    continue;
                }
                if !self.transition(RunStatus::ValidatingAction, None, vec![]).await {
                    return Err(RunEnd::Failed("could not enter VALIDATING_ACTION".into()));
                }
                let outcome = self.handle_tool_call(call).await;
                tool_messages.push(outcome.message);
                if let Some(end) = outcome.end {
                    return Err(end);
                }
                if let Some(wait) = outcome.wait {
                    self.messages.extend(tool_messages);
                    return self.wait_for_external_state(wait).await;
                }
                stop_step = outcome.stop_step;
            }
            self.messages.extend(tool_messages);
            self.pending_tool_calls.clear();

            // Reconcile authoritative state after the step's actions.
            if !self.transition(RunStatus::ReconcilingState, None, vec![]).await {
                return Err(RunEnd::Failed("could not enter RECONCILING_STATE".into()));
            }
            if self.req.has_state_provider {
                if let Some(snap) = self.take_snapshot("post_action").await? {
                    self.last_snapshot = Some(snap);
                }
            }

            // 12. Verify goal progress against authoritative state.
            let outcome = self.evaluate_goal("progress", self.req.goal.success_predicate.is_some()).await?;
            let _ = self.rt.db.upsert_step(self.run_id, self.step as i32, "executing", "completed", step_summary, step_started, Some(Utc::now())).await;
            self.checkpoint().await;
            match outcome {
                GoalOutcome::Verified => {
                    if self.final_output.is_none() {
                        self.final_output = model.content.clone();
                    }
                    return Ok(RunEnd::Completed);
                }
                GoalOutcome::InvariantViolated(reason) => return Err(RunEnd::Failed(format!("invariant violated: {reason}"))),
                GoalOutcome::NotSatisfied(_) | GoalOutcome::NotEvaluated(_) => {
                    if !self.transition(RunStatus::Replanning, None, vec![]).await {
                        return Err(RunEnd::Failed("could not enter REPLANNING".into()));
                    }
                }
            }
        }
    }

    pub(crate) async fn checkpoint(&self) {
        if self.lease_generation != 0 {
            match self.rt.db.lease_is_valid(self.run_id, &self.lease_owner, self.lease_generation).await {
                Ok(true) => {}
                _ => {
                    tracing::warn!(run_id = %self.run_id, "skipping checkpoint; lease is fenced or expired");
                    return;
                }
            }
        }
        if let Err(e) = self.rt.db.save_checkpoint(self.run_id, self.step as i32, self.status, self.checkpoint_data()).await {
            tracing::error!(run_id = %self.run_id, error = %e, "checkpoint failed");
        }
        let _ = self.rt.db.set_step_count(self.run_id, self.step as i32).await;
    }

    async fn restore_from_checkpoint(&mut self) {
        let Ok(Some(cp)) = self.rt.db.latest_checkpoint(self.run_id).await else {
            return;
        };
        let data = &cp.data;
        if let Some(v) = data.get("schema_version").and_then(|x| x.as_u64()) {
            if v > CheckpointV2::SCHEMA_VERSION as u64 {
                self.checkpoint_incompatible = Some(format!(
                    "checkpoint schema_version {v} is newer than supported {}",
                    CheckpointV2::SCHEMA_VERSION
                ));
                return;
            }
        }
        if let Some(h) = data.get("agent_config_hash").and_then(|v| v.as_str()) {
            if !h.is_empty() && h != contract_key(&self.req) {
                self.checkpoint_incompatible = Some("agent or goal contract changed since checkpoint creation".into());
                return;
            }
        }
        if let Some(s) = data.get("step").and_then(|v| v.as_u64()) {
            self.step = s as u32;
        }
        if let Some(s) = data.get("replans").and_then(|v| v.as_u64()) {
            self.replans = s as u32;
        }
        if let Some(t) = data.get("total_tokens").and_then(|v| v.as_u64()) {
            self.total_tokens = t;
        }
        if let Some(s) = data.get("final_output").and_then(|v| v.as_str()) {
            self.final_output = Some(s.to_string());
        }
        if let Some(id) = data.get("pending_invocation_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) {
            self.pending_invocation_id = Some(id);
        }
        if let Some(id) = data.get("pending_approval_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) {
            self.pending_approval_id = Some(id);
        }
        if let Some(s) = data.get("deadline_at").and_then(|v| v.as_str()).and_then(|s| s.parse::<DateTime<Utc>>().ok()) {
            self.execution_deadline_at = s;
        }
        if let Some(s) = data.get("wait_deadline_at").and_then(|v| v.as_str()).and_then(|s| s.parse::<DateTime<Utc>>().ok()) {
            self.wait_deadline_at = Some(s);
        }
        if let Ok(msgs) = serde_json::from_value::<Vec<ModelMessage>>(data.get("messages").cloned().unwrap_or(Value::Null)) {
            if !msgs.is_empty() {
                self.messages = msgs;
            }
        }
        if let Ok(calls) = serde_json::from_value::<Vec<ToolCall>>(data.get("pending_tool_calls").cloned().unwrap_or(Value::Null)) {
            self.pending_tool_calls = calls;
        }
        self.pending_wait = self.wait_spec_from_checkpoint_data(data);
        if self.pending_wait.is_none() {
            self.pending_wait = Some(self.wait_spec_from_checkpoint().await).filter(|w| w.invocation_id != Uuid::nil());
        }
    }

    fn wait_spec_from_checkpoint_data(&self, data: &Value) -> Option<WaitSpec> {
        let w = data.get("wait")?;
        let id = w.get("invocation_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok())?;
        let name = w.get("tool_name").and_then(|v| v.as_str()).map(str::to_string)?;
        Some(WaitSpec {
            poll_after_ms: w.get("poll_after_ms").and_then(|v| v.as_u64()).unwrap_or(500),
            invocation_id: id,
            tool_name: name,
        })
    }

    async fn resume_if_needed(&mut self) -> Result<Option<Result<RunEnd, RunEnd>>, RunEnd> {
        match self.status {
            RunStatus::WaitingForExternalState => {
                let wait = self.pending_wait.clone().unwrap_or(self.wait_spec_from_checkpoint().await);
                return Ok(Some(self.wait_for_external_state(wait).await));
            }
            RunStatus::WaitingForApproval => {
                return Ok(Some(self.resume_pending_invocation().await));
            }
            RunStatus::RecoveryRequired => {
                let write = self.rt.db.latest_write_invocation(self.run_id).await.ok().flatten();
                let effect = write
                    .as_ref()
                    .and_then(|t| EffectCategory::parse(&t.effect))
                    .unwrap_or(EffectCategory::ReadOnly);
                let dispatch = write.as_ref().and_then(|t| DispatchState::parse(&t.dispatch_state));
                let tool_status = write.as_ref().and_then(|t| ToolStatus::parse(&t.status));
                match recovery_action_from_journal(effect, dispatch, self.status, tool_status) {
                    RecoveryAction::WaitForExternalState => {
                        let wait = self.pending_wait.clone().unwrap_or(self.wait_spec_from_checkpoint().await);
                        if !self.transition(RunStatus::WaitingForExternalState, None, vec![]).await {
                            return Err(RunEnd::Failed("could not re-enter WAITING_FOR_EXTERNAL_STATE".into()));
                        }
                        return Ok(Some(self.wait_for_external_state(wait).await));
                    }
                    RecoveryAction::ResumeApprovalWait => {
                        if !self.transition(RunStatus::WaitingForApproval, None, vec![]).await {
                            return Err(RunEnd::Failed("could not re-enter WAITING_FOR_APPROVAL".into()));
                        }
                        return Ok(Some(self.resume_pending_invocation().await));
                    }
                    RecoveryAction::HoldWithoutDispatch | RecoveryAction::RetrySameIdempotencyKey => {
                        return Ok(Some(self.resume_pending_invocation().await));
                    }
                    RecoveryAction::ReconcileDoNotRetry => {
                        return Ok(Some(Err(RunEnd::RecoveryRequired("uncertain non-idempotent write; not retried".into()))));
                    }
                    RecoveryAction::ResumePlanning => {
                        if !self.transition(RunStatus::Planning, None, vec![]).await {
                            return Err(RunEnd::Failed("could not re-enter PLANNING".into()));
                        }
                        return Ok(None);
                    }
                }
            }
            _ => Ok(None),
        }
    }

    /// Reattach to a persisted invocation: wait for approval if needed, then dispatch
    /// only when the journal says the write has not already been accepted.
    async fn resume_pending_invocation(&mut self) -> Result<RunEnd, RunEnd> {
        let inv = match self.load_pending_invocation().await {
            Some(i) => i,
            None => {
                if !self.transition(RunStatus::Planning, None, vec![]).await {
                    return Err(RunEnd::Failed("could not re-enter PLANNING".into()));
                }
                return Err(RunEnd::Failed("recovery found no pending invocation to resume".into()));
            }
        };
        self.pending_invocation_id = Some(inv.id);
        let spec = match self.registry.as_ref().expect("registry").spec(&inv.tool_name) {
            Some(s) => s.clone(),
            None => return Err(RunEnd::Failed(format!("pending tool `{}` is no longer in the agent contract", inv.tool_name))),
        };
        let call = ToolCall { id: inv.tool_call_id.clone(), name: inv.tool_name.clone(), arguments: inv.arguments.clone() };
        let dispatch = DispatchState::parse(&inv.dispatch_state).unwrap_or(DispatchState::Prepared);

        if spec.requires_approval && !matches!(dispatch, DispatchState::Accepted | DispatchState::OutcomePending | DispatchState::OutcomeConfirmed) {
            match self.await_existing_approval(&inv).await {
                Ok(true) => {}
                Ok(false) => {
                    let _ = self.rt.db.finish_tool(inv.id, ToolStatus::Cancelled, None, Some(&json!({"code": "approval_denied"})), Utc::now()).await;
                    return Err(RunEnd::Failed("approval denied or expired after restart".into()));
                }
                Err(end) => return Err(end),
            }
        }

        match dispatch {
            DispatchState::Accepted | DispatchState::OutcomePending | DispatchState::OutcomeConfirmed => {
                return self.wait_for_external_state(WaitSpec {
                    poll_after_ms: 500,
                    invocation_id: inv.id,
                    tool_name: inv.tool_name.clone(),
                }).await;
            }
            DispatchState::DispatchUncertain if !spec.effect.safe_to_retry_when_ambiguous() => {
                return Err(RunEnd::RecoveryRequired(format!("uncertain write on `{}`; not retried", inv.tool_name)));
            }
            DispatchState::RecoveryRequired => {
                return Err(RunEnd::RecoveryRequired(format!("invocation {} requires operator inspection", inv.id)));
            }
            DispatchState::Rejected => {
                if !self.transition(RunStatus::Planning, None, vec![]).await {
                    return Err(RunEnd::Failed("could not re-enter PLANNING".into()));
                }
                return Ok(RunEnd::Failed("pending invocation was rejected".into()));
            }
            DispatchState::Prepared | DispatchState::DispatchUncertain => {}
        }

        let outcome = self.dispatch_existing_invocation(&inv, &spec, &call).await;
        if let Some(end) = outcome.end {
            return Err(end);
        }
        if let Some(wait) = outcome.wait {
            return self.wait_for_external_state(wait).await;
        }
        if !self.transition(RunStatus::ReconcilingState, None, vec![]).await {
            return Err(RunEnd::Failed("could not enter RECONCILING_STATE".into()));
        }
        if self.req.has_state_provider {
            if let Some(snap) = self.take_snapshot("post_action").await? {
                self.last_snapshot = Some(snap);
            }
        }
        match self.evaluate_goal("progress", self.req.goal.success_predicate.is_some()).await? {
            GoalOutcome::Verified => Ok(RunEnd::Completed),
            GoalOutcome::InvariantViolated(reason) => Err(RunEnd::Failed(format!("invariant violated: {reason}"))),
            GoalOutcome::NotSatisfied(_) | GoalOutcome::NotEvaluated(_) => {
                if !self.transition(RunStatus::Replanning, None, vec![]).await {
                    return Err(RunEnd::Failed("could not enter REPLANNING".into()));
                }
                Ok(RunEnd::Failed("resumed action did not satisfy the goal; operator should inspect".into()))
            }
        }
    }

    async fn load_pending_invocation(&self) -> Option<ToolInvocationRow> {
        if let Some(id) = self.pending_invocation_id {
            if let Ok(Some(row)) = self.rt.db.get_tool_invocation(id).await {
                return Some(row);
            }
        }
        self.rt.db.latest_write_invocation(self.run_id).await.ok().flatten()
    }

    async fn await_existing_approval(&mut self, inv: &ToolInvocationRow) -> Result<bool, RunEnd> {
        let approval = if let Some(id) = inv.approval_id.or(self.pending_approval_id) {
            self.rt.db.get_approval(self.project_id, id).await.ok()
        } else {
            self.rt.db.latest_approval_for_run(self.run_id).await.ok().flatten()
        };
        let Some(approval) = approval else {
            return Err(RunEnd::Failed("approval row missing after restart".into()));
        };
        self.pending_approval_id = Some(approval.id);
        crash_at(CrashBarrier::WaitingApproval, self.run_id);
        if approval.status == ApprovalStatus::Approved.as_str() {
            return self.revalidate_after_approval(&approval).await.map(|_| true);
        }
        if approval.status != ApprovalStatus::Pending.as_str() {
            return Ok(false);
        }
        let waiter = self.handle.register_approval_waiter(approval.id).await;
        if self.status != RunStatus::WaitingForApproval
            && !self.transition(RunStatus::WaitingForApproval, None, vec![]).await
        {
            return Err(RunEnd::Failed("could not enter WAITING_FOR_APPROVAL".into()));
        }
        self.checkpoint().await;
        let ttl = (approval.expires_at - Utc::now()).to_std().unwrap_or(Duration::from_secs(1)).min(self.remaining());
        let decided: Option<ApprovalRow> = tokio::select! {
            _ = self.handle.cancel.cancelled() => return Err(RunEnd::Cancelled),
            res = tokio::time::timeout(ttl, waiter) => res.ok().and_then(|r| r.ok()),
        };
        let approval = match decided {
            Some(a) => a,
            None => {
                let row = self.rt.db.get_approval(self.project_id, approval.id).await.ok();
                match row {
                    Some(a) if a.status == ApprovalStatus::Approved.as_str() => a,
                    Some(_) => return Ok(false),
                    None => return Ok(false),
                }
            }
        };
        if approval.status != ApprovalStatus::Approved.as_str() {
            return Ok(false);
        }
        self.revalidate_after_approval(&approval).await?;
        Ok(true)
    }

    async fn revalidate_after_approval(&mut self, approval: &ApprovalRow) -> Result<(), RunEnd> {
        if self.req.has_state_provider {
            let before = self.last_snapshot.as_ref().map(|s| s.state.clone());
            if let Ok(Some(snap)) = self.take_snapshot("pre_approved_action").await {
                let changed = before.as_ref().map(|b| !state_diff::diff(b, &snap.state).is_empty()).unwrap_or(false);
                if changed {
                    let ev = self.event(EventType::StateConflictDetected).with_status(EventStatus::Info)
                        .with_payload(json!({"approval_id": approval.id, "note": "authoritative state changed while waiting for approval; invariants re-evaluated", "changes": snap.changes}));
                    self.emit(ev).await;
                }
                self.last_snapshot = Some(snap);
                if !self.req.goal.invariants.is_empty() {
                    if let GoalOutcome::InvariantViolated(reason) = self.evaluate_goal("pre_action", false).await? {
                        return Err(RunEnd::Failed(format!("invariant violated after approval, before execution: {reason}")));
                    }
                }
            }
        }
        Ok(())
    }

    async fn dispatch_existing_invocation(&mut self, inv: &ToolInvocationRow, spec: &ToolSpec, call: &ToolCall) -> ToolOutcome {
        if !self.transition(RunStatus::Executing, None, vec![]).await {
            return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), "{}"), end: Some(RunEnd::Failed("could not enter EXECUTING".into())), stop_step: true, wait: None };
        }
        self.execute_tool_loop(inv, spec, call, &inv.idempotency_key).await
    }

    async fn wait_spec_from_checkpoint(&self) -> WaitSpec {
        if let Ok(Some(cp)) = self.rt.db.latest_checkpoint(self.run_id).await {
            if let Some(w) = cp.data.get("wait") {
                if let (Some(id), Some(name)) = (
                    w.get("invocation_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()),
                    w.get("tool_name").and_then(|v| v.as_str()).map(str::to_string),
                ) {
                    return WaitSpec {
                        poll_after_ms: w.get("poll_after_ms").and_then(|v| v.as_u64()).unwrap_or(500),
                        invocation_id: id,
                        tool_name: name,
                    };
                }
            }
        }
        WaitSpec { poll_after_ms: 500, invocation_id: Uuid::nil(), tool_name: "unknown".into() }
    }

    /// Poll authoritative state until the goal is verified or the wait deadline elapses.
    /// Does not call the model and does not re-dispatch the original write.
    async fn wait_for_external_state(&mut self, wait: WaitSpec) -> Result<RunEnd, RunEnd> {
        self.pending_wait = Some(WaitSpec { poll_after_ms: wait.poll_after_ms.max(200).min(5_000), invocation_id: wait.invocation_id, tool_name: wait.tool_name.clone() });
        self.pending_invocation_id = Some(wait.invocation_id);
        let poll = Duration::from_millis(self.pending_wait.as_ref().unwrap().poll_after_ms);
        if self.wait_deadline_at.is_none() {
            self.wait_deadline_at = Some(Utc::now() + chrono::Duration::seconds(self.limits.external_wait_seconds as i64));
        }
        if self.status != RunStatus::WaitingForExternalState {
            let ev = self.event(EventType::StateChanged).with_status(EventStatus::Pending).with_payload(json!({
                "reason": "waiting_for_external_state",
                "invocation_id": wait.invocation_id,
                "tool": wait.tool_name,
                "deadline_seconds": self.limits.external_wait_seconds,
                "poll_after_ms": poll.as_millis(),
                "wait_deadline_at": self.wait_deadline_at,
                "resumption_policy": "poll_authoritative_state_do_not_redispatch",
            }));
            if !self.transition(RunStatus::WaitingForExternalState, None, vec![ev]).await {
                return Err(RunEnd::Failed("could not enter WAITING_FOR_EXTERNAL_STATE".into()));
            }
        }
        self.checkpoint().await;

        loop {
            if self.handle.cancel.is_cancelled() {
                return Err(RunEnd::Cancelled);
            }
            let remaining_wait = self.wait_deadline_at.unwrap_or(Utc::now()).signed_duration_since(Utc::now()).to_std().unwrap_or(Duration::from_secs(0));
            if remaining_wait.is_zero() || self.deadline_exceeded() {
                let reason = if remaining_wait.is_zero() {
                    "GOAL_VERIFICATION_INCONCLUSIVE: external settlement did not complete before the wait deadline; the original write was not re-dispatched"
                } else {
                    "GOAL_VERIFICATION_INCONCLUSIVE: the run deadline elapsed while waiting for external settlement; the original write was not re-dispatched"
                };
                let _ = self.rt.db.set_run_verification(self.run_id, VerificationStatus::Inconclusive).await;
                if let Some(inv) = self.pending_invocation_id {
                    // Keep historical dispatch (ACCEPTED / OUTCOME_PENDING). Settlement is unresolved.
                    let _ = self.rt.db.set_dispatch_state(inv, self.run_id, DispatchState::OutcomePending, Some("wait_deadline"), Some("PENDING_SETTLEMENT"), None, None).await;
                    let _ = self.rt.db.set_recovery_disposition(inv, self.run_id, RecoveryDisposition::ManualReconciliationRequired, Some("wait_deadline")).await;
                }
                let ev = self.event(EventType::VerificationInconclusive).with_status(EventStatus::Inconclusive)
                    .with_payload(json!({
                        "subject": "goal",
                        "phase": "wait_deadline",
                        "reason": reason,
                        "conceptual_status": "GOAL_VERIFICATION_INCONCLUSIVE",
                        "wait_deadline_at": self.wait_deadline_at,
                        "external_operation_id": self.pending_invocation_id,
                        "manual_reconciliation_required": true,
                        "dispatch_state": "OUTCOME_PENDING",
                        "recovery_disposition": "MANUAL_RECONCILIATION_REQUIRED",
                    }));
                self.emit(ev).await;
                return Ok(RunEnd::Unverified(reason.into()));
            }
            tokio::select! {
                _ = self.handle.cancel.cancelled() => return Err(RunEnd::Cancelled),
                _ = tokio::time::sleep(poll.min(remaining_wait)) => {}
            }
            if self.req.has_state_provider {
                if let Some(snap) = self.take_snapshot("wait_reconcile").await? {
                    self.last_snapshot = Some(snap);
                }
            }
            match self.evaluate_goal("wait", true).await? {
                GoalOutcome::Verified => {
                    if self.final_output.is_none() {
                        self.final_output = Some(format!("external operation `{}` settled against authoritative state", wait.tool_name));
                    }
                    if let Some(inv) = self.pending_invocation_id {
                        let _ = self.rt.db.set_dispatch_state(inv, self.run_id, DispatchState::OutcomeConfirmed, Some("wait_settled"), Some("SETTLED"), None, None).await;
                    }
                    return Ok(RunEnd::Completed);
                }
                GoalOutcome::InvariantViolated(reason) => {
                    return Err(RunEnd::Failed(format!("invariant violated while waiting: {reason}")));
                }
                GoalOutcome::NotSatisfied(_) | GoalOutcome::NotEvaluated(_) => {
                    if self.status != RunStatus::WaitingForExternalState
                        && !self.transition(RunStatus::WaitingForExternalState, None, vec![]).await
                    {
                        return Err(RunEnd::Failed("could not re-enter WAITING_FOR_EXTERNAL_STATE".into()));
                    }
                    self.checkpoint().await;
                }
            }
        }
    }

    fn system_prompt(&self) -> String {
        let mut s = String::new();
        if !self.req.agent.instructions.trim().is_empty() {
            s.push_str(self.req.agent.instructions.trim());
            s.push_str("\n\n");
        }
        s.push_str("You are operating inside the Axiom runtime. Propose actions by calling the provided tools. ");
        s.push_str("Every action is validated against its contract before execution; tool results are authoritative. ");
        s.push_str("When the goal has been achieved, reply without calling tools and summarize the outcome.");
        s
    }

    fn user_prompt(&self) -> String {
        let input = match &self.req.input {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        };
        if input.is_empty() {
            format!("Goal: {}", self.req.goal.description)
        } else {
            format!("Goal: {}\n\nInput:\n{}", self.req.goal.description, input)
        }
    }

    // ----------------------------------------------------------------------------------
    // Model
    // ----------------------------------------------------------------------------------

    async fn call_model(&mut self) -> Result<ModelGenerateResult, RunEnd> {
        let tools: Vec<ModelToolSchema> = self
            .req
            .agent
            .tools
            .iter()
            .map(|t| ModelToolSchema { name: t.name.clone(), description: t.description.clone(), parameters: t.input_schema.clone() })
            .collect();
        let max_attempts = self.req.agent.retry_policy.max_attempts.max(1);
        let mut attempt = 0;
        loop {
            attempt += 1;
            let span = self.step_span.clone();
            let requested = self.event(EventType::ModelRequested).with_span(span.clone()).with_parent(self.root_span.clone())
                .with_status(EventStatus::Requested)
                .with_payload(json!({
                    "model": self.req.agent.model,
                    "attempt": attempt,
                    "message_count": self.messages.len(),
                    "tools": tools.iter().map(|t| &t.name).collect::<Vec<_>>(),
                    "messages": self.capture(&serde_json::to_value(&self.messages).unwrap_or(Value::Null)),
                }));
            self.emit(requested).await;
            let t0 = Instant::now();
            let req = ModelGenerateRequest { model: self.req.agent.model.clone(), messages: self.messages.clone(), tools: tools.clone(), config: self.req.agent.context_policy.clone(), step: self.step };
            let timeout = self.rt.cfg.worker_request_timeout.min(self.remaining().max(Duration::from_secs(1)));
            let res = self.handle.ask(WorkerRequestKind::ModelGenerate(req), timeout).await;
            let latency = t0.elapsed().as_millis() as i64;
            match res {
                Ok(v) => {
                    let result: ModelGenerateResult = serde_json::from_value(v).map_err(|e| RunEnd::Failed(format!("model adapter returned malformed result: {e}")))?;
                    self.record_model_result(&result, latency).await;
                    return Ok(result);
                }
                Err(AskError::Cancelled) => return Err(RunEnd::Cancelled),
                Err(AskError::Malformed(m)) => return Err(RunEnd::Failed(format!("model adapter: {m}"))),
                Err(err) => {
                    let (code, message, retryable) = match &err {
                        AskError::Worker(w) => (w.code.clone(), w.message.clone(), w.retryable),
                        AskError::Timeout => ("timeout".into(), format!("no model response within {}ms", timeout.as_millis()), true),
                        _ => unreachable!(),
                    };
                    let failed = self.event(EventType::ModelFailed).with_status(EventStatus::Failed).with_duration(latency)
                        .with_payload(json!({"model": self.req.agent.model, "attempt": attempt, "error": {"code": code, "message": message, "retryable": retryable}}));
                    self.emit(failed).await;
                    if retryable && attempt < max_attempts && !self.deadline_exceeded() {
                        let backoff = self.req.agent.retry_policy.initial_backoff_ms as f64 * self.req.agent.retry_policy.backoff_multiplier.powi(attempt as i32 - 1);
                        tokio::time::sleep(Duration::from_millis(backoff as u64)).await;
                        continue;
                    }
                    if matches!(err, AskError::Timeout) && self.deadline_exceeded() {
                        return Err(RunEnd::TimedOut);
                    }
                    return Err(RunEnd::Failed(format!("model call failed after {attempt} attempt(s): {message}")));
                }
            }
        }
    }

    async fn record_model_result(&mut self, r: &ModelGenerateResult, latency_ms: i64) {
        let model = r.model.clone().unwrap_or_else(|| self.req.agent.model.clone());
        let cost = r.usage.as_ref().and_then(|u| self.rt.cfg.pricing.estimate(&model, u));
        if let Some(u) = &r.usage {
            self.total_tokens += u.total();
        }
        match cost {
            Some(c) => self.total_cost += c,
            None => self.cost_known = false,
        }
        let completed = self.event(EventType::ModelCompleted).with_status(EventStatus::Succeeded).with_duration(latency_ms)
            .with_payload(json!({
                "model": model,
                "provider": r.provider,
                "content": self.capture_text(&r.content),
                "tool_calls": self.capture(&serde_json::to_value(&r.tool_calls).unwrap_or(Value::Null)),
                "finish_reason": r.finish_reason,
                "provider_response_id": r.provider_response_id,
                "provider_latency_ms": r.latency_ms,
            }));
        self.emit(completed).await;
        let usage_ev = self.event(EventType::ModelUsageReported).with_status(EventStatus::Info)
            .with_payload(json!({
                "model": model,
                "usage": r.usage,
                "usage_known": r.usage.is_some(),
                "estimated_cost_usd": cost,
                "cost_known": cost.is_some(),
                "pricing_version": self.rt.cfg.pricing.version,
                "run_total_tokens": self.total_tokens,
            }));
        self.emit(usage_ev).await;
        let step_id = format!("step-{}", self.step);
        let pricing_version = self.rt.cfg.pricing.version.clone();
        if let Err(e) = self
            .rt
            .db
            .save_model_usage(self.project_id, self.run_id, &step_id, &model, r.provider.as_deref(), r.usage.as_ref(), r.latency_ms.or(Some(latency_ms)), r.provider_response_id.as_deref(), r.finish_reason.as_deref(), cost.map(|c| (c, pricing_version.as_str())))
            .await
        {
            tracing::error!(error = %e, "save_model_usage");
        }
        let _ = self.rt.db.recompute_run_usage(self.run_id).await;
    }

    // ----------------------------------------------------------------------------------
    // Tools
    // ----------------------------------------------------------------------------------

    pub(crate) async fn handle_tool_call(&mut self, call: &ToolCall) -> ToolOutcome {
        let step_id = format!("step-{}", self.step);
        let requested = self.event(EventType::ToolRequested).with_status(EventStatus::Requested)
            .with_payload(json!({"tool": call.name, "tool_call_id": call.id, "arguments": self.capture(&call.arguments)}));
        self.emit(requested).await;

        let spec = match self.registry.as_ref().expect("registry").validate_call(call) {
            Ok(spec) => spec.clone(),
            Err(err) => {
                let werr = validation_error_to_worker_error(&err);
                let error_json = serde_json::to_value(&werr).unwrap_or(Value::Null);
                let _ = self.rt.db.create_rejected_tool_invocation(self.project_id, self.run_id, &step_id, &call.id, &call.name, &call.arguments, &error_json).await;
                let failed = self.event(EventType::ToolFailed).with_status(EventStatus::Failed)
                    .with_payload(json!({"tool": call.name, "tool_call_id": call.id, "phase": "validation", "error": error_json}));
                self.emit(failed).await;
                return ToolOutcome {
                    message: ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"error": werr}).to_string()),
                    end: None,
                    stop_step: true,
                    wait: None,
                };
            }
        };

        crash_at_write(CrashBarrier::BeforeDispatch, self.run_id, spec.effect);
        let idempotency_key = format!("{}:{}:{}", self.run_id, self.step, call.id);
        let inv = match self.rt.db.create_tool_invocation(self.project_id, self.run_id, &step_id, &call.id, &spec, &call.arguments, &idempotency_key).await {
            Ok(i) => i,
            Err(e) => return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), "{\"error\":{\"code\":\"internal\"}}"), end: Some(RunEnd::Failed(format!("persist tool invocation: {e}"))), stop_step: true, wait: None },
        };
        self.pending_invocation_id = Some(inv.id);
        self.checkpoint().await;
        crash_at_write(CrashBarrier::AfterIntent, self.run_id, spec.effect);
        let _ = self.rt.db.set_tool_status(inv.id, ToolStatus::Validated).await;
        let validated = self.event(EventType::ToolValidated).with_status(EventStatus::Validated)
            .with_payload(json!({
                "tool": call.name, "tool_call_id": call.id, "invocation_id": inv.id,
                "effect": spec.effect, "risk": spec.risk, "requires_approval": spec.requires_approval,
                "required_permissions": spec.required_permissions, "idempotency_key": idempotency_key,
                "preconditions": spec.preconditions,
            }));
        self.emit(validated).await;

        // 7. Approval gate.
        if spec.requires_approval {
            match self.await_approval(&inv.id, &spec, call).await {
                Ok(true) => {}
                Ok(false) => {
                    let _ = self.rt.db.finish_tool(inv.id, ToolStatus::Cancelled, None, Some(&json!({"code": "approval_denied"})), Utc::now()).await;
                    return ToolOutcome {
                        message: ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"error": {"code": "approval_denied", "message": "a human reviewer denied this action; do not retry it without a different justification"}}).to_string()),
                        end: None,
                        stop_step: true,
                        wait: None,
                    };
                }
                Err(end) => {
                    let _ = self.rt.db.finish_tool(inv.id, ToolStatus::Cancelled, None, Some(&json!({"code": "run_ended"})), Utc::now()).await;
                    return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), "{\"error\":{\"code\":\"cancelled\"}}"), end: Some(end), stop_step: true, wait: None };
                }
            }
        }

        if !self.transition(RunStatus::Executing, None, vec![]).await {
            return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), "{}"), end: Some(RunEnd::Failed("could not enter EXECUTING".into())), stop_step: true, wait: None };
        }

        if is_sensitive_write(spec.effect) {
            if let Some(blocked) = self.revalidate_before_write(&spec, call).await {
                let _ = self.rt.db.finish_tool(inv.id, ToolStatus::Cancelled, None, Some(&json!({"code": "revalidation_blocked"})), Utc::now()).await;
                return blocked;
            }
        }

        self.execute_tool_loop(&inv, &spec, call, &idempotency_key).await
    }

    async fn execute_tool_loop(&mut self, inv: &ToolInvocationRow, spec: &ToolSpec, call: &ToolCall, idempotency_key: &str) -> ToolOutcome {
        // 8–10. Execute with retry safety by effect category.
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            let started_at = Utc::now();
            let _ = self.rt.db.mark_tool_started(inv.id, attempt as i32, started_at).await;
            let _ = self.rt.db.set_dispatch_state(inv.id, self.run_id, DispatchState::DispatchUncertain, Some("dispatching"), None, None, None).await;
            let started = self.event(EventType::ToolStarted).with_status(EventStatus::Started)
                .with_payload(json!({"tool": call.name, "invocation_id": inv.id, "attempt": attempt, "timeout_ms": spec.timeout_ms, "dispatch_state": "DISPATCH_UNCERTAIN"}));
            self.emit(started).await;
            let t0 = Instant::now();
            let exec = ToolExecuteRequest { invocation_id: inv.id, tool_name: call.name.clone(), arguments: call.arguments.clone(), idempotency_key: idempotency_key.to_string(), attempt, timeout_ms: spec.timeout_ms };
            let res = self.handle.ask(WorkerRequestKind::ToolExecute(exec), Duration::from_millis(spec.timeout_ms)).await;
            crash_at_write(CrashBarrier::DuringTool, self.run_id, spec.effect);
            let dur = t0.elapsed().as_millis() as i64;
            match res {
                Ok(v) => {
                    let output = match serde_json::from_value::<ToolExecuteResult>(v.clone()) {
                        Ok(r) => r.output,
                        Err(_) => v, // tolerate bare outputs
                    };
                    let (stored, truncated) = bound_result(&output, self.limits.max_tool_result_bytes);
                    self.ingest_from_tool_output(&stored, spec, call).await;
                    let ext = stored.get("refund").and_then(|r| r.get("id")).and_then(|i| i.as_str()).map(str::to_string);
                    let _ = self.rt.db.finish_tool(inv.id, ToolStatus::Succeeded, Some(&stored), None, Utc::now()).await;
                    let business = if stored.get("settled").and_then(|v| v.as_bool()).unwrap_or(false) { "SETTLED" } else if stored.get("accepted").and_then(|v| v.as_bool()).unwrap_or(false) { "ACCEPTED" } else { "ACCEPTED" };
                    let _ = self.rt.db.set_dispatch_state(inv.id, self.run_id, DispatchState::Accepted, Some("provider_ack"), Some(business), ext.as_deref(), Some(&stored)).await;
                    let completed = self.event(EventType::ToolCompleted).with_status(EventStatus::Succeeded).with_duration(dur)
                        .with_payload(json!({"tool": call.name, "invocation_id": inv.id, "attempt": attempt, "result": self.capture(&stored), "truncated": truncated, "effect": spec.effect, "dispatch_state": "ACCEPTED", "external_operation_id": ext, "business_operation_status": business}));
                    self.emit(completed).await;
                    crash_at_write(CrashBarrier::AfterAccept, self.run_id, spec.effect);
                    let mut note = Value::Null;
                    if spec.has_verifier && self.req.has_state_provider {
                        note = self.verify_action(&inv.id, spec, call, Some(&stored), "post_execution").await;
                    }
                    let mut content = json!({"result": stored});
                    if truncated {
                        content["truncated"] = json!(true);
                    }
                    if !note.is_null() {
                        content["verification"] = note.clone();
                    }
                    if note.get("wait").and_then(|v| v.as_bool()).unwrap_or(false) {
                        let _ = self.rt.db.set_dispatch_state(inv.id, self.run_id, DispatchState::OutcomePending, Some("settlement_pending"), Some("PENDING_SETTLEMENT"), ext.as_deref(), None).await;
                        return ToolOutcome {
                            message: ModelMessage::tool(call.id.clone(), call.name.clone(), content.to_string()),
                            end: None,
                            stop_step: false,
                            wait: Some(WaitSpec {
                                poll_after_ms: note.get("poll_after_ms").and_then(|v| v.as_u64()).unwrap_or(500),
                                invocation_id: inv.id,
                                tool_name: call.name.clone(),
                            }),
                        };
                    }
                    if note.get("status").and_then(|s| s.as_str()) == Some("VERIFIED") {
                        let _ = self.rt.db.set_dispatch_state(inv.id, self.run_id, DispatchState::OutcomeConfirmed, Some("verified"), Some("SETTLED"), ext.as_deref(), None).await;
                    }
                    return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), content.to_string()), end: None, stop_step: false, wait: None };
                }
                Err(AskError::Cancelled) => {
                    let _ = self.rt.db.finish_tool(inv.id, ToolStatus::Cancelled, None, Some(&json!({"code": "cancelled"})), Utc::now()).await;
                    let _ = self.rt.db.set_dispatch_state(inv.id, self.run_id, DispatchState::Rejected, Some("cancelled"), None, None, None).await;
                    return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), "{\"error\":{\"code\":\"cancelled\"}}"), end: Some(RunEnd::Cancelled), stop_step: true, wait: None };
                }
                Err(AskError::Malformed(m)) => {
                    let err = json!({"code": "malformed_result", "message": m});
                    self.rt.metrics.failed_actions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let _ = self.rt.db.finish_tool(inv.id, ToolStatus::Failed, None, Some(&err), Utc::now()).await;
                    let _ = self.rt.db.set_dispatch_state(inv.id, self.run_id, DispatchState::Rejected, Some("malformed"), None, None, None).await;
                    let failed = self.event(EventType::ToolFailed).with_status(EventStatus::Failed).with_duration(dur).with_payload(json!({"tool": call.name, "invocation_id": inv.id, "error": err}));
                    self.emit(failed).await;
                    return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"error": err}).to_string()), end: None, stop_step: false, wait: None };
                }
                Err(AskError::Worker(werr)) => {
                    let err = serde_json::to_value(&werr).unwrap_or(Value::Null);
                    let failed = self.event(EventType::ToolFailed).with_status(EventStatus::Failed).with_duration(dur)
                        .with_payload(json!({"tool": call.name, "invocation_id": inv.id, "attempt": attempt, "error": err}));
                    self.emit(failed).await;
                    match decide_retry(spec, attempt, AttemptOutcome::Error { retryable: werr.retryable }) {
                        RetryDecision::Retry { delay_ms, .. } => {
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                            continue;
                        }
                        RetryDecision::Fail { reason } | RetryDecision::Reconcile { reason } => {
                            self.rt.metrics.failed_actions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let _ = self.rt.db.finish_tool(inv.id, ToolStatus::Failed, None, Some(&err), Utc::now()).await;
                            let _ = self.rt.db.set_dispatch_state(inv.id, self.run_id, DispatchState::Rejected, Some(&reason), Some("REJECTED"), None, None).await;
                            return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"error": werr, "runtime_note": reason}).to_string()), end: None, stop_step: false, wait: None };
                        }
                    }
                }
                Err(AskError::Timeout) => {
                    let timed_out = self.event(EventType::ToolTimedOut).with_status(EventStatus::TimedOut).with_duration(dur)
                        .with_payload(json!({"tool": call.name, "invocation_id": inv.id, "attempt": attempt, "timeout_ms": spec.timeout_ms, "effect": spec.effect, "dispatch_state": "DISPATCH_UNCERTAIN"}));
                    self.emit(timed_out).await;
                    match decide_retry(spec, attempt, AttemptOutcome::TimedOut) {
                        RetryDecision::Retry { delay_ms, .. } => {
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                            continue;
                        }
                        RetryDecision::Fail { reason } => {
                            let err = json!({"code": "timeout", "message": reason});
                            let _ = self.rt.db.finish_tool(inv.id, ToolStatus::TimedOut, None, Some(&err), Utc::now()).await;
                            let _ = self.rt.db.set_dispatch_state(inv.id, self.run_id, DispatchState::Rejected, Some("timeout"), None, None, None).await;
                            return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"error": err}).to_string()), end: None, stop_step: false, wait: None };
                        }
                        RetryDecision::Reconcile { reason } => {
                            return self.reconcile_ambiguous(&inv.id, spec, call, reason).await;
                        }
                    }
                }
            }
        }
    }

    /// A non-idempotent action timed out: establish whether it happened before anything else.
    async fn reconcile_ambiguous(&mut self, inv_id: &Uuid, spec: &ToolSpec, call: &ToolCall, reason: String) -> ToolOutcome {
        if !self.transition(RunStatus::ReconcilingState, None, vec![]).await {
            return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), "{}"), end: Some(RunEnd::Failed("could not enter RECONCILING_STATE".into())), stop_step: true, wait: None };
        }
        if spec.has_verifier && self.req.has_state_provider {
            if let Ok(Some(snap)) = self.take_snapshot("reconcile").await {
                self.last_snapshot = Some(snap);
            }
            let note = self.verify_action(inv_id, spec, call, None, "reconciliation").await;
            match note.get("status").and_then(|s| s.as_str()) {
                Some("VERIFIED") => {
                    let _ = self.rt.db.finish_tool(*inv_id, ToolStatus::Succeeded, Some(&json!({"reconciled": true})), None, Utc::now()).await;
                    let ev = self.event(EventType::ToolCompleted).with_status(EventStatus::Succeeded)
                        .with_payload(json!({"tool": call.name, "invocation_id": inv_id, "reconciled": true, "note": "timed out but verified against authoritative state"}));
                    self.emit(ev).await;
                    if !self.transition(RunStatus::Executing, None, vec![]).await {
                        return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), "{}"), end: Some(RunEnd::Failed("could not re-enter EXECUTING".into())), stop_step: true, wait: None };
                    }
                    return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"result": {"reconciled": true}, "verification": note}).to_string()), end: None, stop_step: false, wait: None };
                }
                Some("FAILED") => {
                    let err = json!({"code": "timeout", "message": "the call timed out and authoritative state shows it did not take effect"});
                    let _ = self.rt.db.finish_tool(*inv_id, ToolStatus::TimedOut, None, Some(&err), Utc::now()).await;
                    if !self.transition(RunStatus::Executing, None, vec![]).await {
                        return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), "{}"), end: Some(RunEnd::Failed("could not re-enter EXECUTING".into())), stop_step: true, wait: None };
                    }
                    return ToolOutcome { message: ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"error": err, "verification": note}).to_string()), end: None, stop_step: false, wait: None };
                }
                _ if note.get("wait").and_then(|v| v.as_bool()).unwrap_or(false) => {
                    let _ = self.rt.db.finish_tool(*inv_id, ToolStatus::Succeeded, Some(&json!({"reconciled": "pending_settlement"})), None, Utc::now()).await;
                    return ToolOutcome {
                        message: ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"result": {"accepted": true, "settled": false}, "verification": note}).to_string()),
                        end: None,
                        stop_step: false,
                        wait: Some(WaitSpec {
                            poll_after_ms: note.get("poll_after_ms").and_then(|v| v.as_u64()).unwrap_or(500),
                            invocation_id: *inv_id,
                            tool_name: call.name.clone(),
                        }),
                    };
                }
                _ => {}
            }
        }
        let err = json!({"code": "reconciliation_required", "message": reason});
        let _ = self.rt.db.finish_tool(*inv_id, ToolStatus::ReconciliationRequired, None, Some(&err), Utc::now()).await;
        ToolOutcome {
            message: ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"error": err}).to_string()),
            end: Some(RunEnd::RecoveryRequired(format!("ambiguous side effect on `{}` (invocation {}): {}", call.name, inv_id, reason))),
            stop_step: true,
            wait: None,
        }
    }

    /// Returns Ok(true) when approved, Ok(false) when denied/expired.
    async fn await_approval(&mut self, inv_id: &Uuid, spec: &ToolSpec, call: &ToolCall) -> Result<bool, RunEnd> {
        let expires_at = Utc::now() + chrono::Duration::from_std(self.rt.cfg.approval_ttl).unwrap_or(chrono::Duration::hours(1));
        let reason = format!("{} action `{}` with {} risk requires human approval", spec.effect.as_str(), spec.name, spec.risk.as_str());
        let approval = self
            .rt
            .db
            .create_approval(self.project_id, self.run_id, *inv_id, &self.req.agent.name, &spec.name, &call.arguments, spec.risk.as_str(), &reason, &spec.required_permissions, expires_at)
            .await
            .map_err(|e| RunEnd::Failed(format!("create approval: {e}")))?;
        let _ = self.rt.db.set_tool_approval(*inv_id, approval.id).await;
        let waiter = self.handle.register_approval_waiter(approval.id).await;
        let ev = self.event(EventType::ApprovalRequested).with_status(EventStatus::Pending)
            .with_payload(json!({"approval_id": approval.id, "invocation_id": inv_id, "tool": spec.name, "arguments": self.capture(&call.arguments), "risk": spec.risk, "reason": reason, "expires_at": expires_at, "requested_permissions": spec.required_permissions}));
        if !self.transition(RunStatus::WaitingForApproval, None, vec![ev]).await {
            return Err(RunEnd::Failed("could not enter WAITING_FOR_APPROVAL".into()));
        }
        self.pending_approval_id = Some(approval.id);
        self.pending_invocation_id = Some(*inv_id);
        self.checkpoint().await;
        crash_at(CrashBarrier::WaitingApproval, self.run_id);
        let ttl = self.rt.cfg.approval_ttl.min(self.remaining());
        let decided: Option<ApprovalRow> = tokio::select! {
            _ = self.handle.cancel.cancelled() => return Err(RunEnd::Cancelled),
            res = tokio::time::timeout(ttl, waiter) => res.ok().and_then(|r| r.ok()),
        };
        let approval = match decided {
            Some(a) => a,
            None => {
                let expired = self.rt.db.expire_approval(approval.id).await.ok();
                let ev = self.event(EventType::ApprovalDenied).with_status(EventStatus::Denied)
                    .with_payload(json!({"approval_id": approval.id, "reason": "expired", "expires_at": expires_at}));
                self.emit(ev).await;
                if self.deadline_exceeded() {
                    return Err(RunEnd::TimedOut);
                }
                let _ = expired;
                let _ = self.transition(RunStatus::Replanning, None, vec![]).await;
                return Ok(false);
            }
        };
        if approval.status != ApprovalStatus::Approved.as_str() {
            let ev = self.event(EventType::ApprovalDenied).with_status(EventStatus::Denied)
                .with_payload(json!({"approval_id": approval.id, "decided_by": approval.decided_by, "reason": approval.decision_reason}));
            self.emit(ev).await;
            let _ = self.transition(RunStatus::Replanning, None, vec![]).await;
            return Ok(false);
        }
        let _ = self.rt.db.set_tool_status(*inv_id, ToolStatus::Approved).await;
        let ev = self.event(EventType::ApprovalGranted).with_status(EventStatus::Granted)
            .with_payload(json!({"approval_id": approval.id, "decided_by": approval.decided_by, "reason": approval.decision_reason, "decided_at": approval.decided_at}));
        self.emit(ev).await;

        // 20. Revalidate state before executing a previously approved action.
        if self.req.has_state_provider {
            let before = self.last_snapshot.as_ref().map(|s| s.state.clone());
            if let Ok(Some(snap)) = self.take_snapshot("pre_approved_action").await {
                let changed = before.as_ref().map(|b| !state_diff::diff(b, &snap.state).is_empty()).unwrap_or(false);
                if changed {
                    let ev = self.event(EventType::StateConflictDetected).with_status(EventStatus::Info)
                        .with_payload(json!({"approval_id": approval.id, "note": "authoritative state changed while waiting for approval; invariants re-evaluated", "changes": snap.changes}));
                    self.emit(ev).await;
                }
                self.last_snapshot = Some(snap);
                if let Some(end) = self.apply_evidence_impact(Some(approval.id), spec.name.as_str()).await {
                    return Err(end);
                }
                if !self.req.goal.invariants.is_empty() {
                    if let GoalOutcome::InvariantViolated(reason) = self.evaluate_goal("pre_action", false).await? {
                        return Err(RunEnd::Failed(format!("invariant violated after approval, before execution: {reason}")));
                    }
                }
            }
        }
        Ok(true)
    }

    // ----------------------------------------------------------------------------------
    // State & verification
    // ----------------------------------------------------------------------------------

    async fn take_snapshot(&mut self, reason: &str) -> Result<Option<StateSnapshotRow>, RunEnd> {
        let req = StateSnapshotRequest { reason: reason.into(), step: if self.step > 0 { Some(self.step) } else { None } };
        let timeout = self.rt.cfg.worker_request_timeout.min(self.remaining().max(Duration::from_secs(1)));
        match self.handle.ask(WorkerRequestKind::StateSnapshot(req), timeout).await {
            Ok(v) => {
                let snap: StateSnapshotResult = serde_json::from_value(v).map_err(|e| RunEnd::Failed(format!("state provider returned malformed snapshot: {e}")))?;
                let changes = self.last_snapshot.as_ref().map(|prev| state_diff::diff(&prev.state, &snap.state));
                let changes_json = changes.as_ref().map(|c| serde_json::to_value(c).unwrap_or(Value::Null));
                let step_id = if self.step > 0 { Some(format!("step-{}", self.step)) } else { None };
                let row = self.rt.db.save_state_snapshot(self.run_id, step_id.as_deref(), reason, &snap, changes_json.as_ref()).await.map_err(|e| RunEnd::Failed(format!("persist snapshot: {e}")))?;
                let ev = self.event(EventType::StateSnapshot).with_status(EventStatus::Info)
                    .with_payload(json!({"snapshot_id": row.id, "reason": reason, "source": snap.source, "version": snap.version, "state": self.capture(&snap.state)}));
                self.emit(ev).await;
                if let Some(c) = &changes {
                    if !c.is_empty() {
                        let ev = self.event(EventType::StateChanged).with_status(EventStatus::Info)
                            .with_payload(json!({"snapshot_id": row.id, "reason": reason, "version": snap.version, "previous_version": self.last_snapshot.as_ref().and_then(|p| p.version.clone()), "changes": self.capture(&changes_json.clone().unwrap_or(Value::Null))}));
                        self.emit(ev).await;
                    }
                }
                self.record_snapshot_resources(&row).await;
                Ok(Some(row))
            }
            Err(AskError::Cancelled) => Err(RunEnd::Cancelled),
            Err(AskError::Timeout) if self.deadline_exceeded() => Err(RunEnd::TimedOut),
            Err(err) => {
                let msg = match err { AskError::Worker(w) => w.message, AskError::Timeout => "state provider timed out".into(), AskError::Malformed(m) => m, AskError::Cancelled => unreachable!() };
                let ev = self.event(EventType::VerificationInconclusive).with_status(EventStatus::Inconclusive)
                    .with_payload(json!({"subject": "state_snapshot", "reason": reason, "error": msg}));
                self.emit(ev).await;
                Ok(None)
            }
        }
    }

    async fn evaluate_goal(&mut self, phase: &str, evaluate_success: bool) -> Result<GoalOutcome, RunEnd> {
        let final_phase = phase == "final";
        if final_phase && !self.transition(RunStatus::VerifyingGoal, None, vec![]).await {
            return Err(RunEnd::Failed("could not enter VERIFYING_GOAL".into()));
        }
        if phase == "progress" && !self.transition(RunStatus::VerifyingGoal, None, vec![]).await {
            return Err(RunEnd::Failed("could not enter VERIFYING_GOAL".into()));
        }
        if self.req.goal.verification_policy == VerificationPolicy::None && evaluate_success {
            return Ok(GoalOutcome::NotEvaluated("verification_policy=none".into()));
        }
        if !self.req.has_state_provider {
            return Ok(GoalOutcome::NotEvaluated("no state provider registered; the goal cannot be verified against authoritative state".into()));
        }
        if evaluate_success && self.req.goal.success_predicate.is_none() {
            return Ok(GoalOutcome::NotEvaluated("no success predicate declared".into()));
        }
        if self.req.goal.invariants.is_empty() && !evaluate_success {
            return Ok(GoalOutcome::NotEvaluated("no invariants".into()));
        }

        // Fresh authoritative read for final/progress verification.
        if evaluate_success {
            if let Some(snap) = self.take_snapshot(if final_phase { "final" } else { "verify" }).await? {
                self.last_snapshot = Some(snap);
            }
        }
        let Some(snapshot) = self.last_snapshot.clone() else {
            return Ok(GoalOutcome::NotEvaluated("no authoritative state snapshot available".into()));
        };
        let t0 = Instant::now();
        if evaluate_success {
            let ev = self.event(EventType::VerificationStarted).with_status(EventStatus::Started)
                .with_payload(json!({"subject": "goal", "phase": phase, "predicate": self.req.goal.success_predicate, "invariants": self.req.goal.invariants, "snapshot_id": snapshot.id, "state_version": snapshot.version}));
            self.emit(ev).await;
        }
        let req = GoalEvaluateRequest { state: snapshot.state.clone(), state_version: snapshot.version.clone(), phase: phase.into(), evaluate_success };
        let timeout = self.rt.cfg.worker_request_timeout.min(self.remaining().max(Duration::from_secs(1)));
        let step_id = if self.step > 0 { Some(format!("step-{}", self.step)) } else { None };
        let result: GoalEvaluateResult = match self.handle.ask(WorkerRequestKind::GoalEvaluate(req), timeout).await {
            Ok(v) => serde_json::from_value(v).map_err(|e| RunEnd::Failed(format!("goal evaluator returned malformed result: {e}")))?,
            Err(AskError::Cancelled) => return Err(RunEnd::Cancelled),
            Err(err) => {
                let msg = match err { AskError::Worker(w) => w.message, AskError::Timeout => "goal evaluator timed out".into(), AskError::Malformed(m) => m, AskError::Cancelled => unreachable!() };
                let dur = t0.elapsed().as_millis() as i64;
                let _ = self.rt.db.save_verification(VerificationInsert { run_id: self.run_id, step_id: step_id.as_deref(), subject_type: "goal", subject_id: self.req.goal.success_predicate.as_deref().unwrap_or("goal"), status: VerificationStatus::Inconclusive, method: "predicate", expected: None, actual: None, evidence_source: Some(&snapshot.source), evidence_version: snapshot.version.as_deref(), evidence_timestamp: Some(snapshot.captured_at), duration_ms: Some(dur), reason: Some(&msg) }).await;
                let ev = self.event(EventType::VerificationInconclusive).with_status(EventStatus::Inconclusive).with_duration(dur).with_payload(json!({"subject": "goal", "phase": phase, "reason": msg}));
                self.emit(ev).await;
                let _ = self.rt.db.set_run_verification(self.run_id, VerificationStatus::Inconclusive).await;
                return Ok(GoalOutcome::NotEvaluated(format!("verification inconclusive: {msg}")));
            }
        };
        let dur = t0.elapsed().as_millis() as i64;
        let _ = self.rt.db.save_goal_evaluation(self.run_id, step_id.as_deref(), phase, &result, Some(snapshot.id)).await;
        let violated: Vec<&InvariantResult> = result.invariants.iter().filter(|i| !i.satisfied).collect();
        let ev = self.event(EventType::GoalEvaluated).with_status(if violated.is_empty() { EventStatus::Info } else { EventStatus::Failed }).with_duration(dur)
            .with_payload(json!({"phase": phase, "success": result.success, "invariants": result.invariants, "reason": result.reason, "evidence": self.capture(&result.evidence), "snapshot_id": snapshot.id, "state_version": snapshot.version}));
        self.emit(ev).await;

        if !violated.is_empty() {
            let reason = violated.iter().map(|i| format!("{}{}", i.name, i.reason.as_ref().map(|r| format!(": {r}")).unwrap_or_default())).collect::<Vec<_>>().join("; ");
            let _ = self.rt.db.save_verification(VerificationInsert { run_id: self.run_id, step_id: step_id.as_deref(), subject_type: "goal", subject_id: "invariants", status: VerificationStatus::Failed, method: "invariant", expected: Some(&json!({"all_invariants_satisfied": true})), actual: Some(&serde_json::to_value(&result.invariants).unwrap_or(Value::Null)), evidence_source: Some(&snapshot.source), evidence_version: snapshot.version.as_deref(), evidence_timestamp: Some(snapshot.captured_at), duration_ms: Some(dur), reason: Some(&reason) }).await;
            let ev = self.event(EventType::GoalFailed).with_status(EventStatus::Failed).with_payload(json!({"reason": reason, "phase": phase, "violated": violated}));
            self.emit(ev).await;
            let ev = self.event(EventType::VerificationFailed).with_status(EventStatus::Failed).with_duration(dur).with_payload(json!({"subject": "goal", "phase": phase, "reason": reason}));
            self.emit(ev).await;
            let _ = self.rt.db.set_run_verification(self.run_id, VerificationStatus::Failed).await;
            return Ok(GoalOutcome::InvariantViolated(reason));
        }
        if !evaluate_success {
            return Ok(GoalOutcome::NotEvaluated("invariants only".into()));
        }
        let expected = result.evidence.get("expected").cloned();
        let actual = result.evidence.get("actual").cloned();
        match result.success {
            Some(true) => {
                let _ = self.rt.db.save_verification(VerificationInsert { run_id: self.run_id, step_id: step_id.as_deref(), subject_type: "goal", subject_id: self.req.goal.success_predicate.as_deref().unwrap_or("goal"), status: VerificationStatus::Verified, method: "predicate", expected: expected.as_ref(), actual: actual.as_ref(), evidence_source: Some(&snapshot.source), evidence_version: snapshot.version.as_deref(), evidence_timestamp: Some(snapshot.captured_at), duration_ms: Some(dur), reason: result.reason.as_deref() }).await;
                let ev = self.event(EventType::VerificationPassed).with_status(EventStatus::Passed).with_duration(dur).with_payload(json!({"subject": "goal", "phase": phase, "evidence": self.capture(&result.evidence), "state_version": snapshot.version, "source": snapshot.source}));
                self.emit(ev).await;
                let ev = self.event(EventType::GoalSatisfied).with_status(EventStatus::Succeeded).with_payload(json!({"phase": phase, "predicate": self.req.goal.success_predicate, "state_version": snapshot.version}));
                self.emit(ev).await;
                let _ = self.rt.db.set_run_verification(self.run_id, VerificationStatus::Verified).await;
                Ok(GoalOutcome::Verified)
            }
            Some(false) => {
                let reason = result.reason.clone().unwrap_or_else(|| "success predicate returned false".into());
                if final_phase {
                    let _ = self.rt.db.save_verification(VerificationInsert { run_id: self.run_id, step_id: step_id.as_deref(), subject_type: "goal", subject_id: self.req.goal.success_predicate.as_deref().unwrap_or("goal"), status: VerificationStatus::Failed, method: "predicate", expected: expected.as_ref(), actual: actual.as_ref(), evidence_source: Some(&snapshot.source), evidence_version: snapshot.version.as_deref(), evidence_timestamp: Some(snapshot.captured_at), duration_ms: Some(dur), reason: Some(&reason) }).await;
                    let ev = self.event(EventType::VerificationFailed).with_status(EventStatus::Failed).with_duration(dur).with_payload(json!({"subject": "goal", "phase": phase, "reason": reason, "evidence": self.capture(&result.evidence)}));
                    self.emit(ev).await;
                    let _ = self.rt.db.set_run_verification(self.run_id, VerificationStatus::Failed).await;
                }
                Ok(GoalOutcome::NotSatisfied(reason))
            }
            None => {
                let reason = result.reason.clone().unwrap_or_else(|| "predicate did not return a boolean".into());
                let _ = self.rt.db.save_verification(VerificationInsert { run_id: self.run_id, step_id: step_id.as_deref(), subject_type: "goal", subject_id: self.req.goal.success_predicate.as_deref().unwrap_or("goal"), status: VerificationStatus::Inconclusive, method: "predicate", expected: expected.as_ref(), actual: actual.as_ref(), evidence_source: Some(&snapshot.source), evidence_version: snapshot.version.as_deref(), evidence_timestamp: Some(snapshot.captured_at), duration_ms: Some(dur), reason: Some(&reason) }).await;
                let ev = self.event(EventType::VerificationInconclusive).with_status(EventStatus::Inconclusive).with_duration(dur).with_payload(json!({"subject": "goal", "phase": phase, "reason": reason}));
                self.emit(ev).await;
                let _ = self.rt.db.set_run_verification(self.run_id, VerificationStatus::Inconclusive).await;
                Ok(GoalOutcome::NotEvaluated(reason))
            }
        }
    }

    /// Ask the SDK's outcome verifier whether an action took effect. Returns a JSON note
    /// for the model; persists a verification result and events.
    async fn verify_action(&mut self, inv_id: &Uuid, spec: &ToolSpec, call: &ToolCall, output: Option<&Value>, phase: &str) -> Value {
        crash_at(CrashBarrier::DuringVerify, self.run_id);
        let Some(snapshot) = self.last_snapshot.clone().or({
            match self.take_snapshot("verify_action").await { Ok(s) => s, Err(_) => None }
        }) else {
            return json!({"status": "INCONCLUSIVE", "reason": "no state snapshot"});
        };
        // Always verify against a fresh read when the action just executed.
        let snapshot = if phase == "post_execution" {
            match self.take_snapshot("post_action_verify").await { Ok(Some(s)) => { self.last_snapshot = Some(s.clone()); s }, _ => snapshot }
        } else { snapshot };
        let t0 = Instant::now();
        let ev = self.event(EventType::VerificationStarted).with_status(EventStatus::Started).with_payload(json!({"subject": "action", "tool": spec.name, "invocation_id": inv_id, "phase": phase, "snapshot_id": snapshot.id}));
        self.emit(ev).await;
        let req = ActionVerifyRequest { invocation_id: *inv_id, tool_name: spec.name.clone(), arguments: call.arguments.clone(), output: output.cloned(), state: snapshot.state.clone(), state_version: snapshot.version.clone() };
        let timeout = self.rt.cfg.worker_request_timeout.min(self.remaining().max(Duration::from_secs(1)));
        let step_id = format!("step-{}", self.step);
        let (status, result): (VerificationStatus, ActionVerifyResult) = match self.handle.ask(WorkerRequestKind::ActionVerify(req), timeout).await {
            Ok(v) => match serde_json::from_value::<ActionVerifyResult>(v) {
                Ok(r) => (r.status, r),
                Err(e) => (VerificationStatus::Inconclusive, ActionVerifyResult { status: VerificationStatus::Inconclusive, expected: None, actual: None, reason: Some(format!("malformed verifier result: {e}")), method: None, wait: false, poll_after_ms: None, business_operation_status: None }),
            },
            Err(err) => {
                let msg = match err { AskError::Worker(w) => w.message, AskError::Timeout => "verifier timed out".into(), AskError::Malformed(m) => m, AskError::Cancelled => "cancelled".into() };
                (VerificationStatus::Inconclusive, ActionVerifyResult { status: VerificationStatus::Inconclusive, expected: None, actual: None, reason: Some(msg), method: None, wait: false, poll_after_ms: None, business_operation_status: None })
            }
        };
        let dur = t0.elapsed().as_millis() as i64;
        self.rt.metrics.verification_duration_ms_total.fetch_add(dur.max(0) as u64, std::sync::atomic::Ordering::Relaxed);
        self.rt.metrics.verification_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let method = result.method.clone().unwrap_or_else(|| "action_verifier".into());
        let _ = self.rt.db.save_verification(VerificationInsert { run_id: self.run_id, step_id: Some(&step_id), subject_type: "action", subject_id: &inv_id.to_string(), status, method: &method, expected: result.expected.as_ref(), actual: result.actual.as_ref(), evidence_source: Some(&snapshot.source), evidence_version: snapshot.version.as_deref(), evidence_timestamp: Some(snapshot.captured_at), duration_ms: Some(dur), reason: result.reason.as_deref() }).await;
        let _ = self.rt.db.set_tool_verification(*inv_id, status).await;
        let (etype, estatus) = match status {
            VerificationStatus::Verified => (EventType::VerificationPassed, EventStatus::Passed),
            VerificationStatus::Failed => (EventType::VerificationFailed, EventStatus::Failed),
            _ => (EventType::VerificationInconclusive, EventStatus::Inconclusive),
        };
        let ev = self.event(etype).with_status(estatus).with_duration(dur)
            .with_payload(json!({"subject": "action", "tool": spec.name, "invocation_id": inv_id, "phase": phase, "method": method, "expected": result.expected, "actual": result.actual, "reason": result.reason, "wait": result.wait, "business_operation_status": result.business_operation_status, "state_version": snapshot.version, "source": snapshot.source, "verifier": "sdk.action_verifier"}));
        self.emit(ev).await;
        if let Some(b) = result.business_operation_status {
            let ds = if result.wait {
                DispatchState::OutcomePending
            } else if status == VerificationStatus::Verified {
                DispatchState::OutcomeConfirmed
            } else if status == VerificationStatus::Failed {
                DispatchState::Rejected
            } else {
                DispatchState::Accepted
            };
            let _ = self.rt.db.set_dispatch_state(*inv_id, self.run_id, ds, Some("action_verify"), Some(b.as_str()), None, result.actual.as_ref()).await;
        }
        json!({"status": status, "expected": result.expected, "actual": result.actual, "reason": result.reason, "wait": result.wait, "poll_after_ms": result.poll_after_ms, "business_operation_status": result.business_operation_status})
    }

    async fn ingest_from_tool_output(&mut self, output: &Value, spec: &ToolSpec, call: &ToolCall) {
        let hints = parse_evidence_hints(output);
        let decision = parse_decision_hint(output);
        if hints.is_empty() && decision.is_none() {
            return;
        }
        let action_id = decision
            .as_ref()
            .and_then(|d| d.action.clone())
            .or_else(|| {
                if is_sensitive_write(spec.effect) {
                    Some(format!("act:{}", spec.name))
                } else {
                    None
                }
            });
        let approval_id = self.pending_approval_id.map(|id| format!("appr:{id}"));
        let predicates: Vec<String> = self.req.goal.success_predicate.iter().map(|s| format!("pred:{s}")).collect();
        let invariants: Vec<String> = self.req.goal.invariants.iter().map(|s| format!("inv:{s}")).collect();
        match self
            .rt
            .db
            .ingest_tool_evidence(
                self.project_id,
                self.run_id,
                &hints,
                decision.as_ref(),
                action_id.as_deref(),
                approval_id.as_deref(),
                &predicates,
                &invariants,
            )
            .await
        {
            Ok(rows) => {
                for row in &rows {
                    let ev = self.event(EventType::EvidenceRecorded).with_status(EventStatus::Info).with_payload(json!({
                        "evidence_id": row.id,
                        "source_system": row.source_system,
                        "resource_id": row.resource_id,
                        "resource_version": row.resource_version,
                        "content_fingerprint": row.content_fingerprint,
                        "trust": row.trust,
                        "access_scope": row.access_scope,
                        "inferred": false,
                        "tool": spec.name,
                        "tool_call_id": call.id,
                    }));
                    self.emit(ev).await;
                }
                if let Some(d) = decision {
                    let ev = self.event(EventType::DecisionCreated).with_status(EventStatus::Info).with_payload(json!({
                        "decision_id": d.id,
                        "kind": d.kind,
                        "depends_on": d.depends_on,
                        "inferred": false,
                    }));
                    self.emit(ev).await;
                }
            }
            Err(e) => tracing::warn!(error = %e, "ingest evidence"),
        }
    }

    async fn record_snapshot_resources(&mut self, snap: &StateSnapshotRow) {
        let observed = observed_resources(&snap.state);
        if observed.is_empty() {
            return;
        }
        let Ok(changes) = self.rt.db.detect_resource_changes(self.run_id, &observed, &[]).await else {
            return;
        };
        if changes.is_empty() {
            return;
        }
        for c in &changes {
            let ev = self.event(EventType::EvidenceChanged).with_status(EventStatus::Info).with_payload(json!({
                "resource_id": c.resource_id,
                "old_version": c.old_version,
                "new_version": c.new_version,
                "inaccessible": c.inaccessible,
                "inferred": false,
            }));
            self.emit(ev).await;
        }
        let Ok(graph) = self.rt.db.load_graph(self.run_id).await else {
            return;
        };
        let plan = impact_and_plan(&graph, &changes);
        let report_json = serde_json::to_value(&plan.impact).unwrap_or(Value::Null);
        let _ = self.rt.db.save_impact_report(self.run_id, &changes, &report_json).await;
        let ev = self.event(EventType::ImpactComputed).with_status(EventStatus::Info).with_payload(json!({
            "affected_decisions": plan.decisions_to_recheck,
            "pending_actions": plan.actions_to_block,
            "approvals": plan.approvals_to_invalidate,
            "predicates": plan.predicates_to_recheck,
            "classification": plan.impact.classification,
            "cyclic": plan.impact.cyclic,
            "inferred": false,
        }));
        self.emit(ev).await;
        for id in &plan.decisions_to_recheck {
            let _ = self.rt.db.set_decision_status(self.run_id, id, DecisionStatus::PendingRevalidation).await;
            let ev = self.event(EventType::DecisionRevalidationRequired).with_status(EventStatus::Pending).with_payload(json!({
                "decision_id": id,
                "reason": "authoritative resource version changed",
            }));
            self.emit(ev).await;
        }
    }

    /// Returns Some(end) when a sensitive write must not proceed.
    async fn apply_evidence_impact(&mut self, approval_id: Option<Uuid>, tool_name: &str) -> Option<RunEnd> {
        let snap = self.last_snapshot.as_ref()?;
        let observed = observed_resources(&snap.state);
        let changes = self.rt.db.detect_resource_changes(self.run_id, &observed, &[]).await.ok()?;
        if changes.is_empty() {
            return None;
        }
        let graph = self.rt.db.load_graph(self.run_id).await.ok()?;
        let plan = impact_and_plan(&graph, &changes);
        if plan.action == RevalidationAction::None {
            return None;
        }
        let mut worst = DecisionStatus::Valid;
        let decisions = self.rt.db.list_decisions(self.run_id).await.ok().unwrap_or_default();
        for dec in &decisions {
            if !plan.decisions_to_recheck.iter().any(|id| id == &dec.id) {
                continue;
            }
            let resource = dec
                .payload
                .get("resource_id")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    observed.first().map(|(id, _)| id.as_str())
                })
                .unwrap_or("");
            let current_ver = observed.iter().find(|(id, _)| id == resource).and_then(|(_, v)| v.as_deref());
            let passed = apply_rule(&dec.kind, &snap.state, resource, &dec.payload);
            let status = decision_after_rule(current_ver, passed);
            let _ = self.rt.db.set_decision_status(self.run_id, &dec.id, status).await;
            let _ = self.rt.db.save_revalidation_result(
                self.run_id,
                &dec.id,
                Some(&dec.status),
                status,
                Some("selective dependency revalidation"),
                json!({"rule": dec.kind, "passed": passed}),
                changes.first().and_then(|c| c.old_version.clone()).as_deref(),
                current_ver,
                approval_id,
            ).await;
            let ev = self.event(EventType::DecisionRevalidated).with_status(EventStatus::Info).with_payload(json!({
                "decision_id": dec.id,
                "previous": dec.status,
                "status": status.as_str(),
                "rule": dec.kind,
                "passed": passed,
            }));
            self.emit(ev).await;
            worst = match (worst, status) {
                (_, DecisionStatus::Invalid) => DecisionStatus::Invalid,
                (DecisionStatus::Invalid, _) => DecisionStatus::Invalid,
                (_, DecisionStatus::Inconclusive) => DecisionStatus::Inconclusive,
                (DecisionStatus::Inconclusive, _) => DecisionStatus::Inconclusive,
                (a, _) => a,
            };
        }
        if let Some(aid) = approval_id {
            if let Ok(row) = self.rt.db.invalidate_approval(self.project_id, aid, "material evidence prerequisites changed").await {
                let ev = self.event(EventType::ApprovalInvalidated).with_status(EventStatus::Denied).with_payload(json!({
                    "approval_id": row.id,
                    "reason": "material evidence prerequisites changed",
                    "tool": tool_name,
                }));
                self.emit(ev).await;
            }
        }
        if let Some(inv) = self.pending_invocation_id {
            let _ = self.rt.db.set_recovery_disposition(inv, self.run_id, RecoveryDisposition::RevalidationRequired, Some("evidence_changed")).await;
        }
        match worst {
            DecisionStatus::Invalid => Some(RunEnd::Failed(format!(
                "stale decision cannot authorize `{tool_name}`: authoritative evidence changed and the explicit revalidation rule failed"
            ))),
            DecisionStatus::Inconclusive => Some(RunEnd::Unverified(format!(
                "revalidation of `{tool_name}` was inconclusive (missing or inaccessible evidence)"
            ))),
            _ => {
                // Even if the rule still holds, a version change invalidates the prior approval token.
                if approval_id.is_some() {
                    Some(RunEnd::Failed(format!(
                        "approval for `{tool_name}` was invalidated because evidence versions changed; a new approval against current state is required"
                    )))
                } else {
                    Some(RunEnd::Failed(format!(
                        "sensitive write `{tool_name}` blocked until dependents are revalidated against current evidence"
                    )))
                }
            }
        }
    }

    async fn revalidate_before_write(&mut self, spec: &ToolSpec, call: &ToolCall) -> Option<ToolOutcome> {
        if self.req.has_state_provider {
            if let Ok(Some(snap)) = self.take_snapshot("pre_write").await {
                self.last_snapshot = Some(snap);
            }
        }

        let coverage_block = self.apply_coverage_and_policy_gate(spec, call).await;
        if coverage_block.is_some() {
            return coverage_block;
        }

        // Default v0.3 selective graph path. Still runs after coverage so a complete
        // contract cannot skip a stale graph-dependent write.
        let end = self.apply_evidence_impact(self.pending_approval_id, &spec.name).await?;
        let err = json!({"code": "revalidation_blocked", "message": format!("{end:?}")});
        Some(ToolOutcome {
            message: ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"error": err}).to_string()),
            end: Some(end),
            stop_step: true,
            wait: None,
        })
    }

    /// Coverage + policy gates. Returns Some when the write must not proceed.
    /// Completeness of a declared contract is never treated as goal VERIFIED.
    async fn apply_coverage_and_policy_gate(&mut self, spec: &ToolSpec, call: &ToolCall) -> Option<ToolOutcome> {
        let policy = spec.revalidation_policy;
        if policy == RevalidationPolicy::Selective && spec.evidence_coverage.is_none() {
            return None;
        }

        let observed = self
            .last_snapshot
            .as_ref()
            .map(|s| observed_resources(&s.state))
            .unwrap_or_default();
        let changes = self.rt.db.detect_resource_changes(self.run_id, &observed, &[]).await.ok().unwrap_or_default();
        let records = self.rt.db.list_evidence_records(self.run_id).await.ok().unwrap_or_default();
        let decisions = self.rt.db.list_decisions(self.run_id).await.ok().unwrap_or_default();

        let facts: Vec<EvidenceFact> = records
            .iter()
            .map(|r| {
                let keys = r
                    .metadata
                    .get("keys")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                let current = observed.iter().find(|(id, _)| id == &r.resource_id).and_then(|(_, v)| v.clone());
                EvidenceFact {
                    id: r.id.to_string(),
                    resource_id: r.resource_id.clone(),
                    keys,
                    version: r.resource_version.clone(),
                    trust: TrustClassification::parse(&r.trust).unwrap_or(TrustClassification::Untrusted),
                    retrieved_at: r.retrieved_at,
                    current_version: current,
                    inaccessible: r.trust == "inaccessible",
                    fingerprint: Some(r.content_fingerprint.clone()),
                    source_system: Some(r.source_system.clone()),
                }
            })
            .collect();
        let decision_facts: Vec<DecisionFact> = decisions
            .iter()
            .map(|d| DecisionFact { id: d.id.clone(), kind: d.kind.clone(), status: d.status.clone() })
            .collect();

        let mut coverage: Option<CoverageReport> = None;
        if let Some(contract) = &spec.evidence_coverage {
            let bound = resolve_contract_templates(contract, &call.arguments);
            let report = evaluate_coverage(&bound, &facts, &decision_facts, &changes, Utc::now());
            let ev = self.event(EventType::EvidenceCoverageEvaluated).with_status(EventStatus::Info).with_payload(json!({
                "tool": spec.name,
                "status": report.status.as_str(),
                "missing": report.missing,
                "stale": report.stale,
                "conflicted": report.conflicted,
                "inconclusive": report.inconclusive,
                "policy": policy.as_str(),
                "note": "COMPLETE_FOR_DECLARED_CONTRACT is not goal VERIFIED",
            }));
            self.emit(ev).await;
            coverage = Some(report);
        }

        let graph = self.rt.db.load_graph(self.run_id).await.ok();
        let mut predicted = StrategyOutput::default();
        if let Some(graph) = graph.as_ref() {
            let strat = match policy {
                RevalidationPolicy::Selective => RevalidationStrategy::Selective,
                RevalidationPolicy::FullDeclared => RevalidationStrategy::FullDeclared,
                RevalidationPolicy::SelectivePlusPrewrite => RevalidationStrategy::SelectivePlusPrewrite,
                RevalidationPolicy::StrictContract => RevalidationStrategy::StrictContract,
            };
            let mut ctx = StrategyContext::default();
            if let Some(c) = &spec.evidence_coverage {
                ctx.contracts.insert(spec.name.clone(), c.clone());
                ctx.contracts.insert(format!("act:{}", spec.name), resolve_contract_templates(c, &call.arguments));
            }
            ctx.facts = facts;
            ctx.decisions = decision_facts.clone();
            ctx.now = Some(Utc::now());
            predicted = run_strategy_with_context(strat, graph, &changes, &ctx);
        }

        let act_id = format!("act:{}", spec.name);
        let mut predicted_this = predicted.clone();
        let blocks_this = prediction_blocks_action(&predicted, &[spec.name.as_str(), act_id.as_str()]);
        let full_blocks_this = policy == RevalidationPolicy::FullDeclared && !predicted.actions_to_block.is_empty();
        if blocks_this || full_blocks_this {
            if !predicted_this.actions_to_block.iter().any(|id| id == &spec.name || id == &act_id) {
                predicted_this.actions_to_block.push(act_id.clone());
            }
        } else {
            predicted_this.actions_to_block.retain(|id| id == &spec.name || id == &format!("act:{}", spec.name));
        }
        // A complete live coverage report plus no detected resource change is not a write block.
        // Strategy F may still extra-block when its contract context is incomplete; that must not
        // override the same-run coverage evaluation.
        if changes.is_empty() && coverage.as_ref().is_some_and(|c| c.is_complete()) {
            predicted_this.actions_to_block.retain(|id| {
                id != &spec.name && id != &act_id && !id.ends_with(&format!(":{}", spec.name))
            });
        }

        let approval = if let Some(aid) = self.pending_approval_id {
            self.rt
                .db
                .get_approval(self.project_id, aid)
                .await
                .ok()
                .and_then(|row| ApprovalStatus::parse(&row.status))
        } else {
            None
        };

        let mut spec_for_gate = spec.clone();
        if let Some(c) = spec_for_gate.evidence_coverage.as_mut() {
            *c = resolve_contract_templates(c, &call.arguments);
        }
        let gate = evaluate_action_gate(&spec_for_gate, coverage.as_ref(), &predicted_this, approval);

        if let Some(rep) = &coverage {
            let _ = self
                .rt
                .db
                .save_coverage_report(
                    self.run_id,
                    &spec.name,
                    policy.as_str(),
                    rep.status.as_str(),
                    gate.kind.as_str(),
                    &serde_json::to_value(rep).unwrap_or(json!({})),
                )
                .await;
        } else if gate.is_block() {
            let _ = self
                .rt
                .db
                .save_coverage_report(
                    self.run_id,
                    &spec.name,
                    policy.as_str(),
                    "NONE",
                    gate.kind.as_str(),
                    &json!({"reasons": gate.reasons}),
                )
                .await;
        }

        let ev = self.event(EventType::ActionGated).with_status(if gate.is_block() { EventStatus::Denied } else { EventStatus::Info }).with_payload(json!({
            "tool": spec.name,
            "gate": gate.kind.as_str(),
            "policy": policy.as_str(),
            "reasons": gate.reasons,
            "coverage_status": coverage.as_ref().map(|c| c.status.as_str()),
            "requires_human_review": gate.requires_human_review,
            "predicted_actions_to_block": predicted_this.actions_to_block,
            "resource_changes": changes.iter().map(|c| json!({"resource_id": c.resource_id, "old": c.old_version, "new": c.new_version})).collect::<Vec<_>>(),
        }));
        self.emit(ev).await;

        if !gate.is_block() {
            return None;
        }
        let reason = gate.reasons.join("; ");
        let end = RunEnd::Failed(format!(
            "action `{}` blocked by {} (coverage={}): {reason}",
            spec.name,
            policy.as_str(),
            coverage.as_ref().map(|c| c.status.as_str()).unwrap_or("n/a")
        ));
        let err = json!({"code": "coverage_blocked", "message": reason, "gate": gate.kind.as_str(), "policy": policy.as_str()});
        Some(ToolOutcome {
            message: ModelMessage::tool(call.id.clone(), call.name.clone(), json!({"error": err}).to_string()),
            end: Some(end),
            stop_step: true,
            wait: None,
        })
    }
}

fn is_sensitive_write(effect: EffectCategory) -> bool {
    matches!(
        effect,
        EffectCategory::NonIdempotentWrite | EffectCategory::Irreversible | EffectCategory::IdempotentWrite
    )
}

/// Enforce the tool-result size limit without silently dropping data: oversized results
/// are truncated to a prefix and flagged.
fn bound_result(v: &Value, max_bytes: usize) -> (Value, bool) {
    let s = v.to_string();
    if s.len() <= max_bytes {
        return (v.clone(), false);
    }
    let mut cut = max_bytes.min(s.len());
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    (json!({"truncated": true, "original_bytes": s.len(), "prefix": &s[..cut]}), true)
}
