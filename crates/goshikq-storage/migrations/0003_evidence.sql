-- Versioned evidence, dependency graph, decisions, impact reports.
-- Additive. Historical run/event/journal rows are not rewritten.

ALTER TABLE tool_invocations ADD COLUMN IF NOT EXISTS recovery_disposition TEXT NOT NULL DEFAULT 'NONE';

CREATE TABLE IF NOT EXISTS evidence_records (
    id                      UUID PRIMARY KEY,
    run_id                  UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    project_id              UUID NOT NULL REFERENCES projects(id),
    source_system           TEXT NOT NULL,
    resource_id             TEXT NOT NULL,
    resource_version        TEXT,
    retrieved_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    valid_from              TIMESTAMPTZ,
    valid_until             TIMESTAMPTZ,
    content_fingerprint     TEXT NOT NULL,
    trust                   TEXT NOT NULL DEFAULT 'authoritative',
    access_scope            TEXT,
    inferred                BOOLEAN NOT NULL DEFAULT FALSE,
    metadata                JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS evidence_records_run_idx ON evidence_records(run_id, retrieved_at);
CREATE INDEX IF NOT EXISTS evidence_records_resource_idx ON evidence_records(run_id, resource_id, resource_version);

CREATE TABLE IF NOT EXISTS evidence_nodes (
    id              TEXT NOT NULL,
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,
    resource_id     TEXT,
    version         TEXT,
    label           TEXT,
    payload         JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, id)
);
CREATE INDEX IF NOT EXISTS evidence_nodes_resource_idx ON evidence_nodes(run_id, resource_id);

CREATE TABLE IF NOT EXISTS evidence_edges (
    id              UUID PRIMARY KEY,
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    from_id         TEXT NOT NULL,
    to_id           TEXT NOT NULL,
    kind            TEXT NOT NULL,
    inferred        BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS evidence_edges_run_idx ON evidence_edges(run_id, created_at);
CREATE UNIQUE INDEX IF NOT EXISTS evidence_edges_unique ON evidence_edges(run_id, from_id, to_id, kind);

CREATE TABLE IF NOT EXISTS decision_records (
    id              TEXT NOT NULL,
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL DEFAULT 'unknown',
    status          TEXT NOT NULL DEFAULT 'PENDING',
    payload         JSONB NOT NULL DEFAULT '{}'::jsonb,
    evidence_ids    JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, id)
);

CREATE TABLE IF NOT EXISTS impact_reports (
    id              UUID PRIMARY KEY,
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    changes         JSONB NOT NULL,
    report          JSONB NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS impact_reports_run_idx ON impact_reports(run_id, created_at);

CREATE TABLE IF NOT EXISTS revalidation_results (
    id              UUID PRIMARY KEY,
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    decision_id     TEXT NOT NULL,
    previous_status TEXT,
    new_status      TEXT NOT NULL,
    reason          TEXT,
    predicate_results JSONB NOT NULL DEFAULT '{}'::jsonb,
    old_version     TEXT,
    new_version     TEXT,
    approval_id     UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS revalidation_results_run_idx ON revalidation_results(run_id, created_at);
