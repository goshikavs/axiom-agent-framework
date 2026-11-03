//! Branded environment variables.
//!
//! New deployments use `GOSHIKQ_*`. Existing `AXIOM_*` names are read only when
//! the Goshikq equivalent is unset, so local `.env` files and CI secrets keep working.

/// Read `GOSHIKQ_{suffix}`, then the legacy `AXIOM_{suffix}` name.
pub fn branded(suffix: &str) -> Option<String> {
    read(&format!("GOSHIKQ_{suffix}")).or_else(|| read(&format!("AXIOM_{suffix}")))
}

fn read(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// True when either the current or legacy update-schemas flag is set.
pub fn update_schemas() -> bool {
    std::env::var("GOSHIKQ_UPDATE_SCHEMAS").is_ok() || std::env::var("AXIOM_UPDATE_SCHEMAS").is_ok()
}
