import type { ExecutionEvent } from "./types";

export type GraphNode = {
  id: string;
  type: string;
  label: string;
  status?: string | null;
  eventType: string;
  sequence: number;
  parent?: string | null;
  failed: boolean;
};

export type GraphEdge = {
  id: string;
  source: string;
  target: string;
  kind: "parent" | "sequence";
};

const KIND: Record<string, string> = {
  "run.created": "start",
  "run.started": "agent",
  "run.completed": "completion",
  "run.failed": "error",
  "run.cancelled": "error",
  "goal.created": "goal",
  "goal.evaluated": "decision",
  "goal.satisfied": "goal",
  "goal.failed": "error",
  "model.requested": "model",
  "model.completed": "model",
  "model.failed": "error",
  "tool.requested": "tool",
  "tool.started": "tool",
  "tool.completed": "tool",
  "tool.failed": "error",
  "tool.timed_out": "error",
  "verification.started": "verification",
  "verification.passed": "verification",
  "verification.failed": "error",
  "verification.inconclusive": "verification",
  "state.snapshot": "state",
  "state.changed": "state",
  "approval.requested": "approval",
  "approval.granted": "approval",
  "approval.denied": "approval",
  "approval.invalidated": "approval",
  "evidence.recorded": "state",
  "evidence.changed": "state",
  "decision.created": "decision",
  "decision.revalidation_required": "decision",
  "decision.revalidated": "decision",
  "impact.computed": "verification",
  "recovery.started": "retry",
  "recovery.completed": "retry",
  "evidence.coverage_evaluated": "verification",
  "action.gated": "error",
  "agent.handoff_completed": "agent",
  "agent.delegated": "agent",
  "agent.message_sent": "agent",
  "agent.message_received": "agent",
};

export function eventsToGraph(events: ExecutionEvent[]): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];
  const bySpan = new Map<string, GraphNode>();
  let failedPath = false;
  for (const e of events) {
    const kind = KIND[e.event_type] ?? "decision";
    const label = labelFor(e);
    const node: GraphNode = {
      id: e.span_id || e.event_id,
      type: kind,
      label,
      status: e.status,
      eventType: e.event_type,
      sequence: e.sequence,
      parent: e.parent_span_id,
      failed: ["failed", "error"].includes((e.status || "").toLowerCase()) || e.event_type.endsWith(".failed") || e.event_type.endsWith(".timed_out"),
    };
    if (node.failed) failedPath = true;
    nodes.push(node);
    bySpan.set(node.id, node);
    if (e.parent_span_id) {
      edges.push({ id: `${e.parent_span_id}->${node.id}`, source: e.parent_span_id, target: node.id, kind: "parent" });
    }
  }
  if (failedPath) {
    for (const n of nodes) {
      if (n.failed) {
        let cur: GraphNode | undefined = n;
        while (cur?.parent) {
          const p = bySpan.get(cur.parent);
          if (!p) break;
          p.failed = true;
          cur = p;
        }
      }
    }
  }
  return { nodes, edges };
}

function labelFor(e: ExecutionEvent): string {
  const p = e.payload || {};
  if (typeof p.tool === "string") return `${e.event_type}\n${p.tool}`;
  if (typeof p.model === "string") return `${e.event_type}\n${p.model}`;
  if (typeof p.predicate === "string") return `${e.event_type}\n${p.predicate}`;
  return e.event_type;
}
