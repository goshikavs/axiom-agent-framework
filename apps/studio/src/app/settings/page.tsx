"use client";

import { useQuery } from "@tanstack/react-query";
import { ErrorBanner } from "@/components/metric-card";
import { api } from "@/lib/api";

export default function SettingsPage() {
  const health = useQuery({ queryKey: ["health"], queryFn: () => api<Record<string, unknown>>("/health") });
  const ready = useQuery({ queryKey: ["ready"], queryFn: () => api<Record<string, unknown>>("/ready") });
  if (health.isError) return <ErrorBanner message={(health.error as Error).message} />;
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Settings</h1>
      <p className="text-sm text-ink-muted">
        Local single-user mode binds the runtime to 127.0.0.1 and uses AXIOM_AUTH_MODE=none. Production must set api_key auth. Studio proxies the runtime; provider keys never enter the browser.
      </p>
      <pre className="overflow-auto rounded-lg border border-line bg-canvas-raised p-4 font-mono text-xs">{JSON.stringify({ health: health.data, ready: ready.data }, null, 2)}</pre>
    </div>
  );
}
