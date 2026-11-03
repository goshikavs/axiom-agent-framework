//! Execution event persistence with per-run monotonic sequence allocation and
//! idempotent batch ingestion (ADR-002).

use chrono::Utc;
use goshikq_protocol::{EventRejection, EventType, ExecutionEvent, RunStatus};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{Db, DbResult, EventRow, Page, StorageError, DEFAULT_PROJECT_ID};

/// First-party runtime events. Pre-rebrand rows used `axiom`.
fn is_native_framework(name: &str) -> bool {
    name == "goshikq" || name == "axiom"
}

impl From<EventRow> for ExecutionEvent {
    fn from(r: EventRow) -> Self {
        ExecutionEvent {
            schema_version: r.schema_version,
            event_id: r.event_id,
            run_id: r.run_id,
            trace_id: r.trace_id,
            span_id: r.span_id,
            parent_span_id: r.parent_span_id,
            sequence: r.sequence,
            event_type: EventType::parse(&r.event_type).unwrap_or(EventType::RunStatusChanged),
            timestamp: r.source_timestamp,
            agent_id: r.agent_id,
            step_id: r.step_id,
            source_framework: r.source_framework,
            status: r
                .status
                .and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok()),
            duration_ms: r.duration_ms,
            payload: r.payload,
            metadata: r.metadata,
        }
    }
}

/// Allocate `count` sequence numbers for a run (row-locked, gap-free).
async fn allocate_sequences(
    tx: &mut Transaction<'_, Postgres>,
    run_id: Uuid,
    count: i64,
) -> DbResult<i64> {
    let (next,): (i64,) = sqlx::query_as(
        "UPDATE runs SET next_sequence = next_sequence + $2, updated_at = now() WHERE id = $1 RETURNING next_sequence - $2",
    )
    .bind(run_id)
    .bind(count)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| StorageError::NotFound(format!("run {run_id}")))?;
    Ok(next)
}

/// Insert one event inside an open transaction. Assigns the sequence and writes the
/// outbox row. Returns the stored event (with sequence).
pub async fn append_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    project_id: Uuid,
    mut event: ExecutionEvent,
) -> DbResult<ExecutionEvent> {
    event.sequence = allocate_sequences(tx, event.run_id, 1).await?;
    insert_row(tx, project_id, &event).await?;
    Ok(event)
}

async fn insert_row(
    tx: &mut Transaction<'_, Postgres>,
    project_id: Uuid,
    e: &ExecutionEvent,
) -> DbResult<bool> {
    let status = e
        .status
        .map(|s| serde_json::to_value(s).ok())
        .flatten()
        .and_then(|v| v.as_str().map(String::from));
    let inserted = sqlx::query(
        r#"INSERT INTO execution_events
            (event_id, run_id, project_id, trace_id, span_id, parent_span_id, sequence, event_type,
             schema_version, source_timestamp, agent_id, step_id, source_framework, status, duration_ms, payload, metadata)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
           ON CONFLICT (event_id) DO NOTHING"#,
    )
    .bind(e.event_id)
    .bind(e.run_id)
    .bind(project_id)
    .bind(&e.trace_id)
    .bind(&e.span_id)
    .bind(&e.parent_span_id)
    .bind(e.sequence)
    .bind(e.event_type.as_str())
    .bind(&e.schema_version)
    .bind(e.timestamp)
    .bind(&e.agent_id)
    .bind(&e.step_id)
    .bind(&e.source_framework)
    .bind(status)
    .bind(e.duration_ms)
    .bind(&e.payload)
    .bind(&e.metadata)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1;
    if inserted {
        sqlx::query(
            "INSERT INTO event_outbox (event_id, run_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(e.event_id)
        .bind(e.run_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(inserted)
}

pub struct IngestOutcome {
    pub accepted: Vec<ExecutionEvent>,
    pub duplicates: usize,
    pub rejected: Vec<EventRejection>,
}

impl Db {
    /// Append events for a native run (one transaction).
    pub async fn append_events(
        &self,
        project_id: Uuid,
        events: Vec<ExecutionEvent>,
    ) -> DbResult<Vec<ExecutionEvent>> {
        let mut tx = self.pool.begin().await?;
        let mut out = Vec::with_capacity(events.len());
        for e in events {
            out.push(append_in_tx(&mut tx, project_id, e).await?);
        }
        tx.commit().await?;
        Ok(out)
    }

    /// Idempotent ingestion of externally produced events. Creates a run shell for
    /// unknown run ids so external frameworks need no separate registration call.
    /// Sequences are collector-assigned in arrival order; the source sequence (if any)
    /// is preserved under `metadata.source_sequence`.
    pub async fn ingest_batch(
        &self,
        project_id: Uuid,
        events: Vec<ExecutionEvent>,
    ) -> DbResult<IngestOutcome> {
        let mut tx = self.pool.begin().await?;
        let mut outcome = IngestOutcome {
            accepted: vec![],
            duplicates: 0,
            rejected: vec![],
        };
        for mut e in events {
            if e.schema_version.split('.').next() != Some("1") {
                outcome.rejected.push(EventRejection {
                    event_id: e.event_id,
                    reason: format!("unsupported schema_version {}", e.schema_version),
                });
                continue;
            }
            let (exists,): (bool,) =
                sqlx::query_as("SELECT EXISTS(SELECT 1 FROM execution_events WHERE event_id = $1)")
                    .bind(e.event_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if exists {
                outcome.duplicates += 1;
                continue;
            }
            ensure_external_run(&mut tx, project_id, &e).await?;
            if let serde_json::Value::Object(m) = &mut e.metadata {
                m.insert("source_sequence".into(), serde_json::json!(e.sequence));
            }
            e.sequence = allocate_sequences(&mut tx, e.run_id, 1).await?;
            if insert_row(&mut tx, project_id, &e).await? {
                apply_external_status(&mut tx, &e).await?;
                outcome.accepted.push(e);
            } else {
                outcome.duplicates += 1;
            }
        }
        tx.commit().await?;
        Ok(outcome)
    }

    pub async fn list_events(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        after_sequence: Option<i64>,
        page: Page,
    ) -> DbResult<Vec<EventRow>> {
        let rows = sqlx::query_as::<_, EventRow>(
            r#"SELECT * FROM execution_events
               WHERE project_id = $1 AND run_id = $2 AND sequence > $3
               ORDER BY sequence ASC LIMIT $4 OFFSET $5"#,
        )
        .bind(project_id)
        .bind(run_id)
        .bind(after_sequence.unwrap_or(0))
        .bind(page.limit)
        .bind(page.offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn count_events(&self, project_id: Uuid, run_id: Uuid) -> DbResult<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM execution_events WHERE project_id = $1 AND run_id = $2",
        )
        .bind(project_id)
        .bind(run_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }

    pub async fn list_recent_events(
        &self,
        project_id: Uuid,
        event_type: Option<&str>,
        page: Page,
    ) -> DbResult<Vec<EventRow>> {
        let rows = sqlx::query_as::<_, EventRow>(
            r#"SELECT * FROM execution_events
               WHERE project_id = $1 AND ($2::text IS NULL OR event_type = $2)
               ORDER BY source_timestamp DESC, sequence DESC LIMIT $3 OFFSET $4"#,
        )
        .bind(project_id)
        .bind(event_type)
        .bind(page.limit)
        .bind(page.offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Fetch unpublished outbox events (oldest first) and mark them published.
    pub async fn drain_outbox(&self, limit: i64) -> DbResult<Vec<EventRow>> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, EventRow>(
            r#"SELECT e.* FROM event_outbox o JOIN execution_events e ON e.event_id = o.event_id
               WHERE o.published_at IS NULL ORDER BY o.created_at ASC LIMIT $1 FOR UPDATE OF o SKIP LOCKED"#,
        )
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        if !rows.is_empty() {
            let ids: Vec<Uuid> = rows.iter().map(|r| r.event_id).collect();
            sqlx::query("UPDATE event_outbox SET published_at = now() WHERE event_id = ANY($1)")
                .bind(&ids)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(rows)
    }
}

async fn ensure_external_run(
    tx: &mut Transaction<'_, Postgres>,
    project_id: Uuid,
    e: &ExecutionEvent,
) -> DbResult<()> {
    let (exists,): (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM runs WHERE id = $1 AND project_id = $2)")
            .bind(e.run_id)
            .bind(project_id)
            .fetch_one(&mut **tx)
            .await?;
    if exists {
        return Ok(());
    }
    let agent_name = e
        .agent_id
        .clone()
        .unwrap_or_else(|| format!("{}-agent", e.source_framework));
    let model = e
        .payload
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let goal = e
        .payload
        .get("goal")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    sqlx::query(
        r#"INSERT INTO runs (id, project_id, agent_name, goal_description, trace_id, status, source_framework, model, definition, started_at, metadata)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'{}'::jsonb,$9,$10)"#,
    )
    .bind(e.run_id)
    .bind(project_id)
    .bind(agent_name)
    .bind(goal)
    .bind(&e.trace_id)
    .bind(RunStatus::Planning.as_str())
    .bind(&e.source_framework)
    .bind(model)
    .bind(e.timestamp)
    .bind(serde_json::json!({"external": true, "ingested_at": Utc::now()}))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Reflect lifecycle events from external sources onto the run row so Studio lists them
/// with a status. Only lifecycle events change status; nothing is inferred otherwise.
async fn apply_external_status(
    tx: &mut Transaction<'_, Postgres>,
    e: &ExecutionEvent,
) -> DbResult<()> {
    if is_native_framework(&e.source_framework) {
        return Ok(());
    }
    let status = match e.event_type {
        EventType::RunCompleted => Some(RunStatus::Unverified),
        EventType::RunFailed => Some(RunStatus::Failed),
        EventType::RunCancelled => Some(RunStatus::Cancelled),
        EventType::RunPaused => Some(RunStatus::Paused),
        EventType::RunResumed | EventType::RunStarted => Some(RunStatus::Planning),
        _ => None,
    };
    if let Some(s) = status {
        let ended = s.is_terminal();
        sqlx::query(
            r#"UPDATE runs SET status = $2, updated_at = now(),
                 ended_at = CASE WHEN $3 THEN $4 ELSE ended_at END,
                 duration_ms = CASE WHEN $3 AND started_at IS NOT NULL THEN EXTRACT(EPOCH FROM ($4 - started_at)) * 1000 ELSE duration_ms END
               WHERE id = $1"#,
        )
        .bind(e.run_id)
        .bind(s.as_str())
        .bind(ended)
        .bind(e.timestamp)
        .execute(&mut **tx)
        .await?;
    }
    if !is_native_framework(&e.source_framework) {
        sqlx::query("UPDATE framework_integrations SET last_event_at = now(), updated_at = now() WHERE framework = $1")
            .bind(&e.source_framework)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Convenience for callers without a project (local mode).
pub fn default_project() -> Uuid {
    DEFAULT_PROJECT_ID
}
