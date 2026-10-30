//! OTLP → Axiom execution-event mapping (`otel-genai-v1`).
//!
//! IO-free. Unknown attributes are preserved (after redaction). Missing attributes stay
//! absent. Spans never become goal verification events.

use std::collections::BTreeMap;

use axiom_protocol::{EventStatus, EventType, ExecutionEvent, SCHEMA_VERSION};
use chrono::{DateTime, Utc};
use prost::Message;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::otel_proto::{any_value, AnyValue, ExportTraceServiceRequest, KeyValue};
use crate::redaction::{redact_value, is_secret_key, REDACTED};

/// Version of this mapping. Bump when GenAI semantic-convention interpretation changes.
pub const MAPPING_VERSION: &str = "otel-genai-v1";
pub const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Namespace for deterministic run/event ids derived from trace/span ids.
pub const OTEL_NAMESPACE: Uuid = Uuid::from_bytes([
    0xa1, 0x0e, 0x4e, 0x00, 0x00, 0x04, 0x40, 0x00, 0x80, 0x00, 0x0a, 0x11, 0x07, 0xe1, 0x00, 0x01,
]);

const SENSITIVE_ATTRS: &[&str] = &[
    "gen_ai.prompt",
    "gen_ai.completion",
    "gen_ai.input.messages",
    "gen_ai.output.messages",
    "gen_ai.output.text",
    "input.value",
    "output.value",
    "langsmith.inputs",
    "langsmith.outputs",
    "mlflow.spanInputs",
    "mlflow.spanOutputs",
];

#[derive(Debug, Clone, Default)]
pub struct IncomingSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub kind: i32,
    pub start_unix_nano: u64,
    pub end_unix_nano: u64,
    pub status_code: i32,
    pub status_message: Option<String>,
    pub attributes: BTreeMap<String, Value>,
    pub resource_attributes: BTreeMap<String, Value>,
    pub scope_name: Option<String>,
    pub dropped_attributes_count: u32,
}

#[derive(Debug, Clone)]
pub struct OtelMapOptions {
    pub capture_prompts: bool,
}

impl Default for OtelMapOptions {
    fn default() -> Self {
        Self { capture_prompts: false }
    }
}

#[derive(Debug, Clone)]
pub struct MappedBatch {
    pub events: Vec<ExecutionEvent>,
    pub claimed_project_id: Option<Uuid>,
    pub source_framework: String,
    pub mapping_version: String,
    pub rejected: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpanClass {
    Agent,
    Llm,
    Tool,
    GraphNode,
    Other,
}

pub fn map_spans(spans: &[IncomingSpan], opts: &OtelMapOptions) -> MappedBatch {
    let mut rejected = Vec::new();
    let mut events = Vec::new();
    let mut claimed_project_id = None;
    let mut framework = "otel".to_string();

    for span in spans {
        if let Some(p) = attr_str(span, "axiom.project_id").and_then(|s| Uuid::parse_str(&s).ok()) {
            claimed_project_id = Some(p);
        }
        let fw = detect_framework(span);
        if fw != "otel" {
            framework = fw.clone();
        }
        match map_one(span, opts, &fw) {
            Ok(mut evs) => events.append(&mut evs),
            Err(e) => rejected.push(e),
        }
    }
    if events.iter().any(|e| e.source_framework != "otel") {
        if let Some(e) = events.iter().find(|e| e.source_framework != "otel") {
            framework = e.source_framework.clone();
        }
    }
    MappedBatch {
        events,
        claimed_project_id,
        source_framework: framework,
        mapping_version: MAPPING_VERSION.to_string(),
        rejected,
    }
}

pub fn decode_otlp_protobuf(bytes: &[u8]) -> Result<Vec<IncomingSpan>, String> {
    let req = ExportTraceServiceRequest::decode(bytes).map_err(|e| format!("invalid OTLP protobuf: {e}"))?;
    Ok(request_to_spans(&req))
}

pub fn decode_otlp_json(body: &Value) -> Result<Vec<IncomingSpan>, String> {
    let resource_spans = body
        .get("resourceSpans")
        .or_else(|| body.get("resource_spans"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "OTLP JSON missing resourceSpans".to_string())?;
    let mut out = Vec::new();
    for rs in resource_spans {
        let resource_attrs = json_kvs(rs.get("resource").and_then(|r| r.get("attributes")));
        let scopes = rs
            .get("scopeSpans")
            .or_else(|| rs.get("scope_spans"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for ss in scopes {
            let scope_name = ss
                .get("scope")
                .and_then(|s| s.get("name"))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let spans = ss.get("spans").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            for sp in spans {
                match json_span(&sp, &resource_attrs, scope_name.clone()) {
                    Ok(s) => out.push(s),
                    Err(e) => return Err(e),
                }
            }
        }
    }
    Ok(out)
}

pub fn encode_otlp_protobuf(spans: &[IncomingSpan]) -> Vec<u8> {
    use crate::otel_proto::{InstrumentationScope, Resource, ResourceSpans, ScopeSpans, Span, Status};
    let mut grouped: BTreeMap<String, Vec<&IncomingSpan>> = BTreeMap::new();
    for s in spans {
        grouped.entry(s.trace_id.clone()).or_default().push(s);
    }
    let mut resource_spans = Vec::new();
    for (_, group) in grouped {
        let resource_attrs = group.first().map(|s| s.resource_attributes.clone()).unwrap_or_default();
        let proto_spans: Vec<Span> = group
            .into_iter()
            .map(|s| Span {
                trace_id: hex_to_bytes(&s.trace_id),
                span_id: hex_to_bytes(&s.span_id),
                parent_span_id: s.parent_span_id.as_deref().map(hex_to_bytes).unwrap_or_default(),
                name: s.name.clone(),
                kind: s.kind,
                start_time_unix_nano: s.start_unix_nano,
                end_time_unix_nano: s.end_unix_nano,
                attributes: map_to_kvs(&s.attributes),
                dropped_attributes_count: s.dropped_attributes_count,
                status: Some(Status {
                    message: s.status_message.clone().unwrap_or_default(),
                    code: s.status_code,
                }),
                trace_state: String::new(),
            })
            .collect();
        resource_spans.push(ResourceSpans {
            resource: Some(Resource { attributes: map_to_kvs(&resource_attrs), dropped_attributes_count: 0 }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope { name: "axiom-test".into(), version: ADAPTER_VERSION.into() }),
                spans: proto_spans,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        });
    }
    let mut buf = Vec::new();
    ExportTraceServiceRequest { resource_spans }.encode(&mut buf).expect("encode");
    buf
}

fn request_to_spans(req: &ExportTraceServiceRequest) -> Vec<IncomingSpan> {
    let mut out = Vec::new();
    for rs in &req.resource_spans {
        let resource_attrs = kvs_to_map(rs.resource.as_ref().map(|r| r.attributes.as_slice()).unwrap_or(&[]));
        for ss in &rs.scope_spans {
            let scope_name = ss.scope.as_ref().map(|s| s.name.clone()).filter(|s| !s.is_empty());
            for sp in &ss.spans {
                let trace_id = bytes_to_hex(&sp.trace_id);
                let span_id = bytes_to_hex(&sp.span_id);
                if trace_id.is_empty() || span_id.is_empty() {
                    continue;
                }
                let parent = if sp.parent_span_id.is_empty() { None } else { Some(bytes_to_hex(&sp.parent_span_id)) };
                let (status_code, status_message) = match &sp.status {
                    Some(st) => (st.code, if st.message.is_empty() { None } else { Some(st.message.clone()) }),
                    None => (0, None),
                };
                out.push(IncomingSpan {
                    trace_id,
                    span_id,
                    parent_span_id: parent,
                    name: sp.name.clone(),
                    kind: sp.kind,
                    start_unix_nano: sp.start_time_unix_nano,
                    end_unix_nano: sp.end_time_unix_nano,
                    status_code,
                    status_message,
                    attributes: kvs_to_map(&sp.attributes),
                    resource_attributes: resource_attrs.clone(),
                    scope_name: scope_name.clone(),
                    dropped_attributes_count: sp.dropped_attributes_count,
                });
            }
        }
    }
    out
}

fn map_one(span: &IncomingSpan, opts: &OtelMapOptions, framework: &str) -> Result<Vec<ExecutionEvent>, String> {
    let trace_id = normalize_hex(&span.trace_id).ok_or_else(|| format!("invalid trace_id {}", span.trace_id))?;
    let span_id = normalize_hex(&span.span_id).ok_or_else(|| format!("invalid span_id {}", span.span_id))?;
    let parent = span.parent_span_id.as_deref().and_then(normalize_hex);
    let run_id = run_id_for(span, &trace_id);
    let class = classify(span);
    let failed = span.status_code == 2;
    let status = if failed { EventStatus::Failed } else { EventStatus::Succeeded };
    let duration_ms = duration_ms(span.start_unix_nano, span.end_unix_nano);
    let ts_end = timestamp(span.end_unix_nano, span.start_unix_nano);
    let ts_start = timestamp(span.start_unix_nano, span.end_unix_nano);
    let (payload, missing) = build_payload(span, opts, class);
    let agent_id = attr_str(span, "gen_ai.agent.name")
        .or_else(|| attr_str(span, "agent.name"))
        .or_else(|| attr_str(span, "langgraph.node"))
        .or_else(|| span.resource_attributes.get("service.name").and_then(|v| v.as_str()).map(str::to_string));

    let mut events = Vec::new();
    let is_root = parent.is_none();
    if class == SpanClass::Agent || is_root {
        let start_ty = EventType::RunStarted;
        events.push(base_event(
            run_id, &trace_id, &span_id, parent.clone(), start_ty, ts_start, framework, EventStatus::Started, None, agent_id.clone(), payload.clone(), &missing, span,
        ));
        let end_ty = if failed { EventType::RunFailed } else { EventType::RunCompleted };
        events.push(base_event(
            run_id, &trace_id, &span_id, parent.clone(), end_ty, ts_end, framework, status, duration_ms, agent_id.clone(), payload.clone(), &missing, span,
        ));
        if class == SpanClass::Agent && !is_llm(span) && !is_tool(span) {
            return Ok(events);
        }
    }

    let (ty, st) = match class {
        SpanClass::Llm => {
            if failed {
                (EventType::ModelFailed, EventStatus::Failed)
            } else {
                (EventType::ModelCompleted, EventStatus::Succeeded)
            }
        }
        SpanClass::Tool => {
            if failed {
                (EventType::ToolFailed, EventStatus::Failed)
            } else {
                (EventType::ToolCompleted, EventStatus::Succeeded)
            }
        }
        SpanClass::GraphNode | SpanClass::Other | SpanClass::Agent => (EventType::RunStatusChanged, if failed { EventStatus::Failed } else { EventStatus::Info }),
    };
    if !(class == SpanClass::Agent && is_root) {
        events.push(base_event(
            run_id, &trace_id, &span_id, parent, ty, ts_end, framework, st, duration_ms, agent_id, payload, &missing, span,
        ));
    }

    if let Some(snap) = span.attributes.get("axiom.state_snapshot") {
        let mut p = json!({"snapshot": snap, "source": "otlp_attribute"});
        p = redact_value(&p);
        events.push(base_event(
            run_id,
            &trace_id,
            &span_id,
            span.parent_span_id.clone(),
            EventType::StateSnapshot,
            ts_end,
            framework,
            EventStatus::Info,
            None,
            None,
            p,
            &[],
            span,
        ));
    }
    Ok(events)
}

fn base_event(
    run_id: Uuid,
    trace_id: &str,
    span_id: &str,
    parent: Option<String>,
    event_type: EventType,
    timestamp: DateTime<Utc>,
    framework: &str,
    status: EventStatus,
    duration_ms: Option<i64>,
    agent_id: Option<String>,
    payload: Value,
    missing: &[String],
    span: &IncomingSpan,
) -> ExecutionEvent {
    let key = format!("{}:{}:{}", trace_id, span_id, event_type.as_str());
    let mut metadata = Map::new();
    metadata.insert("mapping".into(), json!(MAPPING_VERSION));
    metadata.insert("raw_source".into(), json!("otlp"));
    metadata.insert("adapter_version".into(), json!(ADAPTER_VERSION));
    metadata.insert("controls_execution".into(), json!(false));
    if !missing.is_empty() {
        metadata.insert("missing_attributes".into(), json!(missing));
    }
    if span.dropped_attributes_count > 0 {
        metadata.insert("dropped_attributes_count".into(), json!(span.dropped_attributes_count));
    }
    if let Some(scope) = &span.scope_name {
        metadata.insert("instrumentation_scope".into(), json!(scope));
    }
    ExecutionEvent {
        schema_version: SCHEMA_VERSION.to_string(),
        event_id: Uuid::new_v5(&OTEL_NAMESPACE, key.as_bytes()),
        run_id,
        trace_id: trace_id.to_string(),
        span_id: span_id.to_string(),
        parent_span_id: parent,
        sequence: 0,
        event_type,
        timestamp,
        agent_id,
        step_id: attr_str(span, "langgraph.step").or_else(|| attr_str(span, "gen_ai.tool.call.id")),
        source_framework: framework.to_string(),
        status: Some(status),
        duration_ms,
        payload,
        metadata: Value::Object(metadata),
    }
}

fn classify(span: &IncomingSpan) -> SpanClass {
    if is_tool(span) {
        return SpanClass::Tool;
    }
    if is_llm(span) {
        return SpanClass::Llm;
    }
    if is_agent(span) {
        return SpanClass::Agent;
    }
    if span.attributes.contains_key("langgraph.node") || span.attributes.contains_key("langgraph.checkpoint_ns") {
        return SpanClass::GraphNode;
    }
    SpanClass::Other
}

fn is_llm(span: &IncomingSpan) -> bool {
    let op = attr_str(span, "gen_ai.operation.name").unwrap_or_default();
    let kind = attr_str(span, "openinference.span.kind").or_else(|| attr_str(span, "langsmith.span.kind")).unwrap_or_default();
    matches!(op.as_str(), "chat" | "completion" | "generate_content" | "text_completion" | "embeddings")
        || matches!(kind.to_ascii_uppercase().as_str(), "LLM" | "GENERATION")
        || span.attributes.contains_key("gen_ai.request.model")
        || span.attributes.contains_key("gen_ai.response.model")
}

fn is_tool(span: &IncomingSpan) -> bool {
    let op = attr_str(span, "gen_ai.operation.name").unwrap_or_default();
    let kind = attr_str(span, "openinference.span.kind").or_else(|| attr_str(span, "langsmith.span.kind")).unwrap_or_default();
    op == "execute_tool"
        || span.attributes.contains_key("gen_ai.tool.name")
        || matches!(kind.to_ascii_uppercase().as_str(), "TOOL" | "FUNCTION")
}

fn is_agent(span: &IncomingSpan) -> bool {
    let op = attr_str(span, "gen_ai.operation.name").unwrap_or_default();
    let kind = attr_str(span, "openinference.span.kind").or_else(|| attr_str(span, "langsmith.span.kind")).unwrap_or_default();
    matches!(op.as_str(), "invoke_agent" | "create_agent")
        || matches!(kind.to_ascii_uppercase().as_str(), "AGENT" | "CHAIN")
        || span.attributes.contains_key("gen_ai.agent.name")
}

fn detect_framework(span: &IncomingSpan) -> String {
    if let Some(v) = attr_str(span, "axiom.source_framework") {
        return v;
    }
    let joined = [
        attr_str(span, "telemetry.sdk.language"),
        span.scope_name.clone(),
        span.resource_attributes.get("service.name").and_then(|v| v.as_str()).map(str::to_string),
        attr_str(span, "gen_ai.system"),
        attr_str(span, "gen_ai.provider.name"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
    .to_ascii_lowercase();
    if joined.contains("langgraph") || span.attributes.keys().any(|k| k.starts_with("langgraph.")) {
        "langgraph".into()
    } else if joined.contains("openai-agents") || joined.contains("openai.agents") || span.attributes.keys().any(|k| k.starts_with("openai.agent")) {
        "openai-agents".into()
    } else {
        "otel".into()
    }
}

fn build_payload(span: &IncomingSpan, opts: &OtelMapOptions, class: SpanClass) -> (Value, Vec<String>) {
    let mut missing = Vec::new();
    let mut obj = Map::new();
    obj.insert("span_name".into(), json!(span.name));
    if class == SpanClass::Other {
        obj.insert("unmapped_span".into(), json!(true));
    }
    if let Some(m) = attr_str(span, "gen_ai.request.model").or_else(|| attr_str(span, "gen_ai.response.model")) {
        obj.insert("model".into(), json!(m));
    } else if class == SpanClass::Llm {
        missing.push("gen_ai.request.model".into());
    }
    if let Some(t) = attr_str(span, "gen_ai.tool.name").or_else(|| attr_str(span, "tool.name")) {
        obj.insert("tool".into(), json!(t));
    } else if class == SpanClass::Tool {
        missing.push("gen_ai.tool.name".into());
    }
    match int_attr(span, "gen_ai.usage.input_tokens").or_else(|| int_attr(span, "llm.token_count.prompt")) {
        Some(n) => {
            obj.insert("input_tokens".into(), json!(n));
        }
        None if class == SpanClass::Llm => missing.push("gen_ai.usage.input_tokens".into()),
        None => {}
    }
    match int_attr(span, "gen_ai.usage.output_tokens").or_else(|| int_attr(span, "llm.token_count.completion")) {
        Some(n) => {
            obj.insert("output_tokens".into(), json!(n));
        }
        None if class == SpanClass::Llm => missing.push("gen_ai.usage.output_tokens".into()),
        None => {}
    }
    if let Some(msg) = &span.status_message {
        obj.insert("error".into(), json!(msg));
    }
    if span.start_unix_nano == 0 {
        missing.push("start_time_unix_nano".into());
    }
    let mut attrs = Map::new();
    for (k, v) in span.attributes.iter().chain(span.resource_attributes.iter()) {
        if should_drop_prompt(k, opts) {
            attrs.insert(k.clone(), json!(REDACTED));
        } else if is_secret_key(k) {
            attrs.insert(k.clone(), json!(REDACTED));
        } else {
            attrs.insert(k.clone(), redact_value(v));
        }
    }
    obj.insert("attributes".into(), Value::Object(attrs));
    // Do not run key-based redaction on the envelope: `input_tokens` contains the
    // fragment `token` but is a usage count, not a credential.
    (Value::Object(obj), missing)
}

fn should_drop_prompt(key: &str, opts: &OtelMapOptions) -> bool {
    if opts.capture_prompts {
        return false;
    }
    SENSITIVE_ATTRS.iter().any(|s| key.eq_ignore_ascii_case(s)) || key.to_ascii_lowercase().contains("prompt") && key.to_ascii_lowercase().contains("gen_ai")
}

fn run_id_for(span: &IncomingSpan, trace_id: &str) -> Uuid {
    if let Some(s) = attr_str(span, "axiom.run_id") {
        if let Ok(id) = Uuid::parse_str(&s) {
            return id;
        }
    }
    Uuid::new_v5(&OTEL_NAMESPACE, trace_id.as_bytes())
}

fn attr_str(span: &IncomingSpan, key: &str) -> Option<String> {
    span.attributes
        .get(key)
        .or_else(|| span.resource_attributes.get(key))
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        })
}

fn int_attr(span: &IncomingSpan, key: &str) -> Option<i64> {
    span.attributes.get(key).or_else(|| span.resource_attributes.get(key)).and_then(|v| match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    })
}

fn duration_ms(start: u64, end: u64) -> Option<i64> {
    if start == 0 || end == 0 || end < start {
        return None;
    }
    Some(((end - start) / 1_000_000) as i64)
}

fn timestamp(primary: u64, fallback: u64) -> DateTime<Utc> {
    let nano = if primary > 0 { primary } else { fallback };
    if nano == 0 {
        return DateTime::<Utc>::UNIX_EPOCH;
    }
    let secs = (nano / 1_000_000_000) as i64;
    let nsecs = (nano % 1_000_000_000) as u32;
    DateTime::from_timestamp(secs, nsecs).unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

fn normalize_hex(s: &str) -> Option<String> {
    let t = s.trim().trim_start_matches("0x").to_ascii_lowercase();
    if t.is_empty() || t.len() % 2 != 0 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(t)
}

fn bytes_to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hex_to_bytes(s: &str) -> Vec<u8> {
    let t = s.trim().trim_start_matches("0x");
    (0..t.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(t.get(i..i + 2)?, 16).ok())
        .collect()
}

fn kvs_to_map(kvs: &[KeyValue]) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    for kv in kvs {
        if let Some(v) = kv.value.as_ref().and_then(any_to_json) {
            m.insert(kv.key.clone(), v);
        }
    }
    m
}

fn map_to_kvs(map: &BTreeMap<String, Value>) -> Vec<KeyValue> {
    map.iter()
        .map(|(k, v)| KeyValue { key: k.clone(), value: Some(json_to_any(v)) })
        .collect()
}

fn any_to_json(v: &AnyValue) -> Option<Value> {
    match v.value.as_ref()? {
        any_value::Value::StringValue(s) => Some(Value::String(s.clone())),
        any_value::Value::BoolValue(b) => Some(Value::Bool(*b)),
        any_value::Value::IntValue(n) => Some(json!(*n)),
        any_value::Value::DoubleValue(n) => Some(json!(*n)),
        any_value::Value::BytesValue(b) => Some(Value::String(bytes_to_hex(b))),
    }
}

fn json_to_any(v: &Value) -> AnyValue {
    let inner = match v {
        Value::String(s) => any_value::Value::StringValue(s.clone()),
        Value::Bool(b) => any_value::Value::BoolValue(*b),
        Value::Number(n) if n.as_i64().is_some() => any_value::Value::IntValue(n.as_i64().unwrap()),
        Value::Number(n) => any_value::Value::DoubleValue(n.as_f64().unwrap_or(0.0)),
        other => any_value::Value::StringValue(other.to_string()),
    };
    AnyValue { value: Some(inner) }
}

fn json_kvs(v: Option<&Value>) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    let Some(arr) = v.and_then(|x| x.as_array()) else { return m };
    for item in arr {
        let key = item.get("key").and_then(|k| k.as_str()).unwrap_or("");
        if key.is_empty() {
            continue;
        }
        if let Some(val) = item.get("value").and_then(json_any) {
            m.insert(key.to_string(), val);
        }
    }
    m
}

fn json_any(v: &Value) -> Option<Value> {
    if let Some(s) = v.get("stringValue").or_else(|| v.get("string_value")).and_then(|x| x.as_str()) {
        return Some(Value::String(s.to_string()));
    }
    if let Some(b) = v.get("boolValue").or_else(|| v.get("bool_value")).and_then(|x| x.as_bool()) {
        return Some(Value::Bool(b));
    }
    if let Some(n) = v.get("intValue").or_else(|| v.get("int_value")) {
        return match n {
            Value::Number(num) => Some(json!(num.as_i64().unwrap_or(0))),
            Value::String(s) => Some(json!(s.parse::<i64>().ok()?)),
            _ => None,
        };
    }
    if let Some(n) = v.get("doubleValue").or_else(|| v.get("double_value")).and_then(|x| x.as_f64()) {
        return Some(json!(n));
    }
    None
}

fn json_span(sp: &Value, resource_attrs: &BTreeMap<String, Value>, scope_name: Option<String>) -> Result<IncomingSpan, String> {
    let trace_id = sp.get("traceId").or_else(|| sp.get("trace_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let span_id = sp.get("spanId").or_else(|| sp.get("span_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    if trace_id.is_empty() || span_id.is_empty() {
        return Err("span missing traceId/spanId".into());
    }
    let parent_raw = sp.get("parentSpanId").or_else(|| sp.get("parent_span_id")).and_then(|v| v.as_str()).unwrap_or("");
    let parent = if parent_raw.is_empty() { None } else { Some(parent_raw.to_string()) };
    let status = sp.get("status");
    let status_code = status.and_then(|s| s.get("code")).and_then(json_int).unwrap_or(0);
    let status_message = status.and_then(|s| s.get("message")).and_then(|v| v.as_str()).map(str::to_string);
    Ok(IncomingSpan {
        trace_id,
        span_id,
        parent_span_id: parent,
        name: sp.get("name").and_then(|v| v.as_str()).unwrap_or("span").to_string(),
        kind: sp.get("kind").and_then(json_int).unwrap_or(0) as i32,
        start_unix_nano: sp.get("startTimeUnixNano").or_else(|| sp.get("start_time_unix_nano")).and_then(json_u64).unwrap_or(0),
        end_unix_nano: sp.get("endTimeUnixNano").or_else(|| sp.get("end_time_unix_nano")).and_then(json_u64).unwrap_or(0),
        status_code: status_code as i32,
        status_message,
        attributes: json_kvs(sp.get("attributes")),
        resource_attributes: resource_attrs.clone(),
        scope_name,
        dropped_attributes_count: sp.get("droppedAttributesCount").or_else(|| sp.get("dropped_attributes_count")).and_then(json_u64).unwrap_or(0) as u32,
    })
}

fn json_int(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn json_u64(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64().or_else(|| n.as_i64().map(|i| i as u64)),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn llm_span() -> IncomingSpan {
        let mut attributes = BTreeMap::new();
        attributes.insert("gen_ai.operation.name".into(), json!("chat"));
        attributes.insert("gen_ai.request.model".into(), json!("gpt-4o-mini"));
        attributes.insert("gen_ai.usage.input_tokens".into(), json!(11));
        attributes.insert("gen_ai.usage.output_tokens".into(), json!(5));
        attributes.insert("gen_ai.prompt".into(), json!("secret user text"));
        IncomingSpan {
            trace_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            span_id: "bbbbbbbbbbbbbbbb".into(),
            parent_span_id: Some("cccccccccccccccc".into()),
            name: "chat gpt-4o-mini".into(),
            kind: 3,
            start_unix_nano: 1_000_000_000,
            end_unix_nano: 1_500_000_000,
            status_code: 1,
            status_message: None,
            attributes,
            resource_attributes: BTreeMap::from([("service.name".into(), json!("demo"))]),
            scope_name: Some("opentelemetry.instrumentation.openai".into()),
            dropped_attributes_count: 0,
        }
    }

    #[test]
    fn llm_maps_to_model_completed_without_verification() {
        let batch = map_spans(&[llm_span()], &OtelMapOptions::default());
        assert!(batch.events.iter().any(|e| e.event_type == EventType::ModelCompleted));
        assert!(!batch.events.iter().any(|e| matches!(
            e.event_type,
            EventType::VerificationPassed | EventType::GoalSatisfied | EventType::VerificationFailed
        )));
        let ev = batch.events.iter().find(|e| e.event_type == EventType::ModelCompleted).unwrap();
        assert_eq!(ev.payload["input_tokens"], 11);
        assert_eq!(ev.payload["output_tokens"], 5);
        assert_eq!(ev.payload["attributes"]["gen_ai.prompt"], REDACTED);
        assert_eq!(ev.metadata["controls_execution"], false);
        assert_eq!(ev.parent_span_id.as_deref(), Some("cccccccccccccccc"));
        assert_eq!(ev.duration_ms, Some(500));
    }

    #[test]
    fn missing_tokens_stay_unknown() {
        let mut s = llm_span();
        s.attributes.remove("gen_ai.usage.input_tokens");
        s.attributes.remove("gen_ai.usage.output_tokens");
        let batch = map_spans(&[s], &OtelMapOptions::default());
        let ev = batch.events.iter().find(|e| e.event_type == EventType::ModelCompleted).unwrap();
        assert!(ev.payload.get("input_tokens").is_none());
        assert!(ev.payload.get("output_tokens").is_none());
        let missing = ev.metadata["missing_attributes"].as_array().unwrap();
        assert!(missing.iter().any(|v| v == "gen_ai.usage.input_tokens"));
    }

    #[test]
    fn tool_and_langgraph_framework() {
        let mut attributes = BTreeMap::new();
        attributes.insert("gen_ai.tool.name".into(), json!("submit_refund"));
        attributes.insert("langgraph.node".into(), json!("refund"));
        let span = IncomingSpan {
            trace_id: "11111111111111111111111111111111".into(),
            span_id: "2222222222222222".into(),
            parent_span_id: None,
            name: "tools.submit_refund".into(),
            kind: 1,
            start_unix_nano: 2,
            end_unix_nano: 4,
            status_code: 1,
            status_message: None,
            attributes,
            resource_attributes: BTreeMap::from([("service.name".into(), json!("langgraph-refund"))]),
            scope_name: Some("langgraph".into()),
            dropped_attributes_count: 0,
        };
        let batch = map_spans(&[span], &OtelMapOptions::default());
        assert_eq!(batch.source_framework, "langgraph");
        assert!(batch.events.iter().any(|e| e.event_type == EventType::ToolCompleted));
        assert!(batch.events.iter().any(|e| e.event_type == EventType::RunCompleted));
        // External completion is not a verified goal.
        assert!(!batch.events.iter().any(|e| e.event_type == EventType::GoalSatisfied));
    }

    #[test]
    fn error_span_is_failed_not_verified() {
        let mut s = llm_span();
        s.status_code = 2;
        s.status_message = Some("rate limited".into());
        let batch = map_spans(&[s], &OtelMapOptions::default());
        let ev = batch.events.iter().find(|e| e.event_type == EventType::ModelFailed).unwrap();
        assert_eq!(ev.status, Some(EventStatus::Failed));
        assert_eq!(ev.payload["error"], "rate limited");
    }

    #[test]
    fn same_trace_shares_run_id_and_event_ids_are_stable() {
        let a = llm_span();
        let mut b = llm_span();
        b.span_id = "dddddddddddddddd".into();
        let first = map_spans(&[a.clone(), b.clone()], &OtelMapOptions::default());
        let second = map_spans(&[a, b], &OtelMapOptions::default());
        assert_eq!(first.events[0].run_id, first.events.last().unwrap().run_id);
        let ids: Vec<_> = first.events.iter().map(|e| e.event_id).collect();
        let ids2: Vec<_> = second.events.iter().map(|e| e.event_id).collect();
        assert_eq!(ids, ids2);
    }

    #[test]
    fn json_and_protobuf_roundtrip() {
        let span = llm_span();
        let bytes = encode_otlp_protobuf(&[span.clone()]);
        let decoded = decode_otlp_protobuf(&bytes).unwrap();
        assert_eq!(decoded[0].trace_id, span.trace_id);
        assert_eq!(decoded[0].attributes["gen_ai.request.model"], "gpt-4o-mini");
        let json_body = json!({
            "resourceSpans": [{
                "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "demo"}}]},
                "scopeSpans": [{
                    "spans": [{
                        "traceId": span.trace_id,
                        "spanId": span.span_id,
                        "parentSpanId": span.parent_span_id,
                        "name": span.name,
                        "kind": 3,
                        "startTimeUnixNano": span.start_unix_nano.to_string(),
                        "endTimeUnixNano": span.end_unix_nano.to_string(),
                        "status": {"code": 1},
                        "attributes": [
                            {"key": "gen_ai.operation.name", "value": {"stringValue": "chat"}},
                            {"key": "gen_ai.usage.input_tokens", "value": {"intValue": "11"}}
                        ]
                    }]
                }]
            }]
        });
        let from_json = decode_otlp_json(&json_body).unwrap();
        assert_eq!(from_json[0].attributes["gen_ai.usage.input_tokens"], 11);
        ExportTraceServiceRequest::decode(bytes.as_slice()).unwrap();
    }

    #[test]
    fn does_not_fabricate_parent() {
        let mut s = llm_span();
        s.parent_span_id = Some("deadbeefdeadbeef".into());
        let batch = map_spans(&[s], &OtelMapOptions::default());
        let ev = batch.events.iter().find(|e| e.event_type == EventType::ModelCompleted).unwrap();
        assert_eq!(ev.parent_span_id.as_deref(), Some("deadbeefdeadbeef"));
        assert!(!batch.events.iter().any(|e| e.span_id == "deadbeefdeadbeef"));
    }
}
