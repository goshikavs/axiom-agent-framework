//! Outbox publisher: the only path from persisted events to live subscribers (SSE).
//! At-least-once; subscribers deduplicate by `event_id` (ADR-002).

use std::sync::Arc;
use std::time::Duration;

use axiom_protocol::ExecutionEvent;
use axiom_storage::Db;
use tokio::sync::{broadcast, Notify};
use tokio_util::sync::CancellationToken;

pub struct Publisher {
    pub tx: broadcast::Sender<ExecutionEvent>,
    pub wake: Arc<Notify>,
}

impl Publisher {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Publisher { tx, wake: Arc::new(Notify::new()) }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ExecutionEvent> {
        self.tx.subscribe()
    }

    /// Drain loop. Wakes on `wake` or every `idle_poll` to pick up events written by
    /// other processes (e.g. batch ingestion from another replica).
    pub async fn run(self: Arc<Self>, db: Db, idle_poll: Duration, shutdown: CancellationToken) {
        loop {
            match db.drain_outbox(500).await {
                Ok(rows) => {
                    let n = rows.len();
                    for row in rows {
                        let ev: ExecutionEvent = row.into();
                        // Errors only mean there are no subscribers right now.
                        let _ = self.tx.send(ev);
                    }
                    if n == 500 {
                        continue; // more pending
                    }
                }
                Err(e) => tracing::warn!(error = %e, "outbox drain failed"),
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = self.wake.notified() => {}
                _ = tokio::time::sleep(idle_poll) => {}
            }
        }
    }
}
