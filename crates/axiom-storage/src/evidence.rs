//! State snapshots, goal evaluations, verification results, model usage and cost records.

use axiom_protocol::{GoalEvaluateResult, StateSnapshotResult, TokenUsage, VerificationStatus};
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::{Db, DbResult, GoalEvaluationRow, ModelUsageRow, StateSnapshotRow, VerificationResultRow};

pub struct VerificationInsert<'a> {
    pub run_id: Uuid,
    pub step_id: Option<&'a str>,
    pub subject_type: &'a str,
    pub subject_id: &'a str,
    pub status: VerificationStatus,
    pub method: &'a str,
    pub expected: Option<&'a Value>,
    pub actual: Option<&'a Value>,
    pub evidence_source: Option<&'a str>,
    pub evidence_version: Option<&'a str>,
    pub evidence_timestamp: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub reason: Option<&'a str>,
}

impl Db {
    pub async fn save_state_snapshot(&self, run_id: Uuid, step_id: Option<&str>, reason: &str, snap: &StateSnapshotResult, changes: Option<&Value>) -> DbResult<StateSnapshotRow> {
        Ok(sqlx::query_as::<_, StateSnapshotRow>(
            r#"INSERT INTO state_snapshots (id, run_id, step_id, reason, source, version, state, changes, captured_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(step_id)
        .bind(reason)
        .bind(&snap.source)
        .bind(&snap.version)
        .bind(&snap.state)
        .bind(changes)
        .bind(snap.captured_at.unwrap_or_else(Utc::now))
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn list_state_snapshots(&self, project_id: Uuid, run_id: Uuid) -> DbResult<Vec<StateSnapshotRow>> {
        Ok(sqlx::query_as::<_, StateSnapshotRow>(
            "SELECT s.* FROM state_snapshots s JOIN runs r ON r.id = s.run_id WHERE r.project_id = $1 AND s.run_id = $2 ORDER BY s.created_at",
        )
        .bind(project_id)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn save_goal_evaluation(&self, run_id: Uuid, step_id: Option<&str>, phase: &str, result: &GoalEvaluateResult, snapshot_id: Option<Uuid>) -> DbResult<GoalEvaluationRow> {
        Ok(sqlx::query_as::<_, GoalEvaluationRow>(
            r#"INSERT INTO goal_evaluations (id, run_id, step_id, phase, success, invariants, evidence, reason, state_snapshot_id)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(step_id)
        .bind(phase)
        .bind(result.success)
        .bind(serde_json::to_value(&result.invariants)?)
        .bind(&result.evidence)
        .bind(&result.reason)
        .bind(snapshot_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn list_goal_evaluations(&self, project_id: Uuid, run_id: Uuid) -> DbResult<Vec<GoalEvaluationRow>> {
        Ok(sqlx::query_as::<_, GoalEvaluationRow>(
            "SELECT g.* FROM goal_evaluations g JOIN runs r ON r.id = g.run_id WHERE r.project_id = $1 AND g.run_id = $2 ORDER BY g.evaluated_at",
        )
        .bind(project_id)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn save_verification(&self, v: VerificationInsert<'_>) -> DbResult<VerificationResultRow> {
        Ok(sqlx::query_as::<_, VerificationResultRow>(
            r#"INSERT INTO verification_results (id, run_id, step_id, subject_type, subject_id, status, method, expected, actual,
                 evidence_source, evidence_version, evidence_timestamp, duration_ms, reason)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(v.run_id)
        .bind(v.step_id)
        .bind(v.subject_type)
        .bind(v.subject_id)
        .bind(v.status.as_str())
        .bind(v.method)
        .bind(v.expected)
        .bind(v.actual)
        .bind(v.evidence_source)
        .bind(v.evidence_version)
        .bind(v.evidence_timestamp)
        .bind(v.duration_ms)
        .bind(v.reason)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn list_verifications(&self, project_id: Uuid, run_id: Uuid) -> DbResult<Vec<VerificationResultRow>> {
        Ok(sqlx::query_as::<_, VerificationResultRow>(
            "SELECT v.* FROM verification_results v JOIN runs r ON r.id = v.run_id WHERE r.project_id = $1 AND v.run_id = $2 ORDER BY v.created_at",
        )
        .bind(project_id)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn save_model_usage(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        step_id: &str,
        model: &str,
        provider: Option<&str>,
        usage: Option<&TokenUsage>,
        latency_ms: Option<i64>,
        provider_response_id: Option<&str>,
        finish_reason: Option<&str>,
        cost: Option<(f64, &str)>,
    ) -> DbResult<ModelUsageRow> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, ModelUsageRow>(
            r#"INSERT INTO model_usage (id, run_id, project_id, step_id, model, provider, input_tokens, output_tokens, cached_tokens, latency_ms, provider_response_id, finish_reason)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(project_id)
        .bind(step_id)
        .bind(model)
        .bind(provider)
        .bind(usage.map(|u| u.input_tokens as i64))
        .bind(usage.map(|u| u.output_tokens as i64))
        .bind(usage.and_then(|u| u.cached_tokens.map(|c| c as i64)))
        .bind(latency_ms)
        .bind(provider_response_id)
        .bind(finish_reason)
        .fetch_one(&mut *tx)
        .await?;
        if let Some((amount, version)) = cost {
            sqlx::query(
                "INSERT INTO cost_records (id, run_id, project_id, model_usage_id, model, pricing_version, estimated_cost_usd) VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(Uuid::now_v7())
            .bind(run_id)
            .bind(project_id)
            .bind(row.id)
            .bind(model)
            .bind(version)
            .bind(amount)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(row)
    }

    pub async fn list_model_usage(&self, project_id: Uuid, run_id: Uuid) -> DbResult<Vec<ModelUsageRow>> {
        Ok(sqlx::query_as::<_, ModelUsageRow>("SELECT * FROM model_usage WHERE project_id = $1 AND run_id = $2 ORDER BY created_at")
            .bind(project_id)
            .bind(run_id)
            .fetch_all(&self.pool)
            .await?)
    }
}
