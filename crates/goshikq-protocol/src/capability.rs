//! Interoperability capability manifests.
//!
//! A manifest states what an adapter actually supports. Unsupported surfaces stay
//! unsupported — they are not implied by the presence of traces.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How completely a surface is implemented for a framework adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityLevel {
    Supported,
    Partial,
    Unsupported,
}

impl CapabilityLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Partial => "partial",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Published by every adapter. Studio renders these as indicators, not as health of the
/// remote framework itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct FrameworkCapabilityManifest {
    pub framework: String,
    /// Framework release the adapter was tested against, or `unknown`.
    pub framework_version: String,
    pub adapter_version: String,
    pub trace_support: CapabilityLevel,
    pub tool_event_support: CapabilityLevel,
    pub state_snapshot_support: CapabilityLevel,
    pub approval_support: CapabilityLevel,
    pub action_interception_support: CapabilityLevel,
    pub pause_resume_support: CapabilityLevel,
    pub outcome_verification_support: CapabilityLevel,
    pub replay_support: CapabilityLevel,
    /// Human-readable limits. Missing data stays unknown.
    pub notes: String,
}

impl FrameworkCapabilityManifest {
    pub fn builtin() -> Vec<Self> {
        vec![
            goshikq_native(),
            goshikq_python(),
            goshikq_typescript(),
            otel_http(),
            langgraph(),
            openai_agents(),
            mcp(),
            a2a(),
        ]
    }
}

fn goshikq_native() -> FrameworkCapabilityManifest {
    FrameworkCapabilityManifest {
        framework: "goshikq".into(),
        framework_version: env!("CARGO_PKG_VERSION").into(),
        adapter_version: env!("CARGO_PKG_VERSION").into(),
        trace_support: CapabilityLevel::Supported,
        tool_event_support: CapabilityLevel::Supported,
        state_snapshot_support: CapabilityLevel::Supported,
        approval_support: CapabilityLevel::Supported,
        action_interception_support: CapabilityLevel::Supported,
        pause_resume_support: CapabilityLevel::Supported,
        outcome_verification_support: CapabilityLevel::Supported,
        replay_support: CapabilityLevel::Partial,
        notes: "Native worker protocol. Replay UI is recorded events only. Historical state comes from persisted snapshots, not a live GET /state.".into(),
    }
}

fn goshikq_python() -> FrameworkCapabilityManifest {
    let mut m = goshikq_native();
    m.framework = "goshikq-python".into();
    m.notes = "Native Python SDK. Tools and predicates run in the SDK worker; Rust owns the execution loop.".into();
    m
}

fn goshikq_typescript() -> FrameworkCapabilityManifest {
    let mut m = goshikq_native();
    m.framework = "goshikq-typescript".into();
    m.notes = "Native TypeScript/Bun SDK. Same worker protocol as Python. Arbitrary TypeScript closures are not resumed after a crash; only persisted runtime state is recovered.".into();
    m
}

fn mcp() -> FrameworkCapabilityManifest {
    FrameworkCapabilityManifest {
        framework: "mcp".into(),
        framework_version: "2024-11-05".into(),
        adapter_version: env!("CARGO_PKG_VERSION").into(),
        trace_support: CapabilityLevel::Supported,
        tool_event_support: CapabilityLevel::Supported,
        state_snapshot_support: CapabilityLevel::Unsupported,
        approval_support: CapabilityLevel::Supported,
        action_interception_support: CapabilityLevel::Partial,
        pause_resume_support: CapabilityLevel::Unsupported,
        outcome_verification_support: CapabilityLevel::Partial,
        replay_support: CapabilityLevel::Unsupported,
        notes: "MCP tools are wrapped as Goshikq tools. Server descriptions cannot override allowlists, permissions, or effect. MCP text is not authoritative business state.".into(),
    }
}

fn otel_http() -> FrameworkCapabilityManifest {
    FrameworkCapabilityManifest {
        framework: "otel".into(),
        framework_version: "otlp-1.5".into(),
        adapter_version: env!("CARGO_PKG_VERSION").into(),
        trace_support: CapabilityLevel::Supported,
        tool_event_support: CapabilityLevel::Partial,
        state_snapshot_support: CapabilityLevel::Partial,
        approval_support: CapabilityLevel::Unsupported,
        action_interception_support: CapabilityLevel::Unsupported,
        pause_resume_support: CapabilityLevel::Unsupported,
        outcome_verification_support: CapabilityLevel::Unsupported,
        replay_support: CapabilityLevel::Unsupported,
        notes: "OTLP/HTTP protobuf and JSON traces. Mapping otel-genai-v1. Tool/LLM classification requires GenAI or OpenInference attributes. Goshikq does not control the traced process. Spans are not verified business outcomes.".into(),
    }
}

fn langgraph() -> FrameworkCapabilityManifest {
    FrameworkCapabilityManifest {
        framework: "langgraph".into(),
        framework_version: "unknown".into(),
        adapter_version: env!("CARGO_PKG_VERSION").into(),
        trace_support: CapabilityLevel::Supported,
        tool_event_support: CapabilityLevel::Partial,
        state_snapshot_support: CapabilityLevel::Partial,
        approval_support: CapabilityLevel::Unsupported,
        action_interception_support: CapabilityLevel::Unsupported,
        pause_resume_support: CapabilityLevel::Unsupported,
        outcome_verification_support: CapabilityLevel::Partial,
        replay_support: CapabilityLevel::Unsupported,
        notes: "LangGraph remains the executor. Goshikq ingests OTLP spans emitted by the example adapter. Optional goshikq.state_snapshot attribute can record authoritative state; it is not inferred. Goshikq does not intercept LangGraph node writes.".into(),
    }
}

fn openai_agents() -> FrameworkCapabilityManifest {
    FrameworkCapabilityManifest {
        framework: "openai-agents".into(),
        framework_version: "unknown".into(),
        adapter_version: env!("CARGO_PKG_VERSION").into(),
        trace_support: CapabilityLevel::Supported,
        tool_event_support: CapabilityLevel::Partial,
        state_snapshot_support: CapabilityLevel::Partial,
        approval_support: CapabilityLevel::Unsupported,
        action_interception_support: CapabilityLevel::Unsupported,
        pause_resume_support: CapabilityLevel::Unsupported,
        outcome_verification_support: CapabilityLevel::Partial,
        replay_support: CapabilityLevel::Unsupported,
        notes: "Tracing-only compatibility via documented TracingProcessor hooks (tested with openai-agents 0.24.0). Goshikq does not execute Runner.run, intercept tool calls, or verify OpenAI Agents SDK outcomes. Do not treat ingested traces as VERIFIED.".into(),
    }
}

fn a2a() -> FrameworkCapabilityManifest {
    FrameworkCapabilityManifest {
        framework: "a2a".into(),
        framework_version: crate::A2A_PROTOCOL_VERSION.into(),
        adapter_version: env!("CARGO_PKG_VERSION").into(),
        trace_support: CapabilityLevel::Partial,
        tool_event_support: CapabilityLevel::Unsupported,
        state_snapshot_support: CapabilityLevel::Unsupported,
        approval_support: CapabilityLevel::Unsupported,
        action_interception_support: CapabilityLevel::Unsupported,
        pause_resume_support: CapabilityLevel::Unsupported,
        outcome_verification_support: CapabilityLevel::Unsupported,
        replay_support: CapabilityLevel::Unsupported,
        notes: "A2A protocol 1.0 JSON-RPC subset: Agent Card, SendMessage, GetTask, CancelTask. Remote task completion is not Goshikq parent-goal verification. gRPC and HTTP+JSON bindings, streaming, and push notifications are unsupported.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_covers_required_frameworks() {
        let names: Vec<_> = FrameworkCapabilityManifest::builtin()
            .into_iter()
            .map(|m| m.framework)
            .collect();
        assert!(names.contains(&"goshikq".into()));
        assert!(names.contains(&"goshikq-python".into()));
        assert!(names.contains(&"goshikq-typescript".into()));
        assert!(names.contains(&"otel".into()));
        assert!(names.contains(&"langgraph".into()));
        assert!(names.contains(&"openai-agents".into()));
        assert!(names.contains(&"mcp".into()));
        assert!(names.contains(&"a2a".into()));
    }

    #[test]
    fn external_adapters_do_not_claim_interception() {
        for m in FrameworkCapabilityManifest::builtin() {
            if m.framework.starts_with("goshikq") || m.framework == "mcp" {
                continue;
            }
            assert_eq!(m.action_interception_support, CapabilityLevel::Unsupported);
            assert_eq!(m.approval_support, CapabilityLevel::Unsupported);
        }
    }
}
