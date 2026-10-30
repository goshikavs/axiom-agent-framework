//! Native multi-agent: child task creation, loop bounds, handoff, shared context.

use std::sync::Arc;

use axiom_protocol::{
    CoordinationPolicy, CreateRunRequest, CreateTaskRequest, EffectCategory, EventStatus, EventType, ExecutionEvent,
    HandoffRequest, RiskLevel, SharedContextPut, ToolSpec,
};
use axiom_storage::{RunRow, RunTaskRow, SharedContextHistoryRow, SharedContextRow};
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::runtime::{Runtime, RuntimeError};

pub struct CreatedTask {
    pub parent: RunRow,
    pub child: RunRow,
    pub task: RunTaskRow,
    pub reused: bool,
    pub approval_id: Option<Uuid>,
}

impl Runtime {
    pub async fn create_child_task(
        self: &Arc<Self>,
        project_id: Uuid,
        parent_run_id: Uuid,
        req: CreateTaskRequest,
        actor: &str,
    ) -> Result<CreatedTask, RuntimeError> {
        let parent = self.db.get_run(project_id, parent_run_id).await?;
        let policy = self.coordination_policy(project_id, &parent).await?;
        let (parent_depth, ancestor_ids, ancestor_names) = self.db.ancestor_chain(project_id, parent_run_id).await?;
        let child_depth = parent_depth + 1;
        if child_depth > policy.max_depth {
            return Err(RuntimeError::Invalid(format!(
                "delegation depth {child_depth} exceeds max_depth {}",
                policy.max_depth
            )));
        }
        if policy.prevent_cycles && ancestor_names.iter().any(|n| n == &req.agent.name) {
            return Err(RuntimeError::Invalid(format!(
                "cycle rejected: agent `{}` already appears in the ancestor chain",
                req.agent.name
            )));
        }
        let existing = self.db.count_child_runs(project_id, parent_run_id).await?;
        if existing as u32 >= policy.max_delegations {
            return Err(RuntimeError::Invalid(format!(
                "parent already has {existing} children; max_delegations is {}",
                policy.max_delegations
            )));
        }

        let task_id = Uuid::now_v7();
        let correlation_id = parent
            .correlation_id
            .clone()
            .unwrap_or_else(|| parent.id.to_string());
        let hold = req.requires_approval || req.defer_start;
        let create = CreateRunRequest {
            project_id: Some(project_id),
            agent: req.agent.clone(),
            goal: req.goal.clone(),
            input: req.input.clone(),
            has_state_provider: req.has_state_provider,
            source_framework: req.source_framework.clone(),
            idempotency_key: req.idempotency_key.clone(),
            metadata: merge_meta(&req.metadata, json!({
                "role": req.role,
                "parent_run_id": parent_run_id,
                "task_id": task_id,
                "correlation_id": correlation_id,
            })),
            defer_start: hold,
            parent_run_id: Some(parent_run_id),
            task_id: Some(task_id),
            correlation_id: Some(correlation_id.clone()),
            trace_id: Some(parent.trace_id.clone()),
        };

        let (child, reused) = self.create_run(project_id, create).await?;
        if reused {
            let task = self
                .db
                .get_run_task_by_child(project_id, child.id)
                .await?
                .ok_or_else(|| RuntimeError::Invalid("idempotent child is missing a run_tasks row".into()))?;
            let approval_id = task.approval_id;
            return Ok(CreatedTask { parent, child, task, reused: true, approval_id });
        }

        let mut task = RunTaskRow {
            id: Uuid::now_v7(),
            project_id,
            parent_run_id,
            child_run_id: child.id,
            task_id,
            correlation_id: correlation_id.clone(),
            role: req.role.clone(),
            status: child.status.clone(),
            verification_status: child.verification_status.clone(),
            requires_approval: req.requires_approval,
            approval_id: None,
            depth: child_depth as i32,
            metadata: req.metadata.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let mut approval_id = None;
        if req.requires_approval {
            let spec = ToolSpec {
                name: "delegate".into(),
                description: "Human approval required before a child agent may start".into(),
                input_schema: json!({"type": "object"}),
                output_schema: None,
                effect: EffectCategory::IdempotentWrite,
                risk: RiskLevel::High,
                timeout_ms: 30_000,
                retry_policy: Default::default(),
                idempotent: true,
                requires_approval: true,
                required_permissions: vec![],
                preconditions: vec![],
                has_verifier: false,
                revalidation_policy: Default::default(),
                evidence_coverage: None,
            };
            let args = json!({
                "child_run_id": child.id,
                "task_id": task_id,
                "agent": req.agent.name,
                "goal": req.goal.description,
            });
            let inv = self
                .db
                .create_tool_invocation(
                    project_id,
                    parent_run_id,
                    "delegate",
                    &format!("delegate-{task_id}"),
                    &spec,
                    &args,
                    &format!("delegate:{task_id}"),
                )
                .await?;
            let approval = self
                .db
                .create_approval(
                    project_id,
                    parent_run_id,
                    inv.id,
                    &parent.agent_name,
                    "delegate",
                    &args,
                    "high",
                    "child task requires human approval before start",
                    &[],
                    Utc::now() + ChronoDuration::hours(1),
                )
                .await?;
            approval_id = Some(approval.id);
            task.approval_id = approval_id;
            let ev = ExecutionEvent::native(parent_run_id, parent.trace_id.clone(), EventType::ApprovalRequested)
                .with_status(EventStatus::Pending)
                .with_payload(json!({"approval_id": approval.id, "child_run_id": child.id, "task_id": task_id}));
            let _ = self.db.append_events(project_id, vec![ev]).await;
        }

        let task = self.db.insert_run_task(&task).await?;
        let delegated = ExecutionEvent::native(parent_run_id, parent.trace_id.clone(), EventType::AgentDelegated)
            .with_agent(parent.agent_name.clone())
            .with_status(EventStatus::Info)
            .with_payload(json!({
                "parent_run_id": parent_run_id,
                "child_run_id": child.id,
                "task_id": task_id,
                "correlation_id": correlation_id,
                "child_agent": req.agent.name,
                "depth": child_depth,
                "requires_approval": req.requires_approval,
                "ancestor_ids": ancestor_ids,
                "actor": actor,
            }));
        let received = ExecutionEvent::native(child.id, parent.trace_id.clone(), EventType::AgentMessageReceived)
            .with_agent(req.agent.name.clone())
            .with_status(EventStatus::Info)
            .with_payload(json!({
                "parent_run_id": parent_run_id,
                "task_id": task_id,
                "correlation_id": correlation_id,
            }));
        let _ = self.db.append_events(project_id, vec![delegated, received]).await;
        self.wake_publisher();
        Ok(CreatedTask { parent, child, task, reused: false, approval_id })
    }

    pub async fn handoff(
        self: &Arc<Self>,
        project_id: Uuid,
        source_run_id: Uuid,
        req: HandoffRequest,
        actor: &str,
    ) -> Result<CreatedTask, RuntimeError> {
        let source = self.db.get_run(project_id, source_run_id).await?;
        let parent_id = source.parent_run_id.unwrap_or(source_run_id);
        let created = self
            .create_child_task(
                project_id,
                parent_id,
                CreateTaskRequest {
                    agent: req.agent,
                    goal: req.goal,
                    input: req.input,
                    has_state_provider: req.has_state_provider,
                    source_framework: req.source_framework,
                    idempotency_key: req.idempotency_key,
                    metadata: req.metadata,
                    role: "handoff".into(),
                    requires_approval: false,
                    defer_start: false,
                },
                actor,
            )
            .await?;
        let ev = ExecutionEvent::native(source_run_id, source.trace_id.clone(), EventType::AgentHandoffCompleted)
            .with_agent(source.agent_name.clone())
            .with_status(EventStatus::Succeeded)
            .with_payload(json!({
                "from_run_id": source_run_id,
                "to_run_id": created.child.id,
                "task_id": created.task.task_id,
                "actor": actor,
            }));
        let _ = self.db.append_events(project_id, vec![ev]).await;
        if req.cancel_source {
            let _ = self.cancel(project_id, source_run_id, actor).await;
        }
        Ok(created)
    }

    pub async fn put_shared(
        &self,
        project_id: Uuid,
        scope_id: Uuid,
        body: SharedContextPut,
        actor: &str,
    ) -> Result<SharedContextRow, RuntimeError> {
        if body.key.trim().is_empty() {
            return Err(RuntimeError::Invalid("shared context key is required".into()));
        }
        if !matches!(body.visibility.as_str(), "private" | "team" | "public") {
            return Err(RuntimeError::Invalid("visibility must be private|team|public".into()));
        }
        let refs = serde_json::to_value(&body.evidence_refs).unwrap_or_else(|_| json!([]));
        let run = self.db.get_run(project_id, scope_id).await?;
        let row = self
            .db
            .put_shared_context(
                project_id,
                scope_id,
                body.key.trim(),
                &body.value,
                body.expected_version,
                &body.owner_agent,
                &body.visibility,
                &refs,
                actor,
            )
            .await?;
        let ev = ExecutionEvent::native(scope_id, run.trace_id, EventType::StateChanged)
            .with_status(EventStatus::Info)
            .with_payload(json!({
                "shared_context_key": row.key,
                "version": row.version,
                "owner_agent": row.owner_agent,
                "evidence_refs": row.evidence_refs,
                "note": "shared context is versioned business state, not natural-language memory",
            }));
        let _ = self.db.append_events(project_id, vec![ev]).await;
        self.wake_publisher();
        Ok(row)
    }

    pub async fn list_shared(&self, project_id: Uuid, scope_id: Uuid) -> Result<Vec<SharedContextRow>, RuntimeError> {
        self.db.get_run(project_id, scope_id).await?;
        Ok(self.db.list_shared_context(project_id, scope_id).await?)
    }

    pub async fn shared_history(
        &self,
        project_id: Uuid,
        scope_id: Uuid,
        key: Option<&str>,
    ) -> Result<Vec<SharedContextHistoryRow>, RuntimeError> {
        self.db.get_run(project_id, scope_id).await?;
        Ok(self.db.list_shared_history(project_id, scope_id, key).await?)
    }

    async fn coordination_policy(&self, project_id: Uuid, parent: &RunRow) -> Result<CoordinationPolicy, RuntimeError> {
        let mut cursor = parent.clone();
        loop {
            if let Some(p) = policy_from_metadata(&cursor.metadata) {
                return Ok(p);
            }
            match cursor.parent_run_id {
                Some(pid) => cursor = self.db.get_run(project_id, pid).await?,
                None => return Ok(CoordinationPolicy::default()),
            }
        }
    }
}

fn policy_from_metadata(meta: &serde_json::Value) -> Option<CoordinationPolicy> {
    meta.get("coordination")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
}

fn merge_meta(base: &serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    let mut out = if base.is_object() { base.clone() } else { json!({}) };
    if let (Some(o), Some(e)) = (out.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    out
}
