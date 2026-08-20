-- Align heartbeat run persistence with Paperclip bounded scheduled retries.
ALTER TYPE heartbeat_run_status ADD VALUE IF NOT EXISTS 'scheduled_retry';

ALTER TABLE heartbeat_runs
    ADD COLUMN IF NOT EXISTS scheduled_retry_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS scheduled_retry_attempt INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS scheduled_retry_reason TEXT;

CREATE INDEX IF NOT EXISTS heartbeat_runs_company_scheduled_retry_idx
    ON heartbeat_runs(company_id, scheduled_retry_at, created_at)
    WHERE status = 'scheduled_retry';
