"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { EmptyState, ErrorBanner } from "@/components/metric-card";
import { api } from "@/lib/api";
import type { ApprovalRow, Paginated } from "@/lib/types";
import { formatTime } from "@/lib/utils";

export default function ApprovalsPage() {
  const qc = useQueryClient();
  const q = useQuery({
    queryKey: ["approvals"],
    queryFn: () => api<Paginated<ApprovalRow>>("/api/v1/approvals?status=pending"),
    refetchInterval: 3000,
  });
  const decide = useMutation({
    mutationFn: ({ id, decision }: { id: string; decision: "approve" | "deny" }) =>
      api(`/api/v1/approvals/${id}/decision`, { method: "POST", body: JSON.stringify({ decision, reason: `studio:${decision}` }) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["approvals"] }),
  });

  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  const items = q.data?.items ?? [];

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Approvals</h1>
      <p className="text-sm text-ink-muted">Local mode records actor `local-user`. Production requires GOSHIKQ_AUTH_MODE=api_key.</p>
      {items.length === 0 ? <EmptyState title="No pending approvals" body="Zero queued actions, not missing data." /> : null}
      <ul className="space-y-3">
        {items.map((a) => (
          <li key={a.id} className="rounded-lg border border-line bg-canvas-raised p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <p className="font-mono text-xs text-ink-faint">{a.id}</p>
                <p className="mt-1 text-sm">
                  <span className="font-medium">{a.agent_name}</span> proposes <span className="font-mono">{a.action_name}</span> ({a.risk})
                </p>
                <p className="mt-1 text-xs text-ink-muted">{a.reason}</p>
                <p className="mt-1 text-xs text-ink-faint">
                  requested {formatTime(a.requested_at)} · expires {formatTime(a.expires_at)} · run{" "}
                  <Link className="text-sky-400" href={`/executions/${a.run_id}`}>
                    {a.run_id.slice(0, 8)}
                  </Link>
                </p>
              </div>
              <div className="flex gap-2">
                <button className="rounded bg-emerald-800 px-3 py-1 text-xs" onClick={() => decide.mutate({ id: a.id, decision: "approve" })}>
                  Approve
                </button>
                <button className="rounded bg-red-900 px-3 py-1 text-xs" onClick={() => decide.mutate({ id: a.id, decision: "deny" })}>
                  Reject
                </button>
              </div>
            </div>
            <pre className="mt-3 overflow-auto rounded bg-canvas p-2 font-mono text-[11px] text-ink-muted">{JSON.stringify(a.arguments, null, 2)}</pre>
          </li>
        ))}
      </ul>
    </div>
  );
}
