//! Per-run bridge between the executor task and the SDK worker (long-poll mailbox),
//! plus approval wake-ups and cancellation.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use goshikq_protocol::{RunStatus, WorkerError, WorkerRequest, WorkerRequestKind, WorkerResponse};
use goshikq_storage::ApprovalRow;
use tokio::sync::{oneshot, Mutex, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug)]
pub enum AskError {
    /// The worker returned a structured error.
    Worker(WorkerError),
    /// No response within the request deadline; the side effect may be ambiguous.
    Timeout,
    /// Run was cancelled while waiting.
    Cancelled,
    /// The result payload did not match the expected schema.
    Malformed(String),
}

pub struct RunHandle {
    pub run_id: Uuid,
    pub cancel: CancellationToken,
    status: RwLock<RunStatus>,
    mailbox: Mutex<VecDeque<WorkerRequest>>,
    mailbox_notify: Notify,
    pending: Mutex<HashMap<Uuid, oneshot::Sender<WorkerResponse>>>,
    approval_waiters: Mutex<HashMap<Uuid, oneshot::Sender<ApprovalRow>>>,
    /// Last time a worker polled this run.
    pub last_poll: RwLock<Option<chrono::DateTime<Utc>>>,
}

impl RunHandle {
    pub fn new(run_id: Uuid) -> Arc<Self> {
        Arc::new(RunHandle {
            run_id,
            cancel: CancellationToken::new(),
            status: RwLock::new(RunStatus::Created),
            mailbox: Mutex::new(VecDeque::new()),
            mailbox_notify: Notify::new(),
            pending: Mutex::new(HashMap::new()),
            approval_waiters: Mutex::new(HashMap::new()),
            last_poll: RwLock::new(None),
        })
    }

    pub async fn status(&self) -> RunStatus {
        *self.status.read().await
    }

    pub async fn set_status(&self, s: RunStatus) {
        *self.status.write().await = s;
        // Wake pollers so they observe terminal states promptly.
        self.mailbox_notify.notify_waiters();
    }

    /// Whether this worker should keep polling. Durable waits and RECOVERY_REQUIRED stay attached.
    pub async fn detached(&self) -> bool {
        self.status().await.is_terminal()
    }

    /// Queue a request and wait for the worker's answer.
    pub async fn ask(
        &self,
        kind: WorkerRequestKind,
        timeout: Duration,
    ) -> Result<serde_json::Value, AskError> {
        let request_id = Uuid::now_v7();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(request_id, tx);
        let req = WorkerRequest {
            request_id,
            run_id: self.run_id,
            kind,
            issued_at: Utc::now(),
            timeout_ms: timeout.as_millis() as u64,
        };
        self.mailbox.lock().await.push_back(req);
        self.mailbox_notify.notify_one();

        let outcome = tokio::select! {
            _ = self.cancel.cancelled() => Err(AskError::Cancelled),
            res = tokio::time::timeout(timeout, rx) => match res {
                Ok(Ok(resp)) => {
                    if resp.ok {
                        Ok(resp.result.unwrap_or(serde_json::Value::Null))
                    } else {
                        Err(AskError::Worker(resp.error.unwrap_or(WorkerError {
                            code: "unknown".into(),
                            message: "worker reported failure without details".into(),
                            retryable: false,
                            details: None,
                        })))
                    }
                }
                Ok(Err(_)) => Err(AskError::Cancelled),
                Err(_) => Err(AskError::Timeout),
            },
        };
        if outcome.is_err() {
            self.pending.lock().await.remove(&request_id);
            // Drop the request from the mailbox if the worker never picked it up.
            self.mailbox
                .lock()
                .await
                .retain(|r| r.request_id != request_id);
        }
        outcome
    }

    /// Long-poll: return the next request or `None` after `wait`.
    pub async fn poll(&self, wait: Duration) -> Option<WorkerRequest> {
        *self.last_poll.write().await = Some(Utc::now());
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            if let Some(req) = self.mailbox.lock().await.pop_front() {
                return Some(req);
            }
            if self.detached().await {
                return None;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let _ = tokio::time::timeout(remaining, self.mailbox_notify.notified()).await;
        }
    }

    /// Deliver a worker response. Returns false if the request is unknown or already answered.
    pub async fn respond(&self, resp: WorkerResponse) -> bool {
        match self.pending.lock().await.remove(&resp.request_id) {
            Some(tx) => tx.send(resp).is_ok(),
            None => false,
        }
    }

    pub async fn register_approval_waiter(
        &self,
        approval_id: Uuid,
    ) -> oneshot::Receiver<ApprovalRow> {
        let (tx, rx) = oneshot::channel();
        self.approval_waiters.lock().await.insert(approval_id, tx);
        rx
    }

    pub async fn notify_approval(&self, approval: ApprovalRow) -> bool {
        match self.approval_waiters.lock().await.remove(&approval.id) {
            Some(tx) => tx.send(approval).is_ok(),
            None => false,
        }
    }

    pub async fn pending_count(&self) -> usize {
        self.pending.lock().await.len()
    }
}
