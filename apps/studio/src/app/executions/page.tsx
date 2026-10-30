"use client";

import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Suspense, useMemo } from "react";
import { EmptyState, ErrorBanner } from "@/components/metric-card";
import { StatusBadge } from "@/components/status-badge";
import { listRuns } from "@/lib/api";
import { sdkOrigin } from "@/lib/sdk-origin";
import { formatCost, formatDuration, formatTime, shortId } from "@/lib/utils";

const PAGE = 50;
const KEYS = ["search", "status", "verification_status", "framework", "agent", "project", "model", "from", "to", "sort", "order", "offset"] as const;

function Explorer() {
  const sp = useSearchParams();
  const router = useRouter();
  const pathname = usePathname();
  const f = useMemo(() => {
    const o: Record<string, string> = {};
    for (const k of KEYS) o[k] = sp.get(k) ?? "";
    return o;
  }, [sp]);

  function set(patch: Record<string, string | number | undefined>) {
    const n = new URLSearchParams(sp.toString());
    for (const [k, v] of Object.entries(patch)) {
      if (v == null || v === "") n.delete(k);
      else n.set(k, String(v));
    }
    if (!("offset" in patch)) n.delete("offset");
    router.replace(`${pathname}?${n.toString()}`);
  }

  const offset = Number(f.offset || 0);
  const q = useQuery({
    queryKey: ["runs", f],
    queryFn: () =>
      listRuns({
        search: f.search || undefined,
        status: f.status || undefined,
        verification_status: f.verification_status || undefined,
        agent: f.agent || undefined,
        project: f.project || undefined,
        framework: f.framework || undefined,
        model: f.model || undefined,
        from: f.from || undefined,
        to: f.to || undefined,
        sort: f.sort || undefined,
        order: f.order || undefined,
        limit: PAGE,
        offset,
      }),
  });

  const total = q.data?.total ?? 0;
  const rows = q.data?.items ?? [];
  const sel = "rounded border border-line bg-canvas px-2 py-1 text-xs";

  function sortBy(col: string) {
    const nextOrder = f.sort === col && f.order !== "asc" ? "asc" : "desc";
    set({ sort: col, order: nextOrder });
  }

  return (
    <div className="space-y-4">
      <header>
        <h1 className="text-2xl font-semibold tracking-tight">Executions</h1>
        <p className="mt-1 text-sm text-ink-muted">Persisted runs only. Filters are stored in the URL so a view can be shared.</p>
      </header>
      <div className="flex flex-wrap gap-2">
        <input className={`${sel} w-56`} placeholder="Search goal, agent, or id" value={f.search} onChange={(e) => set({ search: e.target.value })} data-testid="filter-search" />
        {(["status", "verification_status", "framework", "agent", "project", "model"] as const).map((k) => (
          <input key={k} className={`${sel} w-36`} placeholder={k.replaceAll("_", " ")} value={f[k]} onChange={(e) => set({ [k]: e.target.value })} data-testid={`filter-${k}`} />
        ))}
        <label className="flex items-center gap-1 text-[11px] text-ink-muted">
          from
          <input className={sel} type="datetime-local" value={f.from} onChange={(e) => set({ from: e.target.value ? new Date(e.target.value).toISOString() : "" })} />
        </label>
        <label className="flex items-center gap-1 text-[11px] text-ink-muted">
          to
          <input className={sel} type="datetime-local" value={f.to} onChange={(e) => set({ to: e.target.value ? new Date(e.target.value).toISOString() : "" })} />
        </label>
      </div>
      {q.isError ? <ErrorBanner message={(q.error as Error).message} /> : null}
      {q.isPending ? (
        <div className="h-40 animate-pulse rounded-lg border border-line bg-canvas-raised" data-testid="executions-loading" />
      ) : null}
      {q.isSuccess && rows.length === 0 ? <EmptyState title="No runs" body="Zero matching persisted records." /> : null}
      {rows.length > 0 ? (
        <div className="overflow-x-auto rounded-lg border border-line">
          <table className="w-full min-w-[1280px] text-left text-sm" data-testid="executions-table">
            <thead className="bg-canvas-raised text-[11px] uppercase tracking-wider text-ink-faint">
              <tr>
                {[
                  ["id", "Execution ID"],
                  ["agent_name", "Agent"],
                  ["project_id", "Project"],
                  ["source_framework", "Framework"],
                  ["model", "Model"],
                  ["goal", "Goal"],
                  ["status", "Runtime"],
                  ["verification_status", "Verification"],
                  ["started_at", "Start"],
                  ["duration_ms", "Duration"],
                  ["tokens", "Tokens"],
                  ["cost", "Est. cost"],
                ].map(([col, label]) => (
                  <th key={col} className="px-3 py-2 font-medium">
                    <button type="button" className="hover:text-ink" onClick={() => sortBy(col)} title="Sort">
                      {label}
                      {f.sort === col ? (f.order === "asc" ? " ↑" : " ↓") : ""}
                    </button>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.id} className="border-t border-line hover:bg-canvas-raised" data-testid="execution-row">
                  <td className="px-3 py-2 font-mono text-xs">
                    <Link className="text-sky-400 hover:underline" href={`/executions/${r.id}`} data-testid="execution-link">
                      {shortId(r.id, 12)}
                    </Link>
                  </td>
                  <td className="px-3 py-2">{r.agent_name}</td>
                  <td className="px-3 py-2 font-mono text-[11px] text-ink-muted" title={r.project_id}>
                    {shortId(r.project_id, 8)}
                  </td>
                  <td className="px-3 py-2 font-mono text-xs" title={r.source_framework}>
                    {sdkOrigin(r.source_framework).label}
                  </td>
                  <td className="px-3 py-2 font-mono text-xs">{r.model}</td>
                  <td className="max-w-[240px] truncate px-3 py-2 text-ink-muted">{r.goal_description || "—"}</td>
                  <td className="px-3 py-2">
                    <StatusBadge value={r.status} />
                  </td>
                  <td className="px-3 py-2">
                    <StatusBadge value={r.verification_status} kind="verify" />
                  </td>
                  <td className="px-3 py-2 font-mono text-xs text-ink-muted">{formatTime(r.started_at ?? r.created_at)}</td>
                  <td className="px-3 py-2 font-mono text-xs">{formatDuration(r.duration_ms)}</td>
                  <td className="px-3 py-2 font-mono text-xs">{r.usage_known ? (r.input_tokens + r.output_tokens).toLocaleString() : "unknown"}</td>
                  <td className="px-3 py-2 font-mono text-xs">{formatCost(r.estimated_cost_usd, r.cost_known)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}
      <div className="flex items-center justify-between text-xs text-ink-muted">
        <span>
          {rows.length ? `${offset + 1}–${offset + rows.length} of ${total}` : ""}
        </span>
        <div className="flex gap-2">
          <button className="rounded border border-line px-2 py-1 disabled:opacity-40" disabled={offset === 0} onClick={() => set({ offset: Math.max(0, offset - PAGE) })}>
            Prev
          </button>
          <button className="rounded border border-line px-2 py-1 disabled:opacity-40" disabled={offset + PAGE >= total} onClick={() => set({ offset: offset + PAGE })}>
            Next
          </button>
        </div>
      </div>
    </div>
  );
}

export default function ExecutionsPage() {
  return (
    <Suspense fallback={<p className="text-sm text-ink-muted">Loading executions…</p>}>
      <Explorer />
    </Suspense>
  );
}
