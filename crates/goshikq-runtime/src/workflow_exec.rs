//! Graph walker that compiles a pinned workflow onto the existing run executor.
//! Studio never schedules; this module is the only workflow driver.

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use goshikq_core::actions::ActionRegistry;
use goshikq_core::workflow::{
    apply_mapping, eval_condition, pointer_get, WorkflowDefinition, WorkflowNode, WorkflowNodeKind,
};
use goshikq_protocol::{
    AgentDefinition, ApprovalStatus, CreateTaskRequest, EffectCategory, EventStatus, EventType,
    ExecutionLimits, GoalDefinition, RiskLevel, RunStatus, ToolSpec, VerificationPolicy,
    VerificationStatus,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::executor::{RunEnd, RunExecutor};

pub(crate) fn pinned_definition(metadata: &Value) -> Option<WorkflowDefinition> {
    let raw = metadata
        .get("workflow")
        .and_then(|w| w.get("definition"))
        .or_else(|| metadata.get("workflow_definition"))?;
    serde_json::from_value(raw.clone()).ok()
}

impl RunExecutor {
    pub(crate) async fn execute_workflow(
        &mut self,
        def: WorkflowDefinition,
    ) -> Result<RunEnd, RunEnd> {
        let issues = def.validate();
        if !issues.is_empty() {
            return Err(RunEnd::Failed(format!(
                "pinned workflow is invalid: {}",
                issues[0].message
            )));
        }
        let workflow_id = self
            .req
            .metadata
            .get("workflow")
            .and_then(|w| w.get("id"))
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .or(self.workflow_id)
            .unwrap_or(self.run_id);
        let workflow_version = self
            .req
            .metadata
            .get("workflow")
            .and_then(|w| w.get("version"))
            .and_then(|v| v.as_i64())
            .unwrap_or(def.version as i64) as i32;

        self.ensure_registered_tools(&def)?;
        if self.status == RunStatus::Created {
            let started = self
                .event(EventType::RunStarted)
                .with_status(EventStatus::Started)
                .with_payload(json!({
                    "workflow_id": workflow_id,
                    "workflow_version": workflow_version,
                    "pinned": true,
                    "node_count": def.nodes.len(),
                }));
            if !self
                .transition(RunStatus::Planning, None, vec![started])
                .await
            {
                return Err(RunEnd::Failed("could not persist workflow start".into()));
            }
        }
        // Durable waits must reattach in-place. Forcing EXECUTING here would drop
        // WAITING_FOR_APPROVAL / WAITING_FOR_EXTERNAL_STATE before the node walker
        // re-enters the waiter, and recovery after process restart would fail.
        if !matches!(
            self.status,
            RunStatus::WaitingForApproval | RunStatus::WaitingForExternalState
        ) && !self.enter_executing().await
        {
            return Err(RunEnd::Failed(
                "could not enter EXECUTING for workflow".into(),
            ));
        }

        let mut states = load_states(&self.rt.db, self.project_id, self.run_id).await;
        let mut context = rebuild_context(&self.req.input, &states);
        let mut iterations: HashMap<String, u32> = HashMap::new();
        let mut executed = 0u32;

        loop {
            if self.handle.cancel.is_cancelled() {
                return Err(RunEnd::Cancelled);
            }
            if Utc::now() >= self.execution_deadline_at {
                return Err(RunEnd::TimedOut);
            }
            if executed >= def.max_steps {
                return Err(RunEnd::StepLimitExceeded(format!(
                    "workflow max_steps ({}) exhausted",
                    def.max_steps
                )));
            }

            let ready = ready_nodes(&def, &states);
            if ready.is_empty() {
                if states.values().any(|s| s.status == "waiting") {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
                if def.nodes.iter().any(|n| {
                    n.kind == WorkflowNodeKind::End
                        && states.get(&n.id).map(|s| s.status.as_str()) == Some("completed")
                }) {
                    break;
                }
                return Err(RunEnd::Failed(
                    "workflow stalled with no ready nodes".into(),
                ));
            }

            let mut progressed = false;
            for node in ready {
                if node.kind == WorkflowNodeKind::Join && !join_ready(&def, node, &states) {
                    continue;
                }
                let count = iterations.entry(node.id.clone()).or_insert(0);
                *count += 1;
                if let Some(max) = node.max_iterations {
                    if *count > max {
                        return Err(RunEnd::Failed(format!(
                            "node `{}` exceeded max_iterations {max}",
                            node.id
                        )));
                    }
                }
                executed += 1;
                self.step = executed;
                self.step_span = goshikq_protocol::new_span_id();
                match self
                    .execute_node(
                        &def,
                        node,
                        workflow_id,
                        workflow_version,
                        &mut context,
                        &mut states,
                    )
                    .await
                {
                    Ok(()) => progressed = true,
                    Err(RunEnd::Cancelled) => return Err(RunEnd::Cancelled),
                    Err(other) => {
                        if def.error_policy.on_error == "continue" {
                            self.rt
                                .metrics
                                .workflow_nodes_failed
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            states.insert(
                                node.id.clone(),
                                NodeState {
                                    status: "failed".into(),
                                    output: None,
                                    child_run_id: None,
                                    approval_id: None,
                                },
                            );
                            let _ = persist_node(
                                &self.rt.db,
                                self.project_id,
                                self.run_id,
                                workflow_id,
                                workflow_version,
                                node,
                                "failed",
                                &context,
                                None,
                                Some(&format!("{other:?}")),
                                None,
                                None,
                                None,
                            )
                            .await;
                            progressed = true;
                        } else {
                            return Err(other);
                        }
                    }
                }
                self.checkpoint().await;
            }
            if !progressed {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        self.final_output = Some(context.to_string());
        if def.goal.required || def.nodes.iter().any(|n| n.kind == WorkflowNodeKind::Verify) {
            if states.iter().any(|(id, st)| {
                def.node(id).map(|n| n.kind) == Some(WorkflowNodeKind::Verify)
                    && st.status == "failed"
            }) {
                return Err(RunEnd::Failed("workflow verification failed".into()));
            }
        }
        if self.status == RunStatus::Executing
            && !self
                .transition(RunStatus::VerifyingGoal, None, vec![])
                .await
        {
            return Err(RunEnd::Failed("could not enter VERIFYING_GOAL".into()));
        }
        Ok(RunEnd::Completed)
    }

    async fn enter_executing(&mut self) -> bool {
        match self.status {
            RunStatus::Created => false,
            RunStatus::Planning => {
                self.transition(RunStatus::ValidatingAction, None, vec![])
                    .await
                    && self.transition(RunStatus::Executing, None, vec![]).await
            }
            RunStatus::ValidatingAction
            | RunStatus::WaitingForApproval
            | RunStatus::Paused
            | RunStatus::RecoveryRequired => {
                self.transition(RunStatus::Executing, None, vec![]).await
            }
            RunStatus::WaitingForExternalState => {
                self.transition(RunStatus::Paused, None, vec![]).await
                    && self.transition(RunStatus::Executing, None, vec![]).await
            }
            RunStatus::Executing => true,
            _ => true,
        }
    }

    async fn enter_approval_wait(&mut self, ev: goshikq_protocol::ExecutionEvent) -> bool {
        if self.status == RunStatus::Executing
            && !self
                .transition(RunStatus::ReconcilingState, None, vec![])
                .await
        {
            return false;
        }
        if self.status == RunStatus::ReconcilingState
            && !self
                .transition(RunStatus::ValidatingAction, None, vec![])
                .await
        {
            return false;
        }
        if self.status == RunStatus::Planning
            && !self
                .transition(RunStatus::ValidatingAction, None, vec![])
                .await
        {
            return false;
        }
        self.transition(RunStatus::WaitingForApproval, None, vec![ev])
            .await
    }

    async fn execute_node(
        &mut self,
        def: &WorkflowDefinition,
        node: &WorkflowNode,
        workflow_id: Uuid,
        workflow_version: i32,
        context: &mut Value,
        states: &mut HashMap<String, NodeState>,
    ) -> Result<(), RunEnd> {
        let existing = self
            .rt
            .db
            .get_workflow_node(self.project_id, self.run_id, &node.id)
            .await
            .ok()
            .flatten();
        if existing.as_ref().map(|r| r.status.as_str()) == Some("completed") {
            if let Some(out) = existing.and_then(|r| r.output) {
                merge_node_output(context, &node.id, &out);
            }
            states.insert(
                node.id.clone(),
                NodeState {
                    status: "completed".into(),
                    output: None,
                    child_run_id: None,
                    approval_id: None,
                },
            );
            return Ok(());
        }
        if existing.as_ref().map(|r| r.status.as_str()) == Some("running") {
            let effect = node
                .config
                .get("effect")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if matches!(effect, "NON_IDEMPOTENT_WRITE" | "IRREVERSIBLE") {
                return Err(RunEnd::RecoveryRequired(format!(
                    "node `{}` was running a non-idempotent write at crash; operator reconciliation required",
                    node.id
                )));
            }
        }

        let mut input = json!({});
        for edge in def.incoming(&node.id) {
            apply_mapping(context, &mut input, &edge.mapping);
        }
        if input == json!({}) {
            input = context.clone();
        }

        let _ = persist_node(
            &self.rt.db,
            self.project_id,
            self.run_id,
            workflow_id,
            workflow_version,
            node,
            "running",
            &input,
            None,
            None,
            existing.as_ref().and_then(|r| r.child_run_id),
            existing.as_ref().and_then(|r| r.approval_id),
            existing.as_ref().and_then(|r| r.wait_until),
        )
        .await;
        states.insert(
            node.id.clone(),
            NodeState {
                status: "running".into(),
                output: None,
                child_run_id: existing.as_ref().and_then(|r| r.child_run_id),
                approval_id: existing.as_ref().and_then(|r| r.approval_id),
            },
        );

        let ev = self
            .event(EventType::ToolStarted)
            .with_status(EventStatus::Started)
            .with_payload(json!({"workflow_node": node.id, "kind": node.kind.as_str(), "label": node.label, "input": input}));
        self.emit(ev).await;

        let output = match node.kind {
            WorkflowNodeKind::Start => self.req.input.clone(),
            WorkflowNodeKind::End => context.clone(),
            WorkflowNodeKind::Parallel => json!({"forked": true}),
            WorkflowNodeKind::Join => {
                json!({"joined": node.config.get("join").cloned().unwrap_or(json!("all"))})
            }
            WorkflowNodeKind::Condition => self.run_condition(def, node, context)?,
            WorkflowNodeKind::Tool => self.run_tool_node(node, &input).await?,
            WorkflowNodeKind::Agent => {
                self.run_agent_node(
                    def,
                    node,
                    &input,
                    existing.as_ref().and_then(|r| r.child_run_id),
                )
                .await?
            }
            WorkflowNodeKind::Approval => {
                self.run_approval_node(node, &input, existing.as_ref().and_then(|r| r.approval_id))
                    .await?
            }
            WorkflowNodeKind::Wait => {
                self.run_wait_node(node, existing.as_ref().and_then(|r| r.wait_until))
                    .await?
            }
            WorkflowNodeKind::Verify => self.run_verify_node(def, node, context).await?,
        };

        merge_node_output(context, &node.id, &output);
        let child_run_id = output
            .get("child_run_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .or_else(|| existing.as_ref().and_then(|r| r.child_run_id));
        let approval_id = output
            .get("approval_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .or_else(|| existing.as_ref().and_then(|r| r.approval_id));
        let _ = persist_node(
            &self.rt.db,
            self.project_id,
            self.run_id,
            workflow_id,
            workflow_version,
            node,
            "completed",
            &input,
            Some(&output),
            None,
            child_run_id,
            approval_id,
            None,
        )
        .await;
        self.rt
            .metrics
            .workflow_nodes_completed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        states.insert(
            node.id.clone(),
            NodeState {
                status: "completed".into(),
                output: Some(output.clone()),
                child_run_id,
                approval_id,
            },
        );
        let ev = self
            .event(EventType::ToolCompleted)
            .with_status(EventStatus::Succeeded)
            .with_payload(
                json!({"workflow_node": node.id, "kind": node.kind.as_str(), "output": output}),
            );
        self.emit(ev).await;
        Ok(())
    }

    fn run_condition(
        &self,
        def: &WorkflowDefinition,
        node: &WorkflowNode,
        context: &Value,
    ) -> Result<Value, RunEnd> {
        let expr = node
            .config
            .get("expression")
            .and_then(|v| v.as_str())
            .unwrap_or("true");
        let value = eval_condition(expr, context)
            .map_err(|e| RunEnd::Failed(format!("condition `{}`: {e}", node.id)))?;
        let label = match &value {
            Value::Bool(true) => "true",
            Value::Bool(false) => "false",
            Value::String(s) => s.as_str(),
            other => {
                return Err(RunEnd::Failed(format!(
                    "condition `{}` produced {other}",
                    node.id
                )));
            }
        };
        if !def
            .outgoing(&node.id)
            .any(|e| e.label.as_deref() == Some(label))
        {
            return Err(RunEnd::Failed(format!(
                "condition `{}` has no edge labeled `{label}`",
                node.id
            )));
        }
        Ok(json!({"result": value, "label": label}))
    }

    fn ensure_registered_tools(&mut self, def: &WorkflowDefinition) -> Result<(), RunEnd> {
        let extra = registered_tool_specs(def);
        if extra.is_empty() {
            return Ok(());
        }
        let mut tools = self.req.agent.tools.clone();
        for spec in extra {
            if !tools.iter().any(|t| t.name == spec.name) {
                tools.push(spec);
            }
        }
        self.registry = Some(
            ActionRegistry::new(&tools, &self.req.agent.permissions)
                .map_err(|e| RunEnd::Failed(format!("workflow tool contract: {e}")))?,
        );
        Ok(())
    }

    async fn await_attached_worker(&self, timeout: Duration, what: &str) -> Result<(), RunEnd> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(t) = *self.handle.last_poll.read().await {
                if Utc::now() - t < chrono::Duration::seconds(30) {
                    return Ok(());
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(RunEnd::Failed(format!(
                    "WORKER_UNAVAILABLE: no worker attached for {what}"
                )));
            }
            if self.handle.cancel.is_cancelled() {
                return Err(RunEnd::Cancelled);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn run_tool_node(&mut self, node: &WorkflowNode, input: &Value) -> Result<Value, RunEnd> {
        let name = node
            .config
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(result) = node.config.get("result") {
            return Ok(json!({"tool": name, "native": true, "result": result, "input": input}));
        }
        let handler = node
            .config
            .get("handler")
            .and_then(|v| v.as_str())
            .unwrap_or("echo");
        match handler {
            "echo" => Ok(json!({"tool": name, "native": true, "result": input})),
            "fail" => Err(RunEnd::Failed(format!(
                "tool node `{}` handler=fail",
                node.id
            ))),
            "registered" | "worker" => self.dispatch_registered_tool(node, name, input).await,
            other => {
                if name.is_empty() {
                    Err(RunEnd::Failed(format!(
                        "tool on node `{}` has no native handler `{other}` and no simulated result",
                        node.id
                    )))
                } else {
                    self.dispatch_registered_tool(node, name, input).await
                }
            }
        }
    }

    async fn dispatch_registered_tool(
        &mut self,
        node: &WorkflowNode,
        name: &str,
        input: &Value,
    ) -> Result<Value, RunEnd> {
        if name.is_empty() {
            return Err(RunEnd::Failed(format!(
                "registered tool node `{}` is missing config.tool",
                node.id
            )));
        }
        let wait = Duration::from_secs(
            node.config
                .get("worker_wait_seconds")
                .and_then(|v| v.as_u64())
                .or_else(|| {
                    self.req
                        .metadata
                        .get("worker_attach_seconds")
                        .and_then(|v| v.as_u64())
                })
                .unwrap_or(20),
        );
        self.await_attached_worker(wait, &format!("tool `{name}`"))
            .await?;
        let effect = node
            .config
            .get("effect")
            .and_then(|v| v.as_str())
            .and_then(EffectCategory::parse)
            .unwrap_or(EffectCategory::ReadOnly);
        if !effect.safe_to_retry_when_ambiguous() {
            if let Ok(Some(existing)) = self
                .rt
                .db
                .get_workflow_node(self.project_id, self.run_id, &node.id)
                .await
            {
                if existing.status == "running" {
                    return Err(RunEnd::RecoveryRequired(format!(
                        "tool `{name}` on node `{}` may have already dispatched a non-idempotent write",
                        node.id
                    )));
                }
            }
        }
        let call = goshikq_protocol::ToolCall {
            id: format!("wf-{}-{}", self.run_id, node.id),
            name: name.into(),
            arguments: if input.is_object() {
                input.clone()
            } else {
                json!({"input": input})
            },
        };
        let outcome = self.handle_tool_call(&call).await;
        if let Some(end) = outcome.end {
            return Err(end);
        }
        let content: Value = outcome
            .message
            .content
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Null);
        if content.get("error").is_some() {
            return Err(RunEnd::Failed(format!(
                "registered tool `{name}` on node `{}` failed: {content}",
                node.id
            )));
        }
        Ok(json!({
            "tool": name,
            "registered": true,
            "result": content.get("result").cloned().unwrap_or(content),
        }))
    }

    async fn run_agent_node(
        &mut self,
        def: &WorkflowDefinition,
        node: &WorkflowNode,
        input: &Value,
        existing_child: Option<Uuid>,
    ) -> Result<Value, RunEnd> {
        let name = node
            .config
            .get("agent")
            .and_then(|v| v.as_str())
            .unwrap_or("workflow-agent");
        let spawn = node
            .config
            .get("spawn")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !spawn {
            let ev = self
                .event(EventType::AgentDelegated)
                .with_status(EventStatus::Info)
                .with_payload(json!({"workflow_node": node.id, "agent": name, "native": true, "role": node.config.get("role")}));
            self.emit(ev).await;
            return Ok(json!({"agent": name, "native": true, "completed": true, "input": input}));
        }
        let model = node
            .config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("mock/deterministic");
        let role = node
            .config
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("worker");
        let goal = node
            .config
            .get("goal")
            .and_then(|v| v.as_str())
            .unwrap_or(def.goal.description.as_str());
        let goal = if goal.trim().is_empty() {
            format!("workflow node `{}`", node.id)
        } else {
            goal.to_string()
        };
        let child_id = if let Some(id) = existing_child {
            id
        } else {
            let req = CreateTaskRequest {
                agent: AgentDefinition {
                    name: name.into(),
                    description: node.label.clone(),
                    model: model.into(),
                    instructions: node
                        .config
                        .get("instructions")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                    tools: vec![],
                    execution_limits: ExecutionLimits {
                        max_steps: 4,
                        deadline_seconds: 60,
                        ..Default::default()
                    },
                    retry_policy: Default::default(),
                    context_policy: json!({}),
                    metadata: json!({"workflow_node": node.id}),
                    permissions: vec![],
                },
                goal: GoalDefinition {
                    description: goal.clone(),
                    success_predicate: None,
                    invariants: vec![],
                    max_steps: Some(4),
                    deadline_seconds: Some(60),
                    external_wait_seconds: None,
                    budget: None,
                    verification_policy: VerificationPolicy::None,
                    metadata: json!({"workflow_node": node.id}),
                },
                input: input.clone(),
                has_state_provider: false,
                source_framework: "goshikq-workflow".into(),
                idempotency_key: Some(format!("wf:{}:{}", self.run_id, node.id)),
                metadata: json!({"workflow_node": node.id}),
                role: role.into(),
                requires_approval: false,
                defer_start: false,
            };
            let created = self
                .rt
                .create_child_task(self.project_id, self.run_id, req, "workflow")
                .await
                .map_err(|e| RunEnd::Failed(format!("spawn agent `{}`: {e}", node.id)))?;
            let ev = self
                .event(EventType::AgentDelegated)
                .with_status(EventStatus::Info)
                .with_payload(json!({
                    "workflow_node": node.id,
                    "child_run_id": created.child.id,
                    "task_id": created.task.task_id,
                    "role": role,
                    "agent": name,
                }));
            self.emit(ev).await;
            created.child.id
        };
        let worker_wait = Duration::from_secs(
            node.config
                .get("worker_wait_seconds")
                .and_then(|v| v.as_u64())
                .unwrap_or(20),
        );
        let wait_deadline = Utc::now()
            + chrono::Duration::from_std(worker_wait).unwrap_or(chrono::Duration::seconds(20));
        let mut worker_seen = false;
        loop {
            if self.handle.cancel.is_cancelled() {
                return Err(RunEnd::Cancelled);
            }
            if Utc::now() >= self.execution_deadline_at {
                return Err(RunEnd::TimedOut);
            }
            if !worker_seen {
                if let Some(h) = self.rt.handle(child_id) {
                    if h.last_poll.read().await.is_some() {
                        worker_seen = true;
                    }
                }
                if !worker_seen && Utc::now() > wait_deadline {
                    let _ = self.rt.cancel(self.project_id, child_id, "workflow").await;
                    return Err(RunEnd::Failed(format!(
                        "WORKER_UNAVAILABLE: spawned agent `{}` child {child_id} had no worker before the attach deadline",
                        node.id
                    )));
                }
            }
            let child = self
                .rt
                .db
                .get_run(self.project_id, child_id)
                .await
                .map_err(|e| RunEnd::Failed(e.to_string()))?;
            let status = RunStatus::parse(&child.status).unwrap_or(RunStatus::Failed);
            if status.is_terminal() {
                if matches!(
                    status,
                    RunStatus::Failed | RunStatus::Cancelled | RunStatus::TimedOut
                ) {
                    return Err(RunEnd::Failed(format!(
                        "agent node `{}` child {child_id} ended {}",
                        node.id, child.status
                    )));
                }
                return Ok(json!({
                    "child_run_id": child_id,
                    "status": child.status,
                    "verification_status": child.verification_status,
                    "final_output": child.final_output,
                }));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn run_approval_node(
        &mut self,
        node: &WorkflowNode,
        input: &Value,
        existing: Option<Uuid>,
    ) -> Result<Value, RunEnd> {
        let policy = node
            .config
            .get("policy")
            .and_then(|v| v.as_str())
            .unwrap_or("required");
        if policy == "optional" {
            return Ok(json!({"approved": true, "policy": "optional"}));
        }
        let spec = ToolSpec {
            name: node
                .config
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("workflow_approval")
                .into(),
            description: node.label.clone(),
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
        let approval_id = if let Some(id) = existing {
            id
        } else {
            let inv = self
                .rt
                .db
                .create_tool_invocation(
                    self.project_id,
                    self.run_id,
                    &format!("wf-{}", node.id),
                    &format!("wf-{}-{}", self.run_id, node.id),
                    &spec,
                    input,
                    &format!("wf-approval:{}:{}", self.run_id, node.id),
                )
                .await
                .map_err(|e| RunEnd::Failed(e.to_string()))?;
            let expires_at = Utc::now()
                + chrono::Duration::seconds(
                    self.req
                        .metadata
                        .get("workflow")
                        .and_then(|w| w.get("approval_seconds"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(3600),
                );
            let approval = self
                .rt
                .db
                .create_approval(
                    self.project_id,
                    self.run_id,
                    inv.id,
                    &self.req.agent.name,
                    &spec.name,
                    input,
                    spec.risk.as_str(),
                    &format!("workflow node `{}` requires approval", node.id),
                    &[],
                    expires_at,
                )
                .await
                .map_err(|e| RunEnd::Failed(e.to_string()))?;
            let ev = self
                .event(EventType::ApprovalRequested)
                .with_status(EventStatus::Pending)
                .with_payload(json!({"approval_id": approval.id, "workflow_node": node.id, "action": spec.name}));
            if !self.enter_approval_wait(ev).await {
                return Err(RunEnd::Failed(
                    "could not enter WAITING_FOR_APPROVAL".into(),
                ));
            }
            self.pending_approval_id = Some(approval.id);
            self.checkpoint().await;
            approval.id
        };
        if let Ok(already) = self.rt.db.get_approval(self.project_id, approval_id).await {
            if already.status != ApprovalStatus::Pending.as_str() {
                if already.status != ApprovalStatus::Approved.as_str() {
                    return Err(RunEnd::Failed(format!(
                        "workflow approval `{}` was {}",
                        node.id, already.status
                    )));
                }
                let _ = self.transition(RunStatus::Executing, None, vec![]).await;
                return Ok(json!({"approved": true, "approval_id": already.id}));
            }
        }
        let waiter = self.handle.register_approval_waiter(approval_id).await;
        tokio::pin!(waiter);
        let approval = loop {
            tokio::select! {
                _ = self.handle.cancel.cancelled() => return Err(RunEnd::Cancelled),
                res = &mut waiter => {
                    break res.map_err(|_| RunEnd::Failed("approval waiter closed".into()))?;
                }
                _ = tokio::time::sleep(Duration::from_millis(200)) => {
                    if let Ok(row) = self.rt.db.get_approval(self.project_id, approval_id).await {
                        if row.status != ApprovalStatus::Pending.as_str() {
                            break row;
                        }
                    }
                }
            }
        };
        if approval.status != ApprovalStatus::Approved.as_str() {
            let ev = self
                .event(EventType::ApprovalDenied)
                .with_status(EventStatus::Denied)
                .with_payload(json!({"approval_id": approval.id, "workflow_node": node.id, "status": approval.status}));
            self.emit(ev).await;
            return Err(RunEnd::Failed(format!(
                "workflow approval `{}` was {}",
                node.id, approval.status
            )));
        }
        let ev = self
            .event(EventType::ApprovalGranted)
            .with_status(EventStatus::Succeeded)
            .with_payload(json!({"approval_id": approval.id, "workflow_node": node.id}));
        self.emit(ev).await;
        let _ = self.transition(RunStatus::Executing, None, vec![]).await;
        Ok(json!({"approved": true, "approval_id": approval.id}))
    }

    async fn run_wait_node(
        &mut self,
        node: &WorkflowNode,
        existing_until: Option<chrono::DateTime<Utc>>,
    ) -> Result<Value, RunEnd> {
        let until = if let Some(u) = existing_until {
            u
        } else if let Some(ms) = node.config.get("duration_ms").and_then(|v| v.as_u64()) {
            Utc::now() + chrono::Duration::milliseconds(ms as i64)
        } else {
            return Err(RunEnd::Failed(format!(
                "wait node `{}` missing duration",
                node.id
            )));
        };
        self.wait_deadline_at = Some(until);
        let ev = self
            .event(EventType::RunPaused)
            .with_status(EventStatus::Pending)
            .with_payload(
                json!({"workflow_node": node.id, "wait_until": until, "reason": "durable_wait"}),
            );
        if self.status != RunStatus::Executing && !self.enter_executing().await {
            return Err(RunEnd::Failed(
                "could not enter EXECUTING before wait".into(),
            ));
        }
        if !self
            .transition(RunStatus::WaitingForExternalState, None, vec![ev])
            .await
        {
            return Err(RunEnd::Failed(
                "could not enter WAITING_FOR_EXTERNAL_STATE".into(),
            ));
        }
        self.checkpoint().await;
        while Utc::now() < until {
            if self.handle.cancel.is_cancelled() {
                return Err(RunEnd::Cancelled);
            }
            if Utc::now() >= self.execution_deadline_at {
                return Err(RunEnd::TimedOut);
            }
            let slice = (until - Utc::now())
                .to_std()
                .unwrap_or(Duration::from_millis(10))
                .min(Duration::from_millis(200));
            tokio::time::sleep(slice).await;
        }
        let ev = self
            .event(EventType::RunResumed)
            .with_status(EventStatus::Info)
            .with_payload(json!({"workflow_node": node.id}));
        self.emit(ev).await;
        if !self.enter_executing().await {
            return Err(RunEnd::Failed(
                "could not resume EXECUTING after wait".into(),
            ));
        }
        Ok(json!({"waited_until": until}))
    }

    async fn run_verify_node(
        &mut self,
        def: &WorkflowDefinition,
        node: &WorkflowNode,
        context: &Value,
    ) -> Result<Value, RunEnd> {
        let policy = node
            .config
            .get("verification_policy")
            .and_then(|v| v.as_str())
            .unwrap_or(def.goal.verification_policy.as_str());
        let ev = self
            .event(EventType::VerificationStarted)
            .with_status(EventStatus::Started)
            .with_payload(json!({"workflow_node": node.id, "policy": policy}));
        self.emit(ev).await;
        if policy == "none" {
            let _ = self
                .rt
                .db
                .set_run_verification(self.run_id, VerificationStatus::NotEvaluated)
                .await;
            let ev = self
                .event(EventType::VerificationInconclusive)
                .with_status(EventStatus::Inconclusive)
                .with_payload(
                    json!({"workflow_node": node.id, "reason": "verification_policy=none"}),
                );
            self.emit(ev).await;
            return Ok(json!({"verified": false, "policy": "none"}));
        }
        let path = node
            .config
            .get("path")
            .and_then(|v| v.as_str())
            .or(def.goal.path.as_deref())
            .unwrap_or("/approved");
        let expected = node
            .config
            .get("equals")
            .cloned()
            .or(def.goal.equals.clone())
            .unwrap_or(json!(true));
        let actual = pointer_get(context, path).cloned().unwrap_or(Value::Null);
        let ok = actual == expected;
        if ok {
            let _ = self
                .rt
                .db
                .set_run_verification(self.run_id, VerificationStatus::Verified)
                .await;
            let ev = self
                .event(EventType::VerificationPassed)
                .with_status(EventStatus::Passed)
                .with_payload(json!({"workflow_node": node.id, "path": path, "expected": expected, "actual": actual}));
            self.emit(ev).await;
            let ev = self
                .event(EventType::GoalSatisfied)
                .with_status(EventStatus::Succeeded)
                .with_payload(json!({"workflow_node": node.id}));
            self.emit(ev).await;
            Ok(json!({"verified": true, "path": path, "actual": actual}))
        } else {
            let _ = self
                .rt
                .db
                .set_run_verification(self.run_id, VerificationStatus::Failed)
                .await;
            let ev = self
                .event(EventType::VerificationFailed)
                .with_status(EventStatus::Failed)
                .with_payload(json!({"workflow_node": node.id, "path": path, "expected": expected, "actual": actual}));
            self.emit(ev).await;
            Err(RunEnd::Failed(format!(
                "verify `{}` expected {expected} at {path}, got {actual}",
                node.id
            )))
        }
    }
}

struct NodeState {
    status: String,
    output: Option<Value>,
    #[allow(dead_code)]
    child_run_id: Option<Uuid>,
    #[allow(dead_code)]
    approval_id: Option<Uuid>,
}

fn ready_nodes<'a>(
    def: &'a WorkflowDefinition,
    states: &HashMap<String, NodeState>,
) -> Vec<&'a WorkflowNode> {
    def.nodes
        .iter()
        .filter(|n| {
            let st = states
                .get(&n.id)
                .map(|s| s.status.as_str())
                .unwrap_or("pending");
            if st == "completed" || st == "failed" || st == "skipped" {
                return false;
            }
            if n.kind == WorkflowNodeKind::Start {
                return st == "pending" || st == "running" || st == "waiting";
            }
            let preds: Vec<_> = def.incoming(&n.id).collect();
            if preds.is_empty() {
                return false;
            }
            if n.kind == WorkflowNodeKind::Join {
                return join_ready(def, n, states);
            }
            preds
                .iter()
                .all(|e| states.get(&e.source).map(|s| s.status.as_str()) == Some("completed"))
                && condition_allows(def, n, states)
        })
        .collect()
}

fn condition_allows(
    def: &WorkflowDefinition,
    node: &WorkflowNode,
    states: &HashMap<String, NodeState>,
) -> bool {
    for edge in def.incoming(&node.id) {
        let Some(src) = def.node(&edge.source) else {
            continue;
        };
        if src.kind != WorkflowNodeKind::Condition {
            continue;
        }
        let label = states
            .get(&src.id)
            .and_then(|s| s.output.as_ref())
            .and_then(|o| o.get("label"))
            .and_then(|v| v.as_str());
        if edge.label.as_deref() != label {
            return false;
        }
    }
    true
}

fn join_ready(
    def: &WorkflowDefinition,
    node: &WorkflowNode,
    states: &HashMap<String, NodeState>,
) -> bool {
    let mode = node
        .config
        .get("join")
        .and_then(|v| v.as_str())
        .unwrap_or("all");
    let preds: Vec<_> = def.incoming(&node.id).collect();
    match mode {
        "any" => preds
            .iter()
            .any(|e| states.get(&e.source).map(|s| s.status.as_str()) == Some("completed")),
        _ => preds
            .iter()
            .all(|e| states.get(&e.source).map(|s| s.status.as_str()) == Some("completed")),
    }
}

async fn load_states(
    db: &goshikq_storage::Db,
    project_id: Uuid,
    run_id: Uuid,
) -> HashMap<String, NodeState> {
    match db.list_workflow_nodes(project_id, run_id).await {
        Ok(rows) => rows
            .into_iter()
            .map(|r| {
                (
                    r.node_id,
                    NodeState {
                        status: r.status,
                        output: r.output,
                        child_run_id: r.child_run_id,
                        approval_id: r.approval_id,
                    },
                )
            })
            .collect(),
        Err(_) => HashMap::new(),
    }
}

fn rebuild_context(input: &Value, states: &HashMap<String, NodeState>) -> Value {
    let mut ctx = json!({"input": input, "nodes": {}});
    for (id, st) in states {
        if let Some(out) = &st.output {
            merge_node_output(&mut ctx, id, out);
        }
    }
    ctx
}

fn merge_node_output(context: &mut Value, node_id: &str, output: &Value) {
    if !context.is_object() {
        *context = json!({"nodes": {}});
    }
    let nodes = context
        .as_object_mut()
        .unwrap()
        .entry("nodes")
        .or_insert_with(|| json!({}));
    if let Some(obj) = nodes.as_object_mut() {
        obj.insert(node_id.to_string(), output.clone());
    }
    if let Some(approved) = output.get("approved") {
        context
            .as_object_mut()
            .unwrap()
            .insert("approved".into(), approved.clone());
    }
    if let Some(verified) = output.get("verified") {
        context
            .as_object_mut()
            .unwrap()
            .insert("verified".into(), verified.clone());
    }
}

#[allow(clippy::too_many_arguments)]
async fn persist_node(
    db: &goshikq_storage::Db,
    project_id: Uuid,
    run_id: Uuid,
    workflow_id: Uuid,
    workflow_version: i32,
    node: &WorkflowNode,
    status: &str,
    input: &Value,
    output: Option<&Value>,
    error: Option<&str>,
    child_run_id: Option<Uuid>,
    approval_id: Option<Uuid>,
    wait_until: Option<chrono::DateTime<Utc>>,
) -> Result<(), String> {
    let now = Utc::now();
    db.upsert_workflow_node(
        project_id,
        run_id,
        workflow_id,
        workflow_version,
        &node.id,
        status,
        1,
        input,
        output,
        error,
        child_run_id,
        approval_id,
        wait_until,
        Some(now),
        if status == "completed" || status == "failed" {
            Some(now)
        } else {
            None
        },
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn registered_tool_specs(def: &WorkflowDefinition) -> Vec<ToolSpec> {
    let mut specs = Vec::new();
    for node in &def.nodes {
        if node.kind != WorkflowNodeKind::Tool {
            continue;
        }
        if node.config.get("result").is_some() {
            continue;
        }
        let handler = node
            .config
            .get("handler")
            .and_then(|v| v.as_str())
            .unwrap_or("echo");
        if matches!(handler, "echo" | "fail") {
            continue;
        }
        let Some(name) = node
            .config
            .get("tool")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if specs.iter().any(|s: &ToolSpec| s.name == name) {
            continue;
        }
        specs.push(ToolSpec {
            name: name.into(),
            description: node.label.clone(),
            input_schema: if node.input_schema.is_null() {
                json!({"type": "object"})
            } else {
                node.input_schema.clone()
            },
            output_schema: None,
            effect: node
                .config
                .get("effect")
                .and_then(|v| v.as_str())
                .and_then(EffectCategory::parse)
                .unwrap_or(EffectCategory::ReadOnly),
            risk: RiskLevel::Low,
            timeout_ms: node.timeout_ms.unwrap_or(30_000),
            retry_policy: Default::default(),
            idempotent: node
                .config
                .get("idempotent")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            requires_approval: node
                .config
                .get("requires_approval")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            required_permissions: vec![],
            preconditions: vec![],
            has_verifier: false,
            revalidation_policy: Default::default(),
            evidence_coverage: None,
        });
    }
    specs
}
