-- Migration: Create Execution Workspaces Table
-- Description: Execution workspace management for agent runs

-- Execution Workspaces Table
CREATE TABLE execution_workspaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    project_id UUID,
    project_workspace_id UUID,
    source_issue_id UUID,
    name TEXT NOT NULL,
    mode TEXT NOT NULL DEFAULT 'ephemeral',
    strategy_type TEXT NOT NULL DEFAULT 'git_worktree',
    status TEXT NOT NULL DEFAULT 'provisioning',
    cwd TEXT,
    provider_ref TEXT,
    base_ref TEXT,
    branch_name TEXT,
    repo_url TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX execution_workspaces_company_id_idx ON execution_workspaces(company_id);
CREATE INDEX execution_workspaces_project_id_idx ON execution_workspaces(project_id);
CREATE INDEX execution_workspaces_source_issue_id_idx ON execution_workspaces(source_issue_id);
CREATE INDEX execution_workspaces_status_idx ON execution_workspaces(status);
CREATE INDEX execution_workspaces_mode_idx ON execution_workspaces(mode);
CREATE INDEX execution_workspaces_created_at_idx ON execution_workspaces(created_at DESC);

COMMENT ON TABLE execution_workspaces IS 'Execution workspace instances for agent runs';
COMMENT ON COLUMN execution_workspaces.mode IS 'ephemeral: temporary, cleaned up after use; persistent: long-lived';
COMMENT ON COLUMN execution_workspaces.strategy_type IS 'git_worktree: git worktree isolation; shared_clone: shared with branch switching; isolated: fully isolated clone';
COMMENT ON COLUMN execution_workspaces.status IS 'provisioning: being created; ready: available; running: in use; teardown: cleanup; error: failed; archived: soft deleted';
COMMENT ON COLUMN execution_workspaces.cwd IS 'Current working directory within the workspace';
COMMENT ON COLUMN execution_workspaces.provider_ref IS 'Provider-specific reference (e.g., container ID, VM ID)';
COMMENT ON COLUMN execution_workspaces.base_ref IS 'Git base reference (branch or commit)';
COMMENT ON COLUMN execution_workspaces.branch_name IS 'Workspace-specific branch name';
COMMENT ON COLUMN execution_workspaces.repo_url IS 'Repository URL';

-- Add foreign key to environment_leases for workspace tracking
ALTER TABLE environment_leases
    ADD COLUMN execution_workspace_id UUID REFERENCES execution_workspaces(id) ON DELETE SET NULL;

CREATE INDEX environment_leases_execution_workspace_id_idx ON environment_leases(execution_workspace_id);

COMMENT ON COLUMN environment_leases.execution_workspace_id IS 'Associated execution workspace for this lease';
