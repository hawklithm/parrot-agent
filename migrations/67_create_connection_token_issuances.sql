-- Durable audit records for Agent connection-token broker decisions.
-- Tokens themselves are never stored; only a SHA-256 reference may be kept
-- when a future exchange path mints a short-lived upstream token.

CREATE TABLE IF NOT EXISTS connection_token_issuances (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    application_id UUID REFERENCES tool_applications(id) ON DELETE SET NULL,
    connection_id UUID NOT NULL REFERENCES tool_connections(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    responsible_user_id TEXT,
    path TEXT NOT NULL,
    requested_scope JSONB NOT NULL DEFAULT '[]',
    issued_scope JSONB NOT NULL DEFAULT '[]',
    ttl_seconds INTEGER,
    expires_at TIMESTAMPTZ,
    token_hash TEXT,
    outcome TEXT NOT NULL,
    error_code TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT connection_token_issuances_path_check
        CHECK (path IN ('exchange', 'oauth_access', 'static')),
    CONSTRAINT connection_token_issuances_outcome_check
        CHECK (outcome IN ('success', 'denied', 'rate_limited', 'use_env_lease', 'upstream_error', 'failure')),
    CONSTRAINT connection_token_issuances_ttl_bounds
        CHECK (ttl_seconds IS NULL OR (ttl_seconds >= 1 AND ttl_seconds <= 900)),
    CONSTRAINT connection_token_issuances_token_hash_format
        CHECK (token_hash IS NULL OR token_hash ~ '^[a-f0-9]{64}$')
);

CREATE INDEX IF NOT EXISTS connection_token_issuances_company_created_idx
    ON connection_token_issuances(company_id, created_at);
CREATE INDEX IF NOT EXISTS connection_token_issuances_connection_created_idx
    ON connection_token_issuances(company_id, connection_id, created_at);
CREATE INDEX IF NOT EXISTS connection_token_issuances_agent_connection_idx
    ON connection_token_issuances(company_id, agent_id, connection_id, created_at);
CREATE INDEX IF NOT EXISTS connection_token_issuances_run_idx
    ON connection_token_issuances(company_id, run_id);
