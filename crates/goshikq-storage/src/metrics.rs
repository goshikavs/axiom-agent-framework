//! Aggregations for the Studio overview and cost pages. All values are computed from
//! persisted rows; "unknown" coverage is reported explicitly so the UI never shows
//! missing data as zero.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{Db, DbResult};

#[derive(Debug, Default, Clone)]
pub struct MetricsFilter {
    pub agent: Option<String>,
    pub framework: Option<String>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct OverviewTotals {
    pub total_runs: i64,
    pub active_runs: i64,
    pub completed_runs: i64,
    pub unverified_runs: i64,
    pub failed_runs: i64,
    pub cancelled_runs: i64,
    pub verified_runs: i64,
    pub verification_failed_runs: i64,
    pub inconclusive_runs: i64,
    pub terminal_runs: i64,
    pub avg_duration_ms: Option<f64>,
    pub p50_duration_ms: Option<f64>,
    pub p95_duration_ms: Option<f64>,
    pub p99_duration_ms: Option<f64>,
    /// Sum over runs with known usage.
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cached_tokens: i64,
    /// Runs whose usage is fully known.
    pub usage_known_runs: i64,
    /// Sum over runs with known cost (estimated).
    pub total_estimated_cost_usd: Option<f64>,
    pub cost_known_runs: i64,
    pub verified_runs_with_cost: i64,
    pub verified_cost_usd: Option<f64>,
    /// Sum of verification_results.duration_ms; NULL when no timed verification rows exist.
    pub verification_overhead_ms: Option<f64>,
    pub verification_results_with_duration: i64,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct TimeBucket {
    pub bucket: DateTime<Utc>,
    pub total: i64,
    pub completed: i64,
    pub unverified: i64,
    pub failed: i64,
    pub verified: i64,
    pub estimated_cost_usd: Option<f64>,
    pub cost_known_runs: i64,
    pub p50_duration_ms: Option<f64>,
    pub p95_duration_ms: Option<f64>,
    pub p99_duration_ms: Option<f64>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct ModelBreakdown {
    pub model: String,
    pub calls: i64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub avg_latency_ms: Option<f64>,
    pub estimated_cost_usd: Option<f64>,
    pub priced_calls: i64,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct GroupBreakdown {
    pub key: String,
    pub runs: i64,
    pub verified: i64,
    pub failed: i64,
    pub estimated_cost_usd: Option<f64>,
    pub cost_known_runs: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub usage_known_runs: i64,
    pub avg_duration_ms: Option<f64>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct VerificationBreakdown {
    pub verification_status: String,
    pub runs: i64,
}

const RUN_WHERE: &str = r#"project_id = $1
    AND ($2::text IS NULL OR agent_name = $2)
    AND ($3::text IS NULL OR source_framework = $3)
    AND ($4::text IS NULL OR model = $4)
    AND ($5::text IS NULL OR status = $5)
    AND ($6::timestamptz IS NULL OR created_at >= $6)
    AND ($7::timestamptz IS NULL OR created_at <= $7)"#;

const RUN_WHERE_ALIASED: &str = r#"r.project_id = $1
    AND ($2::text IS NULL OR r.agent_name = $2)
    AND ($3::text IS NULL OR r.source_framework = $3)
    AND ($4::text IS NULL OR r.model = $4)
    AND ($5::text IS NULL OR r.status = $5)
    AND ($6::timestamptz IS NULL OR r.created_at >= $6)
    AND ($7::timestamptz IS NULL OR r.created_at <= $7)"#;

macro_rules! bind_filter {
    ($q:expr, $project:expr, $f:expr) => {
        $q.bind($project)
            .bind(&$f.agent)
            .bind(&$f.framework)
            .bind(&$f.model)
            .bind(&$f.status)
            .bind($f.from)
            .bind($f.to)
    };
}

impl Db {
    pub async fn overview_totals(
        &self,
        project_id: Uuid,
        f: &MetricsFilter,
    ) -> DbResult<OverviewTotals> {
        let sql = format!(
            r#"SELECT
                COUNT(*) AS total_runs,
                COUNT(*) FILTER (WHERE status NOT IN ('COMPLETED','UNVERIFIED','FAILED','CANCELLED','TIMED_OUT','BUDGET_EXCEEDED','STEP_LIMIT_EXCEEDED','MODEL_BUDGET_EXCEEDED')) AS active_runs,
                COUNT(*) FILTER (WHERE status = 'COMPLETED') AS completed_runs,
                COUNT(*) FILTER (WHERE status = 'UNVERIFIED') AS unverified_runs,
                COUNT(*) FILTER (WHERE status IN ('FAILED','TIMED_OUT','BUDGET_EXCEEDED','STEP_LIMIT_EXCEEDED','MODEL_BUDGET_EXCEEDED')) AS failed_runs,
                COUNT(*) FILTER (WHERE status = 'CANCELLED') AS cancelled_runs,
                COUNT(*) FILTER (WHERE verification_status = 'VERIFIED') AS verified_runs,
                COUNT(*) FILTER (WHERE verification_status = 'FAILED') AS verification_failed_runs,
                COUNT(*) FILTER (WHERE verification_status = 'INCONCLUSIVE') AS inconclusive_runs,
                COUNT(*) FILTER (WHERE status IN ('COMPLETED','UNVERIFIED','FAILED','CANCELLED','TIMED_OUT','BUDGET_EXCEEDED','STEP_LIMIT_EXCEEDED','MODEL_BUDGET_EXCEEDED')) AS terminal_runs,
                AVG(duration_ms)::double precision AS avg_duration_ms,
                percentile_cont(0.5) WITHIN GROUP (ORDER BY duration_ms)::double precision AS p50_duration_ms,
                percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms)::double precision AS p95_duration_ms,
                percentile_cont(0.99) WITHIN GROUP (ORDER BY duration_ms)::double precision AS p99_duration_ms,
                COALESCE(SUM(input_tokens) FILTER (WHERE usage_known), 0)::bigint AS total_input_tokens,
                COALESCE(SUM(output_tokens) FILTER (WHERE usage_known), 0)::bigint AS total_output_tokens,
                COALESCE(SUM(cached_tokens) FILTER (WHERE usage_known), 0)::bigint AS total_cached_tokens,
                COUNT(*) FILTER (WHERE usage_known) AS usage_known_runs,
                SUM(estimated_cost_usd) FILTER (WHERE cost_known)::double precision AS total_estimated_cost_usd,
                COUNT(*) FILTER (WHERE cost_known) AS cost_known_runs,
                COUNT(*) FILTER (WHERE cost_known AND verification_status = 'VERIFIED') AS verified_runs_with_cost,
                SUM(estimated_cost_usd) FILTER (WHERE cost_known AND verification_status = 'VERIFIED')::double precision AS verified_cost_usd,
                (SELECT SUM(vr.duration_ms)::double precision FROM verification_results vr JOIN runs r ON r.id = vr.run_id WHERE {RUN_WHERE_ALIASED} AND vr.duration_ms IS NOT NULL) AS verification_overhead_ms,
                (SELECT COUNT(*) FROM verification_results vr JOIN runs r ON r.id = vr.run_id WHERE {RUN_WHERE_ALIASED} AND vr.duration_ms IS NOT NULL)::bigint AS verification_results_with_duration
               FROM runs WHERE {RUN_WHERE}"#
        );
        Ok(
            bind_filter!(sqlx::query_as::<_, OverviewTotals>(&sql), project_id, f)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn time_series(
        &self,
        project_id: Uuid,
        f: &MetricsFilter,
        bucket: &str,
    ) -> DbResult<Vec<TimeBucket>> {
        let unit = match bucket {
            "minute" | "hour" | "day" | "week" => bucket,
            _ => "hour",
        };
        let sql = format!(
            r#"SELECT date_trunc('{unit}', created_at) AS bucket,
                COUNT(*) AS total,
                COUNT(*) FILTER (WHERE status = 'COMPLETED') AS completed,
                COUNT(*) FILTER (WHERE status = 'UNVERIFIED') AS unverified,
                COUNT(*) FILTER (WHERE status IN ('FAILED','TIMED_OUT','BUDGET_EXCEEDED','STEP_LIMIT_EXCEEDED','MODEL_BUDGET_EXCEEDED')) AS failed,
                COUNT(*) FILTER (WHERE verification_status = 'VERIFIED') AS verified,
                SUM(estimated_cost_usd) FILTER (WHERE cost_known)::double precision AS estimated_cost_usd,
                COUNT(*) FILTER (WHERE cost_known) AS cost_known_runs,
                percentile_cont(0.5) WITHIN GROUP (ORDER BY duration_ms)::double precision AS p50_duration_ms,
                percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms)::double precision AS p95_duration_ms,
                percentile_cont(0.99) WITHIN GROUP (ORDER BY duration_ms)::double precision AS p99_duration_ms
               FROM runs WHERE {RUN_WHERE}
               GROUP BY 1 ORDER BY 1"#
        );
        Ok(
            bind_filter!(sqlx::query_as::<_, TimeBucket>(&sql), project_id, f)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn model_breakdown(
        &self,
        project_id: Uuid,
        f: &MetricsFilter,
    ) -> DbResult<Vec<ModelBreakdown>> {
        let sql = format!(
            r#"SELECT mu.model,
                COUNT(*) AS calls,
                SUM(mu.input_tokens)::bigint AS input_tokens,
                SUM(mu.output_tokens)::bigint AS output_tokens,
                SUM(mu.cached_tokens)::bigint AS cached_tokens,
                AVG(mu.latency_ms)::double precision AS avg_latency_ms,
                SUM(c.estimated_cost_usd)::double precision AS estimated_cost_usd,
                COUNT(c.id) AS priced_calls
               FROM model_usage mu
               JOIN runs r ON r.id = mu.run_id
               LEFT JOIN cost_records c ON c.model_usage_id = mu.id
               WHERE {RUN_WHERE_ALIASED}
               GROUP BY mu.model ORDER BY calls DESC"#
        );
        Ok(
            bind_filter!(sqlx::query_as::<_, ModelBreakdown>(&sql), project_id, f)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    /// Breakdown by `agent_name`, `source_framework`, or `model`.
    pub async fn group_breakdown(
        &self,
        project_id: Uuid,
        f: &MetricsFilter,
        group: &str,
    ) -> DbResult<Vec<GroupBreakdown>> {
        let col = match group {
            "agent" => "agent_name",
            "framework" => "source_framework",
            _ => "model",
        };
        let sql = format!(
            r#"SELECT {col} AS key,
                COUNT(*) AS runs,
                COUNT(*) FILTER (WHERE verification_status = 'VERIFIED') AS verified,
                COUNT(*) FILTER (WHERE status IN ('FAILED','TIMED_OUT','BUDGET_EXCEEDED','STEP_LIMIT_EXCEEDED','MODEL_BUDGET_EXCEEDED')) AS failed,
                SUM(estimated_cost_usd) FILTER (WHERE cost_known)::double precision AS estimated_cost_usd,
                COUNT(*) FILTER (WHERE cost_known) AS cost_known_runs,
                COALESCE(SUM(input_tokens) FILTER (WHERE usage_known), 0)::bigint AS input_tokens,
                COALESCE(SUM(output_tokens) FILTER (WHERE usage_known), 0)::bigint AS output_tokens,
                COUNT(*) FILTER (WHERE usage_known) AS usage_known_runs,
                AVG(duration_ms)::double precision AS avg_duration_ms
               FROM runs WHERE {RUN_WHERE}
               GROUP BY 1 ORDER BY runs DESC"#
        );
        Ok(
            bind_filter!(sqlx::query_as::<_, GroupBreakdown>(&sql), project_id, f)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn verification_breakdown(
        &self,
        project_id: Uuid,
        f: &MetricsFilter,
    ) -> DbResult<Vec<VerificationBreakdown>> {
        let sql = format!("SELECT verification_status, COUNT(*) AS runs FROM runs WHERE {RUN_WHERE} GROUP BY 1 ORDER BY 1");
        Ok(bind_filter!(
            sqlx::query_as::<_, VerificationBreakdown>(&sql),
            project_id,
            f
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn distinct_filter_values(
        &self,
        project_id: Uuid,
    ) -> DbResult<(Vec<String>, Vec<String>, Vec<String>)> {
        let agents: Vec<(String,)> =
            sqlx::query_as("SELECT DISTINCT agent_name FROM runs WHERE project_id = $1 ORDER BY 1")
                .bind(project_id)
                .fetch_all(&self.pool)
                .await?;
        let frameworks: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT source_framework FROM runs WHERE project_id = $1 ORDER BY 1",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        let models: Vec<(String,)> =
            sqlx::query_as("SELECT DISTINCT model FROM runs WHERE project_id = $1 ORDER BY 1")
                .bind(project_id)
                .fetch_all(&self.pool)
                .await?;
        Ok((
            agents.into_iter().map(|a| a.0).collect(),
            frameworks.into_iter().map(|a| a.0).collect(),
            models.into_iter().map(|a| a.0).collect(),
        ))
    }
}
