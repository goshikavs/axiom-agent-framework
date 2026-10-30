export type SnapshotContext = {
  runId: string;
  reason: string;
  step: number | null;
};

export class StateSnapshot {
  state: Record<string, unknown>;
  version: string | null;
  source: string;
  capturedAt: string;
  constructor(init: { state: Record<string, unknown>; version?: string | null; source?: string; capturedAt?: string }) {
    this.state = init.state;
    this.version = init.version ?? null;
    this.source = init.source ?? "";
    this.capturedAt = init.capturedAt ?? new Date().toISOString();
  }

  get(path: string, fallback: unknown = undefined): unknown {
    let cur: unknown = this.state;
    for (const part of path.split(".")) {
      if (cur && typeof cur === "object" && !Array.isArray(cur) && part in (cur as Record<string, unknown>)) {
        cur = (cur as Record<string, unknown>)[part];
      } else {
        return fallback;
      }
    }
    return cur;
  }

  toDict(): Record<string, unknown> {
    return { state: this.state, version: this.version, source: this.source, captured_at: this.capturedAt };
  }
}

export interface StateProvider {
  snapshot(context: SnapshotContext): StateSnapshot | Promise<StateSnapshot>;
  version?(resourceId: string): string | null | Promise<string | null>;
}
