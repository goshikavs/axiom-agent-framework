//! Field-level diff between two JSON state snapshots.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct FieldChange {
    /// JSON-pointer-like path, e.g. `/order/status`.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
}

pub fn diff(before: &Value, after: &Value) -> Vec<FieldChange> {
    let mut out = Vec::new();
    walk("", before, after, &mut out);
    out
}

fn walk(path: &str, a: &Value, b: &Value, out: &mut Vec<FieldChange>) {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            let mut keys: Vec<&String> = ma.keys().chain(mb.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let p = format!("{path}/{k}");
                match (ma.get(k), mb.get(k)) {
                    (Some(x), Some(y)) => walk(&p, x, y, out),
                    (Some(x), None) => out.push(FieldChange {
                        path: p,
                        before: Some(x.clone()),
                        after: None,
                    }),
                    (None, Some(y)) => out.push(FieldChange {
                        path: p,
                        before: None,
                        after: Some(y.clone()),
                    }),
                    (None, None) => {}
                }
            }
        }
        _ if a != b => out.push(FieldChange {
            path: if path.is_empty() {
                "/".into()
            } else {
                path.into()
            },
            before: Some(a.clone()),
            after: Some(b.clone()),
        }),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_nested_changes() {
        let a = json!({"order": {"status": "paid", "total": 10}, "refund": null});
        let b =
            json!({"order": {"status": "refunded", "total": 10}, "refund": {"id": "r1"}, "new": 1});
        let d = diff(&a, &b);
        let paths: Vec<_> = d.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, vec!["/new", "/order/status", "/refund"]);
    }
}
