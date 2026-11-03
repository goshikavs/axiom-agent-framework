//! Persistence for versioned evidence, the dependency graph, and impact reports.
//! Historical rows are inserted, never updated except decision status (current projection).
//! Decision status changes also append `revalidation_results`.

use goshikq_core::impact::{EvidenceGraph, GraphEdge, GraphNode};
use goshikq_protocol::{
    DecisionHint, DecisionStatus, EdgeKind, EvidenceHint, NodeKind, ResourceChange,
    TrustClassification,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    sha256_hex, CoverageReportRow, Db, DbResult, DecisionRecordRow, EvidenceEdgeRow,
    EvidenceNodeRow, EvidenceRecordRow, ImpactReportRow, RevalidationResultRow,
};

fn fingerprint_of(hint: &EvidenceHint) -> String {
    if let Some(f) = &hint.fingerprint {
        if !f.is_empty() {
            return f.clone();
        }
    }
    let version = hint
        .resource_version
        .as_deref()
        .or(hint.etag.as_deref())
        .unwrap_or("");
    sha256_hex(format!("{}|{}|{}", hint.source_system, hint.resource_id, version).as_bytes())
}

fn evidence_node_id(hint: &EvidenceHint) -> String {
    if let Some(id) = &hint.id {
        if !id.is_empty() {
            return id.clone();
        }
    }
    let v = hint
        .resource_version
        .as_deref()
        .or(hint.etag.as_deref())
        .unwrap_or("unknown");
    format!("ev:{}@{v}", hint.resource_id)
}

impl Db {
    pub async fn insert_evidence_record(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        hint: &EvidenceHint,
        metadata: Value,
    ) -> DbResult<EvidenceRecordRow> {
        let trust = hint.trust.unwrap_or(TrustClassification::Authoritative);
        let version = hint.resource_version.clone().or(hint.etag.clone());
        let mut metadata = metadata;
        if !hint.keys.is_empty() {
            metadata["keys"] = json!(hint.keys.clone());
        }
        Ok(sqlx::query_as::<_, EvidenceRecordRow>(
            r#"INSERT INTO evidence_records
                 (id, run_id, project_id, source_system, resource_id, resource_version, retrieved_at,
                  valid_from, valid_until, content_fingerprint, trust, access_scope, inferred, metadata)
               VALUES ($1,$2,$3,$4,$5,$6,now(),$7,$8,$9,$10,$11,false,$12)
               RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(project_id)
        .bind(&hint.source_system)
        .bind(&hint.resource_id)
        .bind(&version)
        .bind(hint.valid_from)
        .bind(hint.valid_until)
        .bind(fingerprint_of(hint))
        .bind(trust.as_str())
        .bind(&hint.access_scope)
        .bind(metadata)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn upsert_node(&self, run_id: Uuid, node: &GraphNode) -> DbResult<EvidenceNodeRow> {
        Ok(sqlx::query_as::<_, EvidenceNodeRow>(
            r#"INSERT INTO evidence_nodes (id, run_id, kind, resource_id, version, label, payload)
               VALUES ($1,$2,$3,$4,$5,$6,'{}'::jsonb)
               ON CONFLICT (run_id, id) DO UPDATE SET
                 kind = EXCLUDED.kind,
                 resource_id = COALESCE(EXCLUDED.resource_id, evidence_nodes.resource_id),
                 version = COALESCE(EXCLUDED.version, evidence_nodes.version),
                 label = COALESCE(EXCLUDED.label, evidence_nodes.label)
               RETURNING *"#,
        )
        .bind(&node.id)
        .bind(run_id)
        .bind(node.kind.as_str())
        .bind(&node.resource_id)
        .bind(&node.version)
        .bind(&node.label)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn insert_edge(
        &self,
        run_id: Uuid,
        from: &str,
        to: &str,
        kind: EdgeKind,
    ) -> DbResult<EvidenceEdgeRow> {
        Ok(sqlx::query_as::<_, EvidenceEdgeRow>(
            r#"INSERT INTO evidence_edges (id, run_id, from_id, to_id, kind, inferred)
               VALUES ($1,$2,$3,$4,$5,false)
               ON CONFLICT (run_id, from_id, to_id, kind) DO UPDATE SET kind = EXCLUDED.kind
               RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(from)
        .bind(to)
        .bind(kind.as_str())
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn upsert_decision(
        &self,
        run_id: Uuid,
        hint: &DecisionHint,
        evidence_ids: &[String],
    ) -> DbResult<DecisionRecordRow> {
        Ok(sqlx::query_as::<_, DecisionRecordRow>(
            r#"INSERT INTO decision_records (id, run_id, kind, status, payload, evidence_ids)
               VALUES ($1,$2,$3,$4,$5,$6)
               ON CONFLICT (run_id, id) DO UPDATE SET
                 payload = EXCLUDED.payload,
                 evidence_ids = EXCLUDED.evidence_ids,
                 updated_at = now()
               RETURNING *"#,
        )
        .bind(&hint.id)
        .bind(run_id)
        .bind(hint.kind.as_deref().unwrap_or("unknown"))
        .bind(DecisionStatus::Pending.as_str())
        .bind(hint.payload.clone().unwrap_or(json!({})))
        .bind(json!(evidence_ids))
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn set_decision_status(
        &self,
        run_id: Uuid,
        decision_id: &str,
        status: DecisionStatus,
    ) -> DbResult<()> {
        sqlx::query("UPDATE decision_records SET status = $3, updated_at = now() WHERE run_id = $1 AND id = $2")
            .bind(run_id)
            .bind(decision_id)
            .bind(status.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_graph(&self, run_id: Uuid) -> DbResult<EvidenceGraph> {
        let nodes = sqlx::query_as::<_, EvidenceNodeRow>(
            "SELECT * FROM evidence_nodes WHERE run_id = $1 ORDER BY id",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        let edges = sqlx::query_as::<_, EvidenceEdgeRow>(
            "SELECT * FROM evidence_edges WHERE run_id = $1 ORDER BY from_id, to_id, kind",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        let mut g = EvidenceGraph::new();
        for n in nodes {
            g.insert_node(GraphNode {
                id: n.id,
                kind: NodeKind::parse(&n.kind).unwrap_or(NodeKind::Evidence),
                resource_id: n.resource_id,
                version: n.version,
                label: n.label,
            });
        }
        for e in edges {
            if let Some(kind) = EdgeKind::parse(&e.kind) {
                g.insert_edge(GraphEdge {
                    from: e.from_id,
                    to: e.to_id,
                    kind,
                });
            }
        }
        Ok(g)
    }

    pub async fn list_evidence_records(&self, run_id: Uuid) -> DbResult<Vec<EvidenceRecordRow>> {
        Ok(sqlx::query_as::<_, EvidenceRecordRow>(
            "SELECT * FROM evidence_records WHERE run_id = $1 ORDER BY retrieved_at, id",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_evidence_nodes(&self, run_id: Uuid) -> DbResult<Vec<EvidenceNodeRow>> {
        Ok(sqlx::query_as::<_, EvidenceNodeRow>(
            "SELECT * FROM evidence_nodes WHERE run_id = $1 ORDER BY id",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_evidence_edges(&self, run_id: Uuid) -> DbResult<Vec<EvidenceEdgeRow>> {
        Ok(sqlx::query_as::<_, EvidenceEdgeRow>(
            "SELECT * FROM evidence_edges WHERE run_id = $1 ORDER BY from_id, to_id",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_decisions(&self, run_id: Uuid) -> DbResult<Vec<DecisionRecordRow>> {
        Ok(sqlx::query_as::<_, DecisionRecordRow>(
            "SELECT * FROM decision_records WHERE run_id = $1 ORDER BY id",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_impact_reports(&self, run_id: Uuid) -> DbResult<Vec<ImpactReportRow>> {
        Ok(sqlx::query_as::<_, ImpactReportRow>(
            "SELECT * FROM impact_reports WHERE run_id = $1 ORDER BY created_at",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_revalidation_results(
        &self,
        run_id: Uuid,
    ) -> DbResult<Vec<RevalidationResultRow>> {
        Ok(sqlx::query_as::<_, RevalidationResultRow>(
            "SELECT * FROM revalidation_results WHERE run_id = $1 ORDER BY created_at",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn save_impact_report(
        &self,
        run_id: Uuid,
        changes: &[ResourceChange],
        report: &Value,
    ) -> DbResult<ImpactReportRow> {
        Ok(sqlx::query_as::<_, ImpactReportRow>(
            r#"INSERT INTO impact_reports (id, run_id, changes, report) VALUES ($1,$2,$3,$4) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(json!(changes))
        .bind(report)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn save_revalidation_result(
        &self,
        run_id: Uuid,
        decision_id: &str,
        previous: Option<&str>,
        new_status: DecisionStatus,
        reason: Option<&str>,
        predicate_results: Value,
        old_version: Option<&str>,
        new_version: Option<&str>,
        approval_id: Option<Uuid>,
    ) -> DbResult<RevalidationResultRow> {
        Ok(sqlx::query_as::<_, RevalidationResultRow>(
            r#"INSERT INTO revalidation_results
                 (id, run_id, decision_id, previous_status, new_status, reason, predicate_results, old_version, new_version, approval_id)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(decision_id)
        .bind(previous)
        .bind(new_status.as_str())
        .bind(reason)
        .bind(predicate_results)
        .bind(old_version)
        .bind(new_version)
        .bind(approval_id)
        .fetch_one(&self.pool)
        .await?)
    }

    /// Register evidence + optional decision from a tool result. Does not store source blobs.
    pub async fn ingest_tool_evidence(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        hints: &[EvidenceHint],
        decision: Option<&DecisionHint>,
        action_id: Option<&str>,
        approval_id: Option<&str>,
        goal_predicates: &[String],
        invariants: &[String],
    ) -> DbResult<Vec<EvidenceRecordRow>> {
        let mut rows = Vec::new();
        let mut ev_ids = Vec::new();
        for hint in hints {
            let row = self
                .insert_evidence_record(project_id, run_id, hint, json!({}))
                .await?;
            let nid = evidence_node_id(hint);
            ev_ids.push(nid.clone());
            self.upsert_node(
                run_id,
                &GraphNode {
                    id: nid.clone(),
                    kind: NodeKind::Evidence,
                    resource_id: Some(hint.resource_id.clone()),
                    version: hint.resource_version.clone().or(hint.etag.clone()),
                    label: Some(format!("{} {}", hint.source_system, hint.resource_id)),
                },
            )
            .await?;
            let src_id = format!("src:{}", hint.source_system);
            self.upsert_node(
                run_id,
                &GraphNode {
                    id: src_id.clone(),
                    kind: NodeKind::Source,
                    resource_id: Some(hint.resource_id.clone()),
                    version: None,
                    label: Some(hint.source_system.clone()),
                },
            )
            .await?;
            self.insert_edge(run_id, &src_id, &nid, EdgeKind::Retrieved)
                .await?;
            rows.push(row);
        }
        if let Some(dec) = decision {
            self.upsert_decision(run_id, dec, &ev_ids).await?;
            self.upsert_node(
                run_id,
                &GraphNode {
                    id: dec.id.clone(),
                    kind: NodeKind::Decision,
                    resource_id: None,
                    version: None,
                    label: dec.kind.clone(),
                },
            )
            .await?;
            for ev in &ev_ids {
                self.insert_edge(run_id, ev, &dec.id, EdgeKind::Supports)
                    .await?;
            }
            if let Some(act) = action_id {
                self.upsert_node(
                    run_id,
                    &GraphNode {
                        id: act.to_string(),
                        kind: NodeKind::Action,
                        resource_id: None,
                        version: None,
                        label: Some(act.to_string()),
                    },
                )
                .await?;
                self.insert_edge(run_id, &dec.id, act, EdgeKind::Requires)
                    .await?;
            }
            if let Some(ap) = approval_id {
                self.upsert_node(
                    run_id,
                    &GraphNode {
                        id: ap.to_string(),
                        kind: NodeKind::Approval,
                        resource_id: None,
                        version: None,
                        label: Some("approval".into()),
                    },
                )
                .await?;
                self.insert_edge(run_id, &dec.id, ap, EdgeKind::Authorizes)
                    .await?;
            }
            for p in goal_predicates {
                self.upsert_node(
                    run_id,
                    &GraphNode {
                        id: p.clone(),
                        kind: NodeKind::GoalPredicate,
                        resource_id: None,
                        version: None,
                        label: None,
                    },
                )
                .await?;
                self.insert_edge(run_id, &dec.id, p, EdgeKind::Requires)
                    .await?;
            }
            for inv in invariants {
                self.upsert_node(
                    run_id,
                    &GraphNode {
                        id: inv.clone(),
                        kind: NodeKind::Invariant,
                        resource_id: None,
                        version: None,
                        label: None,
                    },
                )
                .await?;
                self.insert_edge(run_id, &dec.id, inv, EdgeKind::Requires)
                    .await?;
            }
        }
        Ok(rows)
    }

    /// Diff recorded evidence versions against currently observed resources.
    pub async fn detect_resource_changes(
        &self,
        run_id: Uuid,
        observed: &[(String, Option<String>)],
        inaccessible: &[String],
    ) -> DbResult<Vec<ResourceChange>> {
        let recorded = self.list_evidence_records(run_id).await?;
        let mut latest: std::collections::BTreeMap<String, Option<String>> =
            std::collections::BTreeMap::new();
        for r in recorded {
            latest.insert(r.resource_id, r.resource_version);
        }
        let mut changes = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for (rid, ver) in observed {
            seen.insert(rid.clone());
            if let Some(old) = latest.get(rid) {
                if old != ver {
                    changes.push(ResourceChange {
                        resource_id: rid.clone(),
                        old_version: old.clone(),
                        new_version: ver.clone(),
                        inaccessible: inaccessible.contains(rid),
                    });
                }
            }
        }
        for rid in inaccessible {
            if !changes.iter().any(|c| &c.resource_id == rid) {
                changes.push(ResourceChange {
                    resource_id: rid.clone(),
                    old_version: latest.get(rid).cloned().flatten(),
                    new_version: latest.get(rid).cloned().flatten(),
                    inaccessible: true,
                });
            }
        }
        // Deleted: recorded resource absent from observed (only when observed is non-empty).
        if !observed.is_empty() {
            for (rid, old) in &latest {
                if !seen.contains(rid) && !inaccessible.contains(rid) {
                    changes.push(ResourceChange {
                        resource_id: rid.clone(),
                        old_version: old.clone(),
                        new_version: None,
                        inaccessible: false,
                    });
                }
            }
        }
        Ok(changes)
    }

    pub async fn save_coverage_report(
        &self,
        run_id: Uuid,
        tool_name: &str,
        policy: &str,
        status: &str,
        gate: &str,
        report: &Value,
    ) -> DbResult<CoverageReportRow> {
        Ok(sqlx::query_as::<_, CoverageReportRow>(
            r#"INSERT INTO evidence_coverage_reports (id, run_id, tool_name, policy, status, gate, report)
               VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(tool_name)
        .bind(policy)
        .bind(status)
        .bind(gate)
        .bind(report)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn list_coverage_reports(&self, run_id: Uuid) -> DbResult<Vec<CoverageReportRow>> {
        Ok(sqlx::query_as::<_, CoverageReportRow>(
            "SELECT * FROM evidence_coverage_reports WHERE run_id = $1 ORDER BY created_at, id",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }
}
