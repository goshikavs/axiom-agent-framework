//! Local-development PostgreSQL that does not require Docker.
//!
//! When `DATABASE_URL=embedded` (the default in `.env.example`), axiom-server downloads
//! a Theseus PostgreSQL distribution once, stores data under `.axiom/postgres`, and
//! listens on loopback. Production deployments must set a real `DATABASE_URL`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use postgresql_embedded::{PostgreSQL, Settings};

/// Owns the child postgres process for the lifetime of the server.
pub struct EmbeddedPostgres {
    inner: PostgreSQL,
    pub url: String,
}

impl EmbeddedPostgres {
    pub async fn start(data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir).with_context(|| format!("creating {}", data_dir.display()))?;
        let mut settings = Settings::default();
        settings.host = "127.0.0.1".into();
        settings.port = std::env::var("AXIOM_EMBEDDED_PG_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(55432);
        // initdb creates the bootstrap superuser `postgres`; the application database is `axiom`.
        settings.username = postgresql_embedded::BOOTSTRAP_SUPERUSER.into();
        settings.password = "axiom".into();
        settings.temporary = false;
        settings.timeout = Some(std::time::Duration::from_secs(
            std::env::var("AXIOM_EMBEDDED_PG_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(180),
        ));
        settings.data_dir = data_dir.join("data");
        settings.installation_dir = data_dir.join("install");
        settings.password_file = data_dir.join(".pgpass");
        tracing::info!(
            data_dir = %settings.data_dir.display(),
            port = settings.port,
            "starting embedded PostgreSQL (first run downloads binaries into the cache)"
        );
        let mut inner = PostgreSQL::new(settings);
        inner.setup().await.context("downloading/installing embedded PostgreSQL")?;
        inner.start().await.context("starting embedded PostgreSQL")?;
        if !inner.database_exists("axiom").await.unwrap_or(false) {
            inner.create_database("axiom").await.context("creating database `axiom`")?;
        }
        let s = inner.settings();
        let url = format!("postgres://{}:{}@{}:{}/axiom", s.username, s.password, s.host, s.port);
        tracing::info!(url = %redact_url(&url), "embedded PostgreSQL ready");
        Ok(EmbeddedPostgres { inner, url })
    }
}

impl Drop for EmbeddedPostgres {
    fn drop(&mut self) {
        // Best-effort; process exit also tears the child down.
        let _ = self.inner;
    }
}

fn redact_url(url: &str) -> String {
    if let Some(at) = url.find('@') {
        if let Some(scheme) = url.find("://") {
            return format!("{}://***@{}", &url[..scheme], &url[at + 1..]);
        }
    }
    url.to_string()
}

pub fn wants_embedded(database_url: &str) -> bool {
    let v = database_url.trim();
    v.eq_ignore_ascii_case("embedded") || v.eq_ignore_ascii_case("axiom-embedded") || v.starts_with("embedded:")
}

pub fn default_data_dir() -> PathBuf {
    std::env::var("AXIOM_EMBEDDED_PG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".axiom/postgres"))
}
