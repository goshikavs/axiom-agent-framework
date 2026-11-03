//! `goshikq-core` holds the pure, IO-free rules of the Goshikq runtime:
//!
//! - [`state_machine`]: valid run status transitions
//! - [`actions`]: tool-call validation, permission checks, retry safety
//! - [`redaction`]: secret masking applied before anything is persisted
//! - [`state_diff`]: field-level diffs between authoritative state snapshots
//! - [`pricing`]: versioned model pricing for cost estimates
//! - [`workflow`]: workflow graph schema and validation shared by Studio and runtime
//! - [`impact`]: evidence dependency graph and deterministic change-impact
//! - [`revalidation`]: selective revalidation planner (no LLM)
//!
//! Everything here is unit-tested independently of the HTTP layer and the UI.

pub mod actions;
pub mod baselines;
pub mod coverage;
pub mod env;
pub mod gates;
pub mod impact;
pub mod otel;
pub mod otel_proto;
pub mod pricing;
pub mod recovery;
pub mod redaction;
pub mod revalidation;
pub mod state_diff;
pub mod state_machine;
pub mod workflow;

pub use actions::*;
pub use baselines::{
    run_strategy, run_strategy_with_context, RevalidationStrategy, StrategyContext, StrategyOutput,
};
pub use coverage::{evaluate_coverage, resolve_contract_templates, CoverageReport};
pub use gates::{evaluate_action_gate, GateDecision};
pub use impact::{compute_impact, EvidenceGraph, GraphEdge, GraphNode, ImpactReport};
pub use otel::{
    decode_otlp_json, decode_otlp_protobuf, encode_otlp_protobuf, map_spans, IncomingSpan,
    MappedBatch, OtelMapOptions, ADAPTER_VERSION, MAPPING_VERSION,
};
pub use recovery::*;
pub use revalidation::{plan_revalidation, RevalidationPlan};
pub use state_machine::*;
