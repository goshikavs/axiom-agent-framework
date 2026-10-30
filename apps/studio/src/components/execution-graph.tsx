"use client";

import { Background, Controls, MiniMap, ReactFlow, type Node, type Edge } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useMemo } from "react";
import type { GraphEdge, GraphNode } from "@/lib/graph";

const COLORS: Record<string, string> = {
  start: "#64748b",
  goal: "#8b5cf6",
  agent: "#38bdf8",
  model: "#60a5fa",
  tool: "#34d399",
  verification: "#a78bfa",
  state: "#fbbf24",
  approval: "#fb923c",
  decision: "#94a3b8",
  retry: "#f472b6",
  error: "#f87171",
  completion: "#4ade80",
};

export function ExecutionGraph({
  nodes,
  edges,
  selected,
  onSelect,
}: {
  nodes: GraphNode[];
  edges: GraphEdge[];
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const sourceNodes = useMemo(() => (nodes.length > 250 ? nodes.slice(-250) : nodes), [nodes]);
  const rfNodes: Node[] = useMemo(() => {
    const cols = new Map<string, number>();
    return sourceNodes.map((n, i) => {
      const col = n.parent ? (cols.get(n.parent) ?? 0) + 1 : 0;
      cols.set(n.id, col);
      const color = n.failed ? COLORS.error : COLORS[n.type] ?? COLORS.decision;
      return {
        id: n.id,
        position: { x: 40 + col * 220, y: 40 + i * 28 },
        data: { label: n.label.replace("\n", " · ") },
        style: {
          border: selected === n.id ? `2px solid ${color}` : `1px solid ${color}66`,
          background: "#121821",
          color: "#e7ecf3",
          fontSize: 11,
          width: 200,
          padding: 8,
          borderRadius: 6,
          boxShadow: n.failed ? "0 0 0 1px #7f1d1d" : undefined,
        },
      };
    });
  }, [sourceNodes, selected]);

  const rfEdges: Edge[] = useMemo(() => {
    const known = new Set(sourceNodes.map((n) => n.id));
    return edges
      .filter((e) => known.has(e.source) && known.has(e.target))
      .map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
        style: { stroke: e.kind === "parent" ? "#3b82f6" : "#475569" },
        animated: false,
      }));
  }, [edges, sourceNodes]);

  if (nodes.length === 0) {
    return <p className="p-4 text-sm text-ink-muted">No events yet. Edges are parent_span_id relationships, not mere time adjacency.</p>;
  }

  return (
    <div className="h-full min-h-[320px] bg-canvas">
      {nodes.length > 250 ? <p className="px-2 py-1 text-[11px] text-ink-muted">Showing last 250 of {nodes.length} nodes to keep the browser responsive.</p> : null}
      <ReactFlow
        nodes={rfNodes}
        edges={rfEdges}
        fitView
        onNodeClick={(_, n) => onSelect(n.id)}
        proOptions={{ hideAttribution: true }}
      >
        <Background color="#243041" gap={16} />
        <Controls />
        <MiniMap pannable zoomable style={{ background: "#0b1017" }} />
      </ReactFlow>
    </div>
  );
}
