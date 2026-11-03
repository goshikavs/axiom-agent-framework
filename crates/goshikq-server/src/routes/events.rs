use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures::Stream;
use goshikq_protocol::{EventBatch, EventBatchResult, EventType, ExecutionEvent, RunStatus};
use goshikq_storage::Page;
use uuid::Uuid;

use crate::auth::{Permission, Scope};
use crate::dto::*;
use crate::error::ApiError;
use crate::state::AppState;

fn redact_event(mut e: ExecutionEvent) -> ExecutionEvent {
    e.payload = goshikq_core::redaction::redact_value(&e.payload);
    e
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/events", tag = "events", params(("id" = Uuid, Path), EventListQuery),
    responses((status = 200, body = Paginated<ExecutionEvent>)))]
pub async fn list_run_events(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Query(q): Query<EventListQuery>,
) -> Result<Json<Paginated<ExecutionEvent>>, ApiError> {
    scope.require(Permission::RunRead)?;
    st.db.get_run(scope.project_id, id).await?;
    let page = Page::new(q.limit, q.offset);
    let rows = st
        .db
        .list_events(scope.project_id, id, q.after, page)
        .await?;
    let total = st.db.count_events(scope.project_id, id).await?;
    Ok(Json(Paginated {
        items: rows
            .into_iter()
            .map(ExecutionEvent::from)
            .map(redact_event)
            .collect(),
        total: Some(total),
        limit: page.limit,
        offset: page.offset,
    }))
}

#[utoipa::path(get, path = "/api/v1/events", tag = "events", params(RecentEventsQuery), responses((status = 200, body = Paginated<ExecutionEvent>)))]
pub async fn list_recent_events(
    State(st): State<AppState>,
    scope: Scope,
    Query(q): Query<RecentEventsQuery>,
) -> Result<Json<Paginated<ExecutionEvent>>, ApiError> {
    scope.require(Permission::RunRead)?;
    let page = Page::new(q.limit, q.offset);
    let rows = st
        .db
        .list_recent_events(scope.project_id, q.event_type.as_deref(), page)
        .await?;
    Ok(Json(Paginated {
        items: rows
            .into_iter()
            .map(ExecutionEvent::from)
            .map(redact_event)
            .collect(),
        total: None,
        limit: page.limit,
        offset: page.offset,
    }))
}

#[utoipa::path(post, path = "/api/v1/events/batch", tag = "events", request_body = EventBatch,
    responses((status = 200, body = EventBatchResult), (status = 400, body = crate::error::ErrorBody)))]
pub async fn ingest_batch(
    State(st): State<AppState>,
    scope: Scope,
    Json(batch): Json<EventBatch>,
) -> Result<Json<EventBatchResult>, ApiError> {
    scope.require(Permission::TelemetryWrite)?;
    if batch.events.is_empty() {
        return Err(ApiError::bad_request("events must not be empty"));
    }
    if batch.events.len() > 1000 {
        return Err(ApiError::bad_request("at most 1000 events per batch"));
    }
    let outcome = st.db.ingest_batch(scope.project_id, batch.events).await?;
    st.runtime.wake_publisher();
    Ok(Json(EventBatchResult {
        accepted: outcome.accepted.len(),
        duplicates: outcome.duplicates,
        rejected: outcome.rejected,
    }))
}

fn is_terminal_event(e: &ExecutionEvent) -> bool {
    matches!(
        e.event_type,
        EventType::RunCompleted | EventType::RunFailed | EventType::RunCancelled
    )
}

/// Server-Sent Events stream for one run. Resumable via `Last-Event-ID` (sequence) or
/// `?after=`. Persisted events are replayed first, then live events; duplicates are
/// filtered by sequence. Ends with `event: end` once a terminal event has been sent.
#[utoipa::path(get, path = "/api/v1/runs/{id}/events/stream", tag = "events", params(("id" = Uuid, Path), StreamQuery),
    responses((status = 200, description = "text/event-stream of ExecutionEvent JSON")))]
pub async fn stream_run_events(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Query(q): Query<StreamQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    scope.require(Permission::RunRead)?;
    let run = st.db.get_run(scope.project_id, id).await?;
    let last_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok());
    let mut after = goshikq_core::stream_after(last_id, q.after);
    let project_id = scope.project_id;
    let db = st.db.clone();
    // Subscribe before the catch-up query so nothing is missed in between.
    let mut live = st.runtime.subscribe();
    let initial_terminal = RunStatus::parse(&run.status)
        .map(|s| s.is_terminal())
        .unwrap_or(false);

    let stream = async_stream::stream! {
        // Catch-up from persistence.
        let mut page = 0i64;
        let mut sent_terminal = false;
        loop {
            match db.list_events(project_id, id, Some(after), Page::new(Some(500), Some(0))).await {
                Ok(rows) => {
                    if rows.is_empty() { break; }
                    for row in rows {
                        let ev: ExecutionEvent = row.into();
                        after = ev.sequence;
                        sent_terminal |= is_terminal_event(&ev);
                        let ev = redact_event(ev);
                        // Default SSE event type (`message`) so EventSource.onmessage receives it.
                        // The domain type lives in the JSON `event_type` field.
                        yield Ok(Event::default().id(ev.sequence.to_string()).json_data(&ev).unwrap_or_else(|_| Event::default()));
                    }
                    page += 1;
                    if page > 1000 { break; }
                }
                Err(e) => {
                    yield Ok(Event::default().event("error").data(format!("{{\"message\":\"{}\"}}", e.to_string().replace('"', "'"))));
                    break;
                }
            }
        }
        if sent_terminal || initial_terminal {
            yield Ok(Event::default().event("end").data("{\"reason\":\"terminal\"}"));
            return;
        }
        // Live tail.
        loop {
            match live.recv().await {
                Ok(ev) if ev.run_id == id => {
                    if ev.sequence <= after { continue; }
                    // Fill gaps if the broadcast skipped sequences (e.g. lagged receiver).
                    if ev.sequence > after + 1 {
                        if let Ok(rows) = db.list_events(project_id, id, Some(after), Page::new(Some(500), Some(0))).await {
                            for row in rows {
                                let gap: ExecutionEvent = row.into();
                                if gap.sequence <= after { continue; }
                                after = gap.sequence;
                                let terminal = is_terminal_event(&gap);
                                let gap = redact_event(gap);
                                yield Ok(Event::default().id(gap.sequence.to_string()).json_data(&gap).unwrap_or_else(|_| Event::default()));
                                if terminal { yield Ok(Event::default().event("end").data("{\"reason\":\"terminal\"}")); return; }
                            }
                        }
                        if ev.sequence <= after { continue; }
                    }
                    after = ev.sequence;
                    let terminal = is_terminal_event(&ev);
                    let ev = redact_event(ev);
                    yield Ok(Event::default().id(ev.sequence.to_string()).json_data(&ev).unwrap_or_else(|_| Event::default()));
                    if terminal {
                        yield Ok(Event::default().event("end").data("{\"reason\":\"terminal\"}"));
                        return;
                    }
                }
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    if let Ok(rows) = db.list_events(project_id, id, Some(after), Page::new(Some(500), Some(0))).await {
                        for row in rows {
                            let gap: ExecutionEvent = row.into();
                            if gap.sequence <= after { continue; }
                            after = gap.sequence;
                            let terminal = is_terminal_event(&gap);
                            let gap = redact_event(gap);
                            yield Ok(Event::default().id(gap.sequence.to_string()).json_data(&gap).unwrap_or_else(|_| Event::default()));
                            if terminal { yield Ok(Event::default().event("end").data("{\"reason\":\"terminal\"}")); return; }
                        }
                    }
                    continue;
                }
                Err(_) => {
                    yield Ok(Event::default().event("end").data("{\"reason\":\"publisher_closed\"}"));
                    return;
                }
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}
