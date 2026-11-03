"use client";

import { useQuery } from "@tanstack/react-query";
import { ErrorBanner } from "@/components/metric-card";
import { api } from "@/lib/api";

type Integration = {
  id: string;
  framework: string;
  adapter_version: string;
  framework_version: string | null;
  status: string;
  capabilities: unknown;
  unsupported_capabilities: unknown;
  last_event_at: string | null;
  last_error: string | null;
};

type Manifest = {
  framework: string;
  framework_version: string;
  adapter_version: string;
  trace_support: string;
  tool_event_support: string;
  state_snapshot_support: string;
  approval_support: string;
  action_interception_support: string;
  pause_resume_support: string;
  outcome_verification_support: string;
  replay_support: string;
  notes: string;
};

export default function IntegrationsPage() {
  const q = useQuery({ queryKey: ["integrations"], queryFn: () => api<Integration[]>("/api/v1/integrations") });
  const caps = useQuery({ queryKey: ["capabilities"], queryFn: () => api<Manifest[]>("/api/v1/capabilities") });
  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  const items = q.data ?? [];
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Integrations</h1>
      <p className="text-sm text-ink-muted">
        Capability rows below are what the adapter actually supports. Unavailable controls are not shown as if they worked. Native Python and TypeScript SDKs execute through the Rust worker protocol. LangGraph and OpenAI Agents SDK contribute traces only. MCP tools are wrapped with Goshikq allowlists and contracts. A2A remains planned.
      </p>
      <ul className="grid gap-3 md:grid-cols-2">
        {items.map((i) => (
          <li key={i.id} className="rounded-lg border border-line bg-canvas-raised p-4">
            <div className="flex justify-between">
              <h2 className="font-medium">{i.framework}</h2>
              <span className="font-mono text-[11px] text-ink-muted">{i.status}</span>
            </div>
            <p className="mt-1 font-mono text-[11px] text-ink-faint">
              adapter {i.adapter_version}
              {i.framework_version ? ` · framework ${i.framework_version}` : ""}
            </p>
            <p className="mt-2 text-xs text-ink-muted">capabilities: {JSON.stringify(i.capabilities)}</p>
            <p className="text-xs text-ink-faint">unsupported: {JSON.stringify(i.unsupported_capabilities)}</p>
            <p className="mt-2 text-[11px] text-ink-faint">last event {i.last_event_at ?? "never"}</p>
            {i.last_error ? <p className="mt-2 text-xs text-red-300">{i.last_error}</p> : null}
          </li>
        ))}
      </ul>
      {caps.data && caps.data.length > 0 ? (
        <section className="rounded-lg border border-line bg-canvas-raised p-4">
          <h2 className="text-sm font-medium">Capability manifests</h2>
          <p className="mb-3 text-xs text-ink-muted">Unsupported stays unsupported. Partial means attribute-dependent.</p>
          <ul className="space-y-3 text-xs">
            {caps.data.map((m) => {
              const levels: [string, string][] = [
                ["traces", m.trace_support],
                ["tools", m.tool_event_support],
                ["snapshots", m.state_snapshot_support],
                ["approvals", m.approval_support],
                ["intercept", m.action_interception_support],
                ["pause", m.pause_resume_support],
                ["verify", m.outcome_verification_support],
                ["replay", m.replay_support],
              ];
              const available = levels.filter(([, v]) => v !== "unsupported");
              const hidden = levels.filter(([, v]) => v === "unsupported").map(([k]) => k);
              return (
                <li key={m.framework} data-testid={`capability-${m.framework}`}>
                  <p className="font-medium">
                    {m.framework} <span className="font-mono text-ink-faint">adapter {m.adapter_version}</span>
                  </p>
                  <p className="font-mono text-[11px] text-ink-muted">
                    {available.map(([k, v]) => `${k} ${v}`).join(" · ") || "no supported controls"}
                  </p>
                  {hidden.length ? <p className="text-[11px] text-ink-faint">not available: {hidden.join(", ")}</p> : null}
                  <p className="mt-1 text-ink-faint">{m.notes}</p>
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
