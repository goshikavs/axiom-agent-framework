//! JSON Schema export for the protocol types. `protocol/schemas/*.json` in the repository
//! are generated from here and checked by a contract test so they cannot drift.

use schemars::{schema_for, Schema};

pub struct NamedSchema {
    pub file_name: &'static str,
    pub schema: Schema,
}

/// All schemas published under `protocol/schemas/`.
pub fn all() -> Vec<NamedSchema> {
    vec![
        NamedSchema { file_name: "execution-event.v1.json", schema: schema_for!(crate::ExecutionEvent) },
        NamedSchema { file_name: "event-batch.v1.json", schema: schema_for!(crate::EventBatch) },
        NamedSchema { file_name: "create-run-request.v1.json", schema: schema_for!(crate::CreateRunRequest) },
        NamedSchema { file_name: "create-task-request.v1.json", schema: schema_for!(crate::CreateTaskRequest) },
        NamedSchema { file_name: "coordination-policy.v1.json", schema: schema_for!(crate::CoordinationPolicy) },
        NamedSchema { file_name: "worker-request.v1.json", schema: schema_for!(crate::WorkerRequest) },
        NamedSchema { file_name: "worker-response.v1.json", schema: schema_for!(crate::WorkerResponse) },
        NamedSchema { file_name: "model-generate-result.v1.json", schema: schema_for!(crate::ModelGenerateResult) },
        NamedSchema { file_name: "goal-evaluate-result.v1.json", schema: schema_for!(crate::GoalEvaluateResult) },
        NamedSchema { file_name: "state-snapshot-result.v1.json", schema: schema_for!(crate::StateSnapshotResult) },
        NamedSchema { file_name: "action-verify-result.v1.json", schema: schema_for!(crate::ActionVerifyResult) },
    ]
}

/// Serialize a schema with stable formatting (2-space indent, trailing newline).
pub fn render(schema: &Schema) -> String {
    let mut s = serde_json::to_string_pretty(schema).expect("schema serializes");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn schemas_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../protocol/schemas")
    }

    /// Regenerate with `AXIOM_UPDATE_SCHEMAS=1 cargo test -p axiom-protocol`.
    #[test]
    fn published_schemas_match_source() {
        let dir = schemas_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let update = std::env::var("AXIOM_UPDATE_SCHEMAS").is_ok();
        let mut drift = vec![];
        for named in all() {
            let path = dir.join(named.file_name);
            let rendered = render(&named.schema);
            if update || !path.exists() {
                std::fs::write(&path, &rendered).unwrap();
                continue;
            }
            let on_disk = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
            if on_disk != rendered {
                drift.push(named.file_name);
            }
        }
        assert!(drift.is_empty(), "schemas drifted: {drift:?}; run with AXIOM_UPDATE_SCHEMAS=1");
    }

    #[test]
    fn sample_event_validates_against_schema() {
        let schema = schema_for!(crate::ExecutionEvent);
        let validator = jsonschema::validator_for(&serde_json::to_value(&schema).unwrap()).unwrap();
        let ev = crate::ExecutionEvent::native(uuid::Uuid::new_v4(), crate::new_trace_id(), crate::EventType::RunCreated);
        let v = serde_json::to_value(&ev).unwrap();
        assert!(validator.validate(&v).is_ok());
        let mut bad = v.clone();
        bad["event_type"] = serde_json::json!("tool.exploded");
        assert!(validator.validate(&bad).is_err());
    }
}
