-- v0.9 tenancy, membership, API-key lifecycle. Additive.

ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'operator';
ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'service';
ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ;
ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS rotated_from UUID REFERENCES api_keys(id);

CREATE TABLE IF NOT EXISTS project_members (
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    subject     TEXT NOT NULL,
    role        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id, subject)
);
CREATE INDEX IF NOT EXISTS project_members_subject_idx ON project_members(subject);

CREATE TABLE IF NOT EXISTS security_events (
    id          UUID PRIMARY KEY,
    project_id  UUID REFERENCES projects(id),
    tenant_id   UUID,
    actor       TEXT NOT NULL,
    action      TEXT NOT NULL,
    outcome     TEXT NOT NULL,
    details     JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS security_events_created_idx ON security_events(created_at DESC);
CREATE INDEX IF NOT EXISTS security_events_project_idx ON security_events(project_id, created_at DESC);

ALTER TABLE projects ADD COLUMN IF NOT EXISTS retention_days INTEGER NOT NULL DEFAULT 30;
