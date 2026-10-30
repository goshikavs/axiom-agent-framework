"use client";

import { useQuery } from "@tanstack/react-query";
import { useParams } from "next/navigation";
import { JsonView } from "@/components/json-view";
import { ErrorBanner } from "@/components/metric-card";
import { api } from "@/lib/api";

export default function AgentDetailPage() {
  const { id } = useParams<{ id: string }>();
  const q = useQuery({ queryKey: ["agent", id], queryFn: () => api<Record<string, unknown>>(`/api/v1/agents/${id}`) });
  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  if (!q.data) return <p className="text-sm text-ink-muted">Loading…</p>;
  const agent = q.data.agent as { name: string; description: string };
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">{agent?.name}</h1>
      <p className="text-sm text-ink-muted">{agent?.description || "No description."}</p>
      <p className="text-xs text-ink-faint">
        runs {String(q.data.run_count)} · verified {String(q.data.verified_runs)} · failed {String(q.data.failed_runs)}
      </p>
      <JsonView value={q.data} />
    </div>
  );
}
