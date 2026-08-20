-- Paperclip-compatible agent join-request claim flow.
-- Existing deployments keep the historical human join-request columns; these
-- nullable/defaulted additions make the agent claim path explicit and safe.
ALTER TABLE join_requests
    ADD COLUMN IF NOT EXISTS request_type TEXT NOT NULL DEFAULT 'human',
    ADD COLUMN IF NOT EXISTS created_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS claim_secret_hash TEXT,
    ADD COLUMN IF NOT EXISTS claim_secret_expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS claim_secret_consumed_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_join_requests_created_agent_id
    ON join_requests(created_agent_id);

ALTER TABLE agent_api_keys
    ADD COLUMN IF NOT EXISTS scope JSONB NOT NULL DEFAULT '{}'::jsonb;
