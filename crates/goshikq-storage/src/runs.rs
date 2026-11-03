//! Run lifecycle persistence: creation, status transitions (atomic with their events),
//! steps, checkpoints, and startup recovery marking.

use chrono::{DateTime, Utc};
use goshikq_protocol::{
    CreateRunRequest, DispatchState, EventStatus, EventType, ExecutionEvent, RunStatus,
    VerificationStatus,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::events::append_in_tx;
use crate::{sha256_hex, CheckpointRow, Db, DbResult, Page, RunRow, RunStepRow, StorageError};

#[derive(Debug, Default, Clone)]
pub struct RunFilter {
    pub status: Option<String>,
    pub verification_status: Option<String>,
    pub agent: Option<String>,
    pub framework: Option<String>,
    pub model: Option<String>,
    pub search: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub sort: Option<String>,
    pub order: Option<String>,
    pub project: Option<String>,
}

pub struct CreatedRun {
    pub run: RunRow,
    pub events: Vec<ExecutionEvent>,
    pub reused: bool,
}

impl Db {
    /// Create a run and its `run.created` / `goal.created` events atomically. Upserts the
    /// agent and a content-addressed agent version so Studio can show configuration drift.
    pub async fn create_run(
        &self,
        project_id: Uuid,
        req: &CreateRunRequest,
        trace_id: &str,
    ) -> DbResult<CreatedRun> {
        if let Some(key) = &req.idempotency_key {
            if let Some(existing) = sqlx::query_as::<_, RunRow>(
                "SELECT * FROM runs WHERE project_id = $1 AND idempotency_key = $2",
            )
            .bind(project_id)
            .bind(key)
            .fetch_optional(&self.pool)
            .await?
            {
                return Ok(CreatedRun {
                    run: existing,
                    events: vec![],
                    reused: true,
                });
            }
        }
        let mut tx = self.pool.begin().await?;

        let agent_id: Uuid = {
            let (id,): (Uuid,) = sqlx::query_as(
                r#"INSERT INTO agents (id, project_id, name, description, source_framework)
                   VALUES ($1,$2,$3,$4,$5)
                   ON CONFLICT (project_id, name) DO UPDATE SET description = EXCLUDED.description, updated_at = now()
                   RETURNING id"#,
            )
            .bind(Uuid::now_v7())
            .bind(project_id)
            .bind(&req.agent.name)
            .bind(&req.agent.description)
            .bind(&req.source_framework)
            .fetch_one(&mut *tx)
            .await?;
            id
        };

        let definition = serde_json::to_value(&req.agent)?;
        let config_hash = sha256_hex(serde_json::to_vec(&definition)?.as_slice());
        let agent_version_id: Uuid = {
            let existing: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM agent_versions WHERE agent_id = $1 AND config_hash = $2",
            )
            .bind(agent_id)
            .bind(&config_hash)
            .fetch_optional(&mut *tx)
            .await?;
            match existing {
                Some((id,)) => id,
                None => {
                    let (next,): (i32,) =
                        sqlx::query_as("SELECT COALESCE(MAX(version), 0) + 1 FROM agent_versions WHERE agent_id = $1")
                            .bind(agent_id)
                            .fetch_one(&mut *tx)
                            .await?;
                    let id = Uuid::now_v7();
                    sqlx::query(
                        "INSERT INTO agent_versions (id, agent_id, version, definition, config_hash, model) VALUES ($1,$2,$3,$4,$5,$6)",
                    )
                    .bind(id)
                    .bind(agent_id)
                    .bind(next)
                    .bind(&definition)
                    .bind(&config_hash)
                    .bind(&req.agent.model)
                    .execute(&mut *tx)
                    .await?;
                    id
                }
            }
        };

        let goal_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO goals (id, project_id, description, definition) VALUES ($1,$2,$3,$4)",
        )
        .bind(goal_id)
        .bind(project_id)
        .bind(&req.goal.description)
        .bind(serde_json::to_value(&req.goal)?)
        .execute(&mut *tx)
        .await?;

        let run_id = Uuid::now_v7();
        let full_def = serde_json::to_value(req)?;
        let correlation_id = req
            .correlation_id
            .clone()
            .or_else(|| req.parent_run_id.map(|p| p.to_string()))
            .unwrap_or_else(|| run_id.to_string());
        let workflow_id = req
            .metadata
            .get("workflow_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        let run = sqlx::query_as::<_, RunRow>(
            r#"INSERT INTO runs (id, project_id, agent_id, agent_version_id, agent_name, goal_id, goal_description,
                 workflow_id, trace_id, status, source_framework, model, input, definition, idempotency_key, metadata,
                 parent_run_id, task_id, correlation_id)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
               RETURNING *"#,
        )
        .bind(run_id)
        .bind(project_id)
        .bind(agent_id)
        .bind(agent_version_id)
        .bind(&req.agent.name)
        .bind(goal_id)
        .bind(&req.goal.description)
        .bind(workflow_id)
        .bind(trace_id)
        .bind(RunStatus::Created.as_str())
        .bind(&req.source_framework)
        .bind(&req.agent.model)
        .bind(&req.input)
        .bind(&full_def)
        .bind(&req.idempotency_key)
        .bind(&req.metadata)
        .bind(req.parent_run_id)
        .bind(req.task_id)
        .bind(&correlation_id)
        .fetch_one(&mut *tx)
        .await?;

        let root_span = goshikq_protocol::new_span_id();
        let created = ExecutionEvent::native(run_id, trace_id, EventType::RunCreated)
            .with_span(root_span.clone())
            .with_agent(req.agent.name.clone())
            .with_status(EventStatus::Info)
            .with_payload(json!({
                "agent": req.agent.name,
                "model": req.agent.model,
                "goal": req.goal.description,
                "source_framework": req.source_framework,
                "agent_version_id": agent_version_id,
                "limits": req.agent.execution_limits,
                "has_state_provider": req.has_state_provider,
            }));
        let goal_ev = ExecutionEvent::native(run_id, trace_id, EventType::GoalCreated)
            .with_parent(root_span.clone())
            .with_agent(req.agent.name.clone())
            .with_status(EventStatus::Info)
            .with_payload(json!({
                "goal_id": goal_id,
                "description": req.goal.description,
                "success_predicate": req.goal.success_predicate,
                "invariants": req.goal.invariants,
                "verification_policy": req.goal.verification_policy,
                "max_steps": req.goal.max_steps,
            }));
        let e1 = append_in_tx(&mut tx, project_id, created).await?;
        let e2 = append_in_tx(&mut tx, project_id, goal_ev).await?;
        tx.commit().await?;
        let mut run = run;
        run.metadata = json!({"root_span_id": root_span})
            .as_object()
            .map(|m| {
                let mut base = run.metadata.as_object().cloned().unwrap_or_default();
                base.extend(m.clone());
                Value::Object(base)
            })
            .unwrap_or(run.metadata);
        sqlx::query("UPDATE runs SET metadata = $2 WHERE id = $1")
            .bind(run_id)
            .bind(&run.metadata)
            .execute(&self.pool)
            .await?;
        Ok(CreatedRun {
            run,
            events: vec![e1, e2],
            reused: false,
        })
    }

    pub async fn get_run(&self, project_id: Uuid, run_id: Uuid) -> DbResult<RunRow> {
        sqlx::query_as::<_, RunRow>("SELECT * FROM runs WHERE project_id = $1 AND id = $2")
            .bind(project_id)
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("run {run_id}")))
    }

    pub async fn list_runs(
        &self,
        project_id: Uuid,
        f: &RunFilter,
        page: Page,
    ) -> DbResult<(Vec<RunRow>, i64)> {
        let search = f.search.as_ref().map(|s| format!("%{s}%"));
        let order_sql = whitelist_order(f.sort.as_deref(), f.order.as_deref());
        let select = format!(
            r#"SELECT * FROM runs WHERE project_id = $1
                 AND ($2::text IS NULL OR status = $2)
                 AND ($3::text IS NULL OR verification_status = $3)
                 AND ($4::text IS NULL OR agent_name = $4)
                 AND ($5::text IS NULL OR source_framework = $5)
                 AND ($6::text IS NULL OR model = $6)
                 AND ($7::text IS NULL OR goal_description ILIKE $7 OR agent_name ILIKE $7 OR id::text ILIKE $7)
                 AND ($8::timestamptz IS NULL OR created_at >= $8)
                 AND ($9::timestamptz IS NULL OR created_at <= $9)
                 AND ($10::text IS NULL OR project_id::text ILIKE $10)
               ORDER BY {order_sql} LIMIT $11 OFFSET $12"#
        );
        let project_pat = f.project.as_ref().map(|s| format!("%{s}%"));
        let rows = sqlx::query_as::<_, RunRow>(&select)
            .bind(project_id)
            .bind(&f.status)
            .bind(&f.verification_status)
            .bind(&f.agent)
            .bind(&f.framework)
            .bind(&f.model)
            .bind(&search)
            .bind(f.from)
            .bind(f.to)
            .bind(&project_pat)
            .bind(page.limit)
            .bind(page.offset)
            .fetch_all(&self.pool)
            .await?;
        let (total,): (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM runs WHERE project_id = $1
                 AND ($2::text IS NULL OR status = $2)
                 AND ($3::text IS NULL OR verification_status = $3)
                 AND ($4::text IS NULL OR agent_name = $4)
                 AND ($5::text IS NULL OR source_framework = $5)
                 AND ($6::text IS NULL OR model = $6)
                 AND ($7::text IS NULL OR goal_description ILIKE $7 OR agent_name ILIKE $7 OR id::text ILIKE $7)
                 AND ($8::timestamptz IS NULL OR created_at >= $8)
                 AND ($9::timestamptz IS NULL OR created_at <= $9)
                 AND ($10::text IS NULL OR project_id::text ILIKE $10)"#,
        )
        .bind(project_id)
        .bind(&f.status)
        .bind(&f.verification_status)
        .bind(&f.agent)
        .bind(&f.framework)
        .bind(&f.model)
        .bind(&search)
        .bind(f.from)
        .bind(f.to)
        .bind(&project_pat)
        .fetch_one(&self.pool)
        .await?;
        Ok((rows, total))
    }

    /// Persist a status transition and the events describing it in one transaction.
    /// The caller is responsible for having validated the transition (`goshikq-core`).
    pub async fn transition_run(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        to: RunStatus,
        failure_reason: Option<&str>,
        events: Vec<ExecutionEvent>,
    ) -> DbResult<Vec<ExecutionEvent>> {
        self.transition_run_inner(project_id, run_id, to, failure_reason, events, None)
            .await
    }

    /// Same as [`Self::transition_run`], but refuses the write unless this lease generation still owns the run.
    pub async fn transition_run_leased(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        to: RunStatus,
        failure_reason: Option<&str>,
        events: Vec<ExecutionEvent>,
        owner: &str,
        generation: i64,
    ) -> DbResult<Vec<ExecutionEvent>> {
        self.transition_run_inner(
            project_id,
            run_id,
            to,
            failure_reason,
            events,
            Some((owner, generation)),
        )
        .await
    }

    async fn transition_run_inner(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        to: RunStatus,
        failure_reason: Option<&str>,
        events: Vec<ExecutionEvent>,
        lease: Option<(&str, i64)>,
    ) -> DbResult<Vec<ExecutionEvent>> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();
        let terminal = to.is_terminal();
        let (lease_owner, lease_generation): (Option<&str>, i64) = match lease {
            Some((o, g)) => (Some(o), g),
            None => (None, 0),
        };
        let result = sqlx::query(
            r#"UPDATE runs SET status = $2, updated_at = $3,
                 failure_reason = CASE
                    WHEN $2 IN ('COMPLETED') THEN NULL
                    WHEN $4::text IS NOT NULL THEN $4
                    ELSE failure_reason
                 END,
                 started_at = CASE WHEN started_at IS NULL AND $2 <> 'CREATED' THEN $3 ELSE started_at END,
                 ended_at = CASE WHEN $5 THEN $3 ELSE ended_at END,
                 duration_ms = CASE WHEN $5 THEN EXTRACT(EPOCH FROM ($3 - COALESCE(started_at, created_at))) * 1000 ELSE duration_ms END
               WHERE id = $1 AND project_id = $6
                 AND ($7::text IS NULL OR (lease_owner = $7 AND lease_generation = $8 AND (lease_expires_at IS NULL OR lease_expires_at > now())))"#,
        )
        .bind(run_id)
        .bind(to.as_str())
        .bind(now)
        .bind(failure_reason)
        .bind(terminal)
        .bind(project_id)
        .bind(lease_owner)
        .bind(lease_generation)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            return Err(StorageError::Conflict(format!(
                "run {run_id} transition to {} rejected (missing row or fenced lease)",
                to.as_str()
            )));
        }
        let mut out = Vec::with_capacity(events.len());
        for e in events {
            out.push(append_in_tx(&mut tx, project_id, e).await?);
        }
        tx.commit().await?;
        Ok(out)
    }

    pub async fn set_run_verification(
        &self,
        run_id: Uuid,
        status: VerificationStatus,
    ) -> DbResult<()> {
        sqlx::query("UPDATE runs SET verification_status = $2, updated_at = now() WHERE id = $1")
            .bind(run_id)
            .bind(status.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_final_output(&self, run_id: Uuid, output: Option<&str>) -> DbResult<()> {
        sqlx::query("UPDATE runs SET final_output = $2, updated_at = now() WHERE id = $1")
            .bind(run_id)
            .bind(output)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Roll up token usage and estimated cost from `model_usage` / `cost_records`.
    /// `usage_known` is true only if every model call reported usage; `cost_known` only
    /// if every call was priced. Unknown is never rendered as zero.
    pub async fn recompute_run_usage(&self, run_id: Uuid) -> DbResult<()> {
        sqlx::query(
            r#"UPDATE runs r SET
                 input_tokens  = COALESCE((SELECT SUM(input_tokens)  FROM model_usage WHERE run_id = $1), 0),
                 output_tokens = COALESCE((SELECT SUM(output_tokens) FROM model_usage WHERE run_id = $1), 0),
                 cached_tokens = COALESCE((SELECT SUM(cached_tokens) FROM model_usage WHERE run_id = $1), 0),
                 usage_known   = EXISTS (SELECT 1 FROM model_usage WHERE run_id = $1)
                                 AND NOT EXISTS (SELECT 1 FROM model_usage WHERE run_id = $1 AND input_tokens IS NULL),
                 cost_known    = EXISTS (SELECT 1 FROM model_usage WHERE run_id = $1)
                                 AND NOT EXISTS (SELECT 1 FROM model_usage mu WHERE mu.run_id = $1
                                                 AND NOT EXISTS (SELECT 1 FROM cost_records c WHERE c.model_usage_id = mu.id)),
                 estimated_cost_usd = (SELECT SUM(estimated_cost_usd) FROM cost_records WHERE run_id = $1),
                 updated_at = now()
               WHERE r.id = $1"#,
        )
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_step_count(&self, run_id: Uuid, steps: i32) -> DbResult<()> {
        sqlx::query("UPDATE runs SET step_count = $2, updated_at = now() WHERE id = $1")
            .bind(run_id)
            .bind(steps)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn upsert_step(
        &self,
        run_id: Uuid,
        step_number: i32,
        phase: &str,
        status: &str,
        summary: Value,
        started_at: DateTime<Utc>,
        ended_at: Option<DateTime<Utc>>,
    ) -> DbResult<()> {
        let duration = ended_at.map(|e| (e - started_at).num_milliseconds());
        sqlx::query(
            r#"INSERT INTO run_steps (id, run_id, step_number, phase, status, summary, started_at, ended_at, duration_ms)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
               ON CONFLICT (run_id, step_number) DO UPDATE SET phase = EXCLUDED.phase, status = EXCLUDED.status,
                 summary = EXCLUDED.summary, ended_at = EXCLUDED.ended_at, duration_ms = EXCLUDED.duration_ms"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(step_number)
        .bind(phase)
        .bind(status)
        .bind(summary)
        .bind(started_at)
        .bind(ended_at)
        .bind(duration)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_steps(&self, project_id: Uuid, run_id: Uuid) -> DbResult<Vec<RunStepRow>> {
        Ok(sqlx::query_as::<_, RunStepRow>(
            "SELECT s.* FROM run_steps s JOIN runs r ON r.id = s.run_id WHERE r.project_id = $1 AND s.run_id = $2 ORDER BY s.step_number",
        )
        .bind(project_id)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn save_checkpoint(
        &self,
        run_id: Uuid,
        step_number: i32,
        status: RunStatus,
        data: Value,
    ) -> DbResult<()> {
        sqlx::query(
            r#"INSERT INTO checkpoints (id, run_id, step_number, status, data, schema_version, last_event_sequence, recovery_generation)
               SELECT $1, $2, $3, $4,
                      jsonb_set($5::jsonb, '{last_event_sequence}', to_jsonb(GREATEST(r.next_sequence - 1, 0))),
                      2, GREATEST(r.next_sequence - 1, 0), r.recovery_generation
               FROM runs r WHERE r.id = $2"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(step_number)
        .bind(status.as_str())
        .bind(data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn latest_checkpoint(&self, run_id: Uuid) -> DbResult<Option<CheckpointRow>> {
        Ok(sqlx::query_as::<_, CheckpointRow>(
            "SELECT * FROM checkpoints WHERE run_id = $1 ORDER BY step_number DESC, created_at DESC LIMIT 1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Startup recovery (ADR-003): native runs left active by a previous process are
    /// marked `RECOVERY_REQUIRED`. Nothing is re-executed. Returns affected run ids.
    pub async fn mark_orphaned_runs(&self) -> DbResult<Vec<(Uuid, Uuid, String)>> {
        // Durable waits (approval, external settlement, pause) survive restart without
        // RECOVERY_REQUIRED. The original write is not re-dispatched.
        let active: Vec<&str> = [
            RunStatus::Created,
            RunStatus::Planning,
            RunStatus::ValidatingAction,
            RunStatus::Executing,
            RunStatus::ReconcilingState,
            RunStatus::VerifyingGoal,
            RunStatus::Replanning,
        ]
        .iter()
        .map(|s| s.as_str())
        .collect();
        let active: Vec<String> = active.into_iter().map(String::from).collect();
        let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
            r#"SELECT id, project_id, trace_id FROM runs
               WHERE (source_framework LIKE 'goshikq%' OR source_framework LIKE 'axiom%')
                 AND status = ANY($1)
                 AND NOT (status = 'CREATED' AND COALESCE(definition->>'defer_start','false') = 'true')"#,
        )
        .bind(&active)
        .fetch_all(&self.pool)
        .await?;
        for (run_id, project_id, trace_id) in &rows {
            // In-flight tool rows become DISPATCH_UNCERTAIN: the request may have left this process.
            sqlx::query(
                r#"UPDATE tool_invocations SET dispatch_state = $2
                   WHERE run_id = $1 AND dispatch_state IN ('PREPARED') AND status = 'RUNNING'"#,
            )
            .bind(run_id)
            .bind(DispatchState::DispatchUncertain.as_str())
            .execute(&self.pool)
            .await?;
            sqlx::query(
                r#"UPDATE tool_invocations SET dispatch_state = $2
                   WHERE run_id = $1 AND status = 'RUNNING' AND dispatch_state NOT IN ('ACCEPTED','OUTCOME_PENDING','OUTCOME_CONFIRMED')"#,
            )
            .bind(run_id)
            .bind(DispatchState::DispatchUncertain.as_str())
            .execute(&self.pool)
            .await?;
            let ev = ExecutionEvent::native(*run_id, trace_id.clone(), EventType::RecoveryStarted)
                .with_status(EventStatus::Pending)
                .with_payload(json!({"reason": "runtime_restarted", "note": "run left active by a previous runtime process; no action re-executed"}));
            self.transition_run(
                *project_id,
                *run_id,
                RunStatus::RecoveryRequired,
                Some("runtime_restarted"),
                vec![ev],
            )
            .await?;
        }
        Ok(rows)
    }

    /// Delete terminal runs older than `older_than` that are not parents of still-active children.
    /// Active / recovery / paused runs are never deleted. `only_run_ids` further restricts the set.
    pub async fn purge_terminal_runs(
        &self,
        older_than: DateTime<Utc>,
        only_run_ids: Option<Vec<Uuid>>,
        dry_run: bool,
    ) -> DbResult<RetentionPurgeResult> {
        let skipped: i64 = sqlx::query_scalar(
            r#"SELECT count(*)::bigint FROM runs r
               WHERE r.status IN ('COMPLETED','FAILED','CANCELLED','UNVERIFIED','TIMED_OUT','BUDGET_EXCEEDED','STEP_LIMIT_EXCEEDED','MODEL_BUDGET_EXCEEDED')
                 AND r.ended_at IS NOT NULL
                 AND r.ended_at <= $1
                 AND ($2::uuid[] IS NULL OR r.id = ANY($2))
                 AND EXISTS (
                   SELECT 1 FROM runs c
                   WHERE c.parent_run_id = r.id
                     AND c.status NOT IN ('COMPLETED','FAILED','CANCELLED','UNVERIFIED','TIMED_OUT','BUDGET_EXCEEDED','STEP_LIMIT_EXCEEDED','MODEL_BUDGET_EXCEEDED')
                 )"#,
        )
        .bind(older_than)
        .bind(only_run_ids.as_deref())
        .fetch_one(&self.pool)
        .await?;
        let ids: Vec<Uuid> = sqlx::query_scalar(
            r#"SELECT r.id FROM runs r
               WHERE r.status IN ('COMPLETED','FAILED','CANCELLED','UNVERIFIED','TIMED_OUT','BUDGET_EXCEEDED','STEP_LIMIT_EXCEEDED','MODEL_BUDGET_EXCEEDED')
                 AND r.ended_at IS NOT NULL
                 AND r.ended_at <= $1
                 AND ($2::uuid[] IS NULL OR r.id = ANY($2))
                 AND NOT EXISTS (
                   SELECT 1 FROM runs c
                   WHERE c.parent_run_id = r.id
                     AND c.status NOT IN ('COMPLETED','FAILED','CANCELLED','UNVERIFIED','TIMED_OUT','BUDGET_EXCEEDED','STEP_LIMIT_EXCEEDED','MODEL_BUDGET_EXCEEDED')
                 )"#,
        )
        .bind(older_than)
        .bind(only_run_ids.as_deref())
        .fetch_all(&self.pool)
        .await?;
        if dry_run || ids.is_empty() {
            return Ok(RetentionPurgeResult {
                deleted: 0,
                run_ids: ids,
                skipped_active_parents: skipped.max(0) as u64,
            });
        }
        let deleted: Vec<Uuid> =
            sqlx::query_scalar("DELETE FROM runs WHERE id = ANY($1) RETURNING id")
                .bind(&ids)
                .fetch_all(&self.pool)
                .await?;
        Ok(RetentionPurgeResult {
            deleted: deleted.len() as u64,
            run_ids: deleted,
            skipped_active_parents: skipped.max(0) as u64,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RetentionPurgeResult {
    pub deleted: u64,
    pub run_ids: Vec<Uuid>,
    pub skipped_active_parents: u64,
}

fn whitelist_order(sort: Option<&str>, order: Option<&str>) -> &'static str {
    let asc = matches!(order, Some("asc"));
    match (sort, asc) {
        (Some("started_at"), true) => "started_at ASC NULLS LAST, created_at ASC",
        (Some("started_at"), false) => "started_at DESC NULLS LAST, created_at DESC",
        (Some("duration_ms"), true) => "duration_ms ASC NULLS LAST, created_at DESC",
        (Some("duration_ms"), false) => "duration_ms DESC NULLS LAST, created_at DESC",
        (Some("status"), true) => "status ASC, created_at DESC",
        (Some("status"), false) => "status DESC, created_at DESC",
        (Some("verification_status"), true) => "verification_status ASC, created_at DESC",
        (Some("verification_status"), false) => "verification_status DESC, created_at DESC",
        (Some("agent_name"), true) => "agent_name ASC, created_at DESC",
        (Some("agent_name"), false) => "agent_name DESC, created_at DESC",
        (Some("source_framework"), true) => "source_framework ASC, created_at DESC",
        (Some("source_framework"), false) => "source_framework DESC, created_at DESC",
        (Some("model"), true) => "model ASC, created_at DESC",
        (Some("model"), false) => "model DESC, created_at DESC",
        (Some("project_id"), true) => "project_id ASC, created_at DESC",
        (Some("project_id"), false) => "project_id DESC, created_at DESC",
        (Some("id"), true) => "id ASC",
        (Some("id"), false) => "id DESC",
        (Some("goal"), true) => "goal_description ASC, created_at DESC",
        (Some("goal"), false) => "goal_description DESC, created_at DESC",
        (Some("tokens"), true) => "(input_tokens + output_tokens) ASC, created_at DESC",
        (Some("tokens"), false) => "(input_tokens + output_tokens) DESC, created_at DESC",
        (Some("cost"), true) => "estimated_cost_usd ASC NULLS LAST, created_at DESC",
        (Some("cost"), false) => "estimated_cost_usd DESC NULLS LAST, created_at DESC",
        (_, true) => "created_at ASC",
        (_, false) => "created_at DESC",
    }
}
