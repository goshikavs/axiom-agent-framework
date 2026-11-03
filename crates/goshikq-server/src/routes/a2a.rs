//! A2A protocol 1.0 JSON-RPC subset.
//!
//! Supported: Agent Card, SendMessage, GetTask, CancelTask.
//! Unsupported: streaming, push notifications, gRPC, HTTP+JSON REST mapping,
//! extended agent card, ListTasks.
//!
//! A remote `completed` / SUCCESS status is recorded as a Goshikq run whose
//! `verification_status` stays `NOT_EVALUATED` until an independent Goshikq goal
//! evaluation runs. Callers cannot set verification through A2A metadata.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use goshikq_protocol::{
    AgentDefinition, CreateRunRequest, ExecutionLimits, GoalDefinition, RetryPolicy,
    VerificationPolicy, A2A_PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::{Permission, Scope};
use crate::error::ApiError;
use crate::state::AppState;

#[utoipa::path(get, path = "/.well-known/agent-card.json", tag = "integrations",
    responses((status = 200)))]
pub async fn agent_card(State(st): State<AppState>) -> Json<Value> {
    Json(agent_card_body(&st))
}

fn agent_card_body(st: &AppState) -> Value {
    let url = format!("http://{}/a2a", st.config.bind);
    json!({
        "name": "Goshikq Runtime",
        "description": "Outcome-verified agent runtime. Remote A2A completion is not parent-goal verification.",
        "version": env!("CARGO_PKG_VERSION"),
        "protocolVersion": A2A_PROTOCOL_VERSION,
        "capabilities": {
            "streaming": false,
            "pushNotifications": false,
            "extendedAgentCard": false
        },
        "defaultInputModes": ["text/plain", "application/json"],
        "defaultOutputModes": ["application/json"],
        "skills": [{
            "id": "goshikq-delegate",
            "name": "Delegate a bounded Goshikq run",
            "description": "Creates a native Goshikq run. Completion of that run is not VERIFIED unless a success predicate holds against authoritative state.",
            "tags": ["goshikq", "verification"]
        }, {
            "id": "axiom-delegate",
            "name": "Delegate a bounded Goshikq run (compatibility alias)",
            "description": "Pre-rebrand skill id. Prefer goshikq-delegate.",
            "tags": ["goshikq", "verification"]
        }],
        "supportedInterfaces": [{
            "url": url,
            "protocolBinding": "JSONRPC",
            "protocolVersion": A2A_PROTOCOL_VERSION
        }]
    })
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct JsonRpcRequest {
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: Option<String>,
    #[serde(default)]
    pub params: Value,
}

#[utoipa::path(post, path = "/a2a", tag = "integrations",
    responses((status = 200), (status = 401, body = crate::error::ErrorBody)))]
pub async fn jsonrpc(
    State(st): State<AppState>,
    scope: Scope,
    Json(req): Json<JsonRpcRequest>,
) -> Result<Json<Value>, ApiError> {
    scope.require(Permission::A2A)?;
    if req.jsonrpc.as_deref().unwrap_or("2.0") != "2.0" {
        return Ok(Json(rpc_error(req.id, -32600, "jsonrpc must be 2.0")));
    }
    let method = req.method.unwrap_or_default();
    let id = req.id.clone();
    let result = match method.as_str() {
        "SendMessage" | "message/send" => send_message(&st, &scope, req.params).await,
        "GetTask" | "tasks/get" => get_task(&st, &scope, req.params).await,
        "CancelTask" | "tasks/cancel" => cancel_task(&st, &scope, req.params).await,
        "GetExtendedAgentCard" => {
            return Ok(Json(rpc_error(
                id,
                -32004,
                "extended agent card is not configured",
            )));
        }
        other => {
            return Ok(Json(rpc_error(id, -32601, &format!("method `{other}` is not implemented; supported: SendMessage, GetTask, CancelTask"))));
        }
    };
    match result {
        Ok(v) => Ok(Json(json!({"jsonrpc": "2.0", "id": id, "result": v}))),
        Err(ApiError {
            status,
            code,
            message,
            details,
        }) if status == StatusCode::BAD_REQUEST => Ok(Json(rpc_error_details(
            id,
            -32602,
            &message,
            json!({"code": code, "details": details}),
        ))),
        Err(ApiError {
            status, message, ..
        }) if status == StatusCode::NOT_FOUND => Ok(Json(rpc_error(id, -32001, &message))),
        Err(e) => Err(e),
    }
}

async fn send_message(st: &AppState, scope: &Scope, params: Value) -> Result<Value, ApiError> {
    // Untrusted remote payload: never copy verification_status or goal_satisfied from it.
    if params.get("verification_status").is_some()
        || params
            .get("markVerified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    {
        // Ignore; do not error so a naive remote cannot probe for a bypass.
    }
    let message = params.get("message").cloned().unwrap_or(params.clone());
    let text = extract_text(&message);
    if text.trim().is_empty() {
        return Err(ApiError::bad_request("SendMessage requires message text"));
    }
    if text.len() > 16_384 {
        return Err(ApiError::bad_request(
            "SendMessage text exceeds 16384 characters",
        ));
    }
    let context_id = message
        .get("contextId")
        .or_else(|| params.get("contextId"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::now_v7().to_string());
    let create = CreateRunRequest {
        project_id: Some(scope.project_id),
        agent: AgentDefinition {
            name: "a2a-remote".into(),
            description: "Run created from an untrusted A2A SendMessage".into(),
            model: "mock/deterministic".into(),
            instructions: String::new(),
            tools: vec![],
            execution_limits: ExecutionLimits {
                max_steps: 1,
                deadline_seconds: 60,
                ..Default::default()
            },
            retry_policy: RetryPolicy::default(),
            context_policy: json!({}),
            metadata: json!({"a2a": true}),
            permissions: vec![],
        },
        goal: GoalDefinition {
            description: text.clone(),
            success_predicate: None,
            invariants: vec![],
            max_steps: Some(1),
            deadline_seconds: Some(60),
            external_wait_seconds: None,
            budget: None,
            verification_policy: VerificationPolicy::None,
            metadata: json!({"a2a": true}),
        },
        input: json!({"a2a_message": message, "untrusted": true}),
        has_state_provider: false,
        source_framework: "a2a".into(),
        idempotency_key: params
            .get("idempotencyKey")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        metadata: json!({
            "a2a": true,
            "a2a_protocol_version": A2A_PROTOCOL_VERSION,
            "untrusted_remote": true,
            "note": "A2A task completion is not Goshikq parent-goal verification",
        }),
        defer_start: true,
        parent_run_id: None,
        task_id: None,
        correlation_id: Some(context_id.clone()),
        trace_id: None,
    };
    let (run, reused) = st.runtime.create_run(scope.project_id, create).await?;
    Ok(task_object(&run, reused, &context_id))
}

async fn get_task(st: &AppState, scope: &Scope, params: Value) -> Result<Value, ApiError> {
    let id = params
        .get("id")
        .or_else(|| params.get("taskId"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("GetTask requires id"))?;
    let run_id =
        Uuid::parse_str(id).map_err(|_| ApiError::bad_request("task id must be a UUID"))?;
    let run = st.db.get_run(scope.project_id, run_id).await?;
    let ctx = run
        .correlation_id
        .clone()
        .unwrap_or_else(|| run.id.to_string());
    Ok(task_object(&run, false, &ctx))
}

async fn cancel_task(st: &AppState, scope: &Scope, params: Value) -> Result<Value, ApiError> {
    let id = params
        .get("id")
        .or_else(|| params.get("taskId"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("CancelTask requires id"))?;
    let run_id =
        Uuid::parse_str(id).map_err(|_| ApiError::bad_request("task id must be a UUID"))?;
    let run = st
        .runtime
        .cancel(scope.project_id, run_id, &scope.actor)
        .await?;
    let ctx = run
        .correlation_id
        .clone()
        .unwrap_or_else(|| run.id.to_string());
    Ok(task_object(&run, false, &ctx))
}

fn task_object(run: &goshikq_storage::RunRow, reused: bool, context_id: &str) -> Value {
    let state = a2a_state(&run.status);
    json!({
        "id": run.id,
        "contextId": context_id,
        "status": {
            "state": state,
            "message": {
                "role": "agent",
                "parts": [{"kind": "text", "text": run.failure_reason.clone().unwrap_or_else(|| run.status.clone())}]
            }
        },
        "metadata": {
            "goshikqRunStatus": run.status,
            "goshikqVerificationStatus": run.verification_status,
            "goshikqNote": "Remote or A2A task status is not authoritative goal verification. SUCCESS/completed does not imply VERIFIED.",
            "axiomRunStatus": run.status,
            "axiomVerificationStatus": run.verification_status,
            "axiomNote": "Remote or A2A task status is not authoritative goal verification. SUCCESS/completed does not imply VERIFIED.",
            "reused": reused,
            "untrusted": true
        }
    })
}

fn a2a_state(status: &str) -> &'static str {
    match status {
        "CREATED" | "PAUSED" => "submitted",
        "WAITING_FOR_APPROVAL" => "input-required",
        "COMPLETED" | "UNVERIFIED" => "completed",
        "FAILED"
        | "TIMED_OUT"
        | "BUDGET_EXCEEDED"
        | "STEP_LIMIT_EXCEEDED"
        | "MODEL_BUDGET_EXCEEDED" => "failed",
        "CANCELLED" => "canceled",
        "RECOVERY_REQUIRED" => "rejected",
        _ => "working",
    }
}

fn extract_text(message: &Value) -> String {
    if let Some(s) = message.as_str() {
        return s.to_string();
    }
    if let Some(s) = message.get("text").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    let parts = message
        .get("parts")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut bits = Vec::new();
    for p in parts {
        if let Some(t) = p.get("text").and_then(|v| v.as_str()) {
            bits.push(t.to_string());
        }
    }
    bits.join("\n")
}

fn rpc_error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn rpc_error_details(id: Option<Value>, code: i64, message: &str, data: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message, "data": data}})
}
