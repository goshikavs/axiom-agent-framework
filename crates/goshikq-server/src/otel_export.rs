//! Outbound OTLP/HTTP JSON export of Goshikq's own execution spans.
//!
//! Canonical events remain in PostgreSQL. This path is a derived, redacted view for
//! collectors. Payloads, model I/O, and secrets are never included.

use std::sync::Arc;
use std::time::Duration;

use goshikq_protocol::{EventStatus, EventType, ExecutionEvent};
use goshikq_runtime::Runtime;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn spawn(runtime: Arc<Runtime>, endpoint: String, sample: f64, service_name: String) {
    let mut rx = runtime.subscribe();
    tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "otel exporter http client failed");
                return;
            }
        };
        let url = traces_url(&endpoint);
        tracing::info!(%url, sample, service = %service_name, "otel exporter started");
        let mut buf: Vec<ExecutionEvent> = Vec::new();
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            tokio::select! {
                ev = rx.recv() => {
                    match ev {
                        Ok(e) if should_export(&e) && sampled(e.run_id, sample) => buf.push(e),
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::debug!(dropped = n, "otel exporter lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                    if buf.len() >= 16 {
                        flush(&client, &url, &service_name, &mut buf).await;
                    }
                }
                _ = tick.tick() => {
                    if !buf.is_empty() {
                        flush(&client, &url, &service_name, &mut buf).await;
                    }
                }
            }
        }
    });
}

pub fn traces_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1/traces") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1/traces")
    }
}

pub fn sampled(run_id: Uuid, ratio: f64) -> bool {
    if ratio >= 1.0 {
        return true;
    }
    if ratio <= 0.0 {
        return false;
    }
    let n = u64::from_le_bytes(run_id.as_bytes()[0..8].try_into().expect("uuid bytes"));
    (n % 10_000) < (ratio * 10_000.0) as u64
}

pub fn should_export(ev: &ExecutionEvent) -> bool {
    matches!(
        ev.event_type,
        EventType::RunStarted
            | EventType::RunCompleted
            | EventType::RunFailed
            | EventType::RunCancelled
            | EventType::ToolStarted
            | EventType::ToolCompleted
            | EventType::ToolFailed
            | EventType::ToolTimedOut
            | EventType::VerificationStarted
            | EventType::VerificationPassed
            | EventType::VerificationFailed
            | EventType::VerificationInconclusive
            | EventType::RecoveryStarted
            | EventType::RecoveryCompleted
            | EventType::GoalSatisfied
            | EventType::GoalFailed
            | EventType::AgentDelegated
            | EventType::AgentHandoffCompleted
    )
}

pub fn encode_batch(service_name: &str, events: &[ExecutionEvent]) -> Value {
    let spans: Vec<Value> = events.iter().map(span_json).collect();
    json!({
        "resourceSpans": [{
            "resource": {
                "attributes": [
                    {"key": "service.name", "value": {"stringValue": service_name}},
                    {"key": "telemetry.sdk.name", "value": {"stringValue": "goshikq"}},
                    {"key": "telemetry.sdk.language", "value": {"stringValue": "rust"}}
                ]
            },
            "scopeSpans": [{
                "scope": {"name": "goshikq.runtime", "version": env!("CARGO_PKG_VERSION")},
                "spans": spans
            }]
        }]
    })
}

fn span_json(ev: &ExecutionEvent) -> Value {
    let start = (ev.timestamp.timestamp_nanos_opt().unwrap_or(0)).max(0) as u64;
    let duration_ns = ev.duration_ms.unwrap_or(0).max(0) as u64 * 1_000_000;
    let end = start.saturating_add(duration_ns);
    let error = matches!(
        ev.status,
        Some(EventStatus::Failed) | Some(EventStatus::TimedOut) | Some(EventStatus::Cancelled)
    ) || matches!(
        ev.event_type,
        EventType::RunFailed
            | EventType::ToolFailed
            | EventType::GoalFailed
            | EventType::VerificationFailed
    );
    let mut attrs = vec![
        kv("goshikq.run_id", &ev.run_id.to_string()),
        kv("goshikq.event_type", ev.event_type.as_str()),
        kv("goshikq.source_framework", &ev.source_framework),
    ];
    if let Some(agent) = &ev.agent_id {
        attrs.push(kv("goshikq.agent_id", agent));
    }
    if let Some(status) = ev.status {
        attrs.push(kv("goshikq.status", status_name(status)));
    }
    json!({
        "traceId": otel_hex(&ev.trace_id, 16),
        "spanId": otel_hex(&ev.span_id, 8),
        "parentSpanId": ev.parent_span_id.as_deref().map(|p| otel_hex(p, 8)).unwrap_or_default(),
        "name": ev.event_type.as_str(),
        "kind": 1,
        "startTimeUnixNano": start.to_string(),
        "endTimeUnixNano": end.to_string(),
        "status": {"code": if error { 2 } else { 1 }},
        "attributes": attrs
    })
}

fn kv(key: &str, value: &str) -> Value {
    json!({"key": key, "value": {"stringValue": value}})
}

fn status_name(s: EventStatus) -> &'static str {
    match s {
        EventStatus::Info => "info",
        EventStatus::Requested => "requested",
        EventStatus::Validated => "validated",
        EventStatus::Started => "started",
        EventStatus::Running => "running",
        EventStatus::Succeeded => "succeeded",
        EventStatus::Failed => "failed",
        EventStatus::TimedOut => "timed_out",
        EventStatus::Cancelled => "cancelled",
        EventStatus::Passed => "passed",
        EventStatus::Inconclusive => "inconclusive",
        EventStatus::NotEvaluated => "not_evaluated",
        EventStatus::Pending => "pending",
        EventStatus::Granted => "granted",
        EventStatus::Denied => "denied",
        EventStatus::Unverified => "unverified",
    }
}

fn otel_hex(raw: &str, bytes: usize) -> String {
    let cleaned: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let want = bytes * 2;
    if cleaned.len() >= want {
        cleaned[..want].to_ascii_lowercase()
    } else {
        format!("{cleaned:0<want$}")
    }
}

async fn flush(client: &reqwest::Client, url: &str, service: &str, buf: &mut Vec<ExecutionEvent>) {
    if buf.is_empty() {
        return;
    }
    let body = encode_batch(service, buf);
    let n = buf.len();
    buf.clear();
    match client.post(url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!(spans = n, %url, "otel export ok");
        }
        Ok(resp) => {
            tracing::warn!(status = %resp.status(), spans = n, %url, "otel export rejected");
        }
        Err(e) => tracing::warn!(error = %e, spans = n, %url, "otel export failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traces_url_appends_once() {
        assert_eq!(
            traces_url("http://127.0.0.1:4318"),
            "http://127.0.0.1:4318/v1/traces"
        );
        assert_eq!(
            traces_url("http://127.0.0.1:4318/"),
            "http://127.0.0.1:4318/v1/traces"
        );
        assert_eq!(
            traces_url("http://127.0.0.1:4318/v1/traces"),
            "http://127.0.0.1:4318/v1/traces"
        );
    }

    #[test]
    fn sample_extremes() {
        let id = Uuid::nil();
        assert!(sampled(id, 1.0));
        assert!(!sampled(id, 0.0));
    }

    #[test]
    fn redacts_payload() {
        let ev = ExecutionEvent::native(
            Uuid::nil(),
            "aabbccddeeff00112233445566778899",
            EventType::ToolCompleted,
        )
        .with_status(EventStatus::Succeeded)
        .with_payload(serde_json::json!({"secret": "sk-not-for-otel", "api_key": "axk_deadbeef"}));
        let batch = encode_batch("goshikq-runtime", &[ev]);
        let text = batch.to_string();
        assert!(!text.contains("sk-not-for-otel"));
        assert!(!text.contains("axk_deadbeef"));
        assert!(!text.contains("payload"));
        assert!(text.contains("tool.completed"));
        assert!(text.contains("goshikq-runtime"));
    }
}
