"use client";

import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { EmptyState, ErrorBanner } from "@/components/metric-card";
import { listRuns } from "@/lib/api";

export default function TracesPage() {
  const q = useQuery({ queryKey: ["traces"], queryFn: () => listRuns({ limit: 50 }) });
  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  const items = q.data?.items ?? [];
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Traces</h1>
      <p className="text-sm text-ink-muted">One W3C-shaped trace_id per run. Open an execution to see parent/child spans. Missing parents are not fabricated.</p>
      {items.length === 0 ? <EmptyState title="No traces" body="Run an agent first." /> : null}
      <ul className="divide-y divide-line rounded-lg border border-line font-mono text-xs">
        {items.map((r) => (
          <li key={r.id} className="flex justify-between px-4 py-2">
            <span>{r.trace_id}</span>
            <span className="text-ink-muted">{r.source_framework}</span>
            <Link className="text-sky-400" href={`/executions/${r.id}`}>
              {r.agent_name}
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}
