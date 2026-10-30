//! OTLP/HTTP trace ingestion (protobuf and JSON).
//!
//! External spans become execution events. They never mark a goal VERIFIED.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axiom_core::otel::{decode_otlp_json, decode_otlp_protobuf, map_spans, OtelMapOptions};
use axiom_core::otel_proto::{ExportTracePartialSuccess, ExportTraceServiceResponse};
use axiom_runtime::CaptureMode;
use prost::Message;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::{Permission, Scope};
use crate::error::ApiError;
use crate::state::AppState;

pub async fn ingest_traces(State(st): State<AppState>, scope: Scope, headers: HeaderMap, body: Bytes) -> Result<Response, ApiError> {
    scope.require(Permission::TelemetryWrite)?;
    if !st.otlp.allow(scope.project_id) {
        return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", "OTLP ingest rate exceeded for this project"));
    }
    if body.len() > st.config.max_body_bytes {
        return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large", "OTLP body exceeds AXIOM_MAX_BODY_BYTES"));
    }
    if let Some(claimed) = header_project(&headers) {
        if claimed != scope.project_id {
            return Err(ApiError::forbidden("X-Axiom-Project-Id does not match the authenticated project"));
        }
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/x-protobuf")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    let spans = if content_type.contains("json") {
        let v: Value = serde_json::from_slice(&body).map_err(|e| ApiError::bad_request(format!("invalid OTLP JSON: {e}")))?;
        decode_otlp_json(&v).map_err(ApiError::bad_request)?
    } else {
        decode_otlp_protobuf(&body).map_err(ApiError::bad_request)?
    };
    if spans.len() > st.config.otlp_max_spans {
        return Err(ApiError::bad_request(format!("at most {} spans per request", st.config.otlp_max_spans)));
    }
    if spans.is_empty() {
        return otlp_response(&content_type, 0, None);
    }

    let opts = OtelMapOptions { capture_prompts: st.config.capture_model_io == CaptureMode::Full };
    let mapped = map_spans(&spans, &opts);
    if let Some(claimed) = mapped.claimed_project_id {
        if claimed != scope.project_id {
            return Err(ApiError::forbidden("span axiom.project_id does not match the authenticated project"));
        }
    }
    if mapped.events.is_empty() && !mapped.rejected.is_empty() {
        return Err(ApiError::bad_request(mapped.rejected.join("; ")));
    }

    let outcome = st.db.ingest_batch(scope.project_id, mapped.events).await?;
    st.runtime.wake_publisher();

    let rejected = outcome.rejected.len() as i64 + mapped.rejected.len() as i64;
    let msg = if mapped.rejected.is_empty() {
        None
    } else {
        Some(mapped.rejected.join("; "))
    };
    let _ = outcome.accepted.len();
    otlp_response(&content_type, rejected, msg)
}

fn header_project(headers: &HeaderMap) -> Option<Uuid> {
    headers.get("x-axiom-project-id").and_then(|v| v.to_str().ok()).and_then(|s| Uuid::parse_str(s.trim()).ok())
}

fn otlp_response(content_type: &str, rejected: i64, message: Option<String>) -> Result<Response, ApiError> {
    if content_type.contains("json") {
        let body = if rejected == 0 {
            json!({})
        } else {
            json!({"partialSuccess": {"rejectedSpans": rejected, "errorMessage": message.unwrap_or_default()}})
        };
        return Ok((StatusCode::OK, Json(body)).into_response());
    }
    let resp = ExportTraceServiceResponse {
        partial_success: if rejected == 0 {
            None
        } else {
            Some(ExportTracePartialSuccess { rejected_spans: rejected, error_message: message.unwrap_or_default() })
        },
    };
    let mut buf = Vec::new();
    resp.encode(&mut buf).map_err(|e| ApiError::internal(e.to_string()))?;
    Ok((StatusCode::OK, [(header::CONTENT_TYPE, "application/x-protobuf")], buf).into_response())
}
