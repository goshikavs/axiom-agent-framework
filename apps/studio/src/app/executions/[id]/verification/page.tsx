"use client";

import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { useParams } from "next/navigation";
import { JsonView } from "@/components/json-view";
import { ErrorBanner } from "@/components/metric-card";
import { StatusBadge } from "@/components/status-badge";
import { api } from "@/lib/api";
import type { RunDetail, RunEvidenceResponse, VerificationReport } from "@/lib/types";
import { formatDuration } from "@/lib/utils";

export default function VerificationPage() {
  const { id } = useParams<{ id: string }>();
  const run = useQuery({
    queryKey: ["run", id],
    queryFn: () => api<RunDetail>(`/api/v1/runs/${id}`),
  });
  const ev = useQuery({
    queryKey: ["evidence", id],
    queryFn: () => api<RunEvidenceResponse>(`/api/v1/runs/${id}/evidence`),
    refetchInterval: 2500,
  });
  const q = useQuery({
    queryKey: ["verification", id],
    queryFn: () => api<VerificationReport>(`/api/v1/runs/${id}/verification`),
    refetchInterval: (query) => {
      const s = query.state.data?.status;
      if (!s) return 2000;
      if (["COMPLETED", "FAILED", "CANCELLED", "UNVERIFIED", "TIMED_OUT", "BUDGET_EXCEEDED", "STEP_LIMIT_EXCEEDED", "MODEL_BUDGET_EXCEEDED"].includes(s)) return false;
      return 2000;
    },
  });
  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  if (!q.data) return <p className="text-sm text-ink-muted">Loading verification…</p>;
  const r = q.data;

  function exportReport() {
    const blob = new Blob([JSON.stringify(r, null, 2)], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `axiom-verification-${id}.json`;
    a.click();
  }

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <Link href={`/executions/${id}`} className="text-xs text-sky-400 hover:underline">
            ← execution
          </Link>
          <h1 className="mt-1 text-2xl font-semibold">Goal verification</h1>
          <p className="mt-1 max-w-3xl text-sm text-ink-muted">{r.explanation}</p>
        </div>
        <div className="flex items-center gap-2">
          <StatusBadge value={r.verification_status} kind="verify" />
          <button type="button" onClick={exportReport} className="rounded border border-line px-3 py-1 text-xs hover:bg-canvas-raised">
            Export report
          </button>
        </div>
      </header>
      <Card title="Independent outcome fields">
        <p className="mb-3 text-xs text-ink-muted">
          Tool result, dispatch, external business-operation, recovery, and goal verification are stored separately.
          A successfully imported trace is not a verified business goal.
        </p>
        <dl className="grid grid-cols-1 gap-2 font-mono text-xs sm:grid-cols-2">
          <div>execution_status: <StatusBadge value={r.status} /></div>
          <div>tool_status: {run.data?.tool_status ?? r.outcome?.tool_status ?? "—"}</div>
          <div>dispatch_state: {run.data?.dispatch_state ?? "—"}</div>
          <div>business_operation_status: {run.data?.business_operation_status ?? r.outcome?.business_operation_status ?? "—"}</div>
          <div>recovery_disposition: {run.data?.recovery_disposition ?? "—"}</div>
          <div>goal_satisfied: {r.outcome?.goal_satisfied == null ? "—" : String(r.outcome.goal_satisfied)}</div>
          <div>verification_status: <StatusBadge value={r.verification_status} kind="verify" /></div>
          <div>evidence_source: {r.outcome?.evidence_source ?? "—"}</div>
          <div>evidence_version: {r.outcome?.evidence_version ?? "—"}</div>
          <div>observed_at: {r.outcome?.observed_at ?? "—"}</div>
          <div>verifier: {r.outcome?.verifier ?? "none (NOT_EVALUATED unless recorded)"}</div>
        </dl>
      </Card>
      {ev.data?.coverage && ev.data.coverage.length > 0 ? (
        <Card title="Evidence coverage (declared contract)">
          <p className="mb-2 text-xs text-ink-muted">Independent of verification_status. Complete coverage is not a verified goal.</p>
          <ul className="space-y-2 text-xs font-mono">
            {ev.data.coverage.map((c) => (
              <li key={c.id}>
                {c.tool_name} · {c.status} · gate {c.gate} · policy {c.policy}
              </li>
            ))}
          </ul>
        </Card>
      ) : null}
      <section className="grid gap-4 lg:grid-cols-2">
        <Card title="Intended outcome">
          <p className="text-sm">{r.intended_outcome}</p>
          <p className="mt-2 text-xs text-ink-faint">Agent text is not proof of success.</p>
          {r.final_output ? <pre className="mt-3 whitespace-pre-wrap font-mono text-xs text-ink-muted">{r.final_output}</pre> : null}
        </Card>
        <Card title="Observed outcome">
          {r.state_snapshots.length === 0 ? (
            <p className="text-sm text-ink-muted">No authoritative snapshots. Outcome is unverified.</p>
          ) : (
            <p className="text-sm text-ink-muted">
              Latest source {r.state_snapshots.at(-1)?.source} version {r.state_snapshots.at(-1)?.version ?? "none"} at {r.state_snapshots.at(-1)?.captured_at}
            </p>
          )}
        </Card>
      </section>
      <Card title="Verification evidence">
        {r.verification_results.length === 0 ? <p className="text-sm text-ink-muted">No verification rows (NOT_EVALUATED).</p> : null}
        <ul className="space-y-3">
          {r.verification_results.map((v) => (
            <li key={v.id} className="rounded border border-line p-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="font-mono text-xs">
                  {v.subject_type}:{v.subject_id}
                </span>
                <StatusBadge value={v.status} kind="verify" />
              </div>
              <dl className="mt-2 grid grid-cols-2 gap-2 font-mono text-[11px] text-ink-muted">
                <div>method: {v.method}</div>
                <div>duration: {formatDuration(v.duration_ms)}</div>
                <div>source: {v.evidence_source ?? "—"}</div>
                <div>version: {v.evidence_version ?? "—"}</div>
              </dl>
              {v.reason ? <p className="mt-2 text-xs">{v.reason}</p> : null}
              <div className="mt-2 grid gap-2 md:grid-cols-2">
                <div>
                  <p className="text-[11px] uppercase text-ink-faint">Expected</p>
                  <JsonView value={v.expected} />
                </div>
                <div>
                  <p className="text-[11px] uppercase text-ink-faint">Actual</p>
                  <JsonView value={v.actual} />
                </div>
              </div>
            </li>
          ))}
        </ul>
      </Card>
      {ev.data && ev.data.revalidation.length > 0 ? (
        <Card title="Evidence revalidation history">
          <p className="mb-3 text-xs text-ink-muted">Original vs new evidence versions. Approval invalidation is recorded here when material prerequisites change.</p>
          <ul className="space-y-2">
            {ev.data.revalidation.map((row) => (
              <li key={row.id} className="rounded border border-line p-3 font-mono text-[11px]">
                {row.decision_id}: {row.old_version ?? "—"} → {row.new_version ?? "—"} · {row.previous_status} → {row.new_status}
                {row.approval_id ? ` · approval ${row.approval_id} invalidated` : ""}
              </li>
            ))}
          </ul>
        </Card>
      ) : null}
    </div>
  );
}

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-lg border border-line bg-canvas-raised p-4">
      <h2 className="mb-3 text-sm font-medium">{title}</h2>
      {children}
    </section>
  );
}
