export function MetricCard({
  label,
  value,
  hint,
  unknown,
}: {
  label: string;
  value: string;
  hint?: string;
  unknown?: boolean;
}) {
  return (
    <div className="rounded-lg border border-line bg-canvas-raised p-4">
      <p className="text-[11px] uppercase tracking-wider text-ink-faint">{label}</p>
      <p className={`mt-2 font-mono text-2xl tabular-nums ${unknown ? "text-ink-faint" : "text-ink"}`}>{value}</p>
      {hint ? <p className="mt-1 text-xs text-ink-muted">{hint}</p> : null}
    </div>
  );
}

export function EmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="rounded-lg border border-line bg-canvas-raised px-6 py-12 text-center">
      <p className="text-sm font-medium">{title}</p>
      <p className="mt-2 text-sm text-ink-muted">{body}</p>
    </div>
  );
}

export function ErrorBanner({ message }: { message: string }) {
  return (
    <div className="rounded-lg border border-red-900 bg-red-950/40 px-4 py-3 text-sm text-red-200">
      Runtime unreachable or returned an error: {message}
    </div>
  );
}
