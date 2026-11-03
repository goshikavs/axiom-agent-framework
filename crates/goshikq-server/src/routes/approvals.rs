use axum::extract::{Path, Query, State};
use axum::Json;
use goshikq_storage::{ApprovalRow, Page};
use uuid::Uuid;

use crate::auth::{Permission, Scope};
use crate::dto::*;
use crate::error::ApiError;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/v1/approvals", tag = "approvals", params(ApprovalListQuery), responses((status = 200, body = Paginated<ApprovalRow>)))]
pub async fn list_approvals(
    State(st): State<AppState>,
    scope: Scope,
    Query(q): Query<ApprovalListQuery>,
) -> Result<Json<Paginated<ApprovalRow>>, ApiError> {
    scope.require(Permission::ApprovalRead)?;
    let page = Page::new(q.limit, q.offset);
    let rows = st
        .db
        .list_approvals(scope.project_id, q.status.as_deref(), q.run_id, page)
        .await?;
    Ok(Json(Paginated {
        items: rows
            .into_iter()
            .map(|mut a| {
                a.arguments = goshikq_core::redaction::redact_value(&a.arguments);
                a
            })
            .collect(),
        total: None,
        limit: page.limit,
        offset: page.offset,
    }))
}

#[utoipa::path(get, path = "/api/v1/approvals/{id}", tag = "approvals", params(("id" = Uuid, Path)), responses((status = 200, body = ApprovalRow)))]
pub async fn get_approval(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<ApprovalRow>, ApiError> {
    scope.require(Permission::ApprovalRead)?;
    Ok(Json(st.db.get_approval(scope.project_id, id).await?))
}

#[utoipa::path(post, path = "/api/v1/approvals/{id}/decision", tag = "approvals", params(("id" = Uuid, Path)), request_body = ApprovalDecisionRequest,
    responses((status = 200, body = ApprovalDecisionResponse), (status = 409, body = crate::error::ErrorBody)))]
pub async fn decide_approval(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> Result<Json<ApprovalDecisionResponse>, ApiError> {
    scope.require(Permission::ApprovalDecide)?;
    let approve = match body.decision.to_ascii_lowercase().as_str() {
        "approve" | "approved" => true,
        "deny" | "denied" | "reject" | "rejected" => false,
        other => {
            return Err(ApiError::bad_request(format!(
                "decision must be `approve` or `deny`, got `{other}`"
            )))
        }
    };
    let (approval, applied) = st
        .runtime
        .decide_approval(
            scope.project_id,
            id,
            approve,
            &scope.actor,
            body.reason.as_deref(),
        )
        .await?;
    let _ = st
        .db
        .record_security_event(
            Some(scope.project_id),
            scope.tenant_id,
            &scope.actor,
            "approval.decided",
            if applied { "ok" } else { "noop" },
            serde_json::json!({"id": id, "approve": approve}),
        )
        .await;
    Ok(Json(ApprovalDecisionResponse { approval, applied }))
}
