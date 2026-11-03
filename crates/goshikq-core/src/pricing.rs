//! Versioned model pricing used for *estimated* cost. Unknown models yield `None`, which
//! the API and Studio must render as "unknown", never as zero.

use std::collections::BTreeMap;

use goshikq_protocol::TokenUsage;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ModelPrice {
    /// USD per 1,000,000 input tokens.
    pub input_per_million: f64,
    /// USD per 1,000,000 output tokens.
    pub output_per_million: f64,
    /// USD per 1,000,000 cached input tokens, when the provider reports cached usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_per_million: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PricingTable {
    /// Free-form version label recorded on every cost record, e.g. `2026-10-27`.
    pub version: String,
    /// Keyed by provider-qualified model identifier. A key ending in `/*` matches any
    /// model of that provider.
    pub models: BTreeMap<String, ModelPrice>,
}

impl PricingTable {
    pub fn empty(version: &str) -> Self {
        PricingTable {
            version: version.into(),
            models: BTreeMap::new(),
        }
    }

    pub fn lookup(&self, model: &str) -> Option<&ModelPrice> {
        if let Some(p) = self.models.get(model) {
            return Some(p);
        }
        let provider = model.split('/').next()?;
        self.models.get(&format!("{provider}/*"))
    }

    /// Estimated cost in USD, or `None` when the model is not priced or usage is absent.
    pub fn estimate(&self, model: &str, usage: &TokenUsage) -> Option<f64> {
        let p = self.lookup(model)?;
        let cached = usage.cached_tokens.unwrap_or(0).min(usage.input_tokens);
        let uncached = usage.input_tokens - cached;
        let cached_rate = p.cached_input_per_million.unwrap_or(p.input_per_million);
        Some(
            uncached as f64 / 1e6 * p.input_per_million
                + cached as f64 / 1e6 * cached_rate
                + usage.output_tokens as f64 / 1e6 * p.output_per_million,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_models_are_unknown_not_zero() {
        let t = PricingTable::empty("test");
        assert_eq!(
            t.estimate("openai/some-model", &TokenUsage::default()),
            None
        );
    }

    #[test]
    fn wildcard_and_cached() {
        let mut t = PricingTable::empty("test");
        t.models.insert(
            "acme/*".into(),
            ModelPrice {
                input_per_million: 1.0,
                output_per_million: 2.0,
                cached_input_per_million: Some(0.5),
                note: None,
            },
        );
        let u = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cached_tokens: Some(200_000),
        };
        let c = t.estimate("acme/model-x", &u).unwrap();
        assert!((c - (0.8 + 0.1 + 1.0)).abs() < 1e-9);
    }
}
