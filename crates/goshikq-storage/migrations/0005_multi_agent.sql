-- v0.7 multi-agent: parent/child runs, delegated tasks, versioned shared context.

ALTER TABLE runs
    ADD COLUMN IF NOT EXISTS parent_run_id UUID REFERENCES runs(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS task_id UUID,
    ADD COLUMN IF NOT EXISTS correlation_id TEXT;

CREATE INDEX IF NOT EXISTS runs_parent_idx ON runs(parent_run_id);
CREATE INDEX IF NOT EXISTS runs_task_idx ON runs(task_id);
CREATE INDEX IF NOT EXISTS runs_correlation_idx ON runs(project_id, correlation_id);

CREATE TABLE IF NOT EXISTS run_tasks (
    id                   UUID PRIMARY KEY,
    project_id           UUID NOT NULL REFERENCES projects(id),
    parent_run_id        UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    child_run_id         UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    task_id              UUID NOT NULL,
    correlation_id       TEXT NOT NULL,
    role                 TEXT NOT NULL DEFAULT 'worker',
    status               TEXT NOT NULL DEFAULT 'CREATED',
    verification_status  TEXT NOT NULL DEFAULT 'NOT_EVALUATED',
    requires_approval    BOOLEAN NOT NULL DEFAULT false,
    approval_id          UUID,
    depth                INTEGER NOT NULL DEFAULT 1,
    metadata             JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, task_id),
    UNIQUE (child_run_id)
);
CREATE INDEX IF NOT EXISTS run_tasks_parent_idx ON run_tasks(parent_run_id);
CREATE INDEX IF NOT EXISTS run_tasks_project_idx ON run_tasks(project_id, created_at DESC);

CREATE TABLE IF NOT EXISTS shared_context (
    id             UUID PRIMARY KEY,
    project_id     UUID NOT NULL REFERENCES projects(id),
    scope_id       UUID NOT NULL,
    key            TEXT NOT NULL,
    value          JSONB NOT NULL,
    version        BIGINT NOT NULL DEFAULT 1,
    owner_agent    TEXT NOT NULL DEFAULT '',
    visibility     TEXT NOT NULL DEFAULT 'team',
    evidence_refs  JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, scope_id, key)
);
CREATE INDEX IF NOT EXISTS shared_context_scope_idx ON shared_context(project_id, scope_id);

CREATE TABLE IF NOT EXISTS shared_context_history (
    id             UUID PRIMARY KEY,
    context_id     UUID NOT NULL REFERENCES shared_context(id) ON DELETE CASCADE,
    project_id     UUID NOT NULL REFERENCES projects(id),
    scope_id       UUID NOT NULL,
    key            TEXT NOT NULL,
    value          JSONB NOT NULL,
    version        BIGINT NOT NULL,
    owner_agent    TEXT NOT NULL DEFAULT '',
    visibility     TEXT NOT NULL DEFAULT 'team',
    evidence_refs  JSONB NOT NULL DEFAULT '[]'::jsonb,
    actor          TEXT NOT NULL DEFAULT '',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS shared_context_history_idx ON shared_context_history(context_id, version);
