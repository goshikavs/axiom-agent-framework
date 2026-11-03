use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context};
use goshikq_core::pricing::PricingTable;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    None,
    ApiKey,
    Oidc,
}

impl AuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthMode::None => "none",
            AuthMode::ApiKey => "api_key",
            AuthMode::Oidc => "oidc",
        }
    }
}

#[derive(Debug, Clone)]
pub struct OidcSettings {
    pub issuer: String,
    pub audience: String,
    pub jwks_url: Option<String>,
    pub hmac_secret: Option<String>,
    pub allow_test_hmac: bool,
    pub bootstrap_owner: Option<String>,
    /// How long a fetched JWKS document is reused. Unknown `kid` always triggers a refresh.
    pub jwks_cache_ttl: Duration,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
    pub auth_mode: AuthMode,
    pub oidc: OidcSettings,
    pub cors_origins: Vec<String>,
    pub capture_model_io: goshikq_runtime::CaptureMode,
    pub pricing_file: PathBuf,
    pub worker_request_timeout: Duration,
    pub approval_ttl: Duration,
    pub max_body_bytes: usize,
    pub db_max_connections: u32,
    pub otlp_rate_per_minute: u32,
    pub otlp_max_spans: usize,
    pub log_json: bool,
    pub retention_days: i32,
    pub worker_attach_timeout: Duration,
    /// OTLP/HTTP traces endpoint for Goshikq's own execution spans. Inbound `/v1/traces` is unchanged.
    pub otel_export_endpoint: Option<String>,
    pub otel_export_sample: f64,
    pub otel_service_name: String,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn branded(suffix: &str) -> Option<String> {
    goshikq_core::env::branded(suffix)
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind: SocketAddr = branded("BIND")
            .unwrap_or_else(|| "127.0.0.1:7400".into())
            .parse()
            .context("GOSHIKQ_BIND must be host:port")?;
        let database_url = env("DATABASE_URL").unwrap_or_else(|| "embedded".into());
        let auth_mode = match branded("AUTH_MODE")
            .unwrap_or_else(|| "none".into())
            .to_ascii_lowercase()
            .as_str()
        {
            "none" => AuthMode::None,
            "api_key" => AuthMode::ApiKey,
            "oidc" => AuthMode::Oidc,
            other => bail!("GOSHIKQ_AUTH_MODE must be `none`, `api_key`, or `oidc`, got `{other}`"),
        };
        let insecure_ok = branded("ALLOW_INSECURE_BIND")
            .map(|v| v == "true")
            .unwrap_or(false);
        if !bind.ip().is_loopback() && auth_mode == AuthMode::None && !insecure_ok {
            bail!("refusing to bind {bind} without authentication; set GOSHIKQ_AUTH_MODE=oidc or api_key (or GOSHIKQ_ALLOW_INSECURE_BIND=true for isolated networks)");
        }
        let allow_test_hmac = branded("OIDC_ALLOW_TEST_HMAC")
            .map(|v| v == "true")
            .unwrap_or(false);
        if allow_test_hmac && !bind.ip().is_loopback() {
            bail!("GOSHIKQ_OIDC_ALLOW_TEST_HMAC is loopback-only");
        }
        let oidc = OidcSettings {
            issuer: branded("OIDC_ISSUER").unwrap_or_default(),
            audience: branded("OIDC_AUDIENCE").unwrap_or_else(|| "goshikq".into()),
            jwks_url: branded("OIDC_JWKS_URL"),
            hmac_secret: branded("OIDC_HMAC_SECRET"),
            allow_test_hmac,
            bootstrap_owner: branded("OIDC_BOOTSTRAP_OWNER"),
            jwks_cache_ttl: Duration::from_secs(
                branded("OIDC_JWKS_CACHE_SECONDS")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(300),
            ),
        };
        if auth_mode == AuthMode::Oidc {
            if oidc.issuer.is_empty() {
                bail!("GOSHIKQ_OIDC_ISSUER is required when GOSHIKQ_AUTH_MODE=oidc");
            }
            if oidc.jwks_url.is_none() && !(oidc.allow_test_hmac && oidc.hmac_secret.is_some()) {
                bail!("OIDC requires GOSHIKQ_OIDC_JWKS_URL, or loopback HMAC test mode");
            }
        }
        let cors_origins = branded("CORS_ORIGINS")
            .unwrap_or_else(|| "http://localhost:3000,http://127.0.0.1:3000".into())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(Config {
            bind,
            database_url,
            auth_mode,
            oidc,
            cors_origins,
            capture_model_io: goshikq_runtime::CaptureMode::parse(
                &branded("CAPTURE_MODEL_IO").unwrap_or_else(|| "redacted".into()),
            ),
            pricing_file: PathBuf::from(
                branded("PRICING_FILE").unwrap_or_else(|| "config/pricing.json".into()),
            ),
            worker_request_timeout: Duration::from_secs(
                branded("WORKER_REQUEST_TIMEOUT_SECONDS")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(120),
            ),
            approval_ttl: Duration::from_secs(
                branded("APPROVAL_TTL_SECONDS")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(3600),
            ),
            max_body_bytes: branded("MAX_BODY_BYTES")
                .and_then(|v| v.parse().ok())
                .unwrap_or(4 * 1024 * 1024),
            db_max_connections: branded("DB_MAX_CONNECTIONS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(16),
            otlp_rate_per_minute: branded("OTLP_RATE_PER_MINUTE")
                .and_then(|v| v.parse().ok())
                .unwrap_or(600),
            otlp_max_spans: branded("OTLP_MAX_SPANS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(2000),
            log_json: branded("LOG_FORMAT")
                .map(|v| v.eq_ignore_ascii_case("json"))
                .unwrap_or(false),
            retention_days: branded("RETENTION_DAYS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            worker_attach_timeout: Duration::from_secs(
                branded("WORKER_ATTACH_TIMEOUT_SECONDS")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(20),
            ),
            otel_export_endpoint: branded("OTEL_EXPORTER_OTLP_ENDPOINT")
                .or_else(|| env("OTEL_EXPORTER_OTLP_ENDPOINT")),
            otel_export_sample: branded("OTEL_TRACES_SAMPLER_ARG")
                .or_else(|| env("OTEL_TRACES_SAMPLER_ARG"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(1.0_f64)
                .clamp(0.0, 1.0),
            otel_service_name: env("OTEL_SERVICE_NAME").unwrap_or_else(|| "goshikq-runtime".into()),
        })
    }

    /// Load the pricing table; a missing file yields an empty table (all costs unknown).
    pub fn load_pricing(&self) -> PricingTable {
        match std::fs::read_to_string(&self.pricing_file) {
            Ok(s) => match serde_json::from_str::<PricingTable>(&s) {
                Ok(t) => {
                    tracing::info!(file = %self.pricing_file.display(), version = %t.version, models = t.models.len(), "pricing table loaded");
                    t
                }
                Err(e) => {
                    tracing::warn!(file = %self.pricing_file.display(), error = %e, "invalid pricing file; costs will be unknown");
                    PricingTable::empty("invalid")
                }
            },
            Err(_) => {
                tracing::warn!(file = %self.pricing_file.display(), "pricing file not found; costs will be unknown");
                PricingTable::empty("unconfigured")
            }
        }
    }
}
