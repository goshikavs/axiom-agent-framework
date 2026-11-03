use axum::extract::State;
use axum::Json;
use chrono::{Duration, Utc};

use crate::auth::{Permission, Scope};
use crate::dto::{RetentionPurgeRequest, RetentionPurgeResponse};
use crate::error::ApiError;
use crate::state::AppState;

#[utoipa::path(
    post,
    path = "/api/v1/admin/retention/purge",
    tag = "admin",
    request_body = RetentionPurgeRequest,
    responses((status = 200, body = RetentionPurgeResponse), (status = 403))
)]
pub async fn purge_retention(
    State(st): State<AppState>,
    scope: Scope,
    Json(req): Json<RetentionPurgeRequest>,
) -> Result<Json<RetentionPurgeResponse>, ApiError> {
    scope.require(Permission::RetentionPurge)?;
    let dry_run = req.dry_run.unwrap_or(true);
    let days = req
        .older_than_days
        .unwrap_or(st.config.retention_days)
        .max(0);
    let older_than = Utc::now() - Duration::days(days as i64);
    let result = st
        .db
        .purge_terminal_runs(older_than, req.only_run_ids.clone(), dry_run)
        .await?;
    let _ = st
        .db
        .record_security_event(
            Some(scope.project_id),
            scope.tenant_id,
            &scope.actor,
            if dry_run {
                "retention.purge_dry_run"
            } else {
                "retention.purge"
            },
            "ok",
            serde_json::json!({
                "deleted": result.deleted,
                "run_ids": result.run_ids,
                "older_than": older_than,
                "dry_run": dry_run,
            }),
        )
        .await;
    Ok(Json(RetentionPurgeResponse {
        dry_run,
        older_than,
        deleted: result.deleted,
        run_ids: result.run_ids,
        skipped_active_parents: result.skipped_active_parents,
    }))
}
