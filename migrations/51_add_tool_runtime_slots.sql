-- Paperclip parity: tool_runtime_slots
-- Baseline: packages/db/src/schema/tool_access.ts (toolRuntimeSlots)
CREATE TABLE IF NOT EXISTS tool_runtime_slots (
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
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, slot_key)
);

CREATE INDEX IF NOT EXISTS tool_runtime_slots_company_idx ON tool_runtime_slots(company_id);
CREATE INDEX IF NOT EXISTS tool_runtime_slots_connection_idx ON tool_runtime_slots(connection_id);
CREATE INDEX IF NOT EXISTS tool_runtime_slots_execution_workspace_idx ON tool_runtime_slots(company_id, execution_workspace_id);
