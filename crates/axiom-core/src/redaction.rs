//! Secret redaction applied to anything that may be persisted or streamed.
//!
//! Two layers: key-based (field names that conventionally hold secrets) and
//! pattern-based (well-known credential shapes inside strings).

use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

pub const REDACTED: &str = "[REDACTED]";

const SECRET_KEY_FRAGMENTS: &[&str] = &[
    "password", "passwd", "secret", "api_key", "apikey", "api-key", "token", "authorization",
    "cookie", "private_key", "client_secret", "access_key", "card_number", "cvv", "ssn",
];

/// Usage counters whose names contain `token` but are not credentials.
const SAFE_USAGE_KEYS: &[&str] = &["input_tokens", "output_tokens", "cached_tokens", "total_tokens"];

fn patterns() -> &'static [Regex] {
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    P.get_or_init(|| {
        [
            // OpenAI / Anthropic / generic prefixed keys
            r"sk-[A-Za-z0-9_\-]{16,}",
            r"sk-ant-[A-Za-z0-9_\-]{16,}",
            // AWS access key id
            r"AKIA[0-9A-Z]{16}",
            // GitHub tokens
            r"gh[pousr]_[A-Za-z0-9]{30,}",
            // Bearer tokens
            r"(?i)bearer\s+[A-Za-z0-9\-._~+/]{16,}=*",
            // Private key blocks
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
            // Payment card numbers (13-19 digits, optional separators)
            r"\b(?:\d[ -]?){13,19}\b",
        ]
        .iter()
        .map(|p| Regex::new(p).expect("valid redaction regex"))
        .collect()
    })
}

pub fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    if SAFE_USAGE_KEYS.iter().any(|s| k == *s) {
        return false;
    }
    SECRET_KEY_FRAGMENTS.iter().any(|f| k.contains(f))
}

pub fn redact_str(s: &str) -> String {
    let mut out = s.to_string();
    for re in patterns() {
        out = re.replace_all(&out, REDACTED).into_owned();
    }
    out
}

/// Recursively redact a JSON value. Returns a new value; the input is not modified.
pub fn redact_value(v: &Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, val)| {
                    if is_secret_key(k) {
                        (k.clone(), Value::String(REDACTED.into()))
                    } else {
                        (k.clone(), redact_value(val))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::String(s) => Value::String(redact_str(s)),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_keys_and_patterns() {
        let v = json!({
            "api_key": "abc",
            "nested": {"Authorization": "Bearer xyz", "note": "token sk-1234567890abcdefghij here"},
            "card": "4111 1111 1111 1111",
            "order_id": "RF-2048"
        });
        let r = redact_value(&v);
        assert_eq!(r["api_key"], REDACTED);
        assert_eq!(r["nested"]["Authorization"], REDACTED);
        assert!(r["nested"]["note"].as_str().unwrap().contains(REDACTED));
        assert_eq!(r["card"], REDACTED);
        assert_eq!(r["order_id"], "RF-2048");
        let usage = json!({"input_tokens": 9, "output_tokens": 4, "api_token": "secret"});
        let u = redact_value(&usage);
        assert_eq!(u["input_tokens"], 9);
        assert_eq!(u["output_tokens"], 4);
        assert_eq!(u["api_token"], REDACTED);
    }
}
