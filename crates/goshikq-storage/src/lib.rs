//! PostgreSQL persistence layer. Every query that returns run-scoped data is filtered by
//! `project_id` so tenant scoping is enforced in one place.

pub mod approvals;
pub mod catalog;
pub mod events;
pub mod evidence;
pub mod evidence_graph;
pub mod journal;
pub mod leases;
pub mod metrics;
pub mod models;
pub mod orchestration;
pub mod runs;
pub mod tools;
pub mod workflows;

use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;
use uuid::Uuid;

pub use models::*;
pub use runs::RetentionPurgeResult;

/// Project seeded by the initial migration for local single-user development.
pub const DEFAULT_PROJECT_ID: Uuid = Uuid::from_u128(1);

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
}

pub type DbResult<T> = Result<T, StorageError>;

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect(database_url: &str, max_connections: u32) -> DbResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(database_url)
            .await?;
        Ok(Db { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Db { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn migrate(&self) -> DbResult<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub async fn ping(&self) -> DbResult<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    pub fn pool_metrics(&self) -> (u32, u32, usize) {
        (
            self.pool.options().get_max_connections(),
            self.pool.size(),
            self.pool.num_idle(),
        )
    }

    pub async fn runtime_counters(&self) -> DbResult<(i64, i64, i64, i64)> {
        let row: (i64, i64, i64, i64) = sqlx::query_as(
            r#"SELECT
                (SELECT count(*) FROM runs WHERE lease_owner IS NOT NULL AND lease_expires_at > now()),
                (SELECT count(*) FROM runs WHERE lease_owner IS NOT NULL AND lease_expires_at <= now()),
                (SELECT count(*) FROM runs WHERE status = 'RECOVERY_REQUIRED'),
                (SELECT count(*) FROM tool_invocations WHERE status = 'FAILED')
            "#,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }
}

/// Offset pagination shared by list endpoints.
#[derive(Debug, Clone, Copy)]
pub struct Page {
    pub limit: i64,
    pub offset: i64,
}

impl Page {
    pub fn new(limit: Option<i64>, offset: Option<i64>) -> Self {
        Page {
            limit: limit.unwrap_or(50).clamp(1, 500),
            offset: offset.unwrap_or(0).max(0),
        }
    }
}

pub fn sha256_hex(input: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(input))
}
