-- The enum value is committed by migration 32 before this partial index is parsed.
CREATE INDEX IF NOT EXISTS heartbeat_runs_company_scheduled_retry_idx
    ON heartbeat_runs(company_id, scheduled_retry_at, created_at)
    WHERE status = 'scheduled_retry';
