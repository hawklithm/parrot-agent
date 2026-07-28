CREATE TABLE IF NOT EXISTS tool_mcp_gateways (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    gateway_public_id TEXT NOT NULL UNIQUE DEFAULT ('gw_' || replace(gen_random_uuid()::text, '-', '')),
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    profile_id UUID,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, name),
    UNIQUE(company_id, slug)
);

CREATE INDEX IF NOT EXISTS tool_mcp_gateways_company_idx ON tool_mcp_gateways(company_id, status);

CREATE TABLE IF NOT EXISTS tool_mcp_gateway_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    gateway_id UUID NOT NULL REFERENCES tool_mcp_gateways(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL DEFAULT '',
    allowed_actions JSONB NOT NULL DEFAULT '["tools/list", "tools/call"]',
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS tool_mcp_gateway_tokens_gateway_idx ON tool_mcp_gateway_tokens(company_id, gateway_id);
