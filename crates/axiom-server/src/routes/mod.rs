pub mod a2a;
pub mod admin;
pub mod approvals;
pub mod catalog;
pub mod events;
pub mod metrics;
pub mod otlp;
pub mod runs;
pub mod tasks;

use std::time::Duration;

use axum::http::{header, HeaderName, HeaderValue, Method};
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Axiom Runtime API",
        description = "Outcome-driven agent runtime: runs, worker protocol, execution events, approvals, verification, metrics.",
        license(name = "Apache-2.0"),
        version = env!("CARGO_PKG_VERSION"),
    ),
    tags(
        (name = "system"), (name = "projects"), (name = "agents"), (name = "runs"), (name = "worker"),
        (name = "events"), (name = "verification"), (name = "approvals"), (name = "metrics"),
        (name = "integrations"), (name = "workflows"), (name = "models"), (name = "admin")
    ),
    components(schemas(crate::error::ErrorBody, crate::error::ErrorDetail))
)]
pub struct ApiDoc;

/// Build the application router and the OpenAPI document generated from it.
pub fn build(state: AppState) -> (Router, utoipa::openapi::OpenApi) {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(state.config.cors_origins.iter().filter_map(|o| HeaderValue::from_str(o).ok())))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
            HeaderName::from_static("x-axiom-actor"),
            HeaderName::from_static("last-event-id"),
            HeaderName::from_static("x-api-key"),
            HeaderName::from_static("x-axiom-project-id"),
            HeaderName::from_static("x-axiom-worker-id"),
            HeaderName::from_static("x-axiom-worker-capabilities"),
        ])
        .max_age(Duration::from_secs(600));

    // Long-lived endpoints (SSE, long-poll) must not sit behind the request timeout.
    let (streaming, api_a) = OpenApiRouter::new()
        .routes(routes!(events::stream_run_events))
        .routes(routes!(runs::worker_poll))
        .split_for_parts();

    let (regular, api_b) = OpenApiRouter::new()
        .routes(routes!(catalog::health))
        .routes(routes!(catalog::ready))
        .routes(routes!(catalog::whoami))
        .routes(routes!(catalog::list_projects, catalog::create_project))
        .routes(routes!(catalog::list_api_keys, catalog::create_api_key))
        .routes(routes!(catalog::revoke_api_key))
        .routes(routes!(catalog::rotate_api_key))
        .routes(routes!(catalog::list_members, catalog::upsert_member))
        .routes(routes!(catalog::delete_member))
        .routes(routes!(catalog::list_security_events))
        .routes(routes!(metrics::runtime_metrics))
        .routes(routes!(catalog::list_agents, catalog::save_agent))
        .routes(routes!(catalog::get_agent))
        .routes(routes!(runs::create_run, runs::list_runs))
        .routes(routes!(runs::get_run))
        .routes(routes!(runs::cancel_run))
        .routes(routes!(runs::list_steps))
        .routes(routes!(runs::list_tools))
        .routes(routes!(runs::get_verification))
        .routes(routes!(runs::get_state))
        .routes(routes!(runs::get_recovery))
        .routes(routes!(runs::get_evidence))
        .routes(routes!(runs::get_run_metrics))
        .routes(routes!(runs::worker_respond))
        .routes(routes!(events::list_run_events))
        .routes(routes!(events::list_recent_events))
        .routes(routes!(events::ingest_batch))
        .routes(routes!(approvals::list_approvals))
        .routes(routes!(approvals::get_approval))
        .routes(routes!(approvals::decide_approval))
        .routes(routes!(metrics::overview))
        .routes(routes!(metrics::executions))
        .routes(routes!(metrics::costs))
        .routes(routes!(catalog::list_integrations, catalog::upsert_integration))
        .routes(routes!(catalog::list_workflows, catalog::create_workflow))
        .routes(routes!(catalog::get_workflow))
        .routes(routes!(catalog::update_workflow))
        .routes(routes!(catalog::validate_workflow))
        .routes(routes!(catalog::publish_workflow))
        .routes(routes!(catalog::list_workflow_versions))
        .routes(routes!(catalog::duplicate_workflow))
        .routes(routes!(catalog::run_workflow))
        .routes(routes!(runs::list_workflow_nodes))
        .routes(routes!(catalog::models))
        .routes(routes!(catalog::list_capabilities))
        .routes(routes!(tasks::create_task, tasks::list_tasks))
        .routes(routes!(tasks::hierarchy))
        .routes(routes!(tasks::start_run))
        .routes(routes!(tasks::handoff))
        .routes(routes!(tasks::list_shared, tasks::put_shared))
        .routes(routes!(tasks::shared_history))
        .routes(routes!(a2a::agent_card))
        .routes(routes!(a2a::jsonrpc))
        .routes(routes!(admin::purge_retention))
        .split_for_parts();

    let mut api = ApiDoc::openapi();
    api.merge(api_a);
    api.merge(api_b);

    let openapi_json = api.clone();
    let router = Router::new()
        .merge(regular.layer(TimeoutLayer::with_status_code(axum::http::StatusCode::REQUEST_TIMEOUT, Duration::from_secs(60))))
        .merge(streaming)
        .route("/api/v1/openapi.json", get(move || { let doc = openapi_json.clone(); async move { axum::Json(doc) } }))
        .route("/v1/traces", post(otlp::ingest_traces))
        .route("/api/v1/otlp/v1/traces", post(otlp::ingest_traces))
        .layer(RequestBodyLimitLayer::new(state.config.max_body_bytes))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    (router, api)
}
