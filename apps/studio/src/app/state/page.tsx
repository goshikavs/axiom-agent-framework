"use client";

import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { useState } from "react";
import { JsonView } from "@/components/json-view";
import { EmptyState, ErrorBanner } from "@/components/metric-card";
import { api, listRuns } from "@/lib/api";

export default function StatePage() {
  const runs = useQuery({ queryKey: ["runs-state"], queryFn: () => listRuns({ limit: 20 }) });
  const [id, setId] = useState("");
  const q = useQuery({ queryKey: ["state", id], queryFn: () => api<Record<string, unknown>>(`/api/v1/runs/${id}/state`), enabled: !!id });
  if (runs.isError) return <ErrorBanner message={(runs.error as Error).message} />;
  const items = runs.data?.items ?? [];
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">State inspector</h1>
      <p className="text-sm text-ink-muted">Authoritative snapshots captured during a run. This is observed external state, not a reconstruction of model reasoning.</p>
      {items.length === 0 ? <EmptyState title="No runs" body="Nothing to inspect." /> : null}
      <select className="rounded border border-line bg-canvas px-2 py-1 text-sm" value={id} onChange={(e) => setId(e.target.value)}>
        <option value="">Select a run</option>
        {items.map((r) => (
          <option key={r.id} value={r.id}>
            {r.agent_name} · {r.id.slice(0, 8)}
          </option>
        ))}
      </select>
      {id ? (
        <p className="text-xs">
          <Link className="text-sky-400" href={`/executions/${id}`}>
            Open execution
          </Link>
        </p>
      ) : null}
      {q.data ? <JsonView value={q.data} /> : null}
    </div>
  );
}
