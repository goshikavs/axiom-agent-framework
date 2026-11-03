-- Evidence coverage evaluations. Additive. Historical rows are not rewritten.
-- Coverage status is independent of goal verification_status.

CREATE TABLE IF NOT EXISTS evidence_coverage_reports (
    id              UUID PRIMARY KEY,
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    tool_name       TEXT NOT NULL,
    policy          TEXT NOT NULL,
    status          TEXT NOT NULL,
    gate            TEXT NOT NULL,
    report          JSONB NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS evidence_coverage_run_idx ON evidence_coverage_reports(run_id, created_at);
