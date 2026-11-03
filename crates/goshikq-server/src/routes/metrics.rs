use axum::extract::{Query, State};
use axum::Json;
use goshikq_storage::metrics::{MetricsFilter, TimeBucket};

use crate::auth::{Permission, Scope};
use crate::dto::*;
use crate::error::ApiError;
use crate::state::AppState;

fn filter(q: &MetricsQuery) -> MetricsFilter {
    MetricsFilter {
        agent: q.agent.clone(),
        framework: q.framework.clone(),
        model: q.model.clone(),
        status: q.status.clone(),
        from: q.from,
        to: q.to,
    }
}

#[utoipa::path(get, path = "/api/v1/metrics/overview", tag = "metrics", params(MetricsQuery), responses((status = 200, body = OverviewResponse)))]
pub async fn overview(
    State(st): State<AppState>,
    scope: Scope,
    Query(q): Query<MetricsQuery>,
) -> Result<Json<OverviewResponse>, ApiError> {
    scope.require(Permission::MetricsRead)?;
    let f = filter(&q);
    let bucket = q.bucket.clone().unwrap_or_else(|| "hour".into());
    let totals = st.db.overview_totals(scope.project_id, &f).await?;
    let pending_approvals = st.db.count_pending_approvals(scope.project_id).await?;
    let series = st.db.time_series(scope.project_id, &f, &bucket).await?;
    let verification = st.db.verification_breakdown(scope.project_id, &f).await?;
    let models = st.db.model_breakdown(scope.project_id, &f).await?;
    let (agents, frameworks, model_names) = st.db.distinct_filter_values(scope.project_id).await?;
    let by_framework = st
        .db
        .group_breakdown(scope.project_id, &f, "framework")
        .await?;
    Ok(Json(OverviewResponse {
        totals,
        pending_approvals,
        series,
        verification,
        models,
        filters: FilterValues {
            agents,
            frameworks,
            models: model_names,
        },
        by_framework,
        pricing_version: st.runtime.cfg.pricing.version.clone(),
        bucket,
    }))
}

#[utoipa::path(get, path = "/api/v1/metrics/executions", tag = "metrics", params(MetricsQuery), responses((status = 200, body = Vec<TimeBucket>)))]
pub async fn executions(
    State(st): State<AppState>,
    scope: Scope,
    Query(q): Query<MetricsQuery>,
) -> Result<Json<Vec<TimeBucket>>, ApiError> {
    scope.require(Permission::MetricsRead)?;
    let f = filter(&q);
    Ok(Json(
        st.db
            .time_series(scope.project_id, &f, q.bucket.as_deref().unwrap_or("hour"))
            .await?,
    ))
}

#[utoipa::path(get, path = "/api/v1/metrics/costs", tag = "metrics", params(MetricsQuery), responses((status = 200, body = CostResponse)))]
pub async fn costs(
    State(st): State<AppState>,
    scope: Scope,
    Query(q): Query<MetricsQuery>,
) -> Result<Json<CostResponse>, ApiError> {
    scope.require(Permission::MetricsRead)?;
    let f = filter(&q);
    Ok(Json(CostResponse {
        pricing_version: st.runtime.cfg.pricing.version.clone(),
        totals: st.db.overview_totals(scope.project_id, &f).await?,
        by_model: st.db.model_breakdown(scope.project_id, &f).await?,
        by_agent: st.db.group_breakdown(scope.project_id, &f, "agent").await?,
        by_framework: st
            .db
            .group_breakdown(scope.project_id, &f, "framework")
            .await?,
        series: st
            .db
            .time_series(scope.project_id, &f, q.bucket.as_deref().unwrap_or("day"))
            .await?,
    }))
}

#[utoipa::path(get, path = "/api/v1/runtime/metrics", tag = "metrics", responses((status = 200)))]
pub async fn runtime_metrics(
    State(st): State<AppState>,
    scope: Scope,
) -> Result<Json<serde_json::Value>, ApiError> {
    scope.require(Permission::MetricsRead)?;
    let (max_conn, size, idle) = st.db.pool_metrics();
    let (active_leases, expired_leases, recovery_queue, failed_actions) =
        st.db.runtime_counters().await?;
    Ok(Json(serde_json::json!({
        "auth_mode": st.config.auth_mode.as_str(),
        "uptime_seconds": (chrono::Utc::now() - st.started_at).num_seconds(),
        "active_runs": st.runtime.active_run_count(),
        "retention_days": st.config.retention_days,
        "max_body_bytes": st.config.max_body_bytes,
        "db_pool": { "max_connections": max_conn, "size": size, "idle": idle },
        "leases": { "active": active_leases, "expired": expired_leases },
        "recovery_queue": recovery_queue,
        "failed_actions_persisted": failed_actions,
        "runtime": st.runtime.metrics.snapshot(),
    })))
}
