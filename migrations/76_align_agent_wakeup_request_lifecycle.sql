-- Align the wake request lifecycle with Paperclip's durable enqueue contract.
-- Fresh databases must retain the request-to-run link so recovery can tell an
-- in-flight reservation from a request that was never dispatched.
BEGIN;

ALTER TABLE agent_wakeup_requests
    ADD COLUMN IF NOT EXISTS coalesced_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS claimed_at TIMESTAMPTZ;

UPDATE agent_wakeup_requests
   SET source = COALESCE(source, 'on_demand')
 WHERE source IS NULL;

ALTER TABLE agent_wakeup_requests
    ALTER COLUMN source SET DEFAULT 'on_demand',
    ALTER COLUMN source SET NOT NULL;

CREATE INDEX IF NOT EXISTS agent_wakeup_requests_company_agent_status_idx
    ON agent_wakeup_requests(company_id, agent_id, status);

CREATE INDEX IF NOT EXISTS agent_wakeup_requests_company_requested_idx
    ON agent_wakeup_requests(company_id, requested_at);

CREATE INDEX IF NOT EXISTS agent_wakeup_requests_agent_requested_idx
    ON agent_wakeup_requests(agent_id, requested_at);

CREATE INDEX IF NOT EXISTS agent_wakeup_requests_company_payload_issue_idx
    ON agent_wakeup_requests(company_id, ((payload ->> 'issueId')));

COMMIT;
