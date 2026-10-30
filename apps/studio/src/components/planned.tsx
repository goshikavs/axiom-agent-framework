export function Planned({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="rounded-lg border border-dashed border-line bg-canvas-raised p-8">
      <p className="text-xs uppercase tracking-wider text-ink-faint">Planned</p>
      <h1 className="mt-2 text-xl font-semibold">{title}</h1>
      <p className="mt-3 max-w-2xl text-sm leading-6 text-ink-muted">{detail}</p>
      <p className="mt-4 text-xs text-ink-faint">This control is unavailable. It is not wired to a mock or placeholder API.</p>
    </div>
  );
}

export function ExperimentalNote({ children }: { children: React.ReactNode }) {
  return (
    <p className="rounded border border-amber-900/60 bg-amber-950/40 px-3 py-2 text-xs text-amber-200">{children}</p>
  );
}
