-- Migration: Add Tool Access System - Remaining Tables (Phase 2-6)
-- Description: Adds 18 remaining tables for Tool Access System
-- Date: 2026-08-18
-- Tables: catalog, profiles, gateways, runtime, invocations, audit

-- ============================================================================
-- Phase 2: Tool Catalog & Profiles (6 tables)
-- ============================================================================

-- Tool catalog entries (discovered tools from connections)
CREATE TABLE tool_catalog_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    application_id UUID REFERENCES tool_applications(id) ON DELETE CASCADE,
    connection_id UUID NOT NULL REFERENCES tool_connections(id) ON DELETE CASCADE,
    entry_kind TEXT NOT NULL DEFAULT 'tool',
    name TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    title TEXT,
    description TEXT,
    input_schema JSONB NOT NULL DEFAULT '{}',
    output_schema JSONB,
    annotations JSONB NOT NULL DEFAULT '{}',
    risk_level TEXT NOT NULL DEFAULT 'read',
    is_read_only BOOLEAN NOT NULL DEFAULT true,
    is_write BOOLEAN NOT NULL DEFAULT false,
    is_destructive BOOLEAN NOT NULL DEFAULT false,
    status TEXT NOT NULL DEFAULT 'active',
    version TEXT,
    version_hash TEXT NOT NULL,
    schema_hash TEXT,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_at TIMESTAMPTZ,
    reviewed_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    reviewed_by_user_id TEXT,
    quarantined_at TIMESTAMPTZ,
    quarantine_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_catalog_entries_entry_kind_check CHECK (entry_kind IN ('tool', 'prompt', 'resource')),
    CONSTRAINT tool_catalog_entries_risk_level_check CHECK (risk_level IN ('read', 'write', 'destructive')),
    CONSTRAINT tool_catalog_entries_status_check CHECK (status IN ('active', 'deprecated', 'quarantined'))
);

CREATE INDEX tool_catalog_entries_company_idx ON tool_catalog_entries(company_id);
CREATE INDEX tool_catalog_entries_application_idx ON tool_catalog_entries(application_id);
CREATE INDEX tool_catalog_entries_connection_idx ON tool_catalog_entries(connection_id);
CREATE INDEX tool_catalog_entries_company_status_idx ON tool_catalog_entries(company_id, status);
CREATE UNIQUE INDEX tool_catalog_entries_connection_name_uq ON tool_catalog_entries(connection_id, name);

-- Tool profiles (permission sets)
CREATE TABLE tool_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    profile_key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    default_action TEXT NOT NULLeny',
    new_tools_reviewed_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_profiles_status_check CHECK (status IN ('active', 'archived')),
    CONSTRAINT tool_profiles_default_action_check CHECK (default_action IN ('allow', 'deny', 'ask'))
);

CREATE INDEX tool_profiles_company_status_idx ON tool_profiles(company_id, status);
CREATE UNIQUE INDEX tool_profiles_company_key_uq ON tool_profiles(company_id, profile_key);
CREATE UNIQUE INDEX tool_profiles_company_name_uq ON tool_profiles(company_id, name);

-- Tool profile entries (rules within a profile)
CREATE TABLE tool_profile_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    profile_id UUID NOT NULL REFERENCES tool_profiles(id) ON DELETE CASCADE,
    selector_type TEXT NOT NULL,
    effect TEXT NOT NULL DEFAULT 'include',
    application_id UUID REFERENCES tool_applications(id) ON DELETE CASCADE,
    connection_id UUID REFERENCES tool_connections(id) ON DELETE CASCADE,
    catalog_entry_id UUID REFERENCES tool_catalog_entries(id) ON DELETE CASCADE,
    tool_name TEXT,
    risk_level TEXT,
    conditions JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_profile_entries_selector_type_check CHECK (selector_type IN ('application', 'connection', 'catalog_entry', 'tool_name', 'risk_level', 'wildcard')),
    CONSTRAINT tool_profile_entries_effect_check CHECK (effect IN ('include', 'exclude')),
    CONSTRAINT tool_profile_entries_risk_level_check CHECK (risk_level IS NULL OR risk_level IN ('read', 'write', 'destructive'))
);

CREATE INDEX tool_profile_entries_company_profile_idx ON tool_profile_entries(company_id, profile_id);
CREATE INDEX tool_profile_entries_application_idx ON tool_profile_entries(company_id, application_id);
CREATE INDEX tool_profile_entries_connection_idx ON tool_profile_entries(company_id, connection_id);
CREATE INDEX tool_profile_entries_catalog_entry_idx ON tool_profile_entries(company_id, catalog_entry_id);

-- Tool profile bindings (assign profiles to aspaces)
CREATE TABLE tool_profile_bindings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    profile_id UUID NOT NULL REFERENCES tool_profiles(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 100,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT toolbindings_target_type_check CHECK (target_type IN ('company', 'agent', 'project', 'issue'))
);

CREATE INDEX tool_profile_bindings_company_target_idx ON tool_profile_bindings(company_id, target_type, target_id);
CREATE UNIQUE INDEX tool_profile_bindings_target_profile_uq ON tool_profile_bindings(company_id, target_type, target_id, profile_id);

-- Tool policies (rate limits, authorization, etc.)
CREATE TABLE tool_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    policy_typNOT NULL,
    priority INTEGER NOT NULL DEFAULT 100,
    enabled BOOLEAN NOT NULL DEFAULT true,
    selectors JSONB NOT NULL DEFAULT '{}',
    conditions JSONB,
    config JSONB,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_policies_policy_type_check CHECK (policy_type IN ('rate_limit', 'authorization', 'redaction', 'approval_required', 'budget'))
);

CREATE INDEX tool_policies_company_enabled_idx ON tool_policies(company_id, enabled);
CREATE INDEX tool_policies_company_type_idx ON tool_policies(company_id, policy_type);
CREATE UNIQUE INDEX tool_policies_company_name_uq ON tool_policies(company_id, name);

-- Stdio command templates
CREATE TABLE tool_stdio_command_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    template_key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    command TEXT NOT NULL,
    args JSONB NOT NULL DEFAULT '[]',
    env_keys JSONB NOT NULL DEFAULT '[]',
    tools JSONB NOT NULL DEFAULT '[]',
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    disabled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_stdio_command_templates_status_check CHECK (status IN ('active', 'disabled'))
);

CREATE INDEX tool_stdio_command_templates_company_idx ON tool_stdio_command_templates(company_id);
CREATE INDEX tool_stdio_command_templates_company_status_idx ON tool_stdio_command_templates(company_id, status);
CREATE UNIQUE INDEX tool_stdio_command_templates_company_key_uq ON tool_stdio_command_templates(company_id, template_key);

-- ============================================================================
-- Phase 3: MCP Gateways (2 tables)
-- ============================================================================

-- MCP gateway configurations
CREATE TABLE tool_mcp_gateways (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    gateway_public_id TEXT NOT NULL DEFAULT 'gw_' || replace(gen_random_uuid()::text, '-', ''),
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    display_slug TEXT NOT NULL DEFAULT '',
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    profile_id UUID NOT NULL REFERENCES tool_profiles(id) ON DELETE RESTRICT,
    default_profile_mode TEXT NOT NULL DEFAULT 'gateway_only',
    context_scope_type TEXT NOT NULL DEFAULT 'none',
    context_scope_id TEXT,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    approval_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    auth_config JSONB NOT NULL DEFAULT '{"version":1,"bearer":{"enabled":true,"tokenPrefix":"pcgw","defaultTtlSeconds":7776000,"requireFiniteExpiry":true,"longLivedTokenRequiresOverride":true},"oauth":{"enabled":false,"reservedFor":"v1_5","dynamicClientRegistration":false,"authorizationCodePkce":false}}',
    header_policy JSONB NOT NULL DEFAULT '{"version":1,"callerPassthrough":{"enabled":false,"allowedHeaders":[]},"staticHeaders":[],"generatedMetadata":{"enabled":false,"allowedHeaders":[]},"responseHeaders":{"forwardMcpRequiredHeaders":true,"forwardSafeCacheHeaders":true}}',
    metadata_policy JSONB NOT NULL DEFAULT '{"version":1,"forwardCompanyId":false,"forwardGatewayId":false,"forwardProjectId":false,"forwardIssueId":false,"forwardAgentId":false,"forwardRunId":false,"forwardCorrelationId":true}',
    on_demand_tools_config JSONB NOT NULL DEFAULT '{"enabled":false,"searchToolName":"search_tools","runToolName":"run_tool"}',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_mcp_gatewaystatus_check CHECK (status IN ('active', 'disabled', 'archived')),
    CONSTRAINT tool_mcp_gateways_default_profile_mode_check CHECK (default_profile_mode IN ('gateway_only', 'gateway_plus_agent', 'agent_only')),
    CONSTRAINT tool_mcp_gateways_context_scope_type_check CHECK (context_scope_type IN ('none', 'agent', 'project', 'issue'))
);

CREATE INDEX tool_mcp_gateways_company_idx ON tool_mcp_gateways(company_id);
CREATE INDEX tool_mcp_gateways_company_status_idx ON tool_mcp_gateways(company_id, status);
CREATE INDEX tool_mcp_gateways_profile_idx ON tool_mcp_gateways(company_id, profile_id);
CREATE UNIQUE INDEX tool_mcp_gateways_public_id_uq ON tool_mcp_gateways(gateway_public_id);
CREATE UNIQUE INDEX tool_mcp_gateways_company_slug_uq ON tool_mcp_gateways(company_id, slug);
CREATE UNIQUE INDEX tool_mcp_gateways_company_name_uq ON tool_mcp_gateways(company_id, name);

-- MCP gateway tokens (access tokens for gateways)
CREATE TABLE tool_mcp_gateway_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    gateway_id UUID NOT NULL REFERENCES tool_mcp_gateways(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    token_prefix TEXT NOT NULL DEFAULT '',
    subject_type TEXT NOT NULL DEFAULT 'gateway_client',
    subject_id TEXT,
    client_label TEXT NOT NULL DEFAULT '',
    owner_note TEXT NOT NULL DEFAULT '',
    allowed_actions JSONB NOT NULL DEFAULT '["tools/list", "tools/call"]',
    expires_at TIMESTAMPTZ,
    expiry_override_reason TEXT,
    expiry_override_by_user_id TEXT,
    expiry_override_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    expiry_override_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_mcp_gateway_tokens_subject_type_check CHECK (subject_type IN ('gateway_client', 'agent', 'human'))
);

CREATE UNIQUE INDEX tool_mcp_gateway_tokens_token_hash_uq ON tool_mcp_gateway_tokens(token_hash);
CREATE INDEX tool_mcp_gateway_tokens_gateway_idx ON tool_mcp_gateway_tokens(company_id, gateway_id);
CREATE INDEX tool_mcp_gateway_tokens_subject_idx ON tool_mcp_gateway_tokens(company_id, subject_type, subject_id);
CREATE INDEX tool_mcp_gateway_tokens_company_expires_idx ON tool_mcp_gateway_tokens(company_id, expires_at);

-- ============================================================================
-- Phase 4: Runtime Management (3 tables)
-- ============================================================================

-- Tool runtime slots (running tool processes)
CREATE TABLE tool_runtime_slots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    application_id UUID REFERENCES tool_applications(id) ON DELETE SET NULL,
    connection_id UUID REFERENCES tool_connections(id) ON DELETE CASCADE,
    project_workspace_id UUID REFERENCES project_workspaces(id) ON DELETE SET NULL,
    execution_workspace_id UUID REFERENCES execution_workspaces(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    owner_scope_type TEXT NOT NULL DEFAULT 'connection',
    owner_scope_id TEXT,
    runtime_kind TEXT NOT NULL DEFAULT 'local_stdio',
    slot_key TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'stopped',
    reuse_key TEXT,
    workspace_scope TEXT,
    credential_scope_hash TEXT,
    provider TEXT,
    provider_ref TEXT,
    process_id INTEGER,
    command_template_key TEXT,
    health_status TEXT NOT NULL DEFAULT 'unchecked',
    health_message TEXT,
    last_health_check_at TIMESTAMPTZ,
    last_started_at TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    stopped_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    idle_expires_at TIMESTAMPTZ,
    idle_deadline_at TIMESTAMPTZ,
    last_error TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_runtime_slots_runtime_kind_check CHECK (runtime_kind IN ('local_stdio', 'remote_mcp', 'rest_api')),
    CONSTRAINT tool_runtime_slots_status_check CHECK (status IN ('starting', 'running', 'stopping', 'stopped', 'error')),
    CONSTRAINT tool_runtime_slots_health_status_check CHECK (health_status IN ('healthy', 'degraded', 'unhealthy', 'unchecked'))
);

CREATE INDEX tool_runtime_slots_company_idx ON tool_runtime_slots(company_id);
CREATE INDEX tool_runtime_slots_connection_idx ON tool_runtime_slots(connection_id);
CREATE INDEX tool_runtime_slots_execution_workspace_idx ON tool_runtime_slots(company_id, execution_workspace_id);
CREATE UNIQUE INDEX tool_runtime_slots_slot_key_uq ON tool_runtime_slots(company_id, slot_key);

-- Gateway sessions (active MCP gateway connections)
CREATE TABLE tool_gateway_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES heartbeat_runs(id) ON DELETE CASCADE,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    gateway_id UUID REFERENCES tool_mcp_gateways(id) ON DELETE SET NULL,
    gateway_token_id UUID REFERENCES tool_mcp_gateway_tokens(id) ON DELETE SET NULL,
    gateway_public_id TEXT,
    client_subject_type TEXT,
    client_subject_id TEXT,
    client_name TEXT,
    mcp_session_id TEXT,
    correlation_id TEXT,
    token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_gateway_sessions_client_subject_type_check CHECK (client_subject_type IS NULL OR client_subject_type IN ('gateway_client', 'agent', 'human'))
);

CREATE UNIQUE INDEX tool_gateway_sessions_token_hash_uq ON tool_gateway_sessions(token_hash);
CREATE INDEX tool_gateway_sessions_company_agent_idx ON tool_gateway_sessions(company_id, agent_id);
CREATE INDEX tool_gateway_sessions_company_expires_idx ON tool_gateway_sessions(company_id, expires_at);
CREATE INDEX tool_gateway_sessions_run_idx ON tool_gateway_sessions(company_id, run_id);
CREATE INDEX tool_gateway_sessions_issue_idx ON tool_gateway_sessions(company_id, issue_id);
CREATE INDEX tool_gateway_sessions_gateway_idx ON tool_gateway_sessions(company_id, gateway_id);

-- Gateway rate limit counters
CREATE TABLE tool_gateway_rate_limit_counters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    counter_key TEXT NOT NULL,
    window_start_at TIMESTAMPTZ NOT NULL,
    window_ms INTEGER NOT NULL,
    limit_value INTEGER NOT NULL,
    count INTEGER NOT NULL DEFAULT 0,
    reset_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX tool_gateway_rate_limit_counters_company_idx ON tool_gateway_rate_limit_counters(company_id);
CREATE UNIQUE INDEX tool_gateway_rate_limit_counters_window_uq ON tool_gateway_rate_limit_counters(company_id, counter_key, window_start_at);

-- ============================================================================
-- Phase 5: Invocation Tracking (5 tables)
-- ============================================================================

-- Tool invocations (call records)
CREATE TABLE tool_invocations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    idempotency_key TEXT,
    actor_type TEXT NOT NULL DEFAULT 'system',
    actor_id TEXT,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    gateway_id UUID REFERENCES tool_mcp_gateways(id) ON DELETE SET NULL,
    gateway_token_id UUID REFERENCES tool_mcp_gateway_tokens(id) ON DELETE SET NULL,
    gateway_public_id TEXT,
    client_subject_type TEXT,
    client_subject_id TEXT,
    client_name TEXT,
    mcp_session_id TEXT,
    correlation_id TEXT,
    application_id UUID REFERENCES tool_applications(id) ON DELETE SET NULL,
    connection_id UUID REFERENCES tool_connections(id) ON DELETE SET NULL,
    catalog_entry_id UUID REFERENCES tool_catalog_entries(id) ON DELETE SET NULL,
    catalog_version_hash TEXT,
    catalog_schema_hash TEXT,
    provider_type TEXT,
    application_key TEXT,
    upstream_tool_name TEXT,
    risk_level TEXT,
    tool_name TEXT NOT NULL,
    arguments_hash TEXT,
    arguments_summary JSONB,
    policy_decision TEXT,
    matched_policy_ids JSONB NOT NULL DEFAULT '[]',
    policy_explanation JSONB,
    credential_scope_summary JSONB,
    header_policy_summary JSONB,
    approval_state TEXT NOT NULL DEFAULT 'not_required',
    status TEXT NOT NULL DEFAULT 'pending',
    upstream_request_id TEXT,
    result_hash TEXT,
    result_summary JSONB,
    result_size_bytes INTEGER,
    result_artifact_id UUID,
    error_code TEXT,
    error_message TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_invocations_risk_level_check CHECK (risk_level IS NULL OR risk_level IN ('read', 'write', 'destructive')),
    CONSTRAINT tool_invocations_policy_decision_check CHECK (policy_decision IS NULL OR policy_decision IN ('allow', 'deny', 'ask')),
    CONSTRAINT tool_invocations_approval_state_check CHECK (approval_state IN ('not_required', 'pending', 'approved', 'denied', 'expired')),
    CONSTRAINT tool_invocations_status_check CHECK (status IN ('pending', 'running', 'success', 'error', 'denied', 'timeout'))
);

CREATE INDEX tool_invocations_company_created_idx ON tool_invocations(company_id, created_at);
CREATE INDEX tool_invocations_run_idx ON tool_invocations(company_id, run_id);
CREATE INDEX tool_invocations_issue_idx ON tool_invocations(company_id, issue_id);
CREATE INDEX tool_invocations_gateway_idx ON tool_invocations(company_id, gateway_id);
CREATE UNIQUE INDEX tool_invocations_company_idempotency_uq ON tool_invocations(company_id, idempotency_key);

-- Tool action requests (approval requests for tool calls)
CREATE TABLE tool_action_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    invocation_id UUID NOT NULL REFERENCES tool_invocations(id) ON DELETE CASCADE,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    interaction_id UUID REFERENCES issue_thread_interactions(id) ON DELETE SET NULL,
    approval_id UUID REFERENCES approvals(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    canonical_arguments_hash TEXT NOT NULL,
    canonical_arguments_summary JSONB NOT NULL,
    signed_arguments TEXT,
    preview_markdown TEXT,
    requested_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    requested_by_user_id TEXT,
    resolved_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    resolved_by_user_id TEXT,
    decided_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    decided_by_user_id TEXT,
    decided_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_action_requests_status_check CHECK (status IN ('pending', 'approved', 'denied', 'expired', 'cancelled'))
);

CREATE INDEX tool_action_requests_company_status_idx ON tool_action_requests(company_id, status);
CREATE INDEX tool_action_requests_invocation_idx ON tool_action_requests(invocation_id);
CREATE INDEX tool_action_requests_issue_idx ON tool_action_requests(company_id, issue_id);

-- Tool call events (audit trail for tool calls)
CREATE TABLE tool_call_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    actor_type TEXT NOT NULL DEFAULT 'system',
    actor_id TEXT,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    gateway_id UUID REFERENCES tool_mcp_gateways(id) ON DELETE SET NULL,
    gateway_token_id UUID REFERENCES tool_mcp_gateway_tokens(id) ON DELETE SET NULL,
    gateway_public_id TEXT,
    client_subject_type TEXT,
    client_subject_id TEXT,
    client_name TEXT,
    mcp_session_id TEXT,
    correlation_id TEXT,
    application_id UUID REFERENCES tool_applications(id) ON DELETE SET NULL,
    connection_id UUID REFERENCES tool_connections(id) ON DELETE SET NULL,
    catalog_entry_id UUID REFERENCES tool_catalog_entries(id) ON DELETE SET NULL,
    invocation_id UUID REFERENCES tool_invocations(id) ON DELETE SET NULL,
    action_request_id UUID REFERENCES tool_action_requests(id) ON DELETE SET NULL,
    runtime_slot_id UUID REFERENCES tool_runtime_slots(id) ON DELETE SET NULL,
    tool_name TEXT,
    decision TEXT,
    matched_policy_ids JSONB NOT NULL DEFAULT '[]',
    reason_code TEXT,
    policy_explanation JSONB,
    credential_scope_summary JSONB,
    header_policy_summary JSONB,
    outcome TEXT NOT NULL DEFAULT 'pending',
    latency_ms INTEGER,
    arguments_summary JSONB,
    request_hash TEXT,
    request_summary JSONB,
    result_hash TEXT,
    result_summary JSONB,
    result_size_bytes INTEGER,
    redaction_plan JSONB,
    rate_limit_state JSONB,
    metadata JSONB,
    error_code TEXT,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_call_events_event_type_check CHECK (event_type IN ('policy_evaluated', 'tool_called', 'tool_completed', 'tool_failed', 'approval_requested', 'approval_decided')),
    CONSTRAINT tool_call_events_decision_check CHECK (decision IS NULL OR decision IN ('allow', 'deny', 'ask')),
    CONSTRAINT tool_call_events_outcome_check CHECK (outcome IN ('pending', 'success', 'error', 'denied', 'timeout'))
);

CREATE INDEX tool_call_events_company_created_idx ON tool_call_events(company_id, created_at);
CREATE INDEX tool_call_events_run_idx ON tool_call_events(company_id, run_id);
CREATE INDEX tool_call_events_issue_idx ON tool_call_events(company_id, issue_id);
CREATE INDEX tool_call_events_invocation_idx ON tool_call_events(invocation_id);
CREATE INDEX tool_call_events_gateway_idx ON tool_call_events(company_id, gateway_id);

-- Connection token issuances (OAuth token exchange tracking)
CREATE TABLE connection_token_issuances (
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
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT connection_token_issuances_path_check CHECK (path IN ('exchange', 'oauth_access', 'static')),
    CONSTRAINT connection_token_issuances_outcome_check CHECK (outcome IN ('success', 'denied', 'rate_limited', 'use_env_lease', 'upstream_error', 'failure')),
    CONSTRAINT connection_token_issuances_ttl_bounds CHECK (ttl_seconds IS NULL OR (ttl_seconds >= 1 AND ttl_seconds <= 900)),
    CONSTRAINT connection_token_issuances_token_hash_format CHECK (token_hash IS NULL OR token_hash ~ '^[a-f0-9]{64}$')
);

CREATE INDEX connection_token_issuances_company_created_idx ON connection_token_issuances(company_id, created_at);
CREATE INDEX connection_token_issuances_connection_created_idx ON connection_token_issuances(company_id, connection_id, created_at);
CREATE INDEX connection_token_issuances_agent_connection_idx ON connection_token_issuances(company_id, agent_id, connection_id, created_at);
CREATE INDEX connection_token_issuances_run_idx ON connection_token_issuances(company_id, run_id);

-- Tool rate limit counters (per-policy rate limiting)
CREATE TABLE tool_rate_limit_counters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    policy_id UUID NOT NULL REFERENCES tool_policies(id) ON DELETE CASCADE,
    counter_key TEXT NOT NULL,
    scope_type TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    window_kind TEXT NOT NULL,
    window_start_at TIMESTAMPTZ NOT NULL,
    limit_value INTEGER NOT NULL,
    remaining INTEGER NOT NULL,
    reset_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_rate_limit_counters_window_kind_check CHECK (window_kind IN ('sliding', 'fixed', 'daily', 'hourly'))
);

CREATE INDEX tool_rate_limit_counters_company_idx ON tool_rate_limit_counters(company_id);
CREATE UNIQUE INDEX tool_rate_limit_counters_window_uq ON tool_rate_limit_counters(company_id, policy_id, counter_key, window_kind, window_start_at);

-- ============================================================================
-- Phase 6: Metrics & Audit (2 tables)
-- ============================================================================

-- Tool runtime metric counters (aggregated metrics)
CREATE TABLE tool_runtime_metric_counters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    metric TEXT NOT NULL,
    bucket_start_at TIMESTAMPTZ NOT NULL,
    count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT tool_runtime_metric_counters_count_nonnegative CHECK (count >= 0)
);

CREATE INDEX tool_runtime_metric_counters_company_metric_idx ON tool_runtime_metric_counters(company_id, metric, bucket_start_at);
CREATE UNIQUE INDEX tool_runtime_metric_counters_bucket_uq ON tool_runtime_metric_counters(company_id, metric, bucket_start_at);

-- Tool access audit events (high-level audit trail)
CREATE TABLE tool_access_audit_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    gateway_id UUID REFERENCES tool_mcp_gateways(id) ON DELETE SET NULL,
    gateway_token_id UUID REFERENCES tool_mcp_gateway_tokens(id) ON DELETE SET NULL,
    gateway_public_id TEXT,
    client_name TEXT,
    correlation_id TEXT,
    connection_id UUID REFERENCES tool_connections(id) ON DELETE SET NULL,
    catalog_entry_id UUID REFERENCES tool_catalog_entries(id) ON DELETE SET NULL,
    actor_type TEXT NOT NULL DEFAULT 'system',
    actor_id TEXT,
    action TEXT NOT NULL,
    outcome TEXT NOT NULL,
    reason_code TEXT,
    details JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX tool_access_audit_company_created_idx ON tool_access_audit_events(company_id, created_at);
CREATE INDEX tool_access_audit_connection_idx ON tool_access_audit_events(connection_id);
CREATE INDEX tool_access_audit_gateway_idx ON tool_access_audit_events(company_id, gateway_id);

-- ============================================================================
-- Triggers
-- ============================================================================

-- Updated timestamp triggers
CREATE TRIGGER update_tool_catalog_entries_updated_at BEFORE UPDATE ON tool_catalog_entries
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_profiles_updated_at BEFORE UPDATE ON tool_profiles
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_profile_entries_updated_at BEFORE UPDATE ON tool_profile_entries
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_profile_bindings_updated_at BEFORE UPDATE ON tool_profile_bindings
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_mcp_gateways_updated_at BEFORE UPDATE ON tool_mcp_gateways
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_mcp_gateway_tokens_updated_at BEFORE UPDATE ON tool_mcp_gateway_tokens
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_policies_updated_at BEFORE UPDATE ON tool_policies
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_runtime_slots_updated_at BEFORE UPDATE ON tool_runtime_slots
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_stdio_command_templates_updated_at BEFORE UPDATE ON tool_stdio_command_templates
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_gateway_sessions_updated_at BEFORE UPDATE ON tool_gateway_sessions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_gateway_rate_limit_counters_updated_at BEFORE UPDATE ON tool_gateway_rate_limit_counters
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_invocations_updated_at BEFORE UPDATE ON tool_invocations
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_action_requests_updated_at BEFORE UPDATE ON tool_action_requests
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_rate_limit_counters_updated_at BEFORE UPDATE ON tool_rate_limit_counters
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tool_runtime_metric_counters_updated_at BEFORE UPDATE ON tool_runtime_metric_counters
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON TABLE tool_catalog_entries IS 'Discovered tools from MCP connections';
COMMENT ON TABLE tool_profiles IS 'Tool permission profiles (allow/deny sets)';
COMMENT ON TABLE tool_profile_entries IS 'Rules within a tool profile';
COMMENT ON TABLE tool_profile_bindings IS 'Profile assignments to agents/workspaces';
COMMENT ON TABLE tool_mcp_gateways IS 'MCP gateway configurations';
COMMENT ON TABLE tool_mcp_gateway_tokens IS 'Access tokens for MCP gateways';
COMMENT ON TABLE tool_policies IS 'Tool governance policies (rate limits, authorization)';
COMMENT ON TABLE tool_runtime_slots IS 'Running tool process slots';
COMMENT ON TABLE tool_stdio_command_templates IS 'Command templates for stdio tools';
COMMENT ON TABLE tool_gateway_sessions IS 'Active MCP gateway sessions';
COMMENT ON TABLE tool_gateway_rate_limit_counters IS 'Rate limiting for gateway requests';
COMMENT ON TABLE tool_invocations IS 'Tool call records with full metadata';
COMMENT ON TABLE tool_action_requests IS 'Approval requests for tool actions';
COMMENT ON TABLE tool_call_events IS 'Audit trail for all tool-related events';
COMMENT ON TABLE connection_token_issuances IS 'OAuth token exchange tracking';
COMMENT ON TABLE tool_rate_limit_counters IS 'Per-policy rate limit tracking';
COMMENT ON TABLE tool_runtime_metric_counters IS 'Aggregated runtime metrics';
COMMENT ON TABLE tool_access_audit_events IS 'High-level audit events for tool access';
