-- Execution workspaces (paperclip-aligned). Mirrors paperclip 0035_marvelous_satana.sql
-- (execution_workspaces table only; work products / issues columns omitted as
-- they are out of scope for parrot-agent).

CREATE TABLE IF NOT EXISTS execution_workspaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE NO ACTION,
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
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS execution_workspaces_company_project_status_idx
    ON execution_workspaces(company_id, project_id, status);
CREATE INDEX IF NOT EXISTS execution_workspaces_company_project_workspace_status_idx
    ON execution_workspaces(company_id, project_workspace_id, status);
CREATE INDEX IF NOT EXISTS execution_workspaces_company_source_issue_idx
    ON execution_workspaces(company_id, source_issue_id);
CREATE INDEX IF NOT EXISTS execution_workspaces_company_last_used_idx
    ON execution_workspaces(company_id, last_used_at);
CREATE INDEX IF NOT EXISTS execution_workspaces_company_branch_idx
    ON execution_workspaces(company_id, branch_name);
