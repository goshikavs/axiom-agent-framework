use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::{Db, DbResult, RunRow, StorageError};

/// How long a claimed run stays owned without heartbeat.
pub const LEASE_TTL_SECS: i64 = 30;

impl Db {
    /// Unconditional generation bump. Used at process startup when the previous owner
    /// cannot still be running in this process. A stale in-flight worker is fenced out
    /// because its generation no longer matches.
    pub async fn steal_run_lease(&self, run_id: Uuid, owner: &str, ttl: Duration) -> DbResult<i64> {
        let expires = Utc::now() + ttl;
        let row: (i64,) = sqlx::query_as(
            r#"UPDATE runs SET
                 lease_owner = $2,
                 lease_generation = lease_generation + 1,
                 lease_expires_at = $3,
                 updated_at = now()
               WHERE id = $1
               RETURNING lease_generation"#,
        )
        .bind(run_id)
        .bind(owner)
        .bind(expires)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Claim exclusive ownership. Returns the new fencing token, or `Conflict` if another
    /// live owner holds the lease.
    pub async fn claim_run_lease(&self, run_id: Uuid, owner: &str, ttl: Duration) -> DbResult<i64> {
        let expires = Utc::now() + ttl;
        let row: Option<(i64,)> = sqlx::query_as(
            r#"UPDATE runs SET
                 lease_owner = $2,
                 lease_generation = lease_generation + 1,
                 lease_expires_at = $3,
                 updated_at = now()
               WHERE id = $1
                 AND (lease_expires_at IS NULL OR lease_expires_at < now() OR lease_owner = $2)
               RETURNING lease_generation"#,
        )
        .bind(run_id)
        .bind(owner)
        .bind(expires)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| r.0).ok_or_else(|| StorageError::Conflict(format!("run {run_id} is leased by another owner")))
    }

    pub async fn heartbeat_lease(&self, run_id: Uuid, owner: &str, generation: i64, ttl: Duration) -> DbResult<bool> {
        let expires = Utc::now() + ttl;
        let n = sqlx::query(
            r#"UPDATE runs SET lease_expires_at = $4, updated_at = now()
               WHERE id = $1 AND lease_owner = $2 AND lease_generation = $3"#,
        )
        .bind(run_id)
        .bind(owner)
        .bind(generation)
        .bind(expires)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n == 1)
    }

    pub async fn release_lease(&self, run_id: Uuid, owner: &str, generation: i64) -> DbResult<()> {
        sqlx::query(
            r#"UPDATE runs SET lease_owner = NULL, lease_expires_at = NULL, updated_at = now()
               WHERE id = $1 AND lease_owner = $2 AND lease_generation = $3"#,
        )
        .bind(run_id)
        .bind(owner)
        .bind(generation)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// True when this generation still owns the run.
    pub async fn lease_is_valid(&self, run_id: Uuid, owner: &str, generation: i64) -> DbResult<bool> {
        let row: Option<(i64, DateTime<Utc>, Option<String>)> = sqlx::query_as(
            "SELECT lease_generation, COALESCE(lease_expires_at, '-infinity'::timestamptz), lease_owner FROM runs WHERE id = $1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(match row {
            Some((g, exp, Some(o))) => g == generation && o == owner && exp > Utc::now(),
            _ => false,
        })
    }

    pub async fn list_recoverable_runs(&self) -> DbResult<Vec<RunRow>> {
        Ok(sqlx::query_as::<_, RunRow>(
            r#"SELECT * FROM runs
               WHERE source_framework LIKE 'axiom%'
                 AND status NOT IN ('COMPLETED','UNVERIFIED','FAILED','CANCELLED','TIMED_OUT','BUDGET_EXCEEDED','STEP_LIMIT_EXCEEDED','MODEL_BUDGET_EXCEEDED')
                 AND NOT (status = 'CREATED' AND COALESCE(definition->>'defer_start','false') = 'true')
               ORDER BY created_at"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }
}
