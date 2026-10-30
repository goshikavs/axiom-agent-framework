//! The Axiom runtime: owns run lifecycles, dispatches typed work to SDK workers, enforces
//! limits/permissions/approvals, persists events and checkpoints, and orchestrates
//! verification. See ADR-001 and ADR-003.

pub mod bridge;
pub mod crash;
pub mod executor;
pub mod orchestration;
pub mod publisher;
pub mod runtime;
pub mod supervisor;
pub(crate) mod workflow_exec;

pub use bridge::{AskError, RunHandle};
pub use runtime::{CaptureMode, Runtime, RuntimeConfig, RuntimeError, RuntimeMetrics};
