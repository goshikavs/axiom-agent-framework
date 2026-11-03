mod auth;
mod config;
mod dto;
mod embed;
mod error;
mod oidc;
mod otel_export;
mod otlp_limit;
mod routes;
mod state;

use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use goshikq_runtime::{Runtime, RuntimeConfig};
use goshikq_storage::Db;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;

/// Goshikq runtime server.
#[derive(Parser, Debug)]
#[command(name = "goshikq-server", version, about)]
struct Cli {
    /// Write the generated OpenAPI document to this path and exit.
    #[arg(long)]
    export_openapi: Option<std::path::PathBuf>,
    /// Run database migrations and exit.
    #[arg(long)]
    migrate_only: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let json_logs = goshikq_core::env::branded("LOG_FORMAT")
        .map(|v| v.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,goshikq_server=debug,goshikq_runtime=debug"));
    if json_logs {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .with_target(true)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .init();
    }
    let cli = Cli::parse();

    if let Some(path) = cli.export_openapi {
        // Generating the document does not need a database or a real DATABASE_URL
        // (the default `embedded` is not a parseable postgres URL).
        let config = offline_config();
        let db = Db::from_pool(sqlx_lazy_pool(&config.database_url)?);
        let runtime = Runtime::new(db.clone(), RuntimeConfig::default());
        let otlp = Arc::new(crate::otlp_limit::OtlpLimiter::new(
            config.otlp_rate_per_minute,
        ));
        let oidc = Arc::new(crate::oidc::OidcValidator::new(config.oidc.clone()));
        let state = AppState {
            config: Arc::new(config),
            db,
            runtime,
            started_at: chrono::Utc::now(),
            otlp,
            oidc,
        };
        let (_, api) = routes::build(state);
        let json = api.to_pretty_json()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, format!("{json}\n"))?;
        println!("wrote {}", path.display());
        return Ok(());
    }

    let mut config = Config::from_env()?;
    let _embedded = if crate::embed::wants_embedded(&config.database_url) {
        let pg = crate::embed::EmbeddedPostgres::start(crate::embed::default_data_dir()).await?;
        config.database_url = pg.url.clone();
        Some(pg)
    } else {
        None
    };
    let db = Db::connect(&config.database_url, config.db_max_connections).await.context("connecting to PostgreSQL (set DATABASE_URL=embedded for a local download, or start Compose)")?;
    db.migrate().await.context("running migrations")?;
    tracing::info!("database migrated");
    if cli.migrate_only {
        return Ok(());
    }

    let pricing = Arc::new(config.load_pricing());
    let runtime = Runtime::new(
        db.clone(),
        RuntimeConfig {
            worker_request_timeout: config.worker_request_timeout,
            approval_ttl: config.approval_ttl,
            capture_model_io: config.capture_model_io,
            pricing,
            publish_interval: std::time::Duration::from_millis(250),
        },
    );
    runtime.recover_on_startup().await?;
    seed_native_integration(&db).await?;
    seed_external_integrations(&db).await?;
    if let Some(endpoint) = config.otel_export_endpoint.clone() {
        crate::otel_export::spawn(
            runtime.clone(),
            endpoint,
            config.otel_export_sample,
            config.otel_service_name.clone(),
        );
    }

    if let Some(owner) = config.oidc.bootstrap_owner.clone() {
        db.upsert_member(goshikq_storage::DEFAULT_PROJECT_ID, &owner, "owner")
            .await?;
        tracing::info!(subject = %owner, "oidc bootstrap owner seeded on default project");
    }

    let otlp = Arc::new(crate::otlp_limit::OtlpLimiter::new(
        config.otlp_rate_per_minute,
    ));
    let oidc = Arc::new(crate::oidc::OidcValidator::new(config.oidc.clone()));
    let state = AppState {
        config: Arc::new(config.clone()),
        db,
        runtime: runtime.clone(),
        started_at: chrono::Utc::now(),
        otlp,
        oidc,
    };
    let (router, _) = routes::build(state);

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    tracing::info!(bind = %config.bind, auth = ?config.auth_mode, capture = ?config.capture_model_io, json_logs = config.log_json, "goshikq-server listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal(runtime))
        .await?;
    tracing::info!("shutdown complete");
    Ok(())
}

async fn shutdown_signal(runtime: Arc<Runtime>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    tracing::info!("shutdown signal received; cancelling active runs");
    runtime.shutdown();
}

/// Register the native adapter so the integrations page reflects what is actually running.
async fn seed_native_integration(db: &Db) -> anyhow::Result<()> {
    db.upsert_integration(
        goshikq_storage::DEFAULT_PROJECT_ID,
        "goshikq",
        env!("CARGO_PKG_VERSION"),
        Some(env!("CARGO_PKG_VERSION")),
        "connected",
        &serde_json::json!(["trace_ingestion", "tool_events", "state_snapshots", "action_interception", "pause_resume", "outcome_verification"]),
        &serde_json::json!(["replay"]),
        &serde_json::json!({"transport": "worker long-poll", "protocol_version": goshikq_protocol::SCHEMA_VERSION}),
        None,
    )
    .await?;
    Ok(())
}

fn offline_config() -> Config {
    Config {
        bind: "127.0.0.1:7400".parse().expect("valid addr"),
        database_url: "postgres://localhost/unused".into(),
        auth_mode: config::AuthMode::None,
        oidc: config::OidcSettings {
            issuer: String::new(),
            audience: "goshikq".into(),
            jwks_url: None,
            hmac_secret: None,
            allow_test_hmac: false,
            bootstrap_owner: None,
            jwks_cache_ttl: std::time::Duration::from_secs(300),
        },
        cors_origins: vec![],
        capture_model_io: goshikq_runtime::CaptureMode::Redacted,
        pricing_file: "config/pricing.json".into(),
        worker_request_timeout: std::time::Duration::from_secs(120),
        approval_ttl: std::time::Duration::from_secs(3600),
        max_body_bytes: 4 * 1024 * 1024,
        db_max_connections: 1,
        otlp_rate_per_minute: 600,
        otlp_max_spans: 2000,
        log_json: false,
        retention_days: 30,
        worker_attach_timeout: std::time::Duration::from_secs(20),
        otel_export_endpoint: None,
        otel_export_sample: 1.0,
        otel_service_name: "goshikq-runtime".into(),
    }
}

async fn seed_external_integrations(db: &Db) -> anyhow::Result<()> {
    for m in goshikq_protocol::FrameworkCapabilityManifest::builtin() {
        if m.framework == "goshikq" {
            continue;
        }
        let caps: Vec<&str> = [
            ("trace_ingestion", m.trace_support),
            ("tool_events", m.tool_event_support),
            ("state_snapshots", m.state_snapshot_support),
            ("approvals", m.approval_support),
            ("action_interception", m.action_interception_support),
            ("pause_resume", m.pause_resume_support),
            ("outcome_verification", m.outcome_verification_support),
            ("replay", m.replay_support),
        ]
        .into_iter()
        .filter(|(_, lvl)| *lvl != goshikq_protocol::CapabilityLevel::Unsupported)
        .map(|(n, _)| n)
        .collect();
        let unsupported: Vec<&str> = [
            ("trace_ingestion", m.trace_support),
            ("tool_events", m.tool_event_support),
            ("state_snapshots", m.state_snapshot_support),
            ("approvals", m.approval_support),
            ("action_interception", m.action_interception_support),
            ("pause_resume", m.pause_resume_support),
            ("outcome_verification", m.outcome_verification_support),
            ("replay", m.replay_support),
        ]
        .into_iter()
        .filter(|(_, lvl)| *lvl == goshikq_protocol::CapabilityLevel::Unsupported)
        .map(|(n, _)| n)
        .collect();
        db.upsert_integration(
            goshikq_storage::DEFAULT_PROJECT_ID,
            &m.framework,
            &m.adapter_version,
            Some(&m.framework_version),
            "configured",
            &serde_json::json!(caps),
            &serde_json::json!(unsupported),
            &serde_json::json!({"notes": m.notes, "mapping": goshikq_core::MAPPING_VERSION, "controls_execution": false}),
            None,
        )
        .await?;
    }
    Ok(())
}

fn sqlx_lazy_pool(url: &str) -> anyhow::Result<sqlx::PgPool> {
    Ok(sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy(url)?)
}
