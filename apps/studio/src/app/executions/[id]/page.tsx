"use client";

import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { useParams } from "next/navigation";
import { useEffect, useMemo, useRef, useState } from "react";
import { ExecutionGraph } from "@/components/execution-graph";
import { JsonView } from "@/components/json-view";
import { ErrorBanner } from "@/components/metric-card";
import { StatusBadge } from "@/components/status-badge";
import { api, listRunEvents } from "@/lib/api";
import { eventsToGraph } from "@/lib/graph";
import type { ExecutionEvent, RecoveryReport, RunDetail, RunHierarchy } from "@/lib/types";
import { mcpPayload, sdkOrigin } from "@/lib/sdk-origin";
import { copyJson, downloadJson, formatCost, formatDuration, formatTime } from "@/lib/utils";

type Conn = "connected" | "reconnecting" | "disconnected" | "completed";

const TERMINAL = ["COMPLETED", "FAILED", "CANCELLED", "UNVERIFIED", "TIMED_OUT", "BUDGET_EXCEEDED", "STEP_LIMIT_EXCEEDED", "MODEL_BUDGET_EXCEEDED"];

export default function ExecutionDetailPage() {
  const { id } = useParams<{ id: string }>();
  const runQ = useQuery({ queryKey: ["run", id], queryFn: () => api<RunDetail>(`/api/v1/runs/${id}`), refetchInterval: (q) => (q.state.data && TERMINAL.includes(q.state.data.status) ? false : 2000) });
  const [events, setEvents] = useState<ExecutionEvent[]>([]);
  const [conn, setConn] = useState<Conn>("disconnected");
  const [selected, setSelected] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [autoScroll, setAutoScroll] = useState(true);
  const [paused, setPaused] = useState(false);
  const pausedRef = useRef(false);
  pausedRef.current = paused;
  const pending = useRef<ExecutionEvent[]>([]);
  const bottom = useRef<HTMLDivElement>(null);
  const seen = useRef(new Set<string>());
  const lastSeq = useRef(0);

  const ingest = (ev: ExecutionEvent) => {
    if (!ev?.event_id || seen.current.has(ev.event_id)) return;
    if (pausedRef.current) {
      pending.current.push(ev);
      return;
    }
    seen.current.add(ev.event_id);
    lastSeq.current = Math.max(lastSeq.current, ev.sequence ?? 0);
    setEvents((cur) => {
      if (cur.some((e) => e.event_id === ev.event_id || e.sequence === ev.sequence)) return cur;
      return [...cur, ev].sort((a, b) => a.sequence - b.sequence);
    });
  };

  function flushPaused() {
    const queued = pending.current.splice(0);
    for (const ev of queued) ingest(ev);
  }

  useEffect(() => {
    seen.current = new Set();
    lastSeq.current = 0;
    setEvents([]);
    let es: EventSource | null = null;
    let stopped = false;
    let completed = false;
    const onPayload = (raw: string) => {
      if (!raw || raw === "ping") return;
      try {
        const ev = JSON.parse(raw) as ExecutionEvent;
        ingest(ev);
      } catch {
        /* heartbeat or named end payload */
      }
    };
    const connect = (after: number) => {
      if (stopped || completed) return;
      setConn("reconnecting");
      es = new EventSource(`/api/runtime/api/v1/runs/${id}/events/stream?after=${after}`);
      es.onopen = () => setConn("connected");
      es.onmessage = (msg) => onPayload(msg.data);
      es.addEventListener("end", () => {
        completed = true;
        setConn("completed");
        es?.close();
      });
      es.onerror = () => {
        es?.close();
        if (stopped || completed) return;
        setConn("reconnecting");
        setTimeout(() => connect(lastSeq.current), 1500);
      };
    };
    listRunEvents(id)
      .then((page) => {
        if (stopped) return;
        for (const ev of page.items) ingest(ev);
        connect(lastSeq.current);
      })
      .catch(() => {
        if (!stopped) connect(lastSeq.current);
      });
    return () => {
      stopped = true;
      es?.close();
    };
  }, [id]);

  useEffect(() => {
    if (autoScroll) bottom.current?.scrollIntoView({ block: "end" });
  }, [events, autoScroll]);

  const graph = useMemo(() => eventsToGraph(events), [events]);
  const selectedEvent = events.find((e) => e.span_id === selected || e.event_id === selected) ?? events.find((e) => e.event_type === selected);
  const filtered = events.filter((e) => !filter || e.event_type.includes(filter) || JSON.stringify(e.payload).includes(filter));
  const run = runQ.data;

  if (runQ.isError) return <ErrorBanner message={(runQ.error as Error).message} />;
  if (!run) return <p className="text-sm text-ink-muted">Loading run…</p>;

  return (
    <div className="flex h-[calc(100vh-3rem)] flex-col gap-3">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="font-mono text-xs text-ink-faint">{run.id}</p>
          <h1 className="text-xl font-semibold">{run.agent_name}</h1>
          <p className="text-sm text-ink-muted">{run.goal_description}</p>
          <p className="mt-1 font-mono text-[11px] text-ink-faint" data-testid="sdk-origin">
            {sdkOrigin(run.source_framework).label} · {run.source_framework} · trace {run.trace_id}
            {sdkOrigin(run.source_framework).controlsExecution ? "" : " · Goshikq did not control this execution"}
          </p>
          {sdkOrigin(run.source_framework).controlsExecution ? null : (
            <p className="mt-1 text-[11px] text-amber-200">
              Verification availability: imported run.completed is UNVERIFIED unless a Goshikq outcome verifier recorded evidence.
              Adapter capabilities: see Integrations. Missing telemetry stays unknown.
            </p>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <StatusBadge value={run.status} />
          <StatusBadge value={run.verification_status} kind="verify" />
          <span className="rounded border border-line px-2 py-1">{run.source_framework}</span>
          <span className="rounded border border-line px-2 py-1">{run.model}</span>
          <span className="text-ink-muted">{formatDuration(run.duration_ms)}</span>
          <span className="text-ink-muted">{formatCost(run.estimated_cost_usd, run.cost_known)}</span>
          <ConnDot state={conn} />
          <button
            type="button"
            className="rounded border border-line px-2 py-1"
            title="Download run + redacted events as persisted"
            onClick={() => downloadJson(`run-${id}-redacted.json`, { run, events })}
          >
            Download JSON
          </button>
          <label className="flex items-center gap-1 text-[11px] text-ink-muted" title="Pause visual updates only. Execution continues.">
            <input
              type="checkbox"
              checked={paused}
              onChange={(e) => {
                const next = e.target.checked;
                setPaused(next);
                if (!next) flushPaused();
              }}
            />
            pause UI
          </label>
          <Link className="text-sky-400 hover:underline" href={`/executions/${id}/verification`} data-testid="run-verification-link">
            Verification
          </Link>
          <Link className="text-sky-400 hover:underline" href={`/executions/${id}/evidence`} data-testid="run-evidence-link">
            Evidence
          </Link>
        </div>
      </header>
      <RecoveryPanel runId={id} runStatus={run.status} verification={run.verification_status} />
      <TeamPanel runId={id} selected={selected} onSelect={setSelected} events={events} />
      <WorkflowNodesPanel runId={id} />
      <div className="flex min-h-0 flex-1 flex-col gap-3 xl:flex-row">
        <Panel title="Execution graph">
          <ExecutionGraph nodes={graph.nodes} edges={graph.edges} selected={selected} onSelect={setSelected} />
        </Panel>
        <Panel
          title="Timeline"
          extra={
            <label className="flex items-center gap-2 text-[11px] text-ink-muted">
              <input type="checkbox" checked={autoScroll} onChange={(e) => setAutoScroll(e.target.checked)} />
              auto-scroll
            </label>
          }
        >
          <input className="mb-2 w-full rounded border border-line bg-canvas px-2 py-1 text-xs" placeholder="Filter events" value={filter} onChange={(e) => setFilter(e.target.value)} />
          <ol className="space-y-1 overflow-y-auto scrollbar-thin" data-testid="timeline">
            {filtered.map((e) => (
              <li key={e.event_id}>
                <button
                  type="button"
                  data-testid="timeline-event"
                  onClick={() => setSelected(e.span_id || e.event_id)}
                  className={`w-full rounded border px-2 py-1.5 text-left text-xs ${selected === (e.span_id || e.event_id) ? "border-accent bg-accent/10" : "border-transparent hover:bg-canvas"}`}
                >
                  <div className="flex justify-between gap-2 font-mono">
                    <span>{e.event_type}</span>
                    <span className="text-ink-faint">#{e.sequence}</span>
                  </div>
                  <div className="mt-0.5 flex justify-between text-[11px] text-ink-muted">
                    <span>{formatTime(e.timestamp)}</span>
                    <span>{e.duration_ms != null ? `${e.duration_ms} ms` : e.status ?? ""}</span>
                  </div>
                </button>
              </li>
            ))}
            <div ref={bottom} />
          </ol>
        </Panel>
        <Panel title="Inspector">
          <div data-testid="inspector">
          {selectedEvent ? <Inspector ev={selectedEvent} run={run} events={events} /> : <p className="text-sm text-ink-muted">Select a graph node or timeline event.</p>}
          </div>
        </Panel>
      </div>
    </div>
  );
}

function recoveryLabel(status: string, dispatch?: string | null, verification?: string, disposition?: string | null) {
  if (disposition === "MANUAL_RECONCILIATION_REQUIRED") return "Manual reconciliation required";
  if (disposition === "REVALIDATION_REQUIRED") return "Revalidation required";
  if (status === "RECOVERY_REQUIRED") return "Recovery required";
  if (status === "WAITING_FOR_APPROVAL") return "Waiting for approval";
  if (status === "WAITING_FOR_EXTERNAL_STATE") return "Waiting for external state";
  if (verification === "INCONCLUSIVE" && status === "UNVERIFIED") return "Verification inconclusive";
  if (dispatch === "DISPATCH_UNCERTAIN") return "Uncertain dispatch";
  if (dispatch === "OUTCOME_CONFIRMED") return "Operation settled";
  if (dispatch === "OUTCOME_PENDING") return "Operation pending";
  if (dispatch === "ACCEPTED") return "Operation accepted";
  if (status === "PLANNING" || status === "EXECUTING" || status === "RECONCILING_STATE") return "Recovery in progress";
  return status.replaceAll("_", " ");
}

function RecoveryPanel({ runId, runStatus, verification }: { runId: string; runStatus: string; verification: string }) {
  const q = useQuery({
    queryKey: ["recovery", runId],
    queryFn: () => api<RecoveryReport>(`/api/v1/runs/${runId}/recovery`),
    refetchInterval: TERMINAL.includes(runStatus) ? false : 2000,
  });
  const r = q.data;
  if (!r) return null;
  const show =
    ["RECOVERY_REQUIRED", "WAITING_FOR_APPROVAL", "WAITING_FOR_EXTERNAL_STATE", "UNVERIFIED"].includes(r.status) ||
    Boolean(r.uncertain_write) ||
    Boolean(r.recovery_attempts.length) ||
    Boolean(r.warning);
  if (!show) return null;
  return (
    <section className="rounded-lg border border-line bg-canvas-raised p-3 text-xs">
      <header className="mb-2 flex flex-wrap items-center gap-2">
        <span className="font-medium">Recovery</span>
        <StatusBadge value={recoveryLabel(r.status, r.dispatch_state, verification, r.recovery_disposition)} />
        {r.dispatch_state ? <StatusBadge value={r.dispatch_state} /> : null}
        {r.business_operation_status ? <StatusBadge value={r.business_operation_status} /> : null}
        {r.recovery_disposition ? <StatusBadge value={r.recovery_disposition} /> : null}
        {r.pending_approval_id ? (
          <Link className="text-sky-400 hover:underline" href="/approvals">
            Review approval
          </Link>
        ) : null}
      </header>
      {r.warning ? <p className="mb-2 rounded border border-amber-800 bg-amber-950/40 px-2 py-1 text-amber-200">{r.warning}</p> : null}
      <dl className="grid grid-cols-[auto,1fr] gap-x-3 gap-y-1 font-mono text-[11px] text-ink-muted">
        <dt>checkpoint</dt>
        <dd>
          {r.last_checkpoint_phase ?? "—"} {r.last_checkpoint_at ? `· ${r.last_checkpoint_at}` : ""}
        </dd>
        <dt>generation</dt>
        <dd>{r.recovery_generation ?? "—"} / lease {r.lease_generation ?? "—"}</dd>
        <dt>action</dt>
        <dd>
          {r.tool_status ?? "—"} {r.dispatch_state ? `(${r.dispatch_state})` : ""}
        </dd>
        <dt>external id</dt>
        <dd>{r.external_operation_id ?? "—"}</dd>
        <dt>attempts</dt>
        <dd>{r.recovery_attempts.length}</dd>
      </dl>
      {r.recovery_attempts.length > 0 ? (
        <ol className="mt-2 space-y-1">
          {r.recovery_attempts.map((a) => (
            <li key={a.attempt} className="rounded border border-line px-2 py-1">
              #{a.attempt} {a.phase} → {a.action_taken} ({a.result})
              {a.reason ? ` · ${a.reason}` : ""}
            </li>
          ))}
        </ol>
      ) : null}
    </section>
  );
}

function WorkflowNodesPanel({ runId }: { runId: string }) {
  const q = useQuery({
    queryKey: ["workflow-nodes", runId],
    queryFn: () => api<Array<{ node_id: string; status: string; error?: string | null; output?: unknown }>>(`/api/v1/runs/${runId}/workflow-nodes`),
    refetchInterval: 2000,
  });
  const items = q.data ?? [];
  if (!items.length) return null;
  return (
    <section className="rounded-lg border border-line bg-canvas-raised p-3 text-xs" data-testid="workflow-nodes">
      <header className="mb-2 font-medium">Workflow nodes</header>
      <ol className="grid gap-1 md:grid-cols-2">
        {items.map((n) => (
          <li key={n.node_id} className="flex items-center justify-between rounded border border-line px-2 py-1" data-testid="workflow-node">
            <span className="font-mono">{n.node_id}</span>
            <StatusBadge value={n.status} />
          </li>
        ))}
      </ol>
    </section>
  );
}

function TeamPanel({
  runId,
  selected,
  onSelect,
  events,
}: {
  runId: string;
  selected: string | null;
  onSelect: (id: string) => void;
  events: ExecutionEvent[];
}) {
  const q = useQuery({
    queryKey: ["hierarchy", runId],
    queryFn: () => api<RunHierarchy>(`/api/v1/runs/${runId}/hierarchy`),
    refetchInterval: 3000,
  });
  const h = q.data;
  if (!h) return null;
  const hasTeam = Boolean(h.parent) || h.children.length > 0;
  if (!hasTeam) return null;
  const delegated = events.filter((e) => e.event_type === "agent.delegated" || e.event_type === "agent.handoff_completed");
  return (
    <section className="rounded-lg border border-line bg-canvas-raised p-3 text-xs" data-testid="team-panel">
      <header className="mb-2 flex flex-wrap items-center gap-2">
        <span className="font-medium">Multi-agent</span>
        <StatusBadge value={h.run.agent_name} />
        {h.parent ? (
          <Link className="text-sky-400 hover:underline" href={`/executions/${h.parent.id}`} data-testid="parent-agent">
            parent {h.parent.agent_name}
          </Link>
        ) : (
          <span data-testid="parent-agent">coordinator</span>
        )}
      </header>
      <p className="mb-2 text-[11px] text-ink-muted">
        Child completion is not parent verification. Relationships are persisted run_tasks rows, not inferred.
      </p>
      <div className="grid gap-3 md:grid-cols-2" data-testid="delegation-graph">
        <ol className="space-y-1">
          {h.children.map((c) => (
            <li key={c.task.task_id}>
              <Link
                className="flex items-center justify-between rounded border border-line px-2 py-1 hover:bg-canvas"
                href={`/executions/${c.run.id}`}
                data-testid="child-agent"
              >
                <span>
                  {c.run.agent_name} <span className="text-ink-faint">({c.task.role})</span>
                </span>
                <span className="flex gap-1">
                  <StatusBadge value={c.run.status} />
                  <StatusBadge value={c.run.verification_status} kind="verify" />
                </span>
              </Link>
            </li>
          ))}
        </ol>
        <ol className="space-y-1" data-testid="handoff-events">
          {delegated.map((e) => (
            <li key={e.event_id}>
              <button
                type="button"
                className={`w-full rounded border px-2 py-1 text-left font-mono ${selected === (e.span_id || e.event_id) ? "border-accent bg-accent/10" : "border-line"}`}
                onClick={() => onSelect(e.span_id || e.event_id)}
              >
                {e.event_type} #{e.sequence}
              </button>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}

function Panel({ title, extra, children }: { title: string; extra?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="flex min-h-[280px] min-w-[220px] flex-1 flex-col overflow-hidden rounded-lg border border-line bg-canvas-raised" style={{ resize: "horizontal" }}>
      <header className="flex items-center justify-between border-b border-line px-3 py-2 text-sm font-medium">
        {title}
        {extra}
      </header>
      <div className="min-h-0 flex-1 overflow-auto p-3">{children}</div>
    </section>
  );
}

function ConnDot({ state }: { state: Conn }) {
  const color = { connected: "bg-emerald-400", reconnecting: "bg-amber-400 running-pulse", disconnected: "bg-slate-500", completed: "bg-sky-400" }[state];
  return (
    <span className="inline-flex items-center gap-1 text-[11px] text-ink-muted" data-testid="sse-conn">
      <span className={`status-dot ${color}`} />
      {state}
    </span>
  );
}

function McpInspector({ mcp, payload, ev }: { mcp: Record<string, unknown>; payload: unknown; ev: ExecutionEvent }) {
  const output = payload && typeof payload === "object" ? (payload as Record<string, unknown>).output ?? payload : payload;
  const input = payload && typeof payload === "object" ? (payload as Record<string, unknown>).arguments ?? (payload as Record<string, unknown>).input : undefined;
  return (
    <section className="rounded border border-line p-2" data-testid="mcp-inspector">
      <h3 className="mb-1 font-medium">MCP tool</h3>
      <dl className="grid grid-cols-[auto,1fr] gap-x-3 gap-y-1 font-mono text-[11px]">
        <dt className="text-ink-faint">server</dt>
        <dd>{String(mcp.server_id ?? "—")}</dd>
        <dt className="text-ink-faint">tool</dt>
        <dd>{String(mcp.tool ?? "—")}</dd>
        <dt className="text-ink-faint">schema</dt>
        <dd>{mcp.schema_valid === true ? "validated" : mcp.schema_valid === false ? "invalid" : "unknown"}</dd>
        <dt className="text-ink-faint">permissions</dt>
        <dd>{Array.isArray(mcp.permissions) ? mcp.permissions.join(", ") || "—" : "—"}</dd>
        <dt className="text-ink-faint">risk</dt>
        <dd>{String(mcp.risk ?? "—")}</dd>
        <dt className="text-ink-faint">approval</dt>
        <dd>{mcp.approval_required ? "required" : "not required"}</dd>
        <dt className="text-ink-faint">duration</dt>
        <dd>{mcp.duration_ms != null ? `${mcp.duration_ms} ms` : formatDuration(ev.duration_ms)}</dd>
        <dt className="text-ink-faint">error</dt>
        <dd>{ev.status === "error" || ev.status === "failed" ? ev.status : "none"}</dd>
      </dl>
      {input != null ? (
        <div className="mt-2">
          <p className="text-ink-faint">input</p>
          <JsonView value={input} />
        </div>
      ) : null}
      {output != null ? (
        <div className="mt-2">
          <p className="text-ink-faint">output</p>
          <JsonView value={output} />
        </div>
      ) : null}
    </section>
  );
}

function Inspector({ ev, run, events }: { ev: ExecutionEvent; run: RunDetail; events: ExecutionEvent[] }) {
  const eventsHasParent = (e: ExecutionEvent) => Boolean(e.parent_span_id && events.some((x) => x.span_id === e.parent_span_id));
  const mcp = mcpPayload(ev.payload);
  const origin = sdkOrigin(run.source_framework);
  return (
    <div className="space-y-3 text-xs">
      <dl className="grid grid-cols-[auto,1fr] gap-x-3 gap-y-1 font-mono">
        <dt className="text-ink-faint">event</dt>
        <dd>{ev.event_type}</dd>
        <dt className="text-ink-faint">step</dt>
        <dd>{ev.step_id ?? "—"}</dd>
        <dt className="text-ink-faint">span</dt>
        <dd>{ev.span_id}</dd>
        <dt className="text-ink-faint">parent</dt>
        <dd>{ev.parent_span_id ?? "—"}</dd>
        <dt className="text-ink-faint">status</dt>
        <dd>{ev.status ?? "—"}</dd>
        <dt className="text-ink-faint">framework</dt>
        <dd>{origin.label} ({ev.source_framework})</dd>
        <dt className="text-ink-faint">adapter</dt>
        <dd>{typeof ev.metadata?.adapter_version === "string" ? ev.metadata.adapter_version : "—"}</dd>
        <dt className="text-ink-faint">duration</dt>
        <dd>{formatDuration(ev.duration_ms)}</dd>
        <dt className="text-ink-faint">run status</dt>
        <dd>{run.status}</dd>
      </dl>
      {Array.isArray(ev.metadata?.missing_attributes) && ev.metadata.missing_attributes.length > 0 ? (
        <p className="text-[11px] text-amber-300">Missing attributes (unknown, not fabricated): {(ev.metadata.missing_attributes as string[]).join(", ")}</p>
      ) : null}
      {mcp ? <McpInspector mcp={mcp} payload={ev.payload} ev={ev} /> : null}
      {ev.parent_span_id && !eventsHasParent(ev) ? <p className="text-[11px] text-ink-muted">Parent span not in this view (orphaned_parent). Not fabricated.</p> : null}
      <div className="flex gap-2">
        <button type="button" className="rounded border border-line px-2 py-1 text-[11px]" onClick={() => copyJson(ev)}>
          Copy JSON
        </button>
        <button type="button" className="rounded border border-line px-2 py-1 text-[11px]" onClick={() => downloadJson(`event-${ev.event_id}.json`, ev)}>
          Download JSON
        </button>
      </div>
      <JsonView value={ev.payload} />
    </div>
  );
}
