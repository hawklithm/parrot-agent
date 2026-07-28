-- Paperclip-compatible tool invocation audit persistence.
-- The execution gateway can populate these tables; the run-detail API reads
-- them to expose decisions associated with a heartbeat run.

CREATE TABLE IF NOT EXISTS tool_invocations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    idempotency_key TEXT,
    actor_type TEXT NOT NULL DEFAULT 'system',
    actor_id TEXT,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    application_id UUID,
    connection_id UUID,
    catalog_entry_id UUID,
    tool_name TEXT NOT NULL,
    arguments_hash TEXT,
    arguments_summary JSONB,
    policy_decision TEXT,
    matched_policy_ids JSONB NOT NULL DEFAULT '[]',
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
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS tool_invocations_company_created_idx
    ON tool_invocations(company_id, created_at);
CREATE INDEX IF NOT EXISTS tool_invocations_run_idx
    ON tool_invocations(company_id, run_id);

CREATE TABLE IF NOT EXISTS tool_action_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    invocation_id UUID NOT NULL REFERENCES tool_invocations(id) ON DELETE CASCADE,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    interaction_id UUID,
    approval_id UUID,
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
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS tool_action_requests_invocation_idx
    ON tool_action_requests(invocation_id);

CREATE TABLE IF NOT EXISTS tool_call_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    actor_type TEXT NOT NULL DEFAULT 'system',
    actor_id TEXT,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    application_id UUID,
    connection_id UUID,
    catalog_entry_id UUID,
    invocation_id UUID REFERENCES tool_invocations(id) ON DELETE SET NULL,
    action_request_id UUID REFERENCES tool_action_requests(id) ON DELETE SET NULL,
    runtime_slot_id UUID,
    tool_name TEXT,
    decision TEXT,
    matched_policy_ids JSONB NOT NULL DEFAULT '[]',
    reason_code TEXT,
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
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS tool_call_events_run_idx
    ON tool_call_events(company_id, run_id);
CREATE INDEX IF NOT EXISTS tool_call_events_invocation_idx
    ON tool_call_events(invocation_id);

CREATE TABLE IF NOT EXISTS tool_gateway_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES heartbeat_runs(id) ON DELETE CASCADE,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    project_id UUID,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS tool_gateway_sessions_run_idx
    ON tool_gateway_sessions(company_id, run_id);
