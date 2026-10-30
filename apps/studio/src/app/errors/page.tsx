"use client";

import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { useState } from "react";
import { JsonView } from "@/components/json-view";
import { EmptyState, ErrorBanner } from "@/components/metric-card";
import { api, listRuns } from "@/lib/api";
import type { ExecutionEvent, Paginated, RecoveryReport, RunDetail } from "@/lib/types";

export default function ErrorsPage() {
  const q = useQuery({ queryKey: ["failed-runs"], queryFn: () => listRuns({ limit: 50 }) });
  const [id, setId] = useState("");
  const detail = useQuery({ queryKey: ["run", id], queryFn: () => api<RunDetail>(`/api/v1/runs/${id}`), enabled: !!id });
  const events = useQuery({ queryKey: ["events", id], queryFn: () => api<Paginated<ExecutionEvent>>(`/api/v1/runs/${id}/events?limit=2000`), enabled: !!id });
  const recovery = useQuery({ queryKey: ["recovery", id], queryFn: () => api<RecoveryReport>(`/api/v1/runs/${id}/recovery`), enabled: !!id });
  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  const items = (q.data?.items ?? []).filter((r) => r.status === "FAILED" || r.status === "UNVERIFIED" || r.verification_status === "FAILED" || r.verification_status === "INCONCLUSIVE");
  const evs = events.data?.items ?? [];
  const firstFail = evs.find((e) => e.event_type.endsWith(".failed") || e.status === "failed");
  const related = firstFail ? evs.filter((e) => e.sequence <= firstFail.sequence).slice(-8) : [];

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Failure analysis</h1>
      <p className="text-sm text-ink-muted">Observed events only. Suggested causes are labeled as suggestions, not proof.</p>
      {items.length === 0 ? <EmptyState title="No failed or unverified runs" body="Zero matching records in this project." /> : null}
      <ul className="divide-y divide-line rounded-lg border border-line">
        {items.map((r) => (
          <li key={r.id} className="px-4 py-3">
            <button type="button" className="text-left text-sky-400" onClick={() => setId(r.id)} data-testid="failure-run">
              {r.agent_name} · {r.status} · {r.verification_status}
            </button>
            <p className="mt-1 font-mono text-xs text-red-300">{r.failure_reason ?? "no reason recorded"}</p>
            <Link className="text-[11px] text-ink-muted" href={`/executions/${r.id}`}>
              open execution
            </Link>
          </li>
        ))}
      </ul>
      {id && detail.data ? (
        <section className="rounded-lg border border-line p-4" data-testid="failure-panel">
          <h2 className="text-sm font-medium">Investigation {id}</h2>
          <dl className="mt-2 grid gap-1 font-mono text-xs sm:grid-cols-2">
            <div>first observed failure: {firstFail?.event_type ?? "none in event log"}</div>
            <div>failure reason: {detail.data.failure_reason ?? "—"}</div>
            <div>dispatch: {detail.data.dispatch_state ?? "—"}</div>
            <div>tool: {detail.data.tool_status ?? "—"}</div>
            <div>business op: {detail.data.business_operation_status ?? "—"}</div>
            <div>recovery: {detail.data.recovery_disposition ?? recovery.data?.recovery_disposition ?? "—"}</div>
            <div>retries: {recovery.data?.recovery_attempts?.length ?? 0}</div>
          </dl>
          <p className="mt-3 text-xs text-ink-muted">Related previous events (observed, not a root-cause claim)</p>
          <ul className="mt-1 font-mono text-[11px]">
            {related.map((e) => (
              <li key={e.event_id}>
                #{e.sequence} {e.event_type} {e.status ?? ""}
              </li>
            ))}
          </ul>
          {firstFail ? <JsonView value={firstFail.payload} /> : null}
        </section>
      ) : null}
    </div>
  );
}
