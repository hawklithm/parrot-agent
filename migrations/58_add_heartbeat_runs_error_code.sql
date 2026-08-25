-- Migration: Add error_code / error_family / retry_of_run_id to heartbeat_runs
--
-- Aligns with Paperclip run-stop metadata and summary semantics (PAPERCLIP_MIGRATION_PLAN §4B.2):
--   - error_code / error_family: classified failure code persisted on run completion so the
--     dashboard runActivity "failedByErrorCode" breakdown is non-empty (previously the code was
--     computed in-memory but never written to the row; only embedded in result_json).
--   - retry_of_run_id: links a scheduled-retry run to the run it retries, enabling the
--     "recovered" summary counter. Column added now; populated by the retry-promotion path.

ALTER TABLE heartbeat_runs
ADD COLUMN IF NOT EXISTS error_code TEXT,
ADD COLUMN IF NOT EXISTS error_family TEXT,
ADD COLUMN IF NOT EXISTS retry_of_run_id UUID REFERENCES heartbeat_runs (id);

COMMENT ON COLUMN heartbeat_runs.error_code IS 'Classified failure code (e.g. adapter_failed, claude_malformed_response) persisted on run completion';
COMMENT ON COLUMN heartbeat_runs.error_family IS 'Failure family (e.g. adapter, upstream_protocol) for grouping in summaries';
COMMENT ON COLUMN heartbeat_runs.retry_of_run_id IS 'Run this scheduled-retry continues; enables recovered-run detection';
