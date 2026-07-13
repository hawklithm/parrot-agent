-- Task watchdog support: issue watchdogs.
-- Mirrors paperclip issue_watchdogs: one row per (company, watched issue)
-- tracking the subtree evaluation fingerprint and review state.

CREATE TYPE issue_watchdog_status AS ENUM ('active', 'paused', 'resolved', 'archived');

CREATE TABLE issue_watchdogs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    watchdog_agent_id UUID NOT NULL REFERENCES agents(id),
    instructions TEXT,
    status issue_watchdog_status NOT NULL DEFAULT 'active',
    watchdog_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    last_observed_fingerprint TEXT,
    last_reviewed_fingerprint TEXT,
    last_triggered_at TIMESTAMPTZ,
    last_completed_at TIMESTAMPTZ,
    trigger_count INTEGER NOT NULL DEFAULT 0,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    updated_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    updated_by_user_id TEXT,
    updated_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT issue_watchdogs_company_issue_uq UNIQUE (company_id, issue_id),
    CONSTRAINT issue_watchdogs_company_watchdog_issue_uq UNIQUE (company_id, watchdog_issue_id)
);

CREATE INDEX issue_watchdogs_company_status_idx ON issue_watchdogs(company_id, status);
CREATE INDEX issue_watchdogs_company_agent_idx ON issue_watchdogs(company_id, watchdog_agent_id);
