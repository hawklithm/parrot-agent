-- Migration: Add Tool Access System - Core Tables (Phase 1)
-- Description: Adds the 5 core tables for Tool Access System
-- Date: 2026-08-18
-- Tables: tool_applications, tool_connections, connection_grants, tool_connection_installs, tool_oauth_states

-- ============================================================================
-- 1. tool_applications - 工具应用定义
-- ============================================================================
CREATE TABLE IF NOT EXISTS tool_applications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    application_key TEXT,
    name TEXT NOT NULL,
    description TEXT,
    type TEXT NOT NULL CHECK (type IN ('builtin', 'plugin', 'custom')),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
    plugin_id UUID REFERENCES plugins(id) ON DELETE SET NULL,
    owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    owner_user_id TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- The unified baseline already contains an older request-oriented
-- tool_applications table. Extend it in place so this migration remains
-- compatible with both the baseline and a clean database.
ALTER TABLE tool_applications
    ADD COLUMN IF NOT EXISTS application_key TEXT,
    ADD COLUMN IF NOT EXISTS name TEXT,
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS type TEXT,
    ADD COLUMN IF NOT EXISTS plugin_id UUID,
    ADD COLUMN IF NOT EXISTS owner_agent_id UUID,
    ADD COLUMN IF NOT EXISTS owner_user_id TEXT,
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ;

-- Indexes for tool_applications
CREATE INDEX IF NOT EXISTS tool_applications_company_idx ON tool_applications(company_id);
CREATE INDEX IF NOT EXISTS tool_applications_company_status_idx ON tool_applications(company_id, status);
CREATE UNIQUE INDEX IF NOT EXISTS tool_applications_company_name_uq ON tool_applications(company_id, name);
CREATE UNIQUE INDEX IF NOT EXISTS tool_applications_company_key_uq ON tool_applications(company_id, application_key);

-- ============================================================================
-- 2. tool_connections - 工具连接配置
-- ============================================================================
CREATE TABLE IF NOT EXISTS tool_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    -- NO ACTION (not CASCADE) to prevent delete-vs-create race
    -- A connection inserted concurrently takes FOR KEY SHARE lock on parent row,
    -- so delete fails with foreign-key violation instead of silently cascading
    application_id UUID NOT NULL REFERENCES tool_applications(id) ON DELETE NO ACTION,
    name TEXT NOT NULL,
    uid TEXT NOT NULL,
    connection_kind TEXT NOT NULL DEFAULT 'managed' CHECK (connection_kind IN ('managed', 'delegated', 'self_hosted')),
    ownership TEXT NOT NULL DEFAULT 'customer' CHECK (ownership IN ('platform_shared', 'platform_provisioned', 'customer', 'dcr')),
    transport TEXT NOT NULL CHECK (transport IN ('mcp_remote', 'rest_api', 'local_stdio')),
    auth_kind TEXT NOT NULL DEFAULT 'none' CHECK (auth_kind IN ('oauth', 'api_key', 'none')),
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'active', 'disabled', 'error')),
    enabled BOOLEAN NOT NULL DEFAULT false,
    config JSONB NOT NULL DEFAULT '{}',
    transport_config JSONB NOT NULL DEFAULT '{}',
    credential_refs JSONB NOT NULL DEFAULT '[]',
    credential_secret_refs JSONB NOT NULL DEFAULT '[]',
    health_status TEXT NOT NULL DEFAULT 'unchecked' CHECK (health_status IN ('healthy', 'degraded', 'unhealthy', 'unchecked')),
    health_message TEXT,
    health_checked_at TIMESTAMPTZ,
    last_healthy_at TIMESTAMPTZ,
    last_catalog_refresh_at TIMESTAMPTZ,
    last_error TEXT,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT tool_connections_company_id_uq UNIQUE (company_id, id)
);

ALTER TABLE tool_connections
    ADD COLUMN IF NOT EXISTS config JSONB NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS credential_refs JSONB NOT NULL DEFAULT '[]',
    ADD COLUMN IF NOT EXISTS credential_secret_refs JSONB NOT NULL DEFAULT '[]',
    ADD COLUMN IF NOT EXISTS health_status TEXT NOT NULL DEFAULT 'unchecked',
    ADD COLUMN IF NOT EXISTS health_message TEXT,
    ADD COLUMN IF NOT EXISTS health_checked_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_healthy_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_catalog_refresh_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_error TEXT,
    ADD COLUMN IF NOT EXISTS tool_type TEXT;

-- The baseline has the same logical uniqueness but not the compound key used
-- by connection_grants' tenant-scoped foreign key.
CREATE UNIQUE INDEX IF NOT EXISTS tool_connections_company_id_id_uq
    ON tool_connections(company_id, id);

-- Indexes for tool_connections
CREATE INDEX IF NOT EXISTS tool_connections_company_idx ON tool_connections(company_id);
CREATE INDEX IF NOT EXISTS tool_connections_application_idx ON tool_connections(application_id);
CREATE INDEX IF NOT EXISTS tool_connections_company_enabled_idx ON tool_connections(company_id, enabled);
CREATE UNIQUE INDEX IF NOT EXISTS tool_connections_company_uid_uq ON tool_connections(company_id, uid);

-- ============================================================================
-- 3. connection_grants - 连接授权
-- ============================================================================
CREATE TABLE IF NOT EXISTS connection_grants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    connection_id UUID NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('workspace', 'user')),
    subject_user_id TEXT,
    provider_tenant JSONB, -- { name?: string, externalId?: string }
    credential_secret_refs JSONB NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked', 'expired', 'needs_reauthorization')),
    is_default BOOLEAN NOT NULL DEFAULT false,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    revoked_at TIMESTAMPTZ,
    revoked_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    revoked_by_user_id TEXT,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    -- Check constraints
    CONSTRAINT connection_grants_subject_check CHECK (
        (kind = 'user' AND subject_user_id IS NOT NULL) OR
        (kind = 'workspace' AND subject_user_id IS NULL)
    ),
    CONSTRAINT connection_grants_default_check CHECK (
        is_default = false OR kind = 'workspace'
    ),
    -- Foreign key to tool_connections (compound key)
    CONSTRAINT connection_grants_company_connection_fk
        FOREIGN KEY (company_id, connection_id)
        REFERENCES tool_connections(company_id, id)
        ON DELETE CASCADE
);

-- Indexes for connection_grants
CREATE INDEX IF NOT EXISTS connection_grants_company_connection_idx ON connection_grants(company_id, connection_id);
CREATE INDEX IF NOT EXISTS connection_grants_subject_user_idx ON connection_grants(company_id, subject_user_id);
CREATE UNIQUE INDEX IF NOT EXISTS connection_grants_user_uq ON connection_grants(connection_id, subject_user_id);
CREATE UNIQUE INDEX IF NOT EXISTS connection_grants_default_uq ON connection_grants(connection_id)
    WHERE is_default = true AND kind = 'workspace';

-- ============================================================================
-- 4. tool_connection_installs - 工具安装记录
-- ============================================================================
CREATE TABLE IF NOT EXISTS tool_connection_installs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    connection_id UUID NOT NULL REFERENCES tool_connections(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL CHECK (target_type IN ('company', 'agent')),
    target_id TEXT NOT NULL,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for tool_connection_installs
CREATE INDEX IF NOT EXISTS tool_connection_installs_company_target_idx 
    ON tool_connection_installs(company_id, target_type, target_id);
CREATE INDEX IF NOT EXISTS tool_connection_installs_connection_idx 
    ON tool_connection_installs(company_id, connection_id);
CREATE UNIQUE INDEX IF NOT EXISTS tool_connection_installs_target_uq 
    ON tool_connection_installs(company_id, connection_id, target_type, target_id);

-- ============================================================================
-- 5. tool_oauth_states - OAuth状态管理
-- ============================================================================
CREATE TABLE IF NOT EXISTS tool_oauth_states (
    state TEXT PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    connection_id UUID NOT NULL REFERENCES tool_connections(id) ON DELETE CASCADE,
    code_verifier TEXT NOT NULL,
    created_by_actor_type TEXT,
    created_by_actor_id TEXT,
    created_by_session_id TEXT,
    subject_user_id TEXT,
    requested_scopes JSONB, -- string[]
    return_to TEXT,
    issue_id UUID,
    interaction_id UUID,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for tool_oauth_states
CREATE INDEX IF NOT EXISTS tool_oauth_states_company_idx ON tool_oauth_states(company_id);
CREATE INDEX IF NOT EXISTS tool_oauth_states_connection_idx ON tool_oauth_states(connection_id);
CREATE INDEX IF NOT EXISTS tool_oauth_states_actor_idx ON tool_oauth_states(created_by_actor_type, created_by_actor_id);
CREATE INDEX IF NOT EXISTS tool_oauth_states_expires_at_idx ON tool_oauth_states(expires_at);

-- ============================================================================
-- Update triggers for updated_at
-- ============================================================================
CREATE OR REPLACE FUNCTION update_tool_access_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tool_applications_updated_at
    BEFORE UPDATE ON tool_applications
    FOR EACH ROW
    EXECUTE FUNCTION update_tool_access_updated_at();

CREATE TRIGGER tool_connections_updated_at
    BEFORE UPDATE ON tool_connections
    FOR EACH ROW
    EXECUTE FUNCTION update_tool_access_updated_at();

CREATE TRIGGER connection_grants_updated_at
    BEFORE UPDATE ON connection_grants
    FOR EACH ROW
    EXECUTE FUNCTION update_tool_access_updated_at();

-- ============================================================================
-- Comments for documentation
-- ============================================================================
COMMENT ON TABLE tool_applications IS 'Tool application definitions (e.g. GitHub, Slack)';
COMMENT ON TABLE tool_connections IS 'Specific tool connection instances with auth and transport config';
COMMENT ON TABLE connection_grants IS 'User or workspace-level connection authorizations';
COMMENT ON TABLE tool_connection_installs IS 'Tool installations to companies or agents';
COMMENT ON TABLE tool_oauth_states IS 'OAuth flow state tracking with PKCE support';

COMMENT ON COLUMN tool_connections.application_id IS 'NO ACTION (not CASCADE) prevents delete-vs-create race';
COMMENT ON COLUMN connection_grants.is_default IS 'Only workspace grants can be default';
COMMENT ON COLUMN tool_oauth_states.code_verifier IS 'PKCE code verifier for OAuth 2.0 authorization code flow';
