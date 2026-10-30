"use client";

import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { EmptyState, ErrorBanner } from "@/components/metric-card";
import { api } from "@/lib/api";
import type { Paginated } from "@/lib/types";
import { formatTime } from "@/lib/utils";

type AgentRow = { id: string; name: string; description: string; source_framework: string; created_at: string; updated_at: string };

export default function AgentsPage() {
  const q = useQuery({ queryKey: ["agents"], queryFn: () => api<Paginated<AgentRow>>("/api/v1/agents") });
  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  const items = q.data?.items ?? [];
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Agents</h1>
      <p className="text-sm text-ink-muted">Versioned definitions recorded when a run is submitted. The visual builder is experimental; Python/TS SDKs remain the source of truth.</p>
      {items.length === 0 ? <EmptyState title="No agents saved" body="Submit a run from the SDK to create the first agent version." /> : null}
      <ul className="divide-y divide-line rounded-lg border border-line">
        {items.map((a) => (
          <li key={a.id} className="flex items-center justify-between px-4 py-3">
            <div>
              <Link href={`/agents/${a.id}`} className="font-medium hover:text-sky-400">
                {a.name}
              </Link>
              <p className="text-xs text-ink-muted">
                {a.source_framework} · updated {formatTime(a.updated_at)}
              </p>
            </div>
            <span className="font-mono text-[11px] text-ink-faint">{a.id.slice(0, 8)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
