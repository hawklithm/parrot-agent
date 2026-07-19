-- Create budget-related tables (budget_policies, budget_incidents)
-- Mirrors paperclip's budget system

-- Create enum types
DO $$ BEGIN
    CREATE TYPE budget_scope_type AS ENUM ('company', 'agent', 'project');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE budget_window_kind AS ENUM ('calendar_month_utc', 'lifetime');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE budget_metric AS ENUM ('billed_cents');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE budget_threshold_type AS ENUM ('soft', 'hard');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE budget_incident_status AS ENUM ('open', 'resolved', 'dismissed');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE finance_direction AS ENUM ('debit', 'credit');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Create budget_policies table
CREATE TABLE IF NOT EXISTS budget_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    scope_type budget_scope_type NOT NULL,
    scope_id UUID NOT NULL,
    metric budget_metric NOT NULL DEFAULT 'billed_cents',
    window_kind budget_window_kind NOT NULL DEFAULT 'calendar_month_utc',
    amount BIGINT NOT NULL DEFAULT 0,
    warn_percent INTEGER NOT NULL DEFAULT 80,
    hard_stop_enabled BOOLEAN NOT NULL DEFAULT true,
    notify_enabled BOOLEAN NOT NULL DEFAULT true,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_by_user_id UUID,
    updated_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, scope_type, scope_id, metric, window_kind)
);

CREATE INDEX IF NOT EXISTS idx_budget_policies_company_id ON budget_policies(company_id);
CREATE INDEX IF NOT EXISTS idx_budget_policies_scope ON budget_policies(scope_type, scope_id);

-- Create budget_incidents table
CREATE TABLE IF NOT EXISTS budget_incidents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    policy_id UUID NOT NULL REFERENCES budget_policies(id) ON DELETE CASCADE,
    scope_type budget_scope_type NOT NULL,
    scope_id UUID NOT NULL,
    metric budget_metric NOT NULL DEFAULT 'billed_cents',
    window_kind budget_window_kind NOT NULL DEFAULT 'calendar_month_utc',
    window_start TIMESTAMPTZ NOT NULL,
    window_end TIMESTAMPTZ NOT NULL,
    threshold_type budget_threshold_type NOT NULL,
    amount_limit BIGINT NOT NULL,
    amount_observed BIGINT NOT NULL,
    status budget_incident_status NOT NULL DEFAULT 'open',
    approval_id UUID REFERENCES approvals(id) ON DELETE SET NULL,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_budget_incidents_company_id ON budget_incidents(company_id);
CREATE INDEX IF NOT EXISTS idx_budget_incidents_policy_id ON budget_incidents(policy_id);
CREATE INDEX IF NOT EXISTS idx_budget_incidents_status ON budget_incidents(status);

-- Create finance_events table
CREATE TABLE IF NOT EXISTS finance_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    goal_id UUID REFERENCES goals(id) ON DELETE SET NULL,
    heartbeat_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    cost_event_id UUID REFERENCES cost_events(id) ON DELETE SET NULL,
    biller TEXT NOT NULL DEFAULT 'unknown',
    event_kind TEXT NOT NULL,
    direction finance_direction NOT NULL DEFAULT 'debit',
    amount_cents INTEGER NOT NULL DEFAULT 0,
    currency TEXT NOT NULL DEFAULT 'USD',
    estimated BOOLEAN NOT NULL DEFAULT false,
    description TEXT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_finance_events_company_id ON finance_events(company_id);
CREATE INDEX IF NOT EXISTS idx_finance_events_agent_id ON finance_events(agent_id);
CREATE INDEX IF NOT EXISTS idx_finance_events_occurred_at ON finance_events(occurred_at DESC);

-- Add spent_monthly_cents to agents table for tracking agent-level spend
ALTER TABLE agents ADD COLUMN IF NOT EXISTS spent_monthly_cents BIGINT NOT NULL DEFAULT 0;

-- Create triggers for updated_at
CREATE TRIGGER update_budget_policies_updated_at
    BEFORE UPDATE ON budget_policies
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_budget_incidents_updated_at
    BEFORE UPDATE ON budget_incidents
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
