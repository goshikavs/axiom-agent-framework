-- Versioned workflow catalog and per-node execution state (v0.8).
-- Existing `workflows` rows remain drafts. Published snapshots are immutable.

ALTER TABLE workflows ADD COLUMN IF NOT EXISTS description TEXT NOT NULL DEFAULT '';
ALTER TABLE workflows ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'draft';
ALTER TABLE workflows ADD COLUMN IF NOT EXISTS published_version INTEGER;

CREATE TABLE IF NOT EXISTS workflow_versions (
    id           UUID PRIMARY KEY,
    workflow_id  UUID NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    project_id   UUID NOT NULL REFERENCES projects(id),
    version      INTEGER NOT NULL,
    definition   JSONB NOT NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_by TEXT NOT NULL DEFAULT 'local-user',
    UNIQUE (workflow_id, version)
);
CREATE INDEX IF NOT EXISTS workflow_versions_project_idx ON workflow_versions(project_id, published_at DESC);

CREATE TABLE IF NOT EXISTS workflow_node_executions (
    id                UUID PRIMARY KEY,
    project_id        UUID NOT NULL REFERENCES projects(id),
    run_id            UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    workflow_id       UUID NOT NULL,
    workflow_version  INTEGER NOT NULL,
    node_id           TEXT NOT NULL,
    status            TEXT NOT NULL,
    attempt           INTEGER NOT NULL DEFAULT 0,
    input             JSONB NOT NULL DEFAULT '{}'::jsonb,
    output            JSONB,
    error             TEXT,
    child_run_id      UUID,
    approval_id       UUID,
    wait_until        TIMESTAMPTZ,
    started_at        TIMESTAMPTZ,
    completed_at      TIMESTAMPTZ,
    UNIQUE (run_id, node_id)
);
CREATE INDEX IF NOT EXISTS workflow_node_exec_run_idx ON workflow_node_executions(project_id, run_id);
