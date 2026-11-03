-- Forward-only branding default. Historical rows keep source_framework='axiom'.
-- Migration 0001 is not rewritten; existing applied history stays intact.
ALTER TABLE agents ALTER COLUMN source_framework SET DEFAULT 'goshikq';
ALTER TABLE runs ALTER COLUMN source_framework SET DEFAULT 'goshikq';
