-- Migration: Create run_continuations table
--
-- Run Continuation ledger (PAPERCLIP_MIGRATION_PLAN §4B.2 line 325). Records the
-- parent/child relationship when a run is continued by a scheduled-retry
-- promotion, giving the RunContinuationsService (crates/services/src/run_continuations_service.rs)
-- its missing backing table and making the self-healing continuation chain
-- queryable. The service was implemented against this table but the DDL was
-- never written or applied; this migration closes that gap.
--
-- Columns mirror the INSERT/SELECT performed by RunContinuationsService exactly:
--   id                  continuation row id
--   run_id              the continuing run (the promoted retry run)
--   parent_run_id       the run it continues (the original failed run)
--   continuation_point  where execution resumes (e.g. 'scheduled_retry')
--   state_snapshot      JSON state carried into the continuation
--   reason              why the continuation was created

CREATE TABLE IF NOT EXISTS run_continuations (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id              UUID NOT NULL REFERENCES heartbeat_runs (id) ON DELETE CASCADE,
    parent_run_id       UUID REFERENCES heartbeat_runs (id) ON DELETE SET NULL,
    continuation_point  TEXT NOT NULL,
    state_snapshot      JSONB NOT NULL DEFAULT '{}'::jsonb,
    reason              TEXT NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS run_continuations_run_id_idx
    ON run_continuations (run_id, created_at DESC);
CREATE INDEX IF NOT EXISTS run_continuations_parent_run_id_idx
    ON run_continuations (parent_run_id);

COMMENT ON TABLE run_continuations IS 'Ledger of run continuations (scheduled-retry promotions) linking a retry run to the run it continues';
COMMENT ON COLUMN run_continuations.parent_run_id IS 'The original run being continued; NULL for root runs';
