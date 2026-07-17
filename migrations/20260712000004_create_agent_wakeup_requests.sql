-- Task watchdog support: agent wakeup requests.
-- Mirrors paperclip agent_wakeup_requests: a queued/active wake that keeps an
-- issue subtree "live" until the agent run starts.

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'agent_wakeup_request_status') THEN
        CREATE TYPE agent_wakeup_request_status AS ENUM ('queued', 'dispatched', 'running', 'completed', 'failed', 'cancelled');
    END IF;
END
$$;

CREATE TABLE agent_wakeup_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    status agent_wakeup_request_status NOT NULL DEFAULT 'queued',
    payload JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX agent_wakeup_requests_company_status_idx ON agent_wakeup_requests(company_id, status);
CREATE INDEX agent_wakeup_requests_company_agent_idx ON agent_wakeup_requests(company_id, agent_id);
