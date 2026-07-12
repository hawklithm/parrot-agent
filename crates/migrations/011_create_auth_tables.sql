-- Migration 011: Create Authentication & Authorization Tables
-- This migration creates the schema for the auth/authorization subsystem:
-- companies (tenants), auth_users, auth_sessions, API keys (board/agent),
-- CLI auth challenges, instance roles, company memberships, permission grants,
-- invites and join requests.
--
-- Column definitions follow the authoritative Rust structs in
-- crates/repositories/src/models/{auth,auth_keys,authorization,invite}.rs.
-- Role/status/principal_type/actor_source values are stored as TEXT (the
-- repositories crate maps them as String), so no Postgres enum types are
-- required here.

-- ---------------------------------------------------------------------------
-- Companies (multi-tenant root)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS companies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    description TEXT,
    logo_url TEXT,
    website TEXT,
    industry TEXT,
    size TEXT,
    cloud_stack_id TEXT,
    settings JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS companies_slug_idx ON companies(slug);
CREATE INDEX IF NOT EXISTS companies_cloud_stack_id_idx ON companies(cloud_stack_id);

-- ---------------------------------------------------------------------------
-- Auth users
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS auth_users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT NOT NULL,
    name TEXT,
    password_hash TEXT,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    email_verified_at TIMESTAMPTZ,
    avatar_url TEXT,
    oauth_provider TEXT,
    oauth_provider_id TEXT,
    cloud_tenant_id TEXT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    last_login_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS auth_users_email_idx ON auth_users(email);
CREATE INDEX IF NOT EXISTS auth_users_cloud_tenant_id_idx ON auth_users(cloud_tenant_id);

-- ---------------------------------------------------------------------------
-- Auth sessions
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS auth_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    session_token TEXT NOT NULL,
    user_agent TEXT,
    ip_address TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    last_activity_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS auth_sessions_token_idx ON auth_sessions(session_token);
CREATE INDEX IF NOT EXISTS auth_sessions_user_id_idx ON auth_sessions(user_id);

-- ---------------------------------------------------------------------------
-- Board API keys
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS board_api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    key_prefix TEXT NOT NULL,
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    is_revoked BOOLEAN NOT NULL DEFAULT FALSE,
    revoked_at TIMESTAMPTZ,
    revoked_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS board_api_keys_key_hash_idx ON board_api_keys(key_hash);
CREATE INDEX IF NOT EXISTS board_api_keys_user_id_idx ON board_api_keys(user_id);

-- ---------------------------------------------------------------------------
-- Agent API keys
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS agent_api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    key_prefix TEXT NOT NULL,
    scope JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    is_revoked BOOLEAN NOT NULL DEFAULT FALSE,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS agent_api_keys_key_hash_idx ON agent_api_keys(key_hash);
CREATE INDEX IF NOT EXISTS agent_api_keys_agent_id_idx ON agent_api_keys(agent_id);
CREATE INDEX IF NOT EXISTS agent_api_keys_company_id_idx ON agent_api_keys(company_id);

-- ---------------------------------------------------------------------------
-- CLI auth challenges (device authorization flow)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS cli_auth_challenges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    company_id UUID REFERENCES companies(id) ON DELETE SET NULL,
    challenge_code TEXT NOT NULL,
    device_name TEXT,
    requested_access JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL DEFAULT 'pending',
    approved_at TIMESTAMPTZ,
    approved_by_user_id UUID,
    api_key_id UUID REFERENCES board_api_keys(id) ON DELETE SET NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS cli_auth_challenges_code_idx ON cli_auth_challenges(challenge_code);
CREATE INDEX IF NOT EXISTS cli_auth_challenges_user_id_idx ON cli_auth_challenges(user_id);

-- ---------------------------------------------------------------------------
-- Instance user roles (cross-company global roles, e.g. instance_admin)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS instance_user_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    granted_by_user_id UUID,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS instance_user_roles_user_id_idx ON instance_user_roles(user_id);
CREATE UNIQUE INDEX IF NOT EXISTS instance_user_roles_user_role_idx
    ON instance_user_roles(user_id, role);

-- ---------------------------------------------------------------------------
-- Company memberships
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS company_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    principal_type TEXT NOT NULL,
    principal_id UUID NOT NULL,
    role TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS company_memberships_company_idx ON company_memberships(company_id);
CREATE UNIQUE INDEX IF NOT EXISTS company_memberships_principal_idx
    ON company_memberships(company_id, principal_type, principal_id);

-- ---------------------------------------------------------------------------
-- Principal permission grants (explicit fine-grained grants)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS principal_permission_grants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    principal_type TEXT NOT NULL,
    principal_id UUID NOT NULL,
    permission_key TEXT NOT NULL,
    scope JSONB NOT NULL DEFAULT '{}'::jsonb,
    granted_by_user_id UUID NOT NULL,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS ppg_company_principal_idx
    ON principal_permission_grants(company_id, principal_type, principal_id);
CREATE INDEX IF NOT EXISTS ppg_permission_key_idx
    ON principal_permission_grants(permission_key);

-- ---------------------------------------------------------------------------
-- Invites
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS invites (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    invite_type TEXT NOT NULL,
    invited_by_user_id UUID,
    email TEXT,
    token TEXT NOT NULL,
    allowed_join_types TEXT NOT NULL DEFAULT 'both',
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS invites_token_idx ON invites(token);
CREATE INDEX IF NOT EXISTS invites_company_id_idx ON invites(company_id);

-- ---------------------------------------------------------------------------
-- Join requests
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS join_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    principal_type TEXT NOT NULL,
    principal_id UUID NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending_approval',
    requested_role TEXT NOT NULL,
    message TEXT,
    reviewed_by_user_id UUID,
    reviewed_at TIMESTAMPTZ,
    rejection_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS join_requests_company_idx ON join_requests(company_id);
CREATE INDEX IF NOT EXISTS join_requests_principal_idx
    ON join_requests(company_id, principal_type, principal_id);
