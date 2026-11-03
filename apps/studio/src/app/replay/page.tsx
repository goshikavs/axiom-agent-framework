"use client";

import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { JsonView } from "@/components/json-view";
import { ExperimentalNote } from "@/components/planned";
import { StatusBadge } from "@/components/status-badge";
import { api, listRuns } from "@/lib/api";
import type { ExecutionEvent, Paginated } from "@/lib/types";
import { formatTime } from "@/lib/utils";

export default function ReplayPage() {
  const runs = useQuery({ queryKey: ["replay-runs"], queryFn: () => listRuns({ limit: 50 }) });
  const [id, setId] = useState("");
  const [step, setStep] = useState(0);
  const ev = useQuery({
    queryKey: ["replay-events", id],
    queryFn: () => api<Paginated<ExecutionEvent>>(`/api/v1/runs/${id}/events?limit=2000`),
    enabled: !!id,
  });
  const stateQ = useQuery({
    queryKey: ["replay-state", id],
    queryFn: () => api<Record<string, unknown>>(`/api/v1/runs/${id}/state`),
    enabled: !!id,
  });
  const events = ev.data?.items ?? [];
  const current = events[step];
  const visible = useMemo(() => events.slice(0, step + 1), [events, step]);
  const snapshots = visible.filter((e) => e.event_type === "state.snapshot" || e.event_type === "state.changed");

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Historical replay</h1>
      <ExperimentalNote>
        Read-only GET of persisted events. This viewer never calls models, executes tools, submits refunds, or grants approvals.
      </ExperimentalNote>
      <select className="rounded border border-line bg-canvas px-2 py-1 text-sm" value={id} onChange={(e) => { setId(e.target.value); setStep(0); }} data-testid="replay-select">
        <option value="">Select a historical run</option>
        {(runs.data?.items ?? []).map((r) => (
          <option key={r.id} value={r.id}>
            {r.agent_name} · {r.status} · {r.verification_status}
          </option>
        ))}
      </select>
      {id ? (
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <button className="rounded border border-line px-2 py-1" onClick={() => setStep((s) => Math.max(0, s - 1))} data-testid="replay-back">
            Back
          </button>
          <button className="rounded border border-line px-2 py-1" onClick={() => setStep((s) => Math.min(Math.max(events.length - 1, 0), s + 1))} data-testid="replay-forward">
            Forward
          </button>
          <span className="font-mono text-xs text-ink-muted" data-testid="replay-step">
            {events.length ? step + 1 : 0} / {events.length}
          </span>
        </div>
      ) : null}
      <div className="grid gap-4 lg:grid-cols-2">
        <ol className="max-h-[480px] space-y-1 overflow-auto font-mono text-xs" data-testid="replay-list">
          {visible.map((e, i) => (
            <li key={e.event_id} className={`flex justify-between rounded border px-2 py-1 ${i === step ? "border-accent bg-accent/10" : "border-line"}`}>
              <span>{e.event_type}</span>
              <span className="text-ink-faint">
                {formatTime(e.timestamp)} {e.status ? <StatusBadge value={e.status.toUpperCase()} /> : null}
              </span>
            </li>
          ))}
        </ol>
        <div className="space-y-3">
          <p className="text-xs text-ink-muted">Event at this recorded point (not a live re-execution).</p>
          {current ? <JsonView value={current} /> : null}
          <p className="text-xs text-ink-muted" data-testid="replay-historical-state">
            Historical state is reconstructed only from recorded snapshot events ({snapshots.length} up to this step).
            {snapshots.length === 0 ? " This step has no recorded snapshot; Goshikq does not invent one." : ""}
          </p>
          {snapshots.length ? <JsonView value={snapshots[snapshots.length - 1]?.payload ?? snapshots[snapshots.length - 1]} /> : null}
          <details className="rounded border border-amber-900/60 p-2">
            <summary className="cursor-pointer text-xs text-amber-200">Current GET /state (not historical)</summary>
            <p className="my-1 text-[11px] text-ink-muted">This is the latest persisted snapshot endpoint, not the state at the selected event.</p>
            {stateQ.data ? <JsonView value={stateQ.data} /> : <p className="text-xs text-ink-muted">No snapshot endpoint data.</p>}
          </details>
        </div>
      </div>
    </div>
  );
}
