/** Convert values into JSON-serializable structures for the worker protocol. */
export function jsonable(value: unknown): unknown {
  if (value === null || value === undefined) return value ?? null;
  const t = typeof value;
  if (t === "string" || t === "number" || t === "boolean") return value;
  if (t === "bigint") return value.toString();
  if (value instanceof Date) return value.toISOString();
  if (typeof (value as { toJSON?: () => unknown }).toJSON === "function") {
    return jsonable((value as { toJSON: () => unknown }).toJSON());
  }
  if (typeof (value as { toDict?: () => unknown }).toDict === "function") {
    return jsonable((value as { toDict: () => unknown }).toDict());
  }
  if (Array.isArray(value)) return value.map(jsonable);
  if (t === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[k] = jsonable(v);
    }
    return out;
  }
  return String(value);
}

export function asRecord(value: unknown): Record<string, unknown> {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  return { value };
}

export function bytesOf(value: unknown): number {
  return Buffer.byteLength(JSON.stringify(jsonable(value) ?? null), "utf8");
}
