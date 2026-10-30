"use client";

import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { EmptyState, ErrorBanner } from "@/components/metric-card";
import { StatusBadge } from "@/components/status-badge";
import { listRuns } from "@/lib/api";

export default function VerificationIndexPage() {
  const q = useQuery({ queryKey: ["runs-verify"], queryFn: () => listRuns({ limit: 50 }) });
  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  const items = q.data?.items ?? [];
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Verification</h1>
      <p className="text-sm text-ink-muted">Open a run for the intended vs observed report. VERIFIED is only used when a success predicate held against a state snapshot.</p>
      {items.length === 0 ? <EmptyState title="No runs" body="Nothing to verify." /> : null}
      <ul className="divide-y divide-line rounded-lg border border-line">
        {items.map((r) => (
          <li key={r.id} className="flex items-center justify-between px-4 py-2 text-sm">
            <Link className="text-sky-400" href={`/executions/${r.id}/verification`}>
              {r.agent_name} — {r.goal_description}
            </Link>
            <StatusBadge value={r.verification_status} kind="verify" />
          </li>
        ))}
      </ul>
    </div>
  );
}
