//! `goshikq-protocol` defines the framework-neutral execution event envelope and the typed
//! contracts exchanged between the Rust runtime, SDK workers, external framework
//! adapters, and Goshikq Studio.
//!
//! Everything in this crate is plain data: no IO, no async. The JSON Schemas exported to
//! `protocol/schemas/` are generated from these types (see `schemas::export`).

pub mod capability;
pub mod coverage;
pub mod event;
pub mod evidence;
pub mod multi_agent;
pub mod recovery;
pub mod run;
pub mod schemas;
pub mod worker;

pub use capability::*;
pub use coverage::*;
pub use event::*;
pub use evidence::*;
pub use multi_agent::*;
pub use recovery::*;
pub use run::*;
pub use worker::*;

/// Current protocol schema version. Minor bumps add event types or optional fields;
/// major bumps may rename or remove.
pub const SCHEMA_VERSION: &str = "1.0";
