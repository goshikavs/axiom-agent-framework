"use client";

import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { Bar, BarChart, CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { EmptyState, ErrorBanner, MetricCard } from "@/components/metric-card";
import { api, qs } from "@/lib/api";
import type { OverviewResponse } from "@/lib/types";
import { formatCost, formatDuration, formatTokens } from "@/lib/utils";

export default function OverviewPage() {
  const [filters, setFilters] = useState({ agent: "", framework: "", model: "", status: "", range: "24h" });
  const { from, to } = useMemo(() => rangeToIso(filters.range), [filters.range]);
  const q = useQuery({
    queryKey: ["overview", filters],
    queryFn: () =>
      api<OverviewResponse>(
        `/api/v1/metrics/overview${qs({ agent: filters.agent, framework: filters.framework, model: filters.model, status: filters.status, from, to, bucket: filters.range === "7d" ? "day" : "hour" })}`,
      ),
  });

  if (q.isError) return <ErrorBanner message={(q.error as Error).message} />;
  if (q.isLoading || !q.data) return <p className="text-sm text-ink-muted">Loading metrics from the runtime…</p>;

  const t = q.data.totals;
  const empty = t.total_runs === 0;
  const usageUnknown = t.usage_known_runs === 0;
  const costUnknown = t.cost_known_runs === 0;
  const costPerVerified =
    t.verified_runs_with_cost > 0 && t.verified_cost_usd != null ? t.verified_cost_usd / t.verified_runs_with_cost : null;

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Overview</h1>
          <p className="mt-1 text-sm text-ink-muted">Computed from persisted runs. Zero means no matching records; unknown means the data was not reported.</p>
        </div>
        <Filters data={q.data} filters={filters} setFilters={setFilters} />
      </header>

      {empty ? (
        <EmptyState title="No executions in this range" body="Run examples/hello-agent with the runtime listening on :7400. This dashboard does not invent sample metrics." />
      ) : null}

      <section className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
        <MetricCard label="Total executions" value={String(t.total_runs)} />
        <MetricCard label="Successful (COMPLETED)" value={String(t.completed_runs)} hint={`${t.verified_runs} verified`} />
        <MetricCard label="Failed" value={String(t.failed_runs)} />
        <MetricCard label="Unverified outcomes" value={String(t.unverified_runs)} hint="runtime UNVERIFIED, not a success" />
        <MetricCard label="Inconclusive verification" value={String(t.inconclusive_runs ?? 0)} hint="unknown, not zero cost" />
        <MetricCard
          label="Verified outcome rate"
          value={t.terminal_runs ? `${((100 * t.verified_runs) / t.terminal_runs).toFixed(1)}%` : "—"}
          hint={`${t.verified_runs} / ${t.terminal_runs} terminal runs`}
        />
        <MetricCard label="Pending approvals" value={String(q.data.pending_approvals)} />
        <MetricCard label="Avg duration" value={formatDuration(t.avg_duration_ms)} unknown={t.avg_duration_ms == null} />
        <MetricCard label="p50 duration" value={formatDuration(t.p50_duration_ms)} unknown={t.p50_duration_ms == null} />
        <MetricCard label="p95 duration" value={formatDuration(t.p95_duration_ms)} unknown={t.p95_duration_ms == null} />
        <MetricCard
          label="Verification overhead"
          value={t.verification_results_with_duration ? formatDuration(t.verification_overhead_ms) : "unknown"}
          hint={t.verification_results_with_duration ? `${t.verification_results_with_duration} timed verification rows` : "no timed verification_results; not treated as zero"}
          unknown={!t.verification_results_with_duration}
        />
        <MetricCard
          label="Token usage"
          value={formatTokens(t.total_input_tokens + t.total_output_tokens, !usageUnknown)}
          hint={`${t.usage_known_runs} runs with full usage`}
          unknown={usageUnknown}
        />
        <MetricCard label="Estimated LLM cost" value={formatCost(t.total_estimated_cost_usd, !costUnknown)} hint={`pricing ${q.data.pricing_version}`} unknown={costUnknown} />
        <MetricCard
          label="Cost / verified success"
          value={formatCost(costPerVerified, costPerVerified != null)}
          hint={costPerVerified == null ? "needs verified runs with priced usage" : `${t.verified_runs_with_cost} priced verified runs`}
          unknown={costPerVerified == null}
        />
      </section>

      <section className="grid gap-4 lg:grid-cols-2">
        <ChartCard title="Executions over time">
          <ResponsiveContainer width="100%" height={220}>
            <BarChart data={q.data.series}>
              <CartesianGrid stroke="#243041" vertical={false} />
              <XAxis dataKey="bucket" hide />
              <YAxis stroke="#66758c" fontSize={11} allowDecimals={false} />
              <Tooltip contentStyle={tooltipStyle} />
              <Bar dataKey="total" fill="#3b82f6" name="executions" />
            </BarChart>
          </ResponsiveContainer>
        </ChartCard>
        <ChartCard title="Success / failure">
          <ResponsiveContainer width="100%" height={220}>
            <LineChart data={q.data.series}>
              <CartesianGrid stroke="#243041" vertical={false} />
              <XAxis dataKey="bucket" hide />
              <YAxis stroke="#66758c" fontSize={11} allowDecimals={false} />
              <Tooltip contentStyle={tooltipStyle} />
              <Line type="monotone" dataKey="completed" stroke="#34d399" dot={false} name="completed" />
              <Line type="monotone" dataKey="failed" stroke="#f87171" dot={false} name="failed" />
              <Line type="monotone" dataKey="verified" stroke="#60a5fa" dot={false} name="verified" />
            </LineChart>
          </ResponsiveContainer>
        </ChartCard>
        <ChartCard title="Model usage distribution">
          {q.data.models.length === 0 ? (
            <p className="p-6 text-sm text-ink-muted">No model calls in range.</p>
          ) : (
            <ul className="space-y-2 p-4 text-sm">
              {q.data.models.map((m) => (
                <li key={m.model} className="flex justify-between gap-4 font-mono text-xs">
                  <span>{m.model}</span>
                  <span className="text-ink-muted">
                    {m.calls} calls
                    {m.estimated_cost_usd == null ? " · cost unknown" : ` · $${m.estimated_cost_usd.toFixed(4)} est.`}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </ChartCard>
        <ChartCard title="Runs by framework">
          {(q.data.by_framework ?? []).length === 0 ? (
            <p className="p-6 text-sm text-ink-muted">No framework rows in range.</p>
          ) : (
            <ul className="space-y-2 p-4 text-sm">
              {(q.data.by_framework ?? []).map((f) => (
                <li key={f.key} className="flex justify-between gap-4 font-mono text-xs">
                  <span>{f.key}</span>
                  <span className="text-ink-muted">
                    {f.runs} runs · {f.verified} verified · {f.failed} failed
                    {f.cost_known_runs ? "" : " · cost unknown"}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </ChartCard>
        <ChartCard title="Verification outcomes">
          <ul className="space-y-2 p-4 text-sm">
            {q.data.verification.length === 0 ? <li className="text-ink-muted">No terminal runs.</li> : null}
            {q.data.verification.map((v) => (
              <li key={v.verification_status} className="flex justify-between font-mono text-xs">
                <span>{v.verification_status}</span>
                <span>{v.runs}</span>
              </li>
            ))}
          </ul>
        </ChartCard>
        <ChartCard title="Cost trend (estimated)">
          <ResponsiveContainer width="100%" height={220}>
            <LineChart data={q.data.series}>
              <CartesianGrid stroke="#243041" vertical={false} />
              <XAxis dataKey="bucket" hide />
              <YAxis stroke="#66758c" fontSize={11} />
              <Tooltip contentStyle={tooltipStyle} />
              <Line type="monotone" dataKey="estimated_cost_usd" stroke="#a78bfa" dot={false} name="est. USD" />
            </LineChart>
          </ResponsiveContainer>
        </ChartCard>
        <ChartCard title="Latency percentiles">
          <ResponsiveContainer width="100%" height={220}>
            <LineChart data={q.data.series}>
              <CartesianGrid stroke="#243041" vertical={false} />
              <XAxis dataKey="bucket" hide />
              <YAxis stroke="#66758c" fontSize={11} />
              <Tooltip contentStyle={tooltipStyle} />
              <Line type="monotone" dataKey="p50_duration_ms" stroke="#94a3b8" dot={false} name="p50 ms" />
              <Line type="monotone" dataKey="p95_duration_ms" stroke="#3b82f6" dot={false} name="p95 ms" />
              <Line type="monotone" dataKey="p99_duration_ms" stroke="#f59e0b" dot={false} name="p99 ms" />
            </LineChart>
          </ResponsiveContainer>
        </ChartCard>
      </section>
    </div>
  );
}

const tooltipStyle = { background: "#121821", border: "1px solid #243041", fontSize: 12 };

function ChartCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="rounded-lg border border-line bg-canvas-raised">
      <h2 className="border-b border-line px-4 py-2 text-sm font-medium">{title}</h2>
      {children}
    </div>
  );
}

function Filters({
  data,
  filters,
  setFilters,
}: {
  data: OverviewResponse;
  filters: { agent: string; framework: string; model: string; status: string; range: string };
  setFilters: (f: typeof filters) => void;
}) {
  const sel = "rounded border border-line bg-canvas px-2 py-1 text-xs";
  return (
    <div className="flex flex-wrap gap-2">
      <select className={sel} value={filters.range} onChange={(e) => setFilters({ ...filters, range: e.target.value })}>
        <option value="1h">Last 1h</option>
        <option value="24h">Last 24h</option>
        <option value="7d">Last 7d</option>
        <option value="30d">Last 30d</option>
      </select>
      <select className={sel} value={filters.agent} onChange={(e) => setFilters({ ...filters, agent: e.target.value })}>
        <option value="">All agents</option>
        {data.filters.agents.map((a) => (
          <option key={a}>{a}</option>
        ))}
      </select>
      <select className={sel} value={filters.framework} onChange={(e) => setFilters({ ...filters, framework: e.target.value })}>
        <option value="">All frameworks</option>
        {data.filters.frameworks.map((a) => (
          <option key={a}>{a}</option>
        ))}
      </select>
      <select className={sel} value={filters.model} onChange={(e) => setFilters({ ...filters, model: e.target.value })}>
        <option value="">All models</option>
        {data.filters.models.map((a) => (
          <option key={a}>{a}</option>
        ))}
      </select>
      <select className={sel} value={filters.status} onChange={(e) => setFilters({ ...filters, status: e.target.value })}>
        <option value="">All statuses</option>
        {["COMPLETED", "UNVERIFIED", "FAILED", "CANCELLED", "RECOVERY_REQUIRED"].map((s) => (
          <option key={s}>{s}</option>
        ))}
      </select>
    </div>
  );
}

function rangeToIso(range: string): { from: string; to: string } {
  const to = new Date();
  const from = new Date(to);
  if (range === "1h") from.setHours(from.getHours() - 1);
  else if (range === "7d") from.setDate(from.getDate() - 7);
  else if (range === "30d") from.setDate(from.getDate() - 30);
  else from.setDate(from.getDate() - 1);
  return { from: from.toISOString(), to: to.toISOString() };
}
