-- Goshikq initial schema (PostgreSQL). All run-scoped tables carry project_id for tenant scoping.

CREATE TABLE organizations (
    id          UUID PRIMARY KEY,
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE projects (
    id               UUID PRIMARY KEY,
    organization_id  UUID NOT NULL REFERENCES organizations(id),
    name             TEXT NOT NULL,
    slug             TEXT NOT NULL UNIQUE,
    retention_days   INTEGER NOT NULL DEFAULT 30,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE api_keys (
    id            UUID PRIMARY KEY,
    project_id    UUID NOT NULL REFERENCES projects(id),
    name          TEXT NOT NULL,
    key_hash      TEXT NOT NULL UNIQUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at  TIMESTAMPTZ,
    revoked_at    TIMESTAMPTZ
);
CREATE INDEX api_keys_project_idx ON api_keys(project_id);

CREATE TABLE agents (
    id                UUID PRIMARY KEY,
    project_id        UUID NOT NULL REFERENCES projects(id),
    name              TEXT NOT NULL,
    description       TEXT NOT NULL DEFAULT '',
    source_framework  TEXT NOT NULL DEFAULT 'axiom',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, name)
);

CREATE TABLE agent_versions (
    id             UUID PRIMARY KEY,
    agent_id       UUID NOT NULL REFERENCES agents(id),
    version        INTEGER NOT NULL,
    definition     JSONB NOT NULL,
    config_hash    TEXT NOT NULL,
    model          TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (agent_id, version),
    UNIQUE (agent_id, config_hash)
);

CREATE TABLE workflows (
    id          UUID PRIMARY KEY,
    project_id  UUID NOT NULL REFERENCES projects(id),
    name        TEXT NOT NULL,
    version     INTEGER NOT NULL DEFAULT 1,
    definition  JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, name, version)
);

CREATE TABLE goals (
    id           UUID PRIMARY KEY,
    project_id   UUID NOT NULL REFERENCES projects(id),
    description  TEXT NOT NULL,
    definition   JSONB NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE runs (
    id                   UUID PRIMARY KEY,
    project_id           UUID NOT NULL REFERENCES projects(id),
    agent_id             UUID REFERENCES agents(id),
    agent_version_id     UUID REFERENCES agent_versions(id),
    agent_name           TEXT NOT NULL,
    goal_id              UUID REFERENCES goals(id),
    goal_description     TEXT NOT NULL DEFAULT '',
    workflow_id          UUID REFERENCES workflows(id),
    trace_id             TEXT NOT NULL,
    status               TEXT NOT NULL,
    verification_status  TEXT NOT NULL DEFAULT 'NOT_EVALUATED',
    source_framework     TEXT NOT NULL DEFAULT 'axiom',
    model                TEXT NOT NULL,
    input                JSONB NOT NULL DEFAULT '{}'::jsonb,
    definition           JSONB NOT NULL,
    failure_reason       TEXT,
    final_output         TEXT,
    step_count           INTEGER NOT NULL DEFAULT 0,
    next_sequence        BIGINT NOT NULL DEFAULT 1,
    input_tokens         BIGINT NOT NULL DEFAULT 0,
    output_tokens        BIGINT NOT NULL DEFAULT 0,
    cached_tokens        BIGINT NOT NULL DEFAULT 0,
    usage_known          BOOLEAN NOT NULL DEFAULT false,
    estimated_cost_usd   DOUBLE PRECISION,
    cost_known           BOOLEAN NOT NULL DEFAULT false,
    idempotency_key      TEXT,
    metadata             JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at           TIMESTAMPTZ,
    ended_at             TIMESTAMPTZ,
    duration_ms          BIGINT,
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, idempotency_key)
);
CREATE INDEX runs_project_created_idx ON runs(project_id, created_at DESC);
CREATE INDEX runs_status_idx ON runs(project_id, status);
CREATE INDEX runs_trace_idx ON runs(trace_id);
CREATE INDEX runs_agent_idx ON runs(agent_id);

CREATE TABLE run_steps (
    id           UUID PRIMARY KEY,
    run_id       UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    step_number  INTEGER NOT NULL,
    phase        TEXT NOT NULL,
    status       TEXT NOT NULL,
    summary      JSONB NOT NULL DEFAULT '{}'::jsonb,
    started_at   TIMESTAMPTZ NOT NULL,
    ended_at     TIMESTAMPTZ,
    duration_ms  BIGINT,
    UNIQUE (run_id, step_number)
);

CREATE TABLE execution_events (
    event_id          UUID PRIMARY KEY,
    run_id            UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    project_id        UUID NOT NULL REFERENCES projects(id),
    trace_id          TEXT NOT NULL,
    span_id           TEXT NOT NULL,
    parent_span_id    TEXT,
    sequence          BIGINT NOT NULL,
    event_type        TEXT NOT NULL,
    schema_version    TEXT NOT NULL,
    source_timestamp  TIMESTAMPTZ NOT NULL,
    ingested_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    agent_id          TEXT,
    step_id           TEXT,
    source_framework  TEXT NOT NULL,
    status            TEXT,
    duration_ms       BIGINT,
    payload           JSONB NOT NULL DEFAULT '{}'::jsonb,
    metadata          JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (run_id, sequence)
);
CREATE INDEX execution_events_run_seq_idx ON execution_events(run_id, sequence);
CREATE INDEX execution_events_trace_idx ON execution_events(trace_id);
CREATE INDEX execution_events_project_ts_idx ON execution_events(project_id, source_timestamp DESC);
CREATE INDEX execution_events_parent_span_idx ON execution_events(parent_span_id);
CREATE INDEX execution_events_type_idx ON execution_events(project_id, event_type);

CREATE TABLE event_outbox (
    event_id      UUID PRIMARY KEY REFERENCES execution_events(event_id) ON DELETE CASCADE,
    run_id        UUID NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at  TIMESTAMPTZ
);
CREATE INDEX event_outbox_pending_idx ON event_outbox(created_at) WHERE published_at IS NULL;

CREATE TABLE tool_invocations (
    id                 UUID PRIMARY KEY,
    run_id             UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    project_id         UUID NOT NULL REFERENCES projects(id),
    step_id            TEXT NOT NULL,
    tool_call_id       TEXT NOT NULL,
    tool_name          TEXT NOT NULL,
    status             TEXT NOT NULL,
    effect             TEXT NOT NULL,
    risk               TEXT NOT NULL,
    arguments          JSONB NOT NULL,
    result             JSONB,
    error              JSONB,
    idempotency_key    TEXT NOT NULL,
    attempts           INTEGER NOT NULL DEFAULT 0,
    requires_approval  BOOLEAN NOT NULL DEFAULT false,
    approval_id        UUID,
    verification_status TEXT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at         TIMESTAMPTZ,
    ended_at           TIMESTAMPTZ,
    duration_ms        BIGINT
);
CREATE INDEX tool_invocations_run_idx ON tool_invocations(run_id, created_at);

CREATE TABLE state_snapshots (
    id           UUID PRIMARY KEY,
    run_id       UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    step_id      TEXT,
    reason       TEXT NOT NULL,
    source       TEXT NOT NULL DEFAULT '',
    version      TEXT,
    state        JSONB NOT NULL,
    changes      JSONB,
    captured_at  TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX state_snapshots_run_idx ON state_snapshots(run_id, created_at);

CREATE TABLE checkpoints (
    id           UUID PRIMARY KEY,
    run_id       UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    step_number  INTEGER NOT NULL,
    status       TEXT NOT NULL,
    data         JSONB NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX checkpoints_run_idx ON checkpoints(run_id, step_number DESC);

CREATE TABLE goal_evaluations (
    id                 UUID PRIMARY KEY,
    run_id             UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    step_id            TEXT,
    phase              TEXT NOT NULL,
    success            BOOLEAN,
    invariants         JSONB NOT NULL DEFAULT '[]'::jsonb,
    evidence           JSONB NOT NULL DEFAULT '{}'::jsonb,
    reason             TEXT,
    state_snapshot_id  UUID REFERENCES state_snapshots(id),
    evaluated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX goal_evaluations_run_idx ON goal_evaluations(run_id, evaluated_at);

CREATE TABLE approvals (
    id                     UUID PRIMARY KEY,
    run_id                 UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    project_id             UUID NOT NULL REFERENCES projects(id),
    tool_invocation_id     UUID NOT NULL REFERENCES tool_invocations(id),
    status                 TEXT NOT NULL,
    agent_name             TEXT NOT NULL,
    action_name            TEXT NOT NULL,
    arguments              JSONB NOT NULL,
    risk                   TEXT NOT NULL,
    reason                 TEXT NOT NULL,
    requested_permissions  JSONB NOT NULL DEFAULT '[]'::jsonb,
    requested_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at             TIMESTAMPTZ NOT NULL,
    decided_at             TIMESTAMPTZ,
    decided_by             TEXT,
    decision_reason        TEXT
);
CREATE INDEX approvals_pending_idx ON approvals(project_id, status, requested_at);

CREATE TABLE verification_results (
    id                  UUID PRIMARY KEY,
    run_id              UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    step_id             TEXT,
    subject_type        TEXT NOT NULL,
    subject_id          TEXT NOT NULL,
    status              TEXT NOT NULL,
    method              TEXT NOT NULL,
    expected            JSONB,
    actual              JSONB,
    evidence_source     TEXT,
    evidence_version    TEXT,
    evidence_timestamp  TIMESTAMPTZ,
    duration_ms         BIGINT,
    reason              TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX verification_results_run_idx ON verification_results(run_id, created_at);

CREATE TABLE model_usage (
    id                    UUID PRIMARY KEY,
    run_id                UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    project_id            UUID NOT NULL REFERENCES projects(id),
    step_id               TEXT,
    model                 TEXT NOT NULL,
    provider              TEXT,
    input_tokens          BIGINT,
    output_tokens         BIGINT,
    cached_tokens         BIGINT,
    latency_ms            BIGINT,
    provider_response_id  TEXT,
    finish_reason         TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX model_usage_run_idx ON model_usage(run_id);
CREATE INDEX model_usage_project_model_idx ON model_usage(project_id, model, created_at);

CREATE TABLE cost_records (
    id                  UUID PRIMARY KEY,
    run_id              UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    project_id          UUID NOT NULL REFERENCES projects(id),
    model_usage_id      UUID NOT NULL REFERENCES model_usage(id) ON DELETE CASCADE,
    model               TEXT NOT NULL,
    pricing_version     TEXT NOT NULL,
    estimated_cost_usd  DOUBLE PRECISION NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX cost_records_run_idx ON cost_records(run_id);

CREATE TABLE framework_integrations (
    id                       UUID PRIMARY KEY,
    project_id               UUID NOT NULL REFERENCES projects(id),
    framework                TEXT NOT NULL,
    adapter_version          TEXT NOT NULL,
    framework_version        TEXT,
    status                   TEXT NOT NULL,
    capabilities             JSONB NOT NULL DEFAULT '[]'::jsonb,
    unsupported_capabilities JSONB NOT NULL DEFAULT '[]'::jsonb,
    config                   JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_event_at            TIMESTAMPTZ,
    last_error               TEXT,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, framework)
);

CREATE TABLE audit_logs (
    id            UUID PRIMARY KEY,
    project_id    UUID NOT NULL REFERENCES projects(id),
    actor         TEXT NOT NULL,
    action        TEXT NOT NULL,
    subject_type  TEXT NOT NULL,
    subject_id    TEXT NOT NULL,
    details       JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_logs_project_idx ON audit_logs(project_id, created_at DESC);

-- Local single-user development defaults.
INSERT INTO organizations (id, name)
VALUES ('00000000-0000-0000-0000-000000000001', 'Default Organization');
INSERT INTO projects (id, organization_id, name, slug)
VALUES ('00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000001', 'Default Project', 'default');
