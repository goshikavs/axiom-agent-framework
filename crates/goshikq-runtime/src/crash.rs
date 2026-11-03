//! Test-only process abort. Production servers leave `GOSHIKQ_CRASH_AT` unset.

use goshikq_protocol::{CrashBarrier, EffectCategory};
use uuid::Uuid;

/// Abort the process with status 97 when `GOSHIKQ_CRASH_AT` matches `barrier`.
/// Optional `GOSHIKQ_CRASH_RUN_ID` scopes the abort to one run.
pub fn crash_at(barrier: CrashBarrier, run_id: Uuid) {
    let wanted = match goshikq_core::env::branded("CRASH_AT") {
        Some(s) => s,
        None => return,
    };
    if CrashBarrier::parse(&wanted) != Some(barrier) {
        return;
    }
    if let Some(only) = goshikq_core::env::branded("CRASH_RUN_ID") {
        if !only.is_empty() && only != run_id.to_string() {
            return;
        }
    }
    tracing::error!(barrier = barrier.as_str(), run_id = %run_id, "GOSHIKQ_CRASH_AT: aborting process");
    std::process::exit(97);
}

/// Same as [`crash_at`], but ignored for read-only tools so barriers hit the write.
pub fn crash_at_write(barrier: CrashBarrier, run_id: Uuid, effect: EffectCategory) {
    if matches!(effect, EffectCategory::ReadOnly) {
        return;
    }
    crash_at(barrier, run_id);
}
