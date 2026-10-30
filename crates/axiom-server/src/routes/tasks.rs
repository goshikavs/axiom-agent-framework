use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use axiom_protocol::{CreateTaskRequest, HandoffRequest, SharedContextPut};
use axiom_storage::RunTaskRow;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::{Permission, Scope};
use crate::dto::*;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct SharedHistoryQuery {
    pub key: Option<String>,
}

#[utoipa::path(post, path = "/api/v1/runs/{id}/tasks", tag = "runs", request_body = CreateTaskRequest,
    params(("id" = Uuid, Path)),
    responses((status = 201, body = CreateTaskResponse), (status = 400, body = crate::error::ErrorBody)))]
pub async fn create_task(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<CreateTaskResponse>), ApiError> {
    scope.require(Permission::RunWrite)?;
    let created = st.runtime.create_child_task(scope.project_id, id, req, &scope.actor).await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateTaskResponse {
            parent_run_id: created.parent.id,
            child_run_id: created.child.id,
            task_id: created.task.task_id,
            correlation_id: created.task.correlation_id,
            status: created.child.status,
            verification_status: created.child.verification_status,
            reused: created.reused,
            approval_id: created.approval_id,
        }),
    ))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/tasks", tag = "runs", params(("id" = Uuid, Path)),
    responses((status = 200, body = Vec<RunTaskRow>)))]
pub async fn list_tasks(State(st): State<AppState>, scope: Scope, Path(id): Path<Uuid>) -> Result<Json<Vec<RunTaskRow>>, ApiError> {
    scope.require(Permission::RunRead)?;
    st.db.get_run(scope.project_id, id).await?;
    Ok(Json(st.db.list_child_tasks(scope.project_id, id).await?))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/hierarchy", tag = "runs", params(("id" = Uuid, Path)),
    responses((status = 200, body = RunHierarchy)))]
pub async fn hierarchy(State(st): State<AppState>, scope: Scope, Path(id): Path<Uuid>) -> Result<Json<RunHierarchy>, ApiError> {
    scope.require(Permission::RunRead)?;
    let run = st.db.get_run(scope.project_id, id).await?;
    let parent = match run.parent_run_id {
        Some(pid) => st.db.get_run(scope.project_id, pid).await.ok().map(RunSummary::from),
        None => None,
    };
    let tasks = st.db.list_child_tasks(scope.project_id, id).await?;
    let mut children = Vec::new();
    for task in tasks {
        if let Ok(child) = st.db.get_run(scope.project_id, task.child_run_id).await {
            children.push(ChildTaskView { task, run: child.into() });
        }
    }
    Ok(Json(RunHierarchy { run: run.into(), parent, children }))
}

#[utoipa::path(post, path = "/api/v1/runs/{id}/start", tag = "runs", params(("id" = Uuid, Path)),
    responses((status = 202, body = RunSummary)))]
pub async fn start_run(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<RunSummary>), ApiError> {
    scope.require(Permission::RunWrite)?;
    let run = st.runtime.start_run(scope.project_id, id).await?;
    Ok((StatusCode::ACCEPTED, Json(run.into())))
}

#[utoipa::path(post, path = "/api/v1/runs/{id}/handoff", tag = "runs", request_body = HandoffRequest,
    params(("id" = Uuid, Path)),
    responses((status = 201, body = CreateTaskResponse)))]
pub async fn handoff(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Json(req): Json<HandoffRequest>,
) -> Result<(StatusCode, Json<CreateTaskResponse>), ApiError> {
    scope.require(Permission::RunWrite)?;
    let created = st.runtime.handoff(scope.project_id, id, req, &scope.actor).await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateTaskResponse {
            parent_run_id: created.parent.id,
            child_run_id: created.child.id,
            task_id: created.task.task_id,
            correlation_id: created.task.correlation_id,
            status: created.child.status,
            verification_status: created.child.verification_status,
            reused: created.reused,
            approval_id: created.approval_id,
        }),
    ))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/shared-context", tag = "runs", params(("id" = Uuid, Path)),
    responses((status = 200, body = Vec<axiom_storage::SharedContextRow>)))]
pub async fn list_shared(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<axiom_storage::SharedContextRow>>, ApiError> {
    scope.require(Permission::SharedRead)?;
    Ok(Json(st.runtime.list_shared(scope.project_id, id).await?))
}

#[utoipa::path(put, path = "/api/v1/runs/{id}/shared-context", tag = "runs", request_body = SharedContextPut,
    params(("id" = Uuid, Path)),
    responses((status = 200, body = axiom_storage::SharedContextRow), (status = 409, body = crate::error::ErrorBody)))]
pub async fn put_shared(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Json(body): Json<SharedContextPut>,
) -> Result<Json<axiom_storage::SharedContextRow>, ApiError> {
    scope.require(Permission::SharedWrite)?;
    Ok(Json(st.runtime.put_shared(scope.project_id, id, body, &scope.actor).await?))
}

#[utoipa::path(get, path = "/api/v1/runs/{id}/shared-context/history", tag = "runs",
    params(("id" = Uuid, Path), SharedHistoryQuery),
    responses((status = 200, body = Vec<axiom_storage::SharedContextHistoryRow>)))]
pub async fn shared_history(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Query(q): Query<SharedHistoryQuery>,
) -> Result<Json<Vec<axiom_storage::SharedContextHistoryRow>>, ApiError> {
    scope.require(Permission::SharedRead)?;
    Ok(Json(st.runtime.shared_history(scope.project_id, id, q.key.as_deref()).await?))
}
