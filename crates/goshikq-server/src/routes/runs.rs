use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use goshikq_protocol::{CreateRunRequest, WorkerPollResponse, WorkerResponse};
use goshikq_storage::runs::RunFilter;
use goshikq_storage::Page;
use uuid::Uuid;

use crate::auth::{Permission, Scope};
use crate::dto::*;
use crate::error::ApiError;
use crate::state::AppState;

#[utoipa::path(post, path = "/api/v1/runs", tag = "runs", request_body = CreateRunRequest,
    responses((status = 201, body = CreateRunResponse), (status = 400, body = crate::error::ErrorBody)))]
pub async fn create_run(
    State(st): State<AppState>,
    scope: Scope,
    Json(req): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<CreateRunResponse>), ApiError> {
    scope.require(Permission::RunWrite)?;
    let project_id = req.project_id.unwrap_or(scope.project_id);
    if project_id != scope.project_id {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "project_id does not match the authenticated scope",
        ));
    }
    let (run, reused) = st.runtime.create_run(project_id, req).await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateRunResponse {
            run_id: run.id,
            status: run.status,
            trace_id: run.trace_id,
            reused,
        }),
    ))
}

#[utoipa::path(get, path = "/api/v1/runs", tag = "runs", params(RunListQuery), responses((status = 200, body = Paginated<RunSummary>)))]
pub async fn list_runs(
    State(st): State<AppState>,
    scope: Scope,
    Query(q): Query<RunListQuery>,
) -> Result<Json<Paginated<RunSummary>>, ApiError> {
    scope.require(Permission::RunRead)?;
    let page = Page::new(q.limit, q.offset);
    let filter = RunFilter {
        status: q.status,
        verification_status: q.verification_status,
        agent: q.agent,
        framework: q.framework,
        model: q.model,
        search: q.search,
        from: q.from,
        to: q.to,
        sort: q.sort,
        order: q.order,
        project: q.project,
    };
    let (rows, total) = st.db.list_runs(scope.project_id, &filter, page).await?;
    Ok(Json(Paginated {
        items: rows.into_iter().map(RunSummary::from).collect(),
        total: Some(total),
        limit: page.limit,
        offset: page.offset,
    }))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}", tag = "runs", params(("id" = Uuid, Path)), responses((status = 200, body = RunDetail), (status = 404, body = crate::error::ErrorBody)))]
pub async fn get_run(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<RunDetail>, ApiError> {
    scope.require(Permission::RunRead)?;
    let run = st.db.get_run(scope.project_id, id).await?;
    let event_count = st.db.count_events(scope.project_id, id).await?;
    let write = st.db.latest_write_invocation(id).await?;
    let input = run.input.clone();
    let final_output = run.final_output.clone();
    let definition = run.definition.clone();
    Ok(Json(RunDetail {
        summary: run.into(),
        input,
        final_output,
        definition,
        event_count,
        tool_status: write.as_ref().map(|t| t.status.clone()),
        dispatch_state: write.as_ref().and_then(|t| {
            if t.dispatch_state.is_empty() {
                None
            } else {
                Some(t.dispatch_state.clone())
            }
        }),
        business_operation_status: write
            .as_ref()
            .and_then(|t| t.business_operation_status.clone()),
        recovery_disposition: write.and_then(|t| {
            if t.recovery_disposition.is_empty() || t.recovery_disposition == "NONE" {
                None
            } else {
                Some(t.recovery_disposition)
            }
        }),
    }))
}

#[utoipa::path(post, path = "/api/v1/runs/{id}/cancel", tag = "runs", params(("id" = Uuid, Path)), request_body = Option<CancelRunRequest>,
    responses((status = 202, body = RunSummary), (status = 409, body = crate::error::ErrorBody)))]
pub async fn cancel_run(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    body: Option<Json<CancelRunRequest>>,
) -> Result<(StatusCode, Json<RunSummary>), ApiError> {
    scope.require(Permission::RunWrite)?;
    let _reason = body.and_then(|b| b.0.reason);
    let run = st
        .runtime
        .cancel(scope.project_id, id, &scope.actor)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(run.into())))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/steps", tag = "runs", params(("id" = Uuid, Path)), responses((status = 200, body = Vec<goshikq_storage::RunStepRow>)))]
pub async fn list_steps(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<goshikq_storage::RunStepRow>>, ApiError> {
    scope.require(Permission::RunRead)?;
    st.db.get_run(scope.project_id, id).await?;
    Ok(Json(st.db.list_steps(scope.project_id, id).await?))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/tools", tag = "runs", params(("id" = Uuid, Path)), responses((status = 200, body = Vec<goshikq_storage::ToolInvocationRow>)))]
pub async fn list_tools(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<goshikq_storage::ToolInvocationRow>>, ApiError> {
    scope.require(Permission::RunRead)?;
    st.db.get_run(scope.project_id, id).await?;
    let rows = st.db.list_tool_invocations(scope.project_id, id).await?;
    Ok(Json(
        rows.into_iter()
            .map(|mut r| {
                r.arguments = goshikq_core::redaction::redact_value(&r.arguments);
                r.result = r.result.as_ref().map(goshikq_core::redaction::redact_value);
                r
            })
            .collect(),
    ))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/workflow-nodes", tag = "workflows", params(("id" = Uuid, Path)), responses((status = 200, body = Vec<goshikq_storage::WorkflowNodeExecutionRow>)))]
pub async fn list_workflow_nodes(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<goshikq_storage::WorkflowNodeExecutionRow>>, ApiError> {
    scope.require(Permission::WorkflowRead)?;
    st.db.get_run(scope.project_id, id).await?;
    Ok(Json(st.db.list_workflow_nodes(scope.project_id, id).await?))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/verification", tag = "verification", params(("id" = Uuid, Path)), responses((status = 200, body = RunVerificationReport)))]
pub async fn get_verification(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<RunVerificationReport>, ApiError> {
    scope.require(Permission::RunRead)?;
    let run = st.db.get_run(scope.project_id, id).await?;
    let goal_evaluations = st.db.list_goal_evaluations(scope.project_id, id).await?;
    let verification_results = st.db.list_verifications(scope.project_id, id).await?;
    let state_snapshots = st.db.list_state_snapshots(scope.project_id, id).await?;
    let tool_invocations = st.db.list_tool_invocations(scope.project_id, id).await?;
    let goal = run
        .definition
        .get("goal")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let has_provider = run
        .definition
        .get("has_state_provider")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let explanation = match (run.status.as_str(), run.verification_status.as_str()) {
        (_, "VERIFIED") => "The success predicate evaluated to true against an authoritative state snapshot and all invariants held.".to_string(),
        (_, "FAILED") => format!("Verification failed: {}", run.failure_reason.clone().unwrap_or_else(|| "the success predicate was false or an invariant was violated".into())),
        ("WAITING_FOR_EXTERNAL_STATE", _) => "A write was accepted by the external system; settlement is still pending. This is not a verified outcome.".to_string(),
        (_, "INCONCLUSIVE") => "GOAL_VERIFICATION_INCONCLUSIVE: the verifier could not confirm settlement against authoritative state. The outcome is unknown, not successful.".to_string(),
        _ if !has_provider => "No state provider was registered, so the goal could not be checked against authoritative state. The agent's own report is not accepted as proof.".to_string(),
        _ if goal.get("success_predicate").map(|v| v.is_null()).unwrap_or(true) => "No success predicate was declared for this goal; nothing was verified.".to_string(),
        _ => "Verification has not been evaluated yet.".to_string(),
    };
    let last_snap = state_snapshots.last();
    let last_eval = goal_evaluations.last();
    let last_action_v = verification_results
        .iter()
        .rev()
        .find(|v| v.subject_type == "action");
    let write = tool_invocations
        .iter()
        .rev()
        .find(|t| t.effect != "READ_ONLY");
    let business_operation_status = write
        .and_then(|t| t.business_operation_status.clone())
        .or_else(|| {
            last_snap.and_then(|s| {
                s.state
                    .get("refunds")
                    .and_then(|r| r.as_object())
                    .and_then(|m| {
                        m.values().find_map(|rf| {
                            rf.get("status")
                                .and_then(|st| st.as_str())
                                .map(|st| st.to_ascii_uppercase())
                        })
                    })
            })
        });
    let outcome = goshikq_protocol::OutcomeContract {
        execution_status: run.status.clone(),
        tool_status: write.map(|t| t.status.clone()),
        business_operation_status,
        goal_satisfied: last_eval.and_then(|e| e.success),
        verification_status: run.verification_status.clone(),
        evidence_source: last_snap.map(|s| s.source.clone()),
        evidence_version: last_snap.and_then(|s| s.version.clone()),
        observed_at: last_snap.map(|s| s.captured_at),
        verifier: last_action_v
            .map(|v| v.method.clone())
            .or_else(|| last_eval.map(|_| "sdk.goal_predicate".into())),
    };
    Ok(Json(RunVerificationReport {
        run_id: id,
        status: run.status,
        verification_status: run.verification_status,
        intended_outcome: run.goal_description,
        final_output: run.final_output,
        goal,
        goal_evaluations,
        verification_results,
        state_snapshots: state_snapshots
            .into_iter()
            .map(|mut s| {
                s.state = goshikq_core::redaction::redact_value(&s.state);
                s
            })
            .collect(),
        tool_invocations,
        explanation,
        outcome,
    }))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/state", tag = "runs", params(("id" = Uuid, Path)), responses((status = 200, body = RunStateResponse)))]
pub async fn get_state(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<RunStateResponse>, ApiError> {
    scope.require(Permission::RunRead)?;
    st.db.get_run(scope.project_id, id).await?;
    let snapshots: Vec<_> = st
        .db
        .list_state_snapshots(scope.project_id, id)
        .await?
        .into_iter()
        .map(|mut s| {
            s.state = goshikq_core::redaction::redact_value(&s.state);
            s
        })
        .collect();
    let latest = snapshots.last().cloned();
    let checkpoint = st.db.latest_checkpoint(id).await?.map(|mut c| {
        c.data = goshikq_core::redaction::redact_value(&c.data);
        c
    });
    Ok(Json(RunStateResponse {
        run_id: id,
        snapshots,
        latest,
        checkpoint,
    }))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/metrics", tag = "metrics", params(("id" = Uuid, Path)), responses((status = 200, body = RunMetricsResponse)))]
pub async fn get_run_metrics(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<RunMetricsResponse>, ApiError> {
    scope.require(Permission::MetricsRead)?;
    let run = st.db.get_run(scope.project_id, id).await?;
    let usage = st.db.list_model_usage(scope.project_id, id).await?;
    let tools = st.db.list_tool_invocations(scope.project_id, id).await?;
    let steps = st.db.list_steps(scope.project_id, id).await?;
    let model_latency: Option<i64> =
        if usage.iter().all(|u| u.latency_ms.is_some()) && !usage.is_empty() {
            Some(usage.iter().filter_map(|u| u.latency_ms).sum())
        } else {
            None
        };
    let tool_duration: Option<i64> = if tools.iter().all(|t| t.duration_ms.is_some()) {
        Some(tools.iter().filter_map(|t| t.duration_ms).sum())
    } else {
        None
    };
    let overhead = match (run.duration_ms, model_latency, tool_duration) {
        (Some(d), Some(m), Some(t)) => Some((d - m - t).max(0)),
        _ => None,
    };
    Ok(Json(RunMetricsResponse {
        run_id: id,
        duration_ms: run.duration_ms,
        step_count: run.step_count,
        model_calls: usage.len() as i64,
        tool_calls: tools.len() as i64,
        input_tokens: run.input_tokens,
        output_tokens: run.output_tokens,
        cached_tokens: run.cached_tokens,
        usage_known: run.usage_known,
        estimated_cost_usd: if run.cost_known {
            run.estimated_cost_usd
        } else {
            None
        },
        cost_known: run.cost_known,
        model_latency_ms_total: model_latency,
        tool_duration_ms_total: tool_duration,
        framework_overhead_ms: overhead,
        model_usage: usage,
        steps,
    }))
}

// ---------------------------------------------------------------------------------------
// Worker protocol (ADR-001)
// ---------------------------------------------------------------------------------------

#[utoipa::path(post, path = "/api/v1/runs/{id}/worker/poll", tag = "worker", params(("id" = Uuid, Path), WorkerPollQuery),
    responses((status = 200, body = WorkerPollResponse)))]
pub async fn worker_poll(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Query(q): Query<WorkerPollQuery>,
) -> Result<Json<WorkerPollResponse>, ApiError> {
    scope.require(Permission::RunWrite)?;
    let wait = Duration::from_millis(q.timeout_ms.unwrap_or(20_000).min(30_000));
    Ok(Json(st.runtime.poll(scope.project_id, id, wait).await?))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/recovery", tag = "runs", params(("id" = Uuid, Path)),
    responses((status = 200, body = goshikq_protocol::RecoveryReport), (status = 404, body = crate::error::ErrorBody)))]
pub async fn get_recovery(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<goshikq_protocol::RecoveryReport>, ApiError> {
    scope.require(Permission::RunRead)?;
    Ok(Json(st.db.recovery_report(scope.project_id, id).await?))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/evidence", tag = "verification", params(("id" = Uuid, Path)),
    responses((status = 200, body = crate::dto::RunEvidenceResponse), (status = 404, body = crate::error::ErrorBody)))]
pub async fn get_evidence(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::dto::RunEvidenceResponse>, ApiError> {
    scope.require(Permission::RunRead)?;
    st.db.get_run(scope.project_id, id).await?;
    Ok(Json(crate::dto::RunEvidenceResponse {
        run_id: id,
        records: st.db.list_evidence_records(id).await?,
        nodes: st.db.list_evidence_nodes(id).await?,
        edges: st.db.list_evidence_edges(id).await?,
        decisions: st.db.list_decisions(id).await?,
        impact: st.db.list_impact_reports(id).await?,
        revalidation: st.db.list_revalidation_results(id).await?,
        coverage: st.db.list_coverage_reports(id).await?,
        inferred: false,
    }))
}

#[utoipa::path(post, path = "/api/v1/runs/{id}/worker/respond", tag = "worker", params(("id" = Uuid, Path)), request_body = WorkerResponse,
    responses((status = 204), (status = 409, body = crate::error::ErrorBody)))]
pub async fn worker_respond(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Json(resp): Json<WorkerResponse>,
) -> Result<StatusCode, ApiError> {
    scope.require(Permission::RunWrite)?;
    // Ensure the run belongs to the caller's project before accepting a response.
    st.db.get_run(scope.project_id, id).await?;
    st.runtime.respond(id, resp).await?;
    Ok(StatusCode::NO_CONTENT)
}
