import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatDuration(ms: number | null | undefined): string {
  if (ms == null) return "—";
  if (ms < 1000) return `${ms} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(2)} s`;
  return `${(ms / 60_000).toFixed(1)} min`;
}

export function formatCost(value: number | null | undefined, known: boolean): string {
  if (!known || value == null) return "unknown";
  if (value === 0) return "$0.00 (estimated)";
  if (value < 0.0001) return `$${value.toExponential(2)} (est.)`;
  return `$${value.toFixed(4)} (est.)`;
}

export function formatTokens(n: number, known: boolean): string {
  if (!known) return n ? `≥ ${n.toLocaleString()} (partial)` : "unknown";
  return n.toLocaleString();
}

export function formatTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { hour12: false });
}

export function shortId(id: string, n = 8): string {
  return id.replace(/-/g, "").slice(0, n);
}

export function downloadJson(filename: string, value: unknown) {
  const blob = new Blob([JSON.stringify(value ?? null, null, 2)], { type: "application/json" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = filename;
  a.click();
  URL.revokeObjectURL(a.href);
}

export async function copyJson(value: unknown) {
  await navigator.clipboard.writeText(JSON.stringify(value ?? null, null, 2));
}
