use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axiom_core::pricing::PricingTable;
use axiom_core::validate_transition;
use axiom_protocol::{
    CreateRunRequest, EventStatus, EventType, ExecutionEvent, RunStatus, WorkerPollResponse, WorkerResponse,
};
use axiom_storage::{ApprovalRow, Db, RunRow, StorageError};
use dashmap::DashMap;
use serde_json::json;
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::bridge::RunHandle;
use crate::executor::RunExecutor;
use crate::publisher::Publisher;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Full,
    Redacted,
    None,
}

impl CaptureMode {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "full" => CaptureMode::Full,
            "none" => CaptureMode::None,
            _ => CaptureMode::Redacted,
        }
    }
}

#[derive(Clone)]
pub struct RuntimeConfig {
    /// Deadline for non-tool worker requests (model calls, snapshots, predicates).
    pub worker_request_timeout: Duration,
    /// How long an approval stays actionable.
    pub approval_ttl: Duration,
    pub capture_model_io: CaptureMode,
    pub pricing: Arc<PricingTable>,
    /// Outbox idle poll interval.
    pub publish_interval: Duration,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        RuntimeConfig {
            worker_request_timeout: Duration::from_secs(120),
            approval_ttl: Duration::from_secs(3600),
            capture_model_io: CaptureMode::Redacted,
            pricing: Arc::new(PricingTable::empty("unconfigured")),
            publish_interval: Duration::from_millis(250),
        }
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("run {0} is not active on this runtime")]
    NotActive(Uuid),
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("conflict: {0}")]
    Conflict(String),
}

#[derive(Default)]
pub struct RuntimeMetrics {
    pub failed_actions: AtomicU64,
    pub workflow_nodes_completed: AtomicU64,
    pub workflow_nodes_failed: AtomicU64,
    pub verification_duration_ms_total: AtomicU64,
    pub verification_count: AtomicU64,
    pub shutdown_started: AtomicBool,
}

impl RuntimeMetrics {
    pub fn snapshot(&self) -> serde_json::Value {
        json!({
            "failed_actions": self.failed_actions.load(Ordering::Relaxed),
            "workflow_nodes_completed": self.workflow_nodes_completed.load(Ordering::Relaxed),
            "workflow_nodes_failed": self.workflow_nodes_failed.load(Ordering::Relaxed),
            "verification_duration_ms_total": self.verification_duration_ms_total.load(Ordering::Relaxed),
            "verification_count": self.verification_count.load(Ordering::Relaxed),
            "shutdown_started": self.shutdown_started.load(Ordering::Relaxed),
        })
    }
}

pub struct Runtime {
    pub db: Db,
    pub cfg: RuntimeConfig,
    pub instance_id: String,
    pub metrics: RuntimeMetrics,
    runs: DashMap<Uuid, Arc<RunHandle>>,
    publisher: Arc<Publisher>,
    shutdown: CancellationToken,
}

impl Runtime {
    pub fn new(db: Db, cfg: RuntimeConfig) -> Arc<Self> {
        let publisher = Arc::new(Publisher::new(4096));
        let shutdown = CancellationToken::new();
        let rt = Arc::new(Runtime {
            db: db.clone(),
            cfg: cfg.clone(),
            instance_id: crate::supervisor::runtime_instance_id(),
            metrics: RuntimeMetrics::default(),
            runs: DashMap::new(),
            publisher: publisher.clone(),
            shutdown: shutdown.clone(),
        });
        tokio::spawn(publisher.run(db, cfg.publish_interval, shutdown));
        rt
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ExecutionEvent> {
        self.publisher.subscribe()
    }

    pub fn wake_publisher(&self) {
        self.publisher.wake.notify_one();
    }

    pub fn active_run_count(&self) -> usize {
        self.runs.len()
    }

    pub fn handle(&self, run_id: Uuid) -> Option<Arc<RunHandle>> {
        self.runs.get(&run_id).map(|h| h.clone())
    }

    /// Validate, persist, and start a run. Returns immediately; the executor waits for
    /// a worker to poll. The boolean is true when an idempotency key matched an existing run.
    pub async fn create_run(self: &Arc<Self>, project_id: Uuid, req: CreateRunRequest) -> Result<(RunRow, bool), RuntimeError> {
        validate_request(&req)?;
        let trace_id = req.trace_id.clone().filter(|s| !s.is_empty()).unwrap_or_else(axiom_protocol::new_trace_id);
        let created = self.db.create_run(project_id, &req, &trace_id).await?;
        if created.reused {
            let status = RunStatus::parse(&created.run.status).unwrap_or(RunStatus::Failed);
            if status.is_durable_wait() && self.handle(created.run.id).is_none() && !req.defer_start {
                self.spawn_executor(project_id, created.run.clone(), req);
            }
            return Ok((created.run, true));
        }
        self.wake_publisher();
        if !req.defer_start {
            self.spawn_executor(project_id, created.run.clone(), req);
        }
        Ok((created.run, false))
    }

    /// Spawn the executor for a deferred (coordinator or approval-held) run.
    pub async fn start_run(self: &Arc<Self>, project_id: Uuid, run_id: Uuid) -> Result<RunRow, RuntimeError> {
        let run = self.db.get_run(project_id, run_id).await?;
        if self.handle(run_id).is_some() {
            return Ok(run);
        }
        let status = RunStatus::parse(&run.status).unwrap_or(RunStatus::Failed);
        if status.is_terminal() {
            return Err(RuntimeError::Conflict(format!("run is already {status}")));
        }
        let mut req: CreateRunRequest = serde_json::from_value(run.definition.clone())
            .map_err(|e| RuntimeError::Invalid(format!("stored run definition is not a CreateRunRequest: {e}")))?;
        req.defer_start = false;
        self.spawn_executor(project_id, run.clone(), req);
        Ok(run)
    }

    pub(crate) fn spawn_executor(self: &Arc<Self>, project_id: Uuid, run: RunRow, req: CreateRunRequest) {
        let handle = RunHandle::new(run.id);
        let status = RunStatus::parse(&run.status).unwrap_or(RunStatus::Created);
        let handle_for_status = handle.clone();
        self.runs.insert(run.id, handle.clone());
        let executor = RunExecutor::new(self.clone(), handle, project_id, run, req);
        let runs = self.clone();
        tokio::spawn(async move {
            handle_for_status.set_status(status).await;
            let run_id = executor.run_id;
            executor.run().await;
            // Keep the handle briefly so a final poll observes the terminal status.
            tokio::time::sleep(Duration::from_secs(5)).await;
            runs.runs.remove(&run_id);
        });
    }

    pub async fn poll(&self, project_id: Uuid, run_id: Uuid, wait: Duration) -> Result<WorkerPollResponse, RuntimeError> {
        let _ = self.db.get_run(project_id, run_id).await?;
        match self.handle(run_id) {
            Some(h) => {
                let request = h.poll(wait.min(Duration::from_secs(30))).await;
                let status = h.status().await;
                Ok(WorkerPollResponse { request, run_status: status, terminal: h.detached().await })
            }
            None => {
                let run = self.db.get_run(project_id, run_id).await?;
                let status = RunStatus::parse(&run.status).unwrap_or(RunStatus::Failed);
                Ok(WorkerPollResponse { request: None, run_status: status, terminal: status.is_terminal() })
            }
        }
    }

    pub async fn respond(&self, run_id: Uuid, resp: WorkerResponse) -> Result<(), RuntimeError> {
        let h = self.handle(run_id).ok_or(RuntimeError::NotActive(run_id))?;
        if h.respond(resp).await {
            Ok(())
        } else {
            Err(RuntimeError::Invalid("unknown or already answered request_id".into()))
        }
    }

    pub async fn cancel(&self, project_id: Uuid, run_id: Uuid, actor: &str) -> Result<RunRow, RuntimeError> {
        let run = self.db.get_run(project_id, run_id).await?;
        let status = RunStatus::parse(&run.status).unwrap_or(RunStatus::Failed);
        if status.is_terminal() {
            return Err(RuntimeError::Conflict(format!("run is already {status}")));
        }
        if let Some(h) = self.handle(run_id) {
            h.cancel.cancel();
            // The executor records the cancellation itself.
            self.db.audit(project_id, actor, "run.cancel_requested", "run", &run_id.to_string(), json!({})).await?;
            let _ = self.cancel_descendants(project_id, run_id, actor).await;
            return Ok(run);
        }
        // Not active here (e.g. RECOVERY_REQUIRED after a restart): cancel directly.
        validate_transition(status, RunStatus::Cancelled).map_err(|e| RuntimeError::Conflict(e.to_string()))?;
        let ev = ExecutionEvent::native(run_id, run.trace_id.clone(), EventType::RunCancelled)
            .with_status(EventStatus::Cancelled)
            .with_payload(json!({"actor": actor, "previous_status": status}));
        self.db.transition_run(project_id, run_id, RunStatus::Cancelled, Some("cancelled_by_operator"), vec![ev]).await?;
        self.db.audit(project_id, actor, "run.cancelled", "run", &run_id.to_string(), json!({})).await?;
        self.wake_publisher();
        let _ = self.cancel_descendants(project_id, run_id, actor).await;
        self.db.get_run(project_id, run_id).await.map_err(Into::into)
    }

    async fn cancel_descendants(&self, project_id: Uuid, parent_run_id: Uuid, actor: &str) -> Result<(), RuntimeError> {
        let children = self.db.list_descendant_runs(project_id, parent_run_id).await?;
        for child in children {
            let st = RunStatus::parse(&child.status).unwrap_or(RunStatus::Failed);
            if st.is_terminal() {
                continue;
            }
            if let Some(h) = self.handle(child.id) {
                h.cancel.cancel();
                let _ = self.db.audit(project_id, actor, "run.cancel_requested", "run", &child.id.to_string(), json!({"parent": parent_run_id})).await;
                continue;
            }
            if validate_transition(st, RunStatus::Cancelled).is_err() {
                continue;
            }
            let ev = ExecutionEvent::native(child.id, child.trace_id.clone(), EventType::RunCancelled)
                .with_status(EventStatus::Cancelled)
                .with_payload(json!({"actor": actor, "propagated_from": parent_run_id}));
            if let Err(e) = self.db.transition_run(project_id, child.id, RunStatus::Cancelled, Some("cancelled_with_parent"), vec![ev]).await {
                tracing::warn!(error = %e, child = %child.id, "failed to cancel descendant");
            }
        }
        self.wake_publisher();
        Ok(())
    }

    /// Record an approval decision (idempotent) and wake the waiting executor.
    pub async fn decide_approval(self: &Arc<Self>, project_id: Uuid, approval_id: Uuid, approve: bool, decided_by: &str, reason: Option<&str>) -> Result<(ApprovalRow, bool), RuntimeError> {
        let outcome = self.db.decide_approval(project_id, approval_id, approve, decided_by, reason).await?;
        if outcome.applied {
            if let Some(h) = self.handle(outcome.approval.run_id) {
                h.notify_approval(outcome.approval.clone()).await;
            } else {
                // Runtime restarted while the approval was pending: attach an executor now.
                let run = self.db.get_run(project_id, outcome.approval.run_id).await?;
                let _ = self.resume_run(run).await;
                if let Some(h) = self.handle(outcome.approval.run_id) {
                    h.notify_approval(outcome.approval.clone()).await;
                }
            }
            if approve {
                if let Some(child_id) = outcome
                    .approval
                    .arguments
                    .get("child_run_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                {
                    if let Err(e) = self.start_run(project_id, child_id).await {
                        tracing::warn!(error = %e, child = %child_id, "could not start approved child task");
                    }
                }
            }
        }
        Ok((outcome.approval, outcome.applied))
    }

    pub fn shutdown(&self) {
        self.metrics.shutdown_started.store(true, std::sync::atomic::Ordering::Relaxed);
        self.shutdown.cancel();
        for h in self.runs.iter() {
            h.cancel.cancel();
        }
    }
}

fn validate_request(req: &CreateRunRequest) -> Result<(), RuntimeError> {
    if req.agent.name.trim().is_empty() {
        return Err(RuntimeError::Invalid("agent.name is required".into()));
    }
    if req.agent.model.trim().is_empty() || !req.agent.model.contains('/') {
        return Err(RuntimeError::Invalid("agent.model must be provider-qualified, e.g. `mock/deterministic`".into()));
    }
    if req.goal.description.trim().is_empty() {
        return Err(RuntimeError::Invalid("goal.description is required".into()));
    }
    let mut names = std::collections::HashSet::new();
    for t in &req.agent.tools {
        if !names.insert(&t.name) {
            return Err(RuntimeError::Invalid(format!("duplicate tool name `{}`", t.name)));
        }
        if !t.input_schema.is_object() {
            return Err(RuntimeError::Invalid(format!("tool `{}` input_schema must be a JSON Schema object", t.name)));
        }
    }
    if req.goal.success_predicate.is_some() && !req.has_state_provider {
        return Err(RuntimeError::Invalid("a success predicate requires has_state_provider=true (predicates are evaluated against authoritative state)".into()));
    }
    Ok(())
}
