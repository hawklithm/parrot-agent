-- Migration: Add Issue Execution System
-- Description: Adds issue plan decomposition, execution workspaces, and runtime leases
-- Date: 2026-08-18
-- Tables: issue_plan_decompositions, execution_workspaces, execution_workspace_runtime_leases

-- ============================================================================
-- Issue Plan Decomposition
-- ============================================================================

-- Issue plan decompositions (breaking down large issues into subtasks)
CREATE TABLE issue_plan_decompositions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    accepted_plan_revision_id UUID NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
    accepted_interaction_id UUID REFERENCES issue_thread_interactions(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'in_flight',
    request_fingerprint TEXT NOT NULL,
    requested_child_count INTEGER NOT NULL DEFAULT 0,
    requested_children JSONB NOT NULL DEFAULT '[]',
    child_issue_ids JSONB NOT NULL DEFAULT '[]',
    owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    owner_user_id TEXT,
    owner_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT issue_plan_decompositions_status_check CHECK (status IN ('in_flight', 'completed', 'cancelled', 'failed'))
);

CREATE INDEX issue_plan_decompositions_company_source_status_idx ON issue_plan_decompositions(company_id, source_issue_id, status);
CREATE INDEX issue_plan_decompositions_active_owner_idx ON issue_plan_decompositions(company_id, owner_agent_id) WHERE status = 'in_flight';
CREATE UNIQUE INDEX issue_plan_decompositions_source_revision_uq ON issue_plan_decompositions(company_id, source_issue_id, accepted_plan_revision_id);

-- ============================================================================
-- Execution Workspaces
-- ============================================================================

-- Execution workspaces (isolated environments for agent task execution)
CREATE TABLE execution_workspaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uui    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    project_workspace_id UUID REFERENCES project_workspaces(id) ON DELETE SET NULL,
    source_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    mode TEXT NOT NULL,
    strategy_type TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    cwd TEXT,
    repo_url TEXT,
    base_ref TEXT,
    branch_name TEXT,
    provider_type TEXT NOT NULL DEFAULT 'local_fs',
    provider_ref TEXT,
    derived_from_execution_workspace_id UUID REFERENCES execution_workspaces(id) ON DELETE SET NULL,
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    opened_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at TIMESTAMPTZ,
    cleanup_eligible_at TIMESTAMPTZ,
    cleanup_reason TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT execution_workspaces_mode_check CHECK (mode IN ('isolated', 'shared', 'ephemeral', 'persistent')),
    CONSTRAINT execution_workspaces_strategy_type_check CHECK (strategy_type IN ('fork', 'clone', 'snapshot', 'overlay', 'in_place')),
    CONSTRAINT execution_workspaces_status_check CHECK (status IN ('active', 'paused', 'closing', 'closed', 'failed')),
    CONSTRAINT execution_workspaces_provider_type_check CHECK (provider_type IN ('local_fs', 'git', 's3', 'ephemeral')),
    CONSTRAINT execution_workspaces_cleanup_check CHECK (
        (closed_at IS NULL AND cleanup_eligible_at IS NULL AND cleanup_reason IS NULL)
        OR (closed_at IS NOT NULL)
    )
);

CREATE INDEX execution_workspaces_company_project_status_idx ON execution_workspaces(company_id, project_id, status);
CREATE INDEX execution_workspaces_company_project_workspace_status_idx ON execution_workspaces(company_id, project_workspace_id, status);
CREATE INDEX execution_workspaces_company_source_issue_idx ON execution_workspaces(company_id, source_issue_id);
CREATE INDEX execution_workspaces_company_last_used_idx ON execution_workspaces(company_id, last_used_at);
CREATE INDEX execution_workspaces_company_branch_idx ON execution_workspaces(company_id, branch_name);

-- ============================================================================
-- Execution Workspace Runtime Leases
-- ============================================================================

-- Execution workspace runtime leases (exclusivity locks for concurrent execution safety)
-- One durable row per workspace. The unique constraint on execution_workspace_id
-- provides atomicity: concurrent claims from different processes serialize on it.
CREATE TABLE execution_workspace_runtime_leases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    execution_workspace_id UUID NOT NULL UNIQUE REFERENCES execution_workspaces(id) ON DELETE CASCADE,
    -- Durable owner identity (issue:<uuid> or run:<uuid>)
    -- Kept as text so the lease still identifies its owner after FK columns are nulled
    owner_key TEXT NOT NULL,
    owner_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    owner_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    last_action TEXT NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    renewed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT execution_workspace_runtime_leases_owner_key_format_check CHECK (
        owner_key ~ '^(issue|run):[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    ),
    CONSTRAINT execution_workspace_runtime_leases_expires_check CHECK (expires_at > claimed_at)
);

CREATE INDEX execution_workspace_runtime_leases_company_workspace_idx ON execution_workspace_runtime_leases(company_id, execution_workspace_id);
CREATE INDEX execution_workspace_runtime_leases_company_owner_idx ON execution_workspace_runtime_leases(company_id, owner_key);
CREATE INDEX execution_workspace_runtime_leases_expires_at_idx ON execution_workspace_runtime_leases(expires_at);

-- ============================================================================
-- Triggers
-- ============================================================================

CREATE TRIGGER update_issue_plan_decompositions_updated_at BEFORE UPDATE ON issue_plan_decompositions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_execution_workspaces_updated_at BEFORE UPDATE ON execution_workspaces
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_execution_workspace_runtime_leases_updated_at BEFORE UPDATE ON execution_workspace_runtime_leases
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON TABLE issue_plan_decompositions IS 'Tracks decomposition of large issues into child subtasks';
COMMENT ON TABLE execution_workspaces IS 'Isolated execution environments for agent task execution';
COMMENT ON TABLE execution_workspace_runtime_leases IS 'Exclusivity locks preventing concurrent workspace access conflicts';

COMMENT ON COLUMN issue_plan_decompositions.request_fingerprint IS 'Hash of decomposition request to detect duplicates';
COMMENT ON COLUMN issue_plan_decompositions.requested_children IS 'Requested child issue specifications';
COMMENT ON COLUMN issue_plan_decompositions.child_issue_ids IS 'Created child issue UUIDs';

COMMENT ON COLUMN execution_workspaces.mode IS 'Workspace isolation level: isolated/shared/ephemeral/persistent';
COMMENT ON COLUMN execution_workspaces.strategy_type IS 'Workspace creation strategy: fork/clone/snapshot/overlay/in_place';
COMMENT ON COLUMN execution_workspaces.provider_type IS 'Storage backend: local_fs/git/s3/ephemeral';
COMMENT ON COLUMN execution_workspaces.derived_from_execution_workspace_id IS 'Parent workspace if this is a derived workspace';

COMMENT ON COLUMN execution_workspace_runtime_leases.owner_key IS 'Durable owner identity (issue:<uuid> or run:<uuid>), survives FK deletion';
COMMENT ON COLUMN execution_workspace_runtime_leases.last_action IS 'Most recent action taken by lease holder';
COMMENT ON COLUMN execution_workspace_runtime_leases.expires_at IS 'Lease expiration time, must be renewed before expiry';
