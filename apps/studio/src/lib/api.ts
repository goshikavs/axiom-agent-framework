import type { Paginated } from "./types";

const PREFIX = "/api/runtime";

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${PREFIX}${path}`, {
    ...init,
    headers: {
      accept: "application/json",
      ...(init?.body ? { "content-type": "application/json" } : {}),
      ...init?.headers,
    },
    cache: "no-store",
  });
  if (!res.ok) {
    let code = "error";
    let message = res.statusText;
    try {
      const body = await res.json();
      code = body?.error?.code ?? code;
      message = body?.error?.message ?? message;
    } catch {
      /* ignore */
    }
    throw new ApiError(res.status, code, message);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export function qs(params: Record<string, string | number | undefined | null>): string {
  const u = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v != null && v !== "") u.set(k, String(v));
  }
  const s = u.toString();
  return s ? `?${s}` : "";
}

export async function listRuns(params: Record<string, string | number | undefined | null>) {
  return api<Paginated<import("./types").RunSummary>>(`/api/v1/runs${qs(params)}`);
}

export async function listRunEvents(runId: string, after?: number) {
  return api<Paginated<import("./types").ExecutionEvent>>(`/api/v1/runs/${runId}/events${qs({ after, limit: 2000 })}`);
}
