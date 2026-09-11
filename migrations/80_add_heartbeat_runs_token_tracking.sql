-- Migration: Add token and cost tracking columns to heartbeat_runs
--
-- Aligns with Paperclip heartbeat_runs schema which includes:
--   usage_json JSONB — structured token/cost data for run ledger display
--   input_tokens, output_tokens, cached_input_tokens INTEGER — raw counts
--   total_cost_usd DOUBLE PRECISION — cost in USD
--
-- These columns enable the frontend Run Ledger to display per-run token usage
-- and cost without parsing result_json. The values are populated by the
-- HeartbeatService when a run completes (heartbeat_service.rs).
--
-- PAPERCLIP_MIGRATION_PLAN §4B.2 — Run Ledger data parity.

ALTER TABLE heartbeat_runs
ADD COLUMN IF NOT EXISTS input_tokens INTEGER NOT NULL DEFAULT 0,
ADD COLUMN IF NOT EXISTS output_tokens INTEGER NOT NULL DEFAULT 0,
ADD COLUMN IF NOT EXISTS cached_input_tokens INTEGER NOT NULL DEFAULT 0,
ADD COLUMN IF NOT EXISTS total_cost_usd DOUBLE PRECISION;

COMMENT ON COLUMN heartbeat_runs.input_tokens IS 'Input tokens consumed by this run';
COMMENT ON COLUMN heartbeat_runs.output_tokens IS 'Output tokens produced by this run';
COMMENT ON COLUMN heartbeat_runs.cached_input_tokens IS 'Cached input tokens (prompt cache hits)';
COMMENT ON COLUMN heartbeat_runs.total_cost_usd IS 'Estimated cost in USD for this run';
