//! IO-free evaluation of an [`EvidenceCoverageContract`].
//!
//! A complete result means declared requirements were met. It is never a goal
//! verification and never proof that hidden model dependencies are known.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use axiom_protocol::{
    CoverageStatus, DecisionFact, EvidenceCoverageContract, EvidenceFact, RequiredEvidence, ResourceChange,
    TrustClassification,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CoverageReason {
    pub requirement: String,
    pub code: String,
    pub detail: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CoverageReport {
    pub status: CoverageStatus,
    pub reasons: Vec<CoverageReason>,
    pub missing: Vec<String>,
    pub stale: Vec<String>,
    pub conflicted: Vec<String>,
    pub inconclusive: Vec<String>,
    pub observed_ids: Vec<String>,
}

impl CoverageReport {
    pub fn complete() -> Self {
        CoverageReport {
            status: CoverageStatus::CompleteForDeclaredContract,
            reasons: vec![],
            missing: vec![],
            stale: vec![],
            conflicted: vec![],
            inconclusive: vec![],
            observed_ids: vec![],
        }
    }

    /// Coverage is independent of goal verification. Callers must not map this to VERIFIED.
    pub fn is_complete(&self) -> bool {
        self.status == CoverageStatus::CompleteForDeclaredContract
    }
}

fn trust_rank(t: TrustClassification) -> u8 {
    match t {
        TrustClassification::Authoritative => 3,
        TrustClassification::Derived => 2,
        TrustClassification::Untrusted => 1,
        TrustClassification::Inaccessible => 0,
    }
}

pub fn fact_covers(req: &RequiredEvidence, fact: &EvidenceFact) -> bool {
    if fact.keys.iter().any(|k| k == &req.key) {
        return true;
    }
    if let Some(rid) = &req.resource_id {
        if fact.resource_id == *rid {
            return true;
        }
    }
    if let Some(fam) = &req.resource_family {
        if fact.resource_id == *fam
            || fact.resource_id.starts_with(&format!("{fam}:"))
            || fact.resource_id.starts_with(&format!("{fam}."))
        {
            return true;
        }
    }
    fact.resource_id == req.key
}

/// Substitute `{arg}` placeholders from a tool-call argument object.
pub fn resolve_contract_templates(contract: &EvidenceCoverageContract, arguments: &Value) -> EvidenceCoverageContract {
    let mut out = contract.clone();
    for req in &mut out.required {
        req.key = subst(&req.key, arguments);
        if let Some(rid) = req.resource_id.take() {
            req.resource_id = Some(subst(&rid, arguments));
        }
    }
    out
}

fn subst(s: &str, arguments: &Value) -> String {
    let Some(obj) = arguments.as_object() else {
        return s.to_string();
    };
    let mut out = s.to_string();
    for (k, v) in obj {
        let needle = format!("{{{k}}}");
        if let Some(val) = v.as_str() {
            out = out.replace(&needle, val);
        } else if let Some(n) = v.as_i64() {
            out = out.replace(&needle, &n.to_string());
        }
    }
    out
}

pub fn evaluate_coverage(
    contract: &EvidenceCoverageContract,
    facts: &[EvidenceFact],
    decisions: &[DecisionFact],
    changes: &[ResourceChange],
    now: DateTime<Utc>,
) -> CoverageReport {
    if contract.is_empty() {
        return CoverageReport::complete();
    }

    let mut reasons = Vec::new();
    let mut missing = Vec::new();
    let mut stale = Vec::new();
    let mut conflicted = Vec::new();
    let mut inconclusive = Vec::new();
    let mut observed = Vec::new();

    for req in &contract.required {
        let covering: Vec<&EvidenceFact> = facts.iter().filter(|f| fact_covers(req, f)).collect();
        if covering.is_empty() {
            missing.push(req.key.clone());
            reasons.push(CoverageReason {
                requirement: req.key.clone(),
                code: "missing_required_evidence".into(),
                detail: "no recorded fact satisfies this declared requirement".into(),
                evidence_ids: vec![],
            });
            continue;
        }
        for f in &covering {
            observed.push(f.id.clone());
        }

        let fps: Vec<_> = covering.iter().filter_map(|f| f.fingerprint.as_deref()).collect();
        let vers: Vec<_> = covering.iter().filter_map(|f| f.version.as_deref()).collect();
        let conflict = (fps.len() >= 2 && fps.windows(2).any(|w| w[0] != w[1]))
            || (vers.len() >= 2 && vers.windows(2).any(|w| w[0] != w[1]));
        if conflict {
            conflicted.push(req.key.clone());
            reasons.push(CoverageReason {
                requirement: req.key.clone(),
                code: "conflicted_evidence".into(),
                detail: "multiple covering facts disagree on version or fingerprint".into(),
                evidence_ids: covering.iter().map(|f| f.id.clone()).collect(),
            });
            continue;
        }

        let any_inaccessible = covering.iter().any(|f| f.inaccessible || f.trust == TrustClassification::Inaccessible);
        if any_inaccessible {
            inconclusive.push(req.key.clone());
            reasons.push(CoverageReason {
                requirement: req.key.clone(),
                code: "inaccessible_evidence".into(),
                detail: "covering fact is inaccessible".into(),
                evidence_ids: covering.iter().map(|f| f.id.clone()).collect(),
            });
            continue;
        }

        let trust_ok = covering.iter().any(|f| trust_rank(f.trust) >= trust_rank(req.min_trust));
        if !trust_ok {
            inconclusive.push(req.key.clone());
            reasons.push(CoverageReason {
                requirement: req.key.clone(),
                code: "trust_below_minimum".into(),
                detail: format!("required trust {:?}", req.min_trust),
                evidence_ids: covering.iter().map(|f| f.id.clone()).collect(),
            });
            continue;
        }

        if let Some(src) = &req.source_system {
            if !covering.iter().any(|f| f.source_system.as_deref() == Some(src.as_str())) {
                missing.push(req.key.clone());
                reasons.push(CoverageReason {
                    requirement: req.key.clone(),
                    code: "wrong_source_system".into(),
                    detail: format!("required source `{src}`"),
                    evidence_ids: covering.iter().map(|f| f.id.clone()).collect(),
                });
                continue;
            }
        }

        if let Some(max_age) = req.max_age_secs {
            let fresh = covering.iter().any(|f| {
                now.signed_duration_since(f.retrieved_at).num_seconds() <= max_age as i64
            });
            if !fresh {
                stale.push(req.key.clone());
                reasons.push(CoverageReason {
                    requirement: req.key.clone(),
                    code: "stale_freshness".into(),
                    detail: format!("older than {max_age}s"),
                    evidence_ids: covering.iter().map(|f| f.id.clone()).collect(),
                });
                continue;
            }
        }

        let hit = covering.iter().any(|f| {
            changes.iter().any(|c| {
                c.resource_id == f.resource_id
                    && (c.inaccessible || c.new_version.as_deref() != f.version.as_deref() || c.new_version.is_none())
            })
        });
        let version_drift = covering.iter().any(|f| {
            req.require_version
                && f.current_version.as_ref().is_some()
                && f.version.as_ref() != f.current_version.as_ref()
        });
        if hit || version_drift {
            stale.push(req.key.clone());
            reasons.push(CoverageReason {
                requirement: req.key.clone(),
                code: "stale_required_evidence".into(),
                detail: "recorded version is not the current authoritative version".into(),
                evidence_ids: covering.iter().map(|f| f.id.clone()).collect(),
            });
            continue;
        }

        if req.require_version && covering.iter().all(|f| f.version.is_none() && f.current_version.is_none()) {
            inconclusive.push(req.key.clone());
            reasons.push(CoverageReason {
                requirement: req.key.clone(),
                code: "missing_version".into(),
                detail: "requirement demands a version and none is recorded".into(),
                evidence_ids: covering.iter().map(|f| f.id.clone()).collect(),
            });
            continue;
        }

        if let Some(method) = &req.verification_method {
            let ok = decisions.iter().any(|d| {
                d.kind == *method && d.status != "INVALID" && d.status != "INCONCLUSIVE"
            });
            if !ok {
                inconclusive.push(req.key.clone());
                reasons.push(CoverageReason {
                    requirement: req.key.clone(),
                    code: "verification_method_unsatisfied".into(),
                    detail: format!("no non-failed decision of kind `{method}`"),
                    evidence_ids: vec![],
                });
            }
        }
    }

    for inv in &contract.required_invariants {
        let ok = decisions.iter().any(|d| {
            d.kind == *inv && d.status != "INVALID" && d.status != "INCONCLUSIVE"
        });
        if !ok {
            missing.push(format!("invariant:{inv}"));
            reasons.push(CoverageReason {
                requirement: inv.clone(),
                code: "missing_invariant".into(),
                detail: "required business invariant has no current non-failed decision".into(),
                evidence_ids: vec![],
            });
        }
    }

    for auth in &contract.required_authorization {
        let req = RequiredEvidence::from_key(auth);
        let covering: Vec<&EvidenceFact> = facts.iter().filter(|f| fact_covers(&req, f) || f.keys.iter().any(|k| k == auth)).collect();
        if covering.is_empty() {
            missing.push(format!("authorization:{auth}"));
            reasons.push(CoverageReason {
                requirement: auth.clone(),
                code: "missing_authorization_evidence".into(),
                detail: "authorization evidence was declared and not observed".into(),
                evidence_ids: vec![],
            });
        }
    }

    observed.sort();
    observed.dedup();

    let status = if !conflicted.is_empty() {
        CoverageStatus::Conflicted
    } else if !inconclusive.is_empty() {
        CoverageStatus::Inconclusive
    } else if !missing.is_empty() {
        CoverageStatus::Incomplete
    } else if !stale.is_empty() {
        CoverageStatus::Stale
    } else {
        CoverageStatus::CompleteForDeclaredContract
    };

    CoverageReport {
        status,
        reasons,
        missing,
        stale,
        conflicted,
        inconclusive,
        observed_ids: observed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_protocol::RequiredEvidence;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 29, 0, 0, 0).unwrap()
    }

    fn fact(id: &str, resource: &str, version: &str) -> EvidenceFact {
        EvidenceFact {
            id: id.into(),
            resource_id: resource.into(),
            keys: vec![],
            version: Some(version.into()),
            trust: TrustClassification::Authoritative,
            retrieved_at: now(),
            current_version: Some(version.into()),
            inaccessible: false,
            fingerprint: Some(format!("fp-{version}")),
            source_system: Some("policy-admin".into()),
        }
    }

    #[test]
    fn empty_contract_is_complete_for_declared() {
        let r = evaluate_coverage(&EvidenceCoverageContract::default(), &[], &[], &[], now());
        assert_eq!(r.status, CoverageStatus::CompleteForDeclaredContract);
    }

    #[test]
    fn missing_required_is_incomplete() {
        let mut c = EvidenceCoverageContract::default();
        c.required.push(RequiredEvidence::from_key("policy:POL-1"));
        let r = evaluate_coverage(&c, &[], &[], &[], now());
        assert_eq!(r.status, CoverageStatus::Incomplete);
        assert_eq!(r.missing, vec!["policy:POL-1"]);
    }

    #[test]
    fn version_drift_is_stale() {
        let mut c = EvidenceCoverageContract::default();
        c.required.push(RequiredEvidence::from_key("policy:POL-1"));
        let mut f = fact("e1", "policy:POL-1", "4");
        f.current_version = Some("5".into());
        let r = evaluate_coverage(&c, &[f], &[], &[], now());
        assert_eq!(r.status, CoverageStatus::Stale);
    }

    #[test]
    fn change_event_marks_stale() {
        let mut c = EvidenceCoverageContract::default();
        c.required.push(RequiredEvidence::from_key("policy:POL-1"));
        let f = fact("e1", "policy:POL-1", "4");
        let ch = ResourceChange {
            resource_id: "policy:POL-1".into(),
            old_version: Some("4".into()),
            new_version: Some("5".into()),
            inaccessible: false,
        };
        let r = evaluate_coverage(&c, &[f], &[], &[ch], now());
        assert_eq!(r.status, CoverageStatus::Stale);
    }

    #[test]
    fn complete_does_not_imply_verified() {
        let mut c = EvidenceCoverageContract::default();
        c.required.push(RequiredEvidence::from_key("policy:POL-1"));
        let r = evaluate_coverage(&c, &[fact("e1", "policy:POL-1", "4")], &[], &[], now());
        assert!(r.is_complete());
        // Explicit: coverage complete is not a verification status.
        assert_ne!(r.status.as_str(), "VERIFIED");
    }

    #[test]
    fn templates_bind_arguments() {
        let mut c = EvidenceCoverageContract::default();
        c.required.push(RequiredEvidence::from_key("order:{order_id}"));
        let bound = resolve_contract_templates(&c, &serde_json::json!({"order_id": "ORD-2048"}));
        assert_eq!(bound.required[0].resource_id.as_deref(), Some("order:ORD-2048"));
    }

    #[test]
    fn conflicted_fingerprints() {
        let mut c = EvidenceCoverageContract::default();
        c.required.push(RequiredEvidence::from_key("policy:POL-1"));
        let a = fact("e1", "policy:POL-1", "4");
        let mut b = fact("e2", "policy:POL-1", "4");
        b.fingerprint = Some("other".into());
        let r = evaluate_coverage(&c, &[a, b], &[], &[], now());
        assert_eq!(r.status, CoverageStatus::Conflicted);
    }
}
