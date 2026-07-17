-- Task watchdog support: agent execution heartbeat runs.
-- Focused subset of paperclip's heartbeat_runs: only the columns the
-- task-watchdog live-path detection reads (id, company, agent, status,
-- context_snapshot carrying issueId/taskId).

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'heartbeat_run_status') THEN
        CREATE TYPE heartbeat_run_status AS ENUM (
            'queued', 'running', 'succeeded', 'failed', 'cancelled', 'timed_out'
        );
    END IF;
END
$$;

CREATE TABLE heartbeat_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    invocation_source TEXT NOT NULL DEFAULT 'on_demand',
    status heartbeat_run_status NOT NULL DEFAULT 'queued',
    responsible_user_id TEXT,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    error TEXT,
    exit_code INTEGER,
    context_snapshot JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_invocation_source CHECK (invocation_source IN ('on_demand', 'scheduled', 'watchdog'))
);

CREATE INDEX heartbeat_runs_company_agent_started_idx
    ON heartbeat_runs(company_id, agent_id, started_at);
CREATE INDEX heartbeat_runs_company_status_idx
    ON heartbeat_runs(company_id, status);
