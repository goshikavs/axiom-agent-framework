//! Local-development PostgreSQL that does not require Docker.
//!
//! When `DATABASE_URL=embedded` (the default in `.env.example`), goshikq-server downloads
//! a Theseus PostgreSQL distribution once, stores data under `.goshikq/postgres`, and
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
        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("creating {}", data_dir.display()))?;
        let mut settings = Settings::default();
        settings.host = "127.0.0.1".into();
        settings.port = goshikq_core::env::branded("EMBEDDED_PG_PORT")
            .and_then(|s| s.parse().ok())
            .unwrap_or(55432);
        // initdb creates the bootstrap superuser `postgres`. New clusters use
        // database/password `goshikq`; a pre-rebrand `.axiom/postgres` cluster keeps `axiom`.
        let (password, dbname) = cluster_credentials(&data_dir);
        settings.username = postgresql_embedded::BOOTSTRAP_SUPERUSER.into();
        settings.password = password.into();
        settings.temporary = false;
        settings.timeout = Some(std::time::Duration::from_secs(
            goshikq_core::env::branded("EMBEDDED_PG_TIMEOUT_SECS")
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
        inner
            .setup()
            .await
            .context("downloading/installing embedded PostgreSQL")?;
        inner
            .start()
            .await
            .context("starting embedded PostgreSQL")?;
        if !inner.database_exists(dbname).await.unwrap_or(false) {
            inner
                .create_database(dbname)
                .await
                .with_context(|| format!("creating database `{dbname}`"))?;
        }
        let s = inner.settings();
        let url = format!(
            "postgres://{}:{}@{}:{}/{dbname}",
            s.username, s.password, s.host, s.port
        );
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
    v.eq_ignore_ascii_case("embedded")
        || v.eq_ignore_ascii_case("goshikq-embedded")
        || v.eq_ignore_ascii_case("axiom-embedded")
        || v.starts_with("embedded:")
}

pub fn default_data_dir() -> PathBuf {
    if let Some(p) = goshikq_core::env::branded("EMBEDDED_PG_DIR") {
        return PathBuf::from(p);
    }
    let modern = PathBuf::from(".goshikq/postgres");
    let legacy = PathBuf::from(".axiom/postgres");
    if modern.exists() {
        modern
    } else if legacy.exists() {
        legacy
    } else {
        modern
    }
}

fn cluster_credentials(data_dir: &std::path::Path) -> (&'static str, &'static str) {
    if data_dir.to_string_lossy().contains(".axiom") {
        ("axiom", "axiom")
    } else {
        ("goshikq", "goshikq")
    }
}
