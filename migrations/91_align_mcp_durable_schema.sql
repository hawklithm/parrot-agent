-- Paperclip parity: keep the durable MCP gateway/session/invocation contract
-- available to Parrot even when an installation was bootstrapped from the
-- original compact tool-access schema.
--
-- The columns are intentionally additive and nullable where older runtime
-- records cannot provide the value. Existing Parrot routes continue to use
-- the compact columns; new gateway/audit code can populate the richer
-- Paperclip context without a second compatibility table.

ALTER TABLE tool_mcp_gateways
    ADD COLUMN IF NOT EXISTS display_slug TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS default_profile_mode TEXT NOT NULL DEFAULT 'gateway_only',
    ADD COLUMN IF NOT EXISTS context_scope_type TEXT NOT NULL DEFAULT 'none',
    ADD COLUMN IF NOT EXISTS context_scope_id TEXT,
    ADD COLUMN IF NOT EXISTS project_id UUID,
    ADD COLUMN IF NOT EXISTS approval_issue_id UUID,
    ADD COLUMN IF NOT EXISTS auth_config JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS header_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS metadata_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS on_demand_tools_config JSONB NOT NULL DEFAULT '{"enabled":false,"searchToolName":"search_tools","runToolName":"run_tool"}'::jsonb,
    ADD COLUMN IF NOT EXISTS created_by_agent_id UUID,
    ADD COLUMN IF NOT EXISTS created_by_user_id TEXT,
    ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ;

UPDATE tool_mcp_gateways
   SET display_slug = slug
 WHERE display_slug = '';

ALTER TABLE tool_mcp_gateway_tokens
    ADD COLUMN IF NOT EXISTS subject_type TEXT NOT NULL DEFAULT 'gateway_client',
    ADD COLUMN IF NOT EXISTS subject_id TEXT,
    ADD COLUMN IF NOT EXISTS client_label TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS owner_note TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS expiry_override_reason TEXT,
    ADD COLUMN IF NOT EXISTS expiry_override_by_user_id TEXT,
    ADD COLUMN IF NOT EXISTS expiry_override_by_agent_id UUID,
    ADD COLUMN IF NOT EXISTS expiry_override_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS created_by_agent_id UUID,
    ADD COLUMN IF NOT EXISTS created_by_user_id TEXT;

ALTER TABLE tool_gateway_sessions
    ADD COLUMN IF NOT EXISTS gateway_id UUID,
    ADD COLUMN IF NOT EXISTS gateway_token_id UUID,
    ADD COLUMN IF NOT EXISTS gateway_public_id TEXT,
    ADD COLUMN IF NOT EXISTS client_subject_type TEXT,
    ADD COLUMN IF NOT EXISTS client_subject_id TEXT,
    ADD COLUMN IF NOT EXISTS client_name TEXT,
    ADD COLUMN IF NOT EXISTS mcp_session_id TEXT,
    ADD COLUMN IF NOT EXISTS correlation_id TEXT;

ALTER TABLE tool_invocations
    ADD COLUMN IF NOT EXISTS gateway_id UUID,
    ADD COLUMN IF NOT EXISTS gateway_token_id UUID,
    ADD COLUMN IF NOT EXISTS gateway_public_id TEXT,
    ADD COLUMN IF NOT EXISTS client_subject_type TEXT,
    ADD COLUMN IF NOT EXISTS client_subject_id TEXT,
    ADD COLUMN IF NOT EXISTS client_name TEXT,
    ADD COLUMN IF NOT EXISTS mcp_session_id TEXT,
    ADD COLUMN IF NOT EXISTS correlation_id TEXT,
    ADD COLUMN IF NOT EXISTS catalog_version_hash TEXT,
    ADD COLUMN IF NOT EXISTS catalog_schema_hash TEXT,
    ADD COLUMN IF NOT EXISTS provider_type TEXT,
    ADD COLUMN IF NOT EXISTS application_key TEXT,
    ADD COLUMN IF NOT EXISTS upstream_tool_name TEXT,
    ADD COLUMN IF NOT EXISTS risk_level TEXT,
    ADD COLUMN IF NOT EXISTS policy_explanation TEXT,
    ADD COLUMN IF NOT EXISTS credential_scope_summary JSONB,
    ADD COLUMN IF NOT EXISTS header_policy_summary JSONB;

ALTER TABLE tool_call_events
    ADD COLUMN IF NOT EXISTS gateway_id UUID,
    ADD COLUMN IF NOT EXISTS gateway_token_id UUID,
    ADD COLUMN IF NOT EXISTS gateway_public_id TEXT,
    ADD COLUMN IF NOT EXISTS client_subject_type TEXT,
    ADD COLUMN IF NOT EXISTS client_subject_id TEXT,
    ADD COLUMN IF NOT EXISTS client_name TEXT,
    ADD COLUMN IF NOT EXISTS mcp_session_id TEXT,
    ADD COLUMN IF NOT EXISTS correlation_id TEXT,
    ADD COLUMN IF NOT EXISTS catalog_version_hash TEXT,
    ADD COLUMN IF NOT EXISTS catalog_schema_hash TEXT,
    ADD COLUMN IF NOT EXISTS provider_type TEXT,
    ADD COLUMN IF NOT EXISTS application_key TEXT,
    ADD COLUMN IF NOT EXISTS upstream_tool_name TEXT,
    ADD COLUMN IF NOT EXISTS risk_level TEXT,
    ADD COLUMN IF NOT EXISTS policy_explanation TEXT,
    ADD COLUMN IF NOT EXISTS credential_scope_summary JSONB,
    ADD COLUMN IF NOT EXISTS header_policy_summary JSONB;

CREATE INDEX IF NOT EXISTS tool_mcp_gateways_company_display_slug_idx
    ON tool_mcp_gateways(company_id, display_slug);
CREATE INDEX IF NOT EXISTS tool_mcp_gateway_tokens_subject_idx
    ON tool_mcp_gateway_tokens(company_id, subject_type, subject_id);
CREATE INDEX IF NOT EXISTS tool_gateway_sessions_gateway_idx
    ON tool_gateway_sessions(company_id, gateway_id, created_at DESC);
CREATE INDEX IF NOT EXISTS tool_gateway_sessions_correlation_idx
    ON tool_gateway_sessions(company_id, correlation_id);
CREATE INDEX IF NOT EXISTS tool_invocations_gateway_created_idx
    ON tool_invocations(company_id, gateway_id, created_at DESC);
CREATE INDEX IF NOT EXISTS tool_call_events_gateway_created_idx
    ON tool_call_events(company_id, gateway_id, created_at DESC);
