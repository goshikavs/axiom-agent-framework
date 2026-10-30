"use client";

import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { useParams } from "next/navigation";
import { JsonView } from "@/components/json-view";
import { ErrorBanner } from "@/components/metric-card";
import { StatusBadge } from "@/components/status-badge";
import { api } from "@/lib/api";
import type { RunEvidenceResponse, VerificationReport } from "@/lib/types";

export default function ExecutionEvidencePage() {
  const { id } = useParams<{ id: string }>();
  const q = useQuery({
    queryKey: ["evidence", id],
    queryFn: () => api<RunEvidenceResponse>(`/api/v1/runs/${id}/evidence`),
    refetchInterval: 2500,
  });
  const ver = useQuery({
    queryKey: ["verification", id],
    queryFn: () => api<VerificationReport>(`/api/v1/runs/${id}/verification`),
  });
  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  if (!q.data) return <p className="text-sm text-ink-muted">Loading evidence…</p>;
  const d = q.data;
  const latestImpact = d.impact.at(-1);

  return (
    <div className="space-y-6">
      <header>
        <Link href={`/executions/${id}`} className="text-xs text-sky-400 hover:underline">
          ← execution
        </Link>
        <h1 className="mt-1 text-2xl font-semibold">Evidence intelligence</h1>
        <p className="mt-1 max-w-3xl text-sm text-ink-muted">
          Observed facts (source, version, fingerprint) are stored separately from inferred relationships. This page only shows edges the worker registered. <code className="font-mono text-xs">inferred={String(d.inferred)}</code>
        </p>
      </header>

      <section className="rounded-lg border border-line bg-canvas-raised p-4">
        <h2 className="mb-3 text-sm font-medium">Evidence inspector</h2>
        {d.records.length === 0 ? <p className="text-sm text-ink-muted">No evidence records. Tools must attach <code className="font-mono text-xs">_evidence</code>.</p> : null}
        <ul className="space-y-2">
          {d.records.map((r) => {
            const related = d.decisions.filter((dec) => JSON.stringify(dec.evidence_ids).includes(r.id) || JSON.stringify(dec.payload).includes(r.resource_id));
            return (
              <li key={r.id} className="rounded border border-line p-3 font-mono text-[11px]">
                <div className="flex flex-wrap items-center gap-2">
                  <StatusBadge value={r.trust} />
                  <span>{r.source_system}</span>
                  <span>{r.resource_id}@{r.resource_version ?? "?"}</span>
                </div>
                <dl className="mt-2 grid grid-cols-[auto,1fr] gap-x-3 gap-y-1 text-ink-muted">
                  <dt>fingerprint</dt>
                  <dd className="truncate">{r.content_fingerprint}</dd>
                  <dt>retrieved</dt>
                  <dd>{r.retrieved_at}</dd>
                  <dt>scope</dt>
                  <dd>{r.access_scope ?? "—"}</dd>
                  <dt>decisions</dt>
                  <dd>{related.map((x) => x.id).join(", ") || "—"}</dd>
                </dl>
              </li>
            );
          })}
        </ul>
      </section>

      <section className="rounded-lg border border-line bg-canvas-raised p-4">
        <h2 className="mb-3 text-sm font-medium">Dependency graph</h2>
        <p className="mb-3 text-xs text-ink-muted">Evidence → Decision → Action → Goal. Edges are persisted; they are not inferred from the model trace.</p>
        {d.edges.length === 0 ? <p className="text-sm text-ink-muted">No registered edges.</p> : null}
        <svg viewBox={`0 0 720 ${Math.max(120, d.nodes.length * 36 + 20)}`} className="w-full max-h-[420px]">
          {d.nodes.map((n, i) => {
            const y = 24 + i * 36;
            return (
              <g key={n.id}>
                <rect x={12} y={y - 14} width={696} height={28} rx={4} className="fill-canvas stroke-line" />
                <text x={22} y={y + 4} className="fill-ink text-[11px] font-mono">
                  {n.kind} · {n.id}
                  {n.version ? ` @${n.version}` : ""}
                </text>
              </g>
            );
          })}
        </svg>
        <ul className="mt-3 space-y-1 font-mono text-[11px] text-ink-muted" data-testid="evidence-edges">
          {d.edges.map((e) => (
            <li key={e.id}>
              {e.from_id} —{e.kind}→ {e.to_id}
              {e.inferred ? " (inferred)" : " (registered)"}
            </li>
          ))}
        </ul>
      </section>

      <section className="rounded-lg border border-line bg-canvas-raised p-4">
        <h2 className="mb-3 text-sm font-medium">Captured state snapshots (before / after)</h2>
        <p className="mb-3 text-xs text-ink-muted">Diff of consecutive persisted snapshots. This is not a reconstruction of hidden model reasoning.</p>
        {(ver.data?.state_snapshots ?? []).length < 2 ? (
          <p className="text-sm text-ink-muted">{(ver.data?.state_snapshots ?? []).length === 0 ? "No snapshots captured." : "Only one snapshot; no before/after pair."}</p>
        ) : (
          <div className="grid gap-3 md:grid-cols-2">
            <div>
              <p className="text-[11px] uppercase text-ink-faint">Before {ver.data!.state_snapshots.at(-2)?.captured_at}</p>
              <JsonView value={ver.data!.state_snapshots.at(-2)?.state} />
            </div>
            <div>
              <p className="text-[11px] uppercase text-ink-faint">After {ver.data!.state_snapshots.at(-1)?.captured_at}</p>
              <JsonView value={ver.data!.state_snapshots.at(-1)?.state} />
            </div>
          </div>
        )}
      </section>

      <section className="rounded-lg border border-line bg-canvas-raised p-4">
        <h2 className="mb-3 text-sm font-medium">Change impact</h2>
        {!latestImpact ? (
          <p className="text-sm text-ink-muted">No impact reports. Impact is computed when a registered resource version changes.</p>
        ) : (
          <div className="space-y-2">
            <p className="text-xs text-ink-muted">Computed {latestImpact.created_at}. Affected decisions are classified as requiring verification, not as automatically false.</p>
            <JsonView value={latestImpact.report} />
          </div>
        )}
      </section>

      <section className="rounded-lg border border-line bg-canvas-raised p-4">
        <h2 className="mb-3 text-sm font-medium">Evidence coverage</h2>
        <p className="mb-3 text-xs text-ink-muted">
          Coverage is completeness of the <em>declared</em> contract, not proof that every hidden dependency is known. Complete coverage does not mark a goal VERIFIED.
        </p>
        {(d.coverage ?? []).length === 0 ? <p className="text-sm text-ink-muted">No coverage evaluations for this run.</p> : null}
        <ul className="space-y-2">
          {(d.coverage ?? []).map((c) => {
            const report = (c.report ?? {}) as { missing?: string[]; stale?: string[]; reasons?: { code?: string; detail?: string }[] };
            return (
              <li key={c.id} className="rounded border border-line p-3 text-xs">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-mono">{c.tool_name}</span>
                  <StatusBadge value={c.status} />
                  <span className="text-ink-muted">gate {c.gate}</span>
                  <span className="text-ink-muted">policy {c.policy}</span>
                </div>
                {report.missing && report.missing.length > 0 ? <p className="mt-2 text-ink-muted">missing: {report.missing.join(", ")}</p> : null}
                {report.stale && report.stale.length > 0 ? <p className="mt-1 text-ink-muted">stale: {report.stale.join(", ")}</p> : null}
                {c.gate === "BLOCK" || c.gate === "REQUIRE_APPROVAL" ? (
                  <p className="mt-1">Action blocked reason: {report.reasons?.map((r) => r.detail || r.code).join("; ") || c.gate}</p>
                ) : null}
              </li>
            );
          })}
        </ul>
      </section>

      <section className="rounded-lg border border-line bg-canvas-raised p-4">
        <h2 className="mb-3 text-sm font-medium">Verification history</h2>
        {d.revalidation.length === 0 ? <p className="text-sm text-ink-muted">No revalidation rows yet.</p> : null}
        <ul className="space-y-2">
          {d.revalidation.map((r) => (
            <li key={r.id} className="rounded border border-line p-3 text-xs">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-mono">{r.decision_id}</span>
                <StatusBadge value={r.previous_status ?? "—"} />
                <span>→</span>
                <StatusBadge value={r.new_status} kind="verify" />
              </div>
              <dl className="mt-2 grid grid-cols-2 gap-1 font-mono text-[11px] text-ink-muted">
                <div>old version: {r.old_version ?? "—"}</div>
                <div>new version: {r.new_version ?? "—"}</div>
                <div>approval: {r.approval_id ?? "—"}</div>
                <div>reason: {r.reason ?? "—"}</div>
              </dl>
              <JsonView value={r.predicate_results} />
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
