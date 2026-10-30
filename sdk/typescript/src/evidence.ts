export type Trust = "authoritative" | "derived" | "untrusted" | "inaccessible";

export type EvidenceReference = {
  source_system: string;
  resource_id: string;
  resource_version?: string | null;
  etag?: string | null;
  fingerprint?: string | null;
  trust?: Trust;
  access_scope?: string | null;
  valid_from?: string | null;
  valid_until?: string | null;
  id?: string | null;
  keys?: string[];
  observed_at?: string | null;
};

export class Evidence implements EvidenceReference {
  source_system: string;
  resource_id: string;
  resource_version?: string | null;
  etag?: string | null;
  fingerprint?: string | null;
  trust: Trust;
  access_scope?: string | null;
  valid_from?: string | null;
  valid_until?: string | null;
  id?: string | null;
  keys: string[];
  observed_at?: string | null;

  constructor(init: EvidenceReference) {
    this.source_system = init.source_system;
    this.resource_id = init.resource_id;
    this.resource_version = init.resource_version;
    this.etag = init.etag;
    this.fingerprint = init.fingerprint;
    this.trust = init.trust ?? "authoritative";
    this.access_scope = init.access_scope;
    this.valid_from = init.valid_from;
    this.valid_until = init.valid_until;
    this.id = init.id;
    this.keys = init.keys ? [...init.keys] : [];
    this.observed_at = init.observed_at;
  }

  toDict(): Record<string, unknown> {
    const d: Record<string, unknown> = {
      source_system: this.source_system,
      resource_id: this.resource_id,
      trust: this.trust,
    };
    if (this.resource_version != null) d.resource_version = this.resource_version;
    if (this.etag != null) d.etag = this.etag;
    if (this.fingerprint != null) d.fingerprint = this.fingerprint;
    if (this.access_scope != null) d.access_scope = this.access_scope;
    if (this.valid_from != null) d.valid_from = this.valid_from;
    if (this.valid_until != null) d.valid_until = this.valid_until;
    if (this.id != null) d.id = this.id;
    if (this.keys.length) d.keys = [...this.keys];
    if (this.observed_at != null) d.observed_at = this.observed_at;
    return d;
  }
}

export class DecisionRef {
  id: string;
  kind: string;
  depends_on: string[];
  payload: Record<string, unknown>;
  action?: string;
  constructor(init: { id: string; kind?: string; depends_on?: string[]; payload?: Record<string, unknown>; action?: string }) {
    this.id = init.id;
    this.kind = init.kind ?? "unknown";
    this.depends_on = init.depends_on ? [...init.depends_on] : [];
    this.payload = init.payload ? { ...init.payload } : {};
    this.action = init.action;
  }
  toDict(): Record<string, unknown> {
    const d: Record<string, unknown> = {
      id: this.id,
      kind: this.kind,
      depends_on: [...this.depends_on],
      payload: { ...this.payload },
    };
    if (this.action) d.action = this.action;
    return d;
  }
}

export function withEvidence(
  result: Record<string, unknown>,
  evidence: Evidence[],
  decision?: DecisionRef,
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...result, _evidence: evidence.map((e) => e.toDict()) };
  if (decision) out._decision = decision.toDict();
  return out;
}

export function resourceHint(resourceId: string, version: string | null, sourceSystem?: string): Record<string, unknown> {
  const d: Record<string, unknown> = { resource_id: resourceId, version };
  if (sourceSystem) d.source_system = sourceSystem;
  return d;
}
