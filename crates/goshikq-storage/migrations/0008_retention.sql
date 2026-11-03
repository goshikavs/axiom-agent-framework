-- Index terminal runs by ended_at so administrative retention purge does not
-- scan the entire runs table. Purge never deletes non-terminal or parent-of-active rows.
CREATE INDEX IF NOT EXISTS runs_ended_at_terminal_idx
    ON runs (ended_at)
    WHERE status IN (
        'COMPLETED',
        'FAILED',
        'CANCELLED',
        'UNVERIFIED',
        'TIMED_OUT',
        'BUDGET_EXCEEDED',
        'STEP_LIMIT_EXCEEDED',
        'MODEL_BUDGET_EXCEEDED'
    );
