-- Create companies table (prerequisite for agents)
CREATE TABLE IF NOT EXISTS companies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create agents table
CREATE TABLE agents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'general',
    status TEXT NOT NULL DEFAULT 'idle',
    adapter_type TEXT NOT NULL DEFAULT 'process',
    adapter_config JSONB NOT NULL DEFAULT '{}',
    runtime_config JSONB NOT NULL DEFAULT '{}',
    permissions JSONB NOT NULL DEFAULT '{"can_create_agents":false,"can_create_skills":false,"trust_preset":"standard","authorization_policy":"manual"}',
    metadata JSONB NOT NULL DEFAULT '{}',
    budget_monthly_cents INTEGER NOT NULL DEFAULT 0,
    reports_to UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_role CHECK (role IN ('ceo', 'vp', 'manager', 'researcher', 'general')),
    CONSTRAINT valid_status CHECK (status IN ('idle', 'running', 'paused', 'pending_approval', 'terminated'))
);

-- Create indexes for agents
CREATE INDEX idx_agents_company_id ON agents(company_id);
CREATE INDEX idx_agents_status ON agents(status);
CREATE INDEX idx_agents_reports_to ON agents(reports_to);

-- Create agent_config_revisions table
CREATE TABLE agent_config_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    snapshot JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_agent_config_revisions_agent_id ON agent_config_revisions(agent_id);
CREATE INDEX idx_agent_config_revisions_created_at ON agent_config_revisions(created_at DESC);

-- Create cost_events table
CREATE TABLE cost_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    amount_cents INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_cost_events_agent_id ON cost_events(agent_id);
CREATE INDEX idx_cost_events_created_at ON cost_events(created_at DESC);
