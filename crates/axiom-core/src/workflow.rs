//! Versioned workflow graph schema shared by Studio, the CLI, and the Rust runtime.
//!
//! Validation is deterministic and IO-free. Execution lives in `axiom-runtime`
//! (`RunExecutor::execute_workflow`) so Studio never schedules work itself.

use std::collections::{HashMap, HashSet, VecDeque};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowNodeKind {
    Start,
    Agent,
    Tool,
    Condition,
    Parallel,
    Join,
    #[serde(alias = "human_approval")]
    Approval,
    Wait,
    #[serde(alias = "verification")]
    Verify,
    End,
}

impl WorkflowNodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Agent => "agent",
            Self::Tool => "tool",
            Self::Condition => "condition",
            Self::Parallel => "parallel",
            Self::Join => "join",
            Self::Approval => "approval",
            Self::Wait => "wait",
            Self::Verify => "verify",
            Self::End => "end",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema, Default)]
pub struct DataMapping {
    /// JSON pointer (RFC 6901) into the source payload, e.g. `/order/id`.
    #[serde(default)]
    pub from: String,
    /// JSON pointer into the target payload.
    #[serde(default)]
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WorkflowNode {
    pub id: String,
    pub kind: WorkflowNodeKind,
    #[serde(default)]
    pub label: String,
    /// Kind-specific configuration (agent name, tool name, condition expression, ...).
    #[serde(default)]
    pub config: serde_json::Value,
    /// Required for nodes that participate in a cycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(default)]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub output_schema: serde_json::Value,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WorkflowEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    /// Branch label for condition nodes (e.g. `true` / `false`, `approved` / `denied`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub mapping: Vec<DataMapping>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WorkflowErrorPolicy {
    /// `fail_fast` (default) or `continue`.
    #[serde(default = "default_on_error")]
    pub on_error: String,
}

impl Default for WorkflowErrorPolicy {
    fn default() -> Self {
        Self { on_error: default_on_error() }
    }
}
fn default_on_error() -> String {
    "fail_fast".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema, Default)]
pub struct WorkflowTimeouts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WorkflowRetryPolicy {
    #[serde(default = "default_retry_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_backoff_ms")]
    pub backoff_ms: u64,
}

impl Default for WorkflowRetryPolicy {
    fn default() -> Self {
        Self { max_attempts: 1, backoff_ms: 250 }
    }
}
fn default_retry_attempts() -> u32 {
    1
}
fn default_backoff_ms() -> u64 {
    250
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WorkflowGoal {
    #[serde(default)]
    pub description: String,
    /// `none` | `json_predicate` | `authoritative`.
    #[serde(default = "default_verification_policy")]
    pub verification_policy: String,
    /// JSON pointer compared against `equals` when `json_predicate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<serde_json::Value>,
    #[serde(default)]
    pub required: bool,
}

fn default_verification_policy() -> String {
    "none".into()
}

impl Default for WorkflowGoal {
    fn default() -> Self {
        Self {
            description: String::new(),
            verification_policy: default_verification_policy(),
            path: None,
            equals: None,
            required: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WorkflowDefinition {
    /// Optional identity assigned by the catalog. Ignored on import until saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_version")]
    pub version: u32,
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
    /// Upper bound on total node executions for one workflow run.
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    #[serde(default)]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub output_schema: serde_json::Value,
    #[serde(default)]
    pub error_policy: WorkflowErrorPolicy,
    #[serde(default)]
    pub timeouts: WorkflowTimeouts,
    #[serde(default)]
    pub retry_policy: WorkflowRetryPolicy,
    #[serde(default)]
    pub required_permissions: Vec<String>,
    #[serde(default)]
    pub goal: WorkflowGoal,
}

fn default_version() -> u32 {
    1
}
fn default_max_steps() -> u32 {
    50
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct WorkflowIssue {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_id: Option<String>,
}

fn issue(code: &str, message: String, node_id: Option<&str>, edge_id: Option<&str>) -> WorkflowIssue {
    WorkflowIssue { code: code.into(), message, node_id: node_id.map(String::from), edge_id: edge_id.map(String::from) }
}

/// Allowed successor kinds per node kind.
fn compatible(from: WorkflowNodeKind, to: WorkflowNodeKind) -> bool {
    use WorkflowNodeKind::*;
    match from {
        End => false,
        _ => to != Start,
    }
}

fn schema_kind(v: &serde_json::Value) -> Option<&str> {
    v.get("type").and_then(|t| t.as_str())
}

fn schemas_compatible(from: &serde_json::Value, to: &serde_json::Value) -> bool {
    if from.is_null() || to.is_null() || from == &serde_json::json!({}) || to == &serde_json::json!({}) {
        return true;
    }
    match (schema_kind(from), schema_kind(to)) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

impl WorkflowDefinition {
    pub fn node(&self, id: &str) -> Option<&WorkflowNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn outgoing<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a WorkflowEdge> {
        self.edges.iter().filter(move |e| e.source == id)
    }

    pub fn incoming<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a WorkflowEdge> {
        self.edges.iter().filter(move |e| e.target == id)
    }

    /// Returns an empty vector when the workflow is valid.
    pub fn validate(&self) -> Vec<WorkflowIssue> {
        let mut issues = Vec::new();
        if self.name.trim().is_empty() {
            issues.push(issue("missing_name", "workflow name is required".into(), None, None));
        }
        if self.version == 0 {
            issues.push(issue("invalid_version", "version must be greater than zero".into(), None, None));
        }
        match self.error_policy.on_error.as_str() {
            "fail_fast" | "continue" => {}
            other => issues.push(issue("invalid_error_policy", format!("unknown on_error `{other}`"), None, None)),
        }
        match self.goal.verification_policy.as_str() {
            "none" | "json_predicate" | "authoritative" => {}
            other => issues.push(issue("invalid_verification_policy", format!("unknown verification_policy `{other}`"), None, None)),
        }

        let mut ids = HashSet::new();
        for n in &self.nodes {
            if n.id.trim().is_empty() {
                issues.push(issue("empty_node_id", "node id must not be empty".into(), None, None));
            }
            if !ids.insert(n.id.as_str()) {
                issues.push(issue("duplicate_node_id", format!("node id `{}` is duplicated", n.id), Some(&n.id), None));
            }
        }
        let kinds: HashMap<&str, WorkflowNodeKind> = self.nodes.iter().map(|n| (n.id.as_str(), n.kind)).collect();
        let starts: Vec<&WorkflowNode> = self.nodes.iter().filter(|n| n.kind == WorkflowNodeKind::Start).collect();
        if starts.len() != 1 {
            issues.push(issue("start_node_count", format!("expected exactly one start node, found {}", starts.len()), None, None));
        }
        if !self.nodes.iter().any(|n| n.kind == WorkflowNodeKind::End) {
            issues.push(issue("missing_end_node", "workflow has no end node".into(), None, None));
        }
        if self.max_steps == 0 {
            issues.push(issue("invalid_max_steps", "max_steps must be greater than zero".into(), None, None));
        }
        if self.max_steps > 10_000 {
            issues.push(issue("invalid_max_steps", "max_steps exceeds the 10000 resource limit".into(), None, None));
        }
        if self.nodes.len() > 128 {
            issues.push(issue("graph_too_large", format!("workflow has {} nodes; maximum is 128", self.nodes.len()), None, None));
        }
        if self.edges.len() > 256 {
            issues.push(issue("graph_too_large", format!("workflow has {} edges; maximum is 256", self.edges.len()), None, None));
        }

        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut incoming_count: HashMap<&str, usize> = HashMap::new();
        let mut edge_ids = HashSet::new();
        for e in &self.edges {
            if !edge_ids.insert(e.id.as_str()) {
                issues.push(issue("duplicate_edge_id", format!("edge id `{}` is duplicated", e.id), None, Some(&e.id)));
            }
            let (Some(sk), Some(tk)) = (kinds.get(e.source.as_str()), kinds.get(e.target.as_str())) else {
                issues.push(issue("dangling_edge", format!("edge `{}` references an unknown node", e.id), None, Some(&e.id)));
                continue;
            };
            if !compatible(*sk, *tk) {
                issues.push(issue("incompatible_connection", format!("edge `{}` connects {:?} -> {:?}", e.id, sk, tk), None, Some(&e.id)));
            }
            if let (Some(src), Some(dst)) = (self.node(&e.source), self.node(&e.target)) {
                if !schemas_compatible(&src.output_schema, &dst.input_schema) {
                    issues.push(issue(
                        "incompatible_schema",
                        format!("edge `{}` maps incompatible output/input schemas", e.id),
                        None,
                        Some(&e.id),
                    ));
                }
            }
            adj.entry(e.source.as_str()).or_default().push(e.target.as_str());
            *incoming_count.entry(e.target.as_str()).or_default() += 1;
        }

        let has_verify = self.nodes.iter().any(|n| n.kind == WorkflowNodeKind::Verify);
        if self.goal.required && !has_verify && self.goal.verification_policy != "none" {
            issues.push(issue("missing_required_verification", "goal.required is set but the graph has no verify node".into(), None, None));
        }

        for n in &self.nodes {
            let out = adj.get(n.id.as_str()).map(|v| v.len()).unwrap_or(0);
            let inn = *incoming_count.get(n.id.as_str()).unwrap_or(&0);
            match n.kind {
                WorkflowNodeKind::Condition if out < 2 => {
                    issues.push(issue("condition_needs_branches", format!("condition node `{}` needs at least two outgoing edges", n.id), Some(&n.id), None));
                }
                WorkflowNodeKind::Condition => {
                    let labeled = self.edges.iter().filter(|e| e.source == n.id && e.label.is_some()).count();
                    if labeled != out {
                        issues.push(issue("unlabeled_branch", format!("all outgoing edges of condition `{}` must be labeled", n.id), Some(&n.id), None));
                    }
                }
                WorkflowNodeKind::Parallel if out < 2 => {
                    issues.push(issue("parallel_needs_branches", format!("parallel node `{}` needs at least two outgoing edges", n.id), Some(&n.id), None));
                }
                WorkflowNodeKind::Join => {
                    if inn < 2 {
                        issues.push(issue("invalid_join", format!("join node `{}` needs at least two incoming edges", n.id), Some(&n.id), None));
                    }
                    if out != 1 {
                        issues.push(issue("invalid_join", format!("join node `{}` must have exactly one outgoing edge", n.id), Some(&n.id), None));
                    }
                    let join = n.config.get("join").and_then(|v| v.as_str()).unwrap_or("all");
                    if !matches!(join, "all" | "any") {
                        issues.push(issue("invalid_join", format!("join node `{}` join must be all|any", n.id), Some(&n.id), None));
                    }
                }
                WorkflowNodeKind::End => {}
                _ if out == 0 => {
                    issues.push(issue("dead_end", format!("node `{}` has no outgoing edge and is not an end node", n.id), Some(&n.id), None));
                }
                _ => {}
            }
            match n.kind {
                WorkflowNodeKind::Agent if n.config.get("agent").and_then(|v| v.as_str()).unwrap_or("").is_empty() => {
                    issues.push(issue("missing_reference", format!("agent node `{}` must reference an agent", n.id), Some(&n.id), None));
                }
                WorkflowNodeKind::Tool if n.config.get("tool").and_then(|v| v.as_str()).unwrap_or("").is_empty() => {
                    issues.push(issue("missing_tool_registration", format!("tool node `{}` must reference a tool", n.id), Some(&n.id), None));
                }
                WorkflowNodeKind::Wait => {
                    let has_ms = n.config.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0) > 0;
                    let has_until = n.config.get("until").and_then(|v| v.as_str()).is_some();
                    if !has_ms && !has_until {
                        issues.push(issue("invalid_wait", format!("wait node `{}` needs duration_ms or until", n.id), Some(&n.id), None));
                    }
                }
                WorkflowNodeKind::Approval => {
                    let policy = n.config.get("policy").and_then(|v| v.as_str()).unwrap_or("required");
                    if !matches!(policy, "required" | "optional") {
                        issues.push(issue("invalid_approval_policy", format!("approval node `{}` policy must be required|optional", n.id), Some(&n.id), None));
                    }
                }
                WorkflowNodeKind::Verify if self.goal.verification_policy == "json_predicate" => {
                    if n.config.get("path").and_then(|v| v.as_str()).is_none() && self.goal.path.is_none() {
                        issues.push(issue("missing_required_verification", format!("verify node `{}` needs a JSON path", n.id), Some(&n.id), None));
                    }
                }
                _ => {}
            }
            if let Some(perms) = n.config.get("permissions").and_then(|v| v.as_array()) {
                for p in perms {
                    if let Some(s) = p.as_str() {
                        if !self.required_permissions.is_empty() && !self.required_permissions.iter().any(|g| g == s) {
                            issues.push(issue(
                                "unauthorized_action",
                                format!("node `{}` requires permission `{s}` not listed on the workflow", n.id),
                                Some(&n.id),
                                None,
                            ));
                        }
                    }
                }
            }
            if n.kind == WorkflowNodeKind::Tool {
                let effect = n.config.get("effect").and_then(|v| v.as_str()).unwrap_or("");
                if matches!(effect, "NON_IDEMPOTENT_WRITE" | "IRREVERSIBLE") && n.config.get("idempotency_key_path").is_none() {
                    issues.push(issue(
                        "unsafe_write",
                        format!("tool node `{}` is a non-idempotent write and must declare idempotency_key_path", n.id),
                        Some(&n.id),
                        None,
                    ));
                }
            }
        }

        if let Some(start) = starts.first() {
            let reachable = bfs(start.id.as_str(), &adj);
            for n in &self.nodes {
                if !reachable.contains(n.id.as_str()) {
                    issues.push(issue("unreachable_node", format!("node `{}` is not reachable from start", n.id), Some(&n.id), None));
                }
            }
            let mut rev: HashMap<&str, Vec<&str>> = HashMap::new();
            for e in &self.edges {
                rev.entry(e.target.as_str()).or_default().push(e.source.as_str());
            }
            let can_end: HashSet<&str> = self
                .nodes
                .iter()
                .filter(|n| n.kind == WorkflowNodeKind::End)
                .flat_map(|n| bfs(n.id.as_str(), &rev))
                .collect();
            if !can_end.contains(start.id.as_str()) {
                issues.push(issue("no_terminal_path", "no path from start to an end node".into(), None, None));
            }
        }

        for cycle_node in nodes_in_cycles(&self.nodes, &adj) {
            let node = self.nodes.iter().find(|n| n.id == cycle_node).expect("node exists");
            if node.max_iterations.is_none() {
                issues.push(issue(
                    "unbounded_cycle",
                    format!("node `{}` participates in a cycle and must declare max_iterations", node.id),
                    Some(&node.id),
                    None,
                ));
            }
        }
        issues
    }
}

fn bfs<'a>(start: &'a str, adj: &HashMap<&'a str, Vec<&'a str>>) -> HashSet<&'a str> {
    let mut seen = HashSet::new();
    let mut q = VecDeque::from([start]);
    while let Some(n) = q.pop_front() {
        if seen.insert(n) {
            if let Some(next) = adj.get(n) {
                q.extend(next.iter().copied());
            }
        }
    }
    seen
}

/// Nodes that can reach themselves.
fn nodes_in_cycles<'a>(nodes: &'a [WorkflowNode], adj: &HashMap<&'a str, Vec<&'a str>>) -> Vec<&'a str> {
    nodes
        .iter()
        .filter(|n| {
            adj.get(n.id.as_str())
                .map(|succ| succ.iter().any(|s| bfs(s, adj).contains(n.id.as_str())))
                .unwrap_or(false)
        })
        .map(|n| n.id.as_str())
        .collect()
}

/// Apply RFC 6901 pointer `from` on `src` and set `to` on `dst`.
pub fn apply_mapping(src: &serde_json::Value, dst: &mut serde_json::Value, mapping: &[DataMapping]) {
    if mapping.is_empty() {
        if dst.is_null() {
            *dst = src.clone();
        }
        return;
    }
    for m in mapping {
        let value = pointer_get(src, &m.from).cloned().unwrap_or(serde_json::Value::Null);
        pointer_set(dst, &m.to, value);
    }
}

pub fn pointer_get<'a>(v: &'a serde_json::Value, pointer: &str) -> Option<&'a serde_json::Value> {
    if pointer.is_empty() || pointer == "/" || pointer == "$" {
        return Some(v);
    }
    let p = if let Some(rest) = pointer.strip_prefix("$.") {
        format!("/{}", rest.replace('.', "/"))
    } else {
        pointer.to_string()
    };
    v.pointer(&p)
}

pub fn pointer_set(v: &mut serde_json::Value, pointer: &str, value: serde_json::Value) {
    if pointer.is_empty() || pointer == "/" || pointer == "$" {
        *v = value;
        return;
    }
    let p = if let Some(rest) = pointer.strip_prefix("$.") {
        rest.replace('.', "/")
    } else {
        pointer.trim_start_matches('/').to_string()
    };
    if !v.is_object() {
        *v = serde_json::json!({});
    }
    let mut cur = v;
    let parts: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        if last {
            if let Some(obj) = cur.as_object_mut() {
                obj.insert((*part).to_string(), value);
            }
            return;
        }
        if cur.get(*part).is_none() {
            if let Some(obj) = cur.as_object_mut() {
                obj.insert((*part).to_string(), serde_json::json!({}));
            }
        }
        cur = match cur.get_mut(*part) {
            Some(n) => n,
            None => return,
        };
        if !cur.is_object() {
            *cur = serde_json::json!({});
        }
    }
}

/// Very small condition language: literals, JSON pointers, `==` / `!=`.
pub fn eval_condition(expr: &str, ctx: &serde_json::Value) -> Result<serde_json::Value, String> {
    let expr = expr.trim();
    if expr.is_empty() {
        return Err("empty condition".into());
    }
    if expr == "true" {
        return Ok(serde_json::Value::Bool(true));
    }
    if expr == "false" {
        return Ok(serde_json::Value::Bool(false));
    }
    if let Some((left, right)) = expr.split_once("==") {
        let l = eval_atom(left.trim(), ctx)?;
        let r = eval_atom(right.trim(), ctx)?;
        return Ok(serde_json::Value::Bool(l == r));
    }
    if let Some((left, right)) = expr.split_once("!=") {
        let l = eval_atom(left.trim(), ctx)?;
        let r = eval_atom(right.trim(), ctx)?;
        return Ok(serde_json::Value::Bool(l != r));
    }
    eval_atom(expr, ctx)
}

fn eval_atom(atom: &str, ctx: &serde_json::Value) -> Result<serde_json::Value, String> {
    if atom == "true" {
        return Ok(serde_json::Value::Bool(true));
    }
    if atom == "false" {
        return Ok(serde_json::Value::Bool(false));
    }
    if let Ok(n) = atom.parse::<i64>() {
        return Ok(serde_json::json!(n));
    }
    if (atom.starts_with('"') && atom.ends_with('"')) || (atom.starts_with('\'') && atom.ends_with('\'')) {
        return Ok(serde_json::Value::String(atom[1..atom.len() - 1].into()));
    }
    if atom.starts_with('$') || atom.starts_with('/') {
        return Ok(pointer_get(ctx, atom).cloned().unwrap_or(serde_json::Value::Null));
    }
    Err(format!("unsupported condition atom `{atom}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: &str, kind: WorkflowNodeKind) -> WorkflowNode {
        let config = match kind {
            WorkflowNodeKind::Agent => json!({"agent": "a"}),
            WorkflowNodeKind::Tool => json!({"tool": "t"}),
            WorkflowNodeKind::Wait => json!({"duration_ms": 10}),
            _ => json!({}),
        };
        WorkflowNode {
            id: id.into(),
            kind,
            label: id.into(),
            config,
            max_iterations: None,
            input_schema: json!({}),
            output_schema: json!({}),
            timeout_ms: None,
        }
    }
    fn edge(id: &str, s: &str, t: &str, label: Option<&str>) -> WorkflowEdge {
        WorkflowEdge { id: id.into(), source: s.into(), target: t.into(), label: label.map(String::from), mapping: vec![] }
    }

    fn def(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowDefinition {
        WorkflowDefinition {
            id: None,
            name: "w".into(),
            description: String::new(),
            version: 1,
            nodes,
            edges,
            max_steps: 10,
            input_schema: json!({}),
            output_schema: json!({}),
            error_policy: WorkflowErrorPolicy::default(),
            timeouts: WorkflowTimeouts::default(),
            retry_policy: WorkflowRetryPolicy::default(),
            required_permissions: vec![],
            goal: WorkflowGoal::default(),
        }
    }

    #[test]
    fn linear_workflow_is_valid() {
        let wf = def(
            vec![node("s", WorkflowNodeKind::Start), node("a", WorkflowNodeKind::Agent), node("e", WorkflowNodeKind::End)],
            vec![edge("1", "s", "a", None), edge("2", "a", "e", None)],
        );
        assert!(wf.validate().is_empty(), "{:?}", wf.validate());
    }

    #[test]
    fn parallel_join_is_valid() {
        let wf = def(
            vec![
                node("s", WorkflowNodeKind::Start),
                node("p", WorkflowNodeKind::Parallel),
                node("a", WorkflowNodeKind::Agent),
                node("b", WorkflowNodeKind::Agent),
                WorkflowNode { config: json!({"join": "all"}), ..node("j", WorkflowNodeKind::Join) },
                node("ap", WorkflowNodeKind::Approval),
                node("v", WorkflowNodeKind::Verify),
                node("e", WorkflowNodeKind::End),
            ],
            vec![
                edge("1", "s", "p", None),
                edge("2", "p", "a", None),
                edge("3", "p", "b", None),
                edge("4", "a", "j", None),
                edge("5", "b", "j", None),
                edge("6", "j", "ap", None),
                edge("7", "ap", "v", None),
                edge("8", "v", "e", None),
            ],
        );
        assert!(wf.validate().is_empty(), "{:?}", wf.validate());
    }

    #[test]
    fn detects_common_problems() {
        let wf = def(
            vec![
                node("s", WorkflowNodeKind::Start),
                node("c", WorkflowNodeKind::Condition),
                node("a", WorkflowNodeKind::Agent),
                node("orphan", WorkflowNodeKind::Tool),
                node("e", WorkflowNodeKind::End),
            ],
            vec![
                edge("1", "s", "c", None),
                edge("2", "c", "a", Some("yes")),
                edge("3", "c", "e", None),
                edge("4", "a", "c", None),
                edge("5", "orphan", "e", None),
                edge("6", "ghost", "e", None),
            ],
        );
        let codes: HashSet<String> = wf.validate().into_iter().map(|i| i.code).collect();
        for c in ["unlabeled_branch", "unreachable_node", "dangling_edge", "unbounded_cycle"] {
            assert!(codes.contains(c), "missing {c} in {codes:?}");
        }
    }

    #[test]
    fn rejects_invalid_join() {
        let wf = def(
            vec![
                node("s", WorkflowNodeKind::Start),
                node("a", WorkflowNodeKind::Agent),
                node("j", WorkflowNodeKind::Join),
                node("e", WorkflowNodeKind::End),
            ],
            vec![edge("1", "s", "a", None), edge("2", "a", "j", None), edge("3", "j", "e", None)],
        );
        let codes: HashSet<String> = wf.validate().into_iter().map(|i| i.code).collect();
        assert!(codes.contains("invalid_join"), "{codes:?}");
    }

    #[test]
    fn eval_condition_json_pointer() {
        let ctx = json!({"approved": true, "n": 2});
        assert_eq!(eval_condition("$.approved == true", &ctx).unwrap(), json!(true));
        assert_eq!(eval_condition("/n == 2", &ctx).unwrap(), json!(true));
        assert_eq!(eval_condition("false", &ctx).unwrap(), json!(false));
    }

    #[test]
    fn rejects_oversized_graph() {
        let mut nodes = vec![node("s", WorkflowNodeKind::Start), node("e", WorkflowNodeKind::End)];
        nodes.extend((0..130).map(|i| node(&format!("n{i}"), WorkflowNodeKind::Agent)));
        let wf = def(nodes, vec![edge("1", "s", "e", None)]);
        let codes: HashSet<String> = wf.validate().into_iter().map(|i| i.code).collect();
        assert!(codes.contains("graph_too_large"), "{codes:?}");
    }
}
