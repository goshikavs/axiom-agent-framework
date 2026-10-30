"use client";

import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { ErrorBanner } from "@/components/metric-card";
import { api } from "@/lib/api";
import type { ExecutionEvent, Paginated } from "@/lib/types";
import { formatTime } from "@/lib/utils";

export default function EventsPage() {
  const q = useQuery({ queryKey: ["events"], queryFn: () => api<Paginated<ExecutionEvent>>("/api/v1/events?limit=100"), refetchInterval: 4000 });
  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  const items = q.data?.items ?? [];
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Events</h1>
      <p className="text-sm text-ink-muted">Recent ingested events across the project, ordered by source timestamp.</p>
      <div className="overflow-x-auto rounded-lg border border-line">
        <table className="w-full text-left text-sm">
          <thead className="bg-canvas-raised text-[11px] uppercase text-ink-faint">
            <tr>
              {["Time", "Type", "Run", "Seq", "Status"].map((h) => (
                <th key={h} className="px-3 py-2">
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {items.map((e) => (
              <tr key={e.event_id} className="border-t border-line">
                <td className="px-3 py-1.5 font-mono text-xs">{formatTime(e.timestamp)}</td>
                <td className="px-3 py-1.5 font-mono text-xs">{e.event_type}</td>
                <td className="px-3 py-1.5 font-mono text-xs">
                  <Link className="text-sky-400" href={`/executions/${e.run_id}`}>
                    {e.run_id.slice(0, 8)}
                  </Link>
                </td>
                <td className="px-3 py-1.5 font-mono text-xs">{e.sequence}</td>
                <td className="px-3 py-1.5 text-xs">{e.status ?? "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
