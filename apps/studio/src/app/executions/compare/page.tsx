"use client";

import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { ExperimentalNote } from "@/components/planned";
import { StatusBadge } from "@/components/status-badge";
import { api, listRuns } from "@/lib/api";
import type { ExecutionEvent, Paginated, RunDetail } from "@/lib/types";
import { formatCost, formatDuration } from "@/lib/utils";

type Match = { a?: ExecutionEvent; b?: ExecutionEvent; key: string; confidence: "identity" | "heuristic" | "unmatched" };

function eventKey(e: ExecutionEvent): string {
  const p = e.payload || {};
  const tool = typeof p.tool === "string" ? p.tool : "";
  const model = typeof p.model === "string" ? p.model : "";
  return `${e.event_type}|${tool}|${model}|${e.span_id}`;
}

function matchEvents(a: ExecutionEvent[], b: ExecutionEvent[]): Match[] {
  const usedB = new Set<string>();
  const out: Match[] = [];
  for (const ev of a) {
    const ident = b.find((x) => !usedB.has(x.event_id) && x.span_id === ev.span_id && x.event_type === ev.event_type);
    if (ident) {
      usedB.add(ident.event_id);
      out.push({ a: ev, b: ident, key: ev.event_id, confidence: "identity" });
      continue;
    }
    const heur = b.find((x) => !usedB.has(x.event_id) && x.event_type === ev.event_type && eventKey(x).split("|").slice(0, 3).join("|") === eventKey(ev).split("|").slice(0, 3).join("|"));
    if (heur) {
      usedB.add(heur.event_id);
      out.push({ a: ev, b: heur, key: ev.event_id, confidence: "heuristic" });
    } else {
      out.push({ a: ev, key: ev.event_id, confidence: "unmatched" });
    }
  }
  for (const ev of b) {
    if (!usedB.has(ev.event_id)) out.push({ b: ev, key: ev.event_id, confidence: "unmatched" });
  }
  return out;
}

function Cell({ r }: { r?: RunDetail }) {
  if (!r) return <pre className="rounded border border-line bg-canvas-raised p-3 font-mono text-[11px]">—</pre>;
  return (
    <dl className="space-y-1 rounded border border-line bg-canvas-raised p-3 font-mono text-[11px]">
      <div>id {r.id}</div>
      <div>framework {r.source_framework}</div>
      <div>model {r.model}</div>
      <div>goal {r.goal_description || "—"}</div>
      <div>
        status <StatusBadge value={r.status} /> verify <StatusBadge value={r.verification_status} kind="verify" />
      </div>
      <div>duration {formatDuration(r.duration_ms)}</div>
      <div>tokens {r.usage_known ? r.input_tokens + r.output_tokens : "unknown"}</div>
      <div>cost {formatCost(r.estimated_cost_usd, r.cost_known)}</div>
      <div>dispatch {r.dispatch_state ?? "—"} tool {r.tool_status ?? "—"}</div>
    </dl>
  );
}

export default function ComparePage() {
  const runs = useQuery({ queryKey: ["compare-runs"], queryFn: () => listRuns({ limit: 50 }) });
  const [a, setA] = useState("");
  const [b, setB] = useState("");
  const left = useQuery({ queryKey: ["run", a], queryFn: () => api<RunDetail>(`/api/v1/runs/${a}`), enabled: !!a });
  const right = useQuery({ queryKey: ["run", b], queryFn: () => api<RunDetail>(`/api/v1/runs/${b}`), enabled: !!b });
  const leftEv = useQuery({ queryKey: ["events", a], queryFn: () => api<Paginated<ExecutionEvent>>(`/api/v1/runs/${a}/events?limit=2000`), enabled: !!a });
  const rightEv = useQuery({ queryKey: ["events", b], queryFn: () => api<Paginated<ExecutionEvent>>(`/api/v1/runs/${b}/events?limit=2000`), enabled: !!b });
  const items = runs.data?.items ?? [];
  const matches = useMemo(() => matchEvents(leftEv.data?.items ?? [], rightEv.data?.items ?? []), [leftEv.data, rightEv.data]);

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Compare runs</h1>
      <ExperimentalNote>
        Matching uses span_id identity when both runs share it; otherwise event_type + tool/model heuristics. Uncertain matches are labeled. No side effects.
      </ExperimentalNote>
      <div className="flex gap-2">
        <select className="rounded border border-line bg-canvas px-2 py-1 text-sm" value={a} onChange={(e) => setA(e.target.value)} data-testid="compare-a">
          <option value="">Run A</option>
          {items.map((r) => (
            <option key={r.id} value={r.id}>{r.agent_name} {r.id.slice(0, 8)}</option>
          ))}
        </select>
        <select className="rounded border border-line bg-canvas px-2 py-1 text-sm" value={b} onChange={(e) => setB(e.target.value)} data-testid="compare-b">
          <option value="">Run B</option>
          {items.map((r) => (
            <option key={r.id} value={r.id}>{r.agent_name} {r.id.slice(0, 8)}</option>
          ))}
        </select>
      </div>
      <div className="grid gap-4 md:grid-cols-2" data-testid="compare-summaries">
        <Cell r={left.data} />
        <Cell r={right.data} />
      </div>
      {a && b ? (
        <table className="w-full text-left text-xs" data-testid="compare-events">
          <thead>
            <tr className="text-ink-faint">
              <th className="py-1">A</th>
              <th>B</th>
              <th>Match</th>
            </tr>
          </thead>
          <tbody>
            {matches.map((m) => (
              <tr key={m.key} className="border-t border-line font-mono">
                <td className="py-1">{m.a?.event_type ?? "—"}</td>
                <td>{m.b?.event_type ?? "—"}</td>
                <td className={m.confidence === "unmatched" ? "text-amber-300" : "text-ink-muted"}>{m.confidence}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
    </div>
  );
}
