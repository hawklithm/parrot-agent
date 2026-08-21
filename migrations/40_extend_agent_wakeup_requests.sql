DO $$
BEGIN
    ALTER TYPE agent_wakeup_request_status ADD VALUE IF NOT EXISTS 'skipped';
END $$;

ALTER TABLE agent_wakeup_requests
    ADD COLUMN IF NOT EXISTS source TEXT,
    ADD COLUMN IF NOT EXISTS trigger_detail TEXT,
    ADD COLUMN IF NOT EXISTS reason TEXT,
    ADD COLUMN IF NOT EXISTS error TEXT,
    ADD COLUMN IF NOT EXISTS requested_by_actor_type TEXT,
    ADD COLUMN IF NOT EXISTS requested_by_actor_id UUID,
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT,
    ADD COLUMN IF NOT EXISTS finished_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS agent_wakeup_requests_issue_reason_idx
    ON agent_wakeup_requests(company_id, agent_id, reason, created_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS agent_wakeup_requests_idempotency_key_uq
    ON agent_wakeup_requests(company_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
