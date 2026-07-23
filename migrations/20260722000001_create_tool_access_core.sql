-- Paperclip tool-access core persistence used by the company tools UI.
CREATE TABLE IF NOT EXISTS tool_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    application_id UUID,
    name TEXT NOT NULL,
    uid TEXT NOT NULL,
    connection_kind TEXT NOT NULL DEFAULT 'managed',
    ownership TEXT NOT NULL DEFAULT 'customer',
    transport TEXT NOT NULL DEFAULT 'mcp_remote',
    auth_kind TEXT NOT NULL DEFAULT 'none',
    status TEXT NOT NULL DEFAULT 'active',
    transport_config JSONB NOT NULL DEFAULT '{}',
    credential_secret_refs JSONB NOT NULL DEFAULT '[]',
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_by_agent_id UUID,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, uid)
);

CREATE TABLE IF NOT EXISTS tool_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    profile_key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    default_action TEXT NOT NULL DEFAULT 'deny',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, profile_key),
    UNIQUE(company_id, name)
);

CREATE TABLE IF NOT EXISTS tool_profile_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    profile_id UUID NOT NULL REFERENCES tool_profiles(id) ON DELETE CASCADE,
    selector_type TEXT NOT NULL,
    selector_value TEXT NOT NULL,
    effect TEXT NOT NULL DEFAULT 'allow',
    connection_id UUID REFERENCES tool_connections(id) ON DELETE CASCADE,
    tool_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS tool_profile_bindings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    profile_id UUID NOT NULL REFERENCES tool_profiles(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,
    target_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, target_type, target_id)
);

CREATE TABLE IF NOT EXISTS tool_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    policy_type TEXT NOT NULL DEFAULT 'allow',
    priority INTEGER NOT NULL DEFAULT 0,
    enabled BOOLEAN NOT NULL DEFAULT true,
    selectors JSONB NOT NULL DEFAULT '{}',
    conditions JSONB,
    config JSONB,
    created_by_agent_id UUID,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, name)
);
