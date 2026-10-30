"use client";

import { useQuery } from "@tanstack/react-query";
import { ErrorBanner } from "@/components/metric-card";
import { api } from "@/lib/api";

export default function PerformancePage() {
  const q = useQuery({ queryKey: ["costs"], queryFn: () => api<Record<string, unknown>>("/api/v1/metrics/costs") });
  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Performance and cost</h1>
      <p className="text-sm text-ink-muted">Per-model, per-agent, and per-framework breakdowns from persisted usage. Framework overhead is only shown on a run when both model and tool timings are known.</p>
      <pre className="overflow-auto rounded-lg border border-line bg-canvas-raised p-4 font-mono text-xs">{JSON.stringify(q.data ?? {}, null, 2)}</pre>
    </div>
  );
}
