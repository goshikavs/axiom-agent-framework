-- Durable recovery: leases, fencing, action journal, structured checkpoints.
-- Additive. Historical run/event rows are not rewritten.

ALTER TABLE runs ADD COLUMN IF NOT EXISTS lease_owner TEXT;
ALTER TABLE runs ADD COLUMN IF NOT EXISTS lease_generation BIGINT NOT NULL DEFAULT 0;
ALTER TABLE runs ADD COLUMN IF NOT EXISTS lease_expires_at TIMESTAMPTZ;
ALTER TABLE runs ADD COLUMN IF NOT EXISTS recovery_generation INTEGER NOT NULL DEFAULT 0;

ALTER TABLE tool_invocations ADD COLUMN IF NOT EXISTS dispatch_state TEXT NOT NULL DEFAULT 'PREPARED';
ALTER TABLE tool_invocations ADD COLUMN IF NOT EXISTS input_fingerprint TEXT;
ALTER TABLE tool_invocations ADD COLUMN IF NOT EXISTS external_operation_id TEXT;
ALTER TABLE tool_invocations ADD COLUMN IF NOT EXISTS business_operation_status TEXT;
ALTER TABLE tool_invocations ADD COLUMN IF NOT EXISTS last_provider_ack JSONB;
ALTER TABLE tool_invocations ADD COLUMN IF NOT EXISTS recovery_policy TEXT;
ALTER TABLE tool_invocations ADD COLUMN IF NOT EXISTS reconciliation_at TIMESTAMPTZ;

UPDATE tool_invocations SET dispatch_state = CASE
    WHEN status = 'RUNNING' THEN 'DISPATCH_UNCERTAIN'
    WHEN status = 'RECONCILIATION_REQUIRED' THEN 'RECOVERY_REQUIRED'
    WHEN status IN ('FAILED', 'CANCELLED', 'TIMED_OUT') THEN 'REJECTED'
    WHEN status = 'SUCCEEDED' THEN 'ACCEPTED'
    WHEN status IN ('REQUESTED', 'VALIDATED', 'APPROVAL_REQUIRED', 'APPROVED') THEN 'PREPARED'
    ELSE dispatch_state
END
WHERE dispatch_state = 'PREPARED' AND status <> 'REQUESTED';

ALTER TABLE checkpoints ADD COLUMN IF NOT EXISTS schema_version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE checkpoints ADD COLUMN IF NOT EXISTS last_event_sequence BIGINT;
ALTER TABLE checkpoints ADD COLUMN IF NOT EXISTS recovery_generation INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS action_journal (
    id              UUID PRIMARY KEY,
    invocation_id   UUID NOT NULL REFERENCES tool_invocations(id) ON DELETE CASCADE,
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    dispatch_state  TEXT NOT NULL,
    detail          JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS action_journal_run_idx ON action_journal(run_id, created_at);
CREATE INDEX IF NOT EXISTS action_journal_inv_idx ON action_journal(invocation_id, created_at);

CREATE TABLE IF NOT EXISTS recovery_attempts (
    id                 UUID PRIMARY KEY,
    run_id             UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    project_id         UUID NOT NULL REFERENCES projects(id),
    attempt            INTEGER NOT NULL,
    phase              TEXT NOT NULL,
    reason             TEXT,
    action_taken       TEXT NOT NULL,
    lease_generation   BIGINT,
    result             TEXT NOT NULL,
    detail             JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS recovery_attempts_run_idx ON recovery_attempts(run_id, created_at);

CREATE INDEX IF NOT EXISTS runs_lease_idx ON runs(lease_expires_at) WHERE lease_expires_at IS NOT NULL;
