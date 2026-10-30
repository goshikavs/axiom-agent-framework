"use client";

import {
  Background,
  Controls,
  MiniMap,
  Position,
  ReactFlow,
  ReactFlowProvider,
  addEdge,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
  type Node,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ErrorBanner } from "@/components/metric-card";
import { api } from "@/lib/api";

type WfNode = {
  id: string;
  kind: string;
  label: string;
  config: Record<string, unknown>;
  max_iterations?: number;
};
type WfEdge = { id: string; source: string; target: string; label?: string; mapping?: { from: string; to: string }[] };
type Definition = {
  name: string;
  description: string;
  version: number;
  max_steps: number;
  nodes: WfNode[];
  edges: WfEdge[];
  goal: { description: string; verification_policy: string; path?: string; equals?: unknown; required?: boolean };
  error_policy: { on_error: string };
  timeouts: { run_seconds?: number };
  retry_policy: { max_attempts: number; backoff_ms: number };
  required_permissions: string[];
  input_schema: Record<string, unknown>;
  output_schema: Record<string, unknown>;
};
type WorkflowRow = { id: string; name: string; version: number; status: string; published_version: number | null; definition: Definition; description: string };
type Issue = { code: string; message: string; node_id?: string; edge_id?: string };

const KINDS = ["start", "agent", "tool", "condition", "parallel", "join", "approval", "wait", "verify", "end"] as const;
const COLORS: Record<string, string> = {
  start: "#64748b",
  agent: "#38bdf8",
  tool: "#34d399",
  condition: "#fbbf24",
  parallel: "#a78bfa",
  join: "#c084fc",
  approval: "#fb923c",
  wait: "#94a3b8",
  verify: "#818cf8",
  end: "#4ade80",
};

function emptyDef(): Definition {
  return {
    name: "new-workflow",
    description: "",
    version: 1,
    max_steps: 20,
    nodes: [
      { id: "start", kind: "start", label: "Start", config: {} },
      { id: "end", kind: "end", label: "End", config: {} },
    ],
    edges: [{ id: "e-start-end", source: "start", target: "end" }],
    goal: { description: "", verification_policy: "json_predicate", path: "/approved", equals: true, required: true },
    error_policy: { on_error: "fail_fast" },
    timeouts: { run_seconds: 120 },
    retry_policy: { max_attempts: 1, backoff_ms: 250 },
    required_permissions: [],
    input_schema: { type: "object" },
    output_schema: { type: "object" },
  };
}

function toFlow(def: Definition): { nodes: Node[]; edges: Edge[] } {
  const nodes: Node[] = def.nodes.map((n, i) => ({
    id: n.id,
    position: { x: 80 + (i % 4) * 220, y: 80 + Math.floor(i / 4) * 140 },
    data: { label: `${n.label || n.id}\n${n.kind}`, kind: n.kind },
    style: { borderColor: COLORS[n.kind] ?? "#64748b", borderWidth: 2, background: "#121821", color: "#e7ecf3", padding: 8, borderRadius: 8, minWidth: 140, fontSize: 12 },
    sourcePosition: Position.Right,
    targetPosition: Position.Left,
  }));
  const edges = def.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    label: e.label,
    animated: false,
  }));
  return { nodes, edges };
}

export default function WorkflowsPage() {
  return (
    <ReactFlowProvider>
      <WorkflowEditor />
    </ReactFlowProvider>
  );
}

function WorkflowEditor() {
  const qc = useQueryClient();
  const list = useQuery({ queryKey: ["workflows"], queryFn: () => api<{ items: WorkflowRow[] }>("/api/v1/workflows") });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const versions = useQuery({
    queryKey: ["workflow-versions", selectedId],
    queryFn: () => api<Array<{ version: number; published_at?: string }>>(`/api/v1/workflows/${selectedId}/versions`),
    enabled: Boolean(selectedId),
  });
  const [def, setDef] = useState<Definition>(emptyDef());
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [issues, setIssues] = useState<Issue[]>([]);
  const [runId, setRunId] = useState<string | null>(null);
  const [importText, setImportText] = useState("");
  const flow = useMemo(() => toFlow(def), [def]);
  const [nodes, setNodes, onNodesChange] = useNodesState(flow.nodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(flow.edges);

  useEffect(() => {
    const f = toFlow(def);
    setNodes(f.nodes);
    setEdges(f.edges);
  }, [def, setNodes, setEdges]);

  const load = (row: WorkflowRow) => {
    setSelectedId(row.id);
    setDef({ ...emptyDef(), ...row.definition, name: row.name, description: row.description || row.definition.description });
    setRunId(null);
  };

  const onConnect = useCallback((c: Connection) => {
    setEdges((eds) => addEdge(c, eds));
    setDef((d) => ({
      ...d,
      edges: [...d.edges, { id: `e-${c.source}-${c.target}-${d.edges.length}`, source: c.source!, target: c.target! }],
    }));
  }, [setEdges]);

  const addNode = (kind: string) => {
    const id = `${kind}-${crypto.randomUUID().slice(0, 8)}`;
    const config: Record<string, unknown> =
      kind === "agent" ? { agent: "worker" } :
      kind === "tool" ? { tool: "echo", handler: "echo" } :
      kind === "wait" ? { duration_ms: 500 } :
      kind === "join" ? { join: "all" } :
      kind === "approval" ? { policy: "required" } :
      kind === "condition" ? { expression: "true" } :
      kind === "verify" ? { verification_policy: "json_predicate", path: "/approved", equals: true } : {};
    setDef((d) => ({ ...d, nodes: [...d.nodes, { id, kind, label: kind, config }] }));
  };

  const validate = useMutation({
    mutationFn: () => api<{ valid: boolean; issues: Issue[] }>("/api/v1/workflows/validate", { method: "POST", body: JSON.stringify({ definition: def, draft: true }) }),
    onSuccess: (r) => setIssues(r.issues ?? []),
  });
  const save = useMutation({
    mutationFn: async () => {
      if (selectedId) {
        return api<WorkflowRow>(`/api/v1/workflows/${selectedId}`, { method: "PUT", body: JSON.stringify({ definition: def, draft: true }) });
      }
      return api<WorkflowRow>("/api/v1/workflows", { method: "POST", body: JSON.stringify({ definition: def, draft: true }) });
    },
    onSuccess: (row) => {
      setSelectedId(row.id);
      qc.invalidateQueries({ queryKey: ["workflows"] });
    },
  });
  const publish = useMutation({
    mutationFn: async () => {
      const row = selectedId
        ? await api<WorkflowRow>(`/api/v1/workflows/${selectedId}`, { method: "PUT", body: JSON.stringify({ definition: def, draft: true }) })
        : await api<WorkflowRow>("/api/v1/workflows", { method: "POST", body: JSON.stringify({ definition: def, draft: true }) });
      setSelectedId(row.id);
      return api<WorkflowRow>(`/api/v1/workflows/${row.id}/publish`, { method: "POST", body: "{}" });
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["workflows"] }),
  });
  const run = useMutation({
    mutationFn: () => api<{ run_id: string }>(`/api/v1/workflows/${selectedId}/run`, { method: "POST", body: JSON.stringify({ input: {} }) }),
    onSuccess: (r) => setRunId(r.run_id),
  });
  const duplicate = useMutation({
    mutationFn: () => api<WorkflowRow>(`/api/v1/workflows/${selectedId}/duplicate`, { method: "POST", body: JSON.stringify({ name: `${def.name}-copy` }) }),
    onSuccess: (row) => {
      load(row);
      qc.invalidateQueries({ queryKey: ["workflows"] });
    },
  });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        save.mutate();
      }
      if (e.key === "Delete" && selectedNode) {
        setDef((d) => ({
          ...d,
          nodes: d.nodes.filter((n) => n.id !== selectedNode),
          edges: d.edges.filter((ed) => ed.source !== selectedNode && ed.target !== selectedNode),
        }));
        setSelectedNode(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [save, selectedNode]);

  const nodeCfg = def.nodes.find((n) => n.id === selectedNode);

  return (
    <div className="flex h-[calc(100vh-4rem)] flex-col gap-3">
      <header className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h1 className="text-2xl font-semibold">Workflows</h1>
          <p className="text-xs text-ink-muted">Graphs are validated and executed by the Rust runtime. Studio does not schedule work.</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <button className="rounded border border-line px-3 py-1 text-sm" onClick={() => { setSelectedId(null); setDef(emptyDef()); }} data-testid="wf-new">New</button>
          <button className="rounded border border-line px-3 py-1 text-sm" onClick={() => validate.mutate()} data-testid="wf-validate">Validate</button>
          <button className="rounded border border-line px-3 py-1 text-sm" onClick={() => save.mutate()} data-testid="wf-save">Save draft</button>
          <button className="rounded border border-line px-3 py-1 text-sm" onClick={() => publish.mutate()} data-testid="wf-publish">Publish</button>
          <button className="rounded bg-accent px-3 py-1 text-sm text-white disabled:opacity-40" disabled={!selectedId} onClick={() => run.mutate()} data-testid="wf-run">Execute</button>
          <button className="rounded border border-line px-3 py-1 text-sm disabled:opacity-40" disabled={!selectedId} onClick={() => duplicate.mutate()} data-testid="wf-duplicate">Duplicate</button>
          <a
            className="rounded border border-line px-3 py-1 text-sm"
            href={`data:application/json,${encodeURIComponent(JSON.stringify(def, null, 2))}`}
            download={`${def.name}.json`}
          >
            Export
          </a>
        </div>
      </header>
      {(save.error || publish.error || run.error || validate.error) && (
        <ErrorBanner message={((save.error || publish.error || run.error || validate.error) as Error).message} />
      )}
      {runId ? (
        <p className="text-sm">
          Run <Link className="text-sky-400" href={`/executions/${runId}`} data-testid="wf-run-link">{runId}</Link>
        </p>
      ) : null}
      <div className="grid min-h-0 flex-1 grid-cols-[180px_1fr_280px] gap-3">
        <aside className="space-y-3 overflow-auto rounded-lg border border-line bg-canvas-raised p-2 text-sm">
          <p className="text-xs uppercase text-ink-faint">Palette</p>
          {KINDS.map((k) => (
            <button key={k} className="block w-full rounded border border-line px-2 py-1 text-left" onClick={() => addNode(k)} data-testid={`palette-${k}`}>
              {k}
            </button>
          ))}
          <p className="pt-2 text-xs uppercase text-ink-faint">Saved</p>
          {(list.data?.items ?? []).map((w) => (
            <button key={w.id} className={`block w-full truncate rounded px-2 py-1 text-left ${selectedId === w.id ? "bg-accent/20" : ""}`} onClick={() => load(w)} data-testid={`wf-saved-${w.name}`}>
              {w.name} v{w.published_version ?? w.version} ({w.status})
            </button>
          ))}
          {selectedId && (versions.data?.length ?? 0) > 0 ? (
            <div data-testid="wf-versions">
              <p className="pt-2 text-xs uppercase text-ink-faint">Published versions</p>
              <ul className="space-y-1 text-xs text-ink-muted">
                {(versions.data ?? []).map((v) => (
                  <li key={v.version}>v{v.version}{v.published_at ? ` · ${v.published_at.slice(0, 19)}` : ""}</li>
                ))}
              </ul>
            </div>
          ) : null}
        </aside>
        <div className="overflow-hidden rounded-lg border border-line" data-testid="workflow-canvas">
          <ReactFlow
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onNodeClick={(_, n) => setSelectedNode(n.id)}
            fitView
            deleteKeyCode={["Backspace", "Delete"]}
          >
            <Background />
            <Controls />
            <MiniMap />
          </ReactFlow>
        </div>
        <aside className="space-y-3 overflow-auto rounded-lg border border-line bg-canvas-raised p-3 text-sm">
          <label className="block text-xs text-ink-faint">Name
            <input className="mt-1 w-full rounded border border-line bg-canvas px-2 py-1" value={def.name} onChange={(e) => setDef({ ...def, name: e.target.value })} data-testid="wf-name" />
          </label>
          <label className="block text-xs text-ink-faint">Description
            <textarea className="mt-1 w-full rounded border border-line bg-canvas px-2 py-1" value={def.description} onChange={(e) => setDef({ ...def, description: e.target.value })} />
          </label>
          {nodeCfg ? (
            <div className="space-y-2" data-testid="wf-config">
              <p className="font-medium">{nodeCfg.kind} `{nodeCfg.id}`</p>
              <label className="block text-xs">Label
                <input className="mt-1 w-full rounded border border-line bg-canvas px-2 py-1" value={nodeCfg.label} onChange={(e) => setDef({ ...def, nodes: def.nodes.map((n) => n.id === nodeCfg.id ? { ...n, label: e.target.value } : n) })} />
              </label>
              {nodeCfg.kind === "agent" ? (
                <label className="block text-xs">Agent
                  <input className="mt-1 w-full rounded border border-line bg-canvas px-2 py-1" value={String(nodeCfg.config.agent ?? "")} onChange={(e) => setDef({ ...def, nodes: def.nodes.map((n) => n.id === nodeCfg.id ? { ...n, config: { ...n.config, agent: e.target.value } } : n) })} />
                </label>
              ) : null}
              {nodeCfg.kind === "tool" ? (
                <label className="block text-xs">Tool
                  <input className="mt-1 w-full rounded border border-line bg-canvas px-2 py-1" value={String(nodeCfg.config.tool ?? "")} onChange={(e) => setDef({ ...def, nodes: def.nodes.map((n) => n.id === nodeCfg.id ? { ...n, config: { ...n.config, tool: e.target.value, handler: "echo" } } : n) })} />
                </label>
              ) : null}
              {nodeCfg.kind === "condition" ? (
                <label className="block text-xs">Expression
                  <input className="mt-1 w-full rounded border border-line bg-canvas px-2 py-1" value={String(nodeCfg.config.expression ?? "")} onChange={(e) => setDef({ ...def, nodes: def.nodes.map((n) => n.id === nodeCfg.id ? { ...n, config: { ...n.config, expression: e.target.value } } : n) })} />
                </label>
              ) : null}
            </div>
          ) : <p className="text-ink-muted">Select a node to configure it. Ctrl+S saves a draft.</p>}
          {issues.length ? (
            <ul className="space-y-1 text-xs text-amber-200" data-testid="wf-issues">
              {issues.map((i, idx) => <li key={idx}>{i.code}: {i.message}</li>)}
            </ul>
          ) : <p className="text-xs text-ink-faint">No validation issues (run Validate).</p>}
          <label className="block text-xs text-ink-faint">Import JSON
            <textarea className="mt-1 h-24 w-full rounded border border-line bg-canvas p-1 font-mono text-[10px]" value={importText} onChange={(e) => setImportText(e.target.value)} data-testid="wf-import" />
          </label>
          <button
            className="rounded border border-line px-2 py-1 text-xs"
            onClick={() => {
              try {
                const parsed = JSON.parse(importText) as Definition;
                setDef({ ...emptyDef(), ...parsed });
                setSelectedId(null);
              } catch {
                setIssues([{ code: "invalid_json", message: "import is not valid JSON" }]);
              }
            }}
          >
            Import
          </button>
        </aside>
      </div>
    </div>
  );
}
