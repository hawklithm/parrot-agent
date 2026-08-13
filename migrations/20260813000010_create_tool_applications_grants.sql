-- 对齐 Paperclip tool-access 域：补 tool_applications + tool_connection_grants
CREATE TABLE IF NOT EXISTS tool_applications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
    connection_id UUID REFERENCES tool_connections(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending',   -- pending|approved|denied
    justification TEXT,
    reviewed_by_user_id TEXT,
    reviewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (agent_id, connection_id)
);

CREATE INDEX IF NOT EXISTS idx_tool_applications_company ON tool_applications(company_id, status);

CREATE TABLE IF NOT EXISTS tool_connection_grants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    connection_id UUID NOT NULL REFERENCES tool_connections(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    grant_type TEXT NOT NULL DEFAULT 'explicit',
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (connection_id, agent_id)
);

CREATE INDEX IF NOT EXISTS idx_tool_connection_grants_company ON tool_connection_grants(company_id);
