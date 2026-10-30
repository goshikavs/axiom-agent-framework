"use client";

import { useQuery } from "@tanstack/react-query";
import { ErrorBanner } from "@/components/metric-card";
import { api } from "@/lib/api";

export default function ModelsPage() {
  const q = useQuery({ queryKey: ["models"], queryFn: () => api<Record<string, unknown>>("/api/v1/models") });
  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Models</h1>
      <p className="text-sm text-ink-muted">Adapter capability manifest and versioned pricing. Costs are estimates. Unknown pricing is shown as unknown, never as $0.</p>
      <pre className="overflow-auto rounded-lg border border-line bg-canvas-raised p-4 font-mono text-xs">{JSON.stringify(q.data ?? {}, null, 2)}</pre>
    </div>
  );
}
