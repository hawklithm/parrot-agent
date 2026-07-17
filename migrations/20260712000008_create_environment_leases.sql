-- Environment leases (paperclip-aligned). Mirrors paperclip 0065_environments.sql
-- (environment_leases table). Maps to parrot-agent RuntimeLease model.

CREATE TABLE IF NOT EXISTS environment_leases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    execution_workspace_id UUID REFERENCES execution_workspaces(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    heartbeat_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'active',
    lease_policy TEXT NOT NULL DEFAULT 'ephemeral',
    provider TEXT,
    provider_lease_id TEXT,
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ,
    released_at TIMESTAMPTZ,
    failure_reason TEXT,
    cleanup_status TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS environment_leases_company_environment_status_idx
    ON environment_leases(company_id, environment_id, status);
CREATE INDEX IF NOT EXISTS environment_leases_company_execution_workspace_idx
    ON environment_leases(company_id, execution_workspace_id);
CREATE INDEX IF NOT EXISTS environment_leases_company_issue_idx
    ON environment_leases(company_id, issue_id);
CREATE INDEX IF NOT EXISTS environment_leases_heartbeat_run_idx
    ON environment_leases(heartbeat_run_id);
CREATE INDEX IF NOT EXISTS environment_leases_company_last_used_idx
    ON environment_leases(company_id, last_used_at);
CREATE INDEX IF NOT EXISTS environment_leases_provider_lease_idx
    ON environment_leases(provider_lease_id);
