-- Create project status enum
CREATE TYPE project_status AS ENUM ('backlog', 'todo', 'in_progress', 'in_review', 'blocked', 'done');

-- Create execution workspace policy enum
CREATE TYPE execution_workspace_policy AS ENUM ('shared', 'isolated_per_issue', 'isolated_per_agent');

-- Create membership state enum
CREATE TYPE membership_state AS ENUM ('joined', 'left');

-- Create projects table
CREATE TABLE projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    goal_id UUID,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    status project_status NOT NULL DEFAULT 'backlog',
    lead_agent_id UUID,
    target_date TIMESTAMPTZ,
    color VARCHAR(7), -- hex color
    icon VARCHAR(50),
    env JSONB,
    pause_reason TEXT,
    paused_at TIMESTAMPTZ,
    execution_workspace_policy execution_workspace_policy NOT NULL DEFAULT 'shared',
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create project workspaces table
CREATE TABLE project_workspaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    is_primary BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(project_id, name)
);

-- Create project memberships table
CREATE TABLE project_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    state membership_state NOT NULL DEFAULT 'joined',
    starred_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, project_id, user_id)
);

-- Create agent memberships table
CREATE TABLE agent_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL,
    user_id UUID NOT NULL,
    state membership_state NOT NULL DEFAULT 'joined',
    starred_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, agent_id, user_id)
);

-- Create indexes
CREATE INDEX idx_projects_company_id ON projects(company_id);
CREATE INDEX idx_projects_status ON projects(status);
CREATE INDEX idx_projects_goal_id ON projects(goal_id);
CREATE INDEX idx_projects_lead_agent_id ON projects(lead_a
CREATE INDEX idx_project_workspaces_project_id ON project_workspaces(project_id);
CREATE INDEX idx_project_workspaces_is_primary ON project_workspaces(is_primary);

CREATE INDEX idx_project_memberships_company_id ON project_memberships(company_id);
CREATE INDEX idx_project_memberships_project_id ON project_memberships(project_id);
CREATE INDEX idx_project_memberships_user_id ON project_memberships(user_id);
CREATE INDEX idx_project_memberships_state ON project_memberships(state);
CREATE INDEX idx_project_memberships_starred ON project_memberships(starred_at) WHERE starred_at IS NOT NULL;

CREATE INDEX idx_agent_memberships_company_id ON agent_memberships(company_id);
CREATE INDEX idx_agent_memberships_agent_id ON agent_memberships(agent_id);
CREATE INDEX idx_agent_memberships_user_id ON agent_memberships(user_id);
CREATE INDEX idx_agent_memberships_state ON agent_memberships(state);
CREATE INDEX idx_agent_memberships_starred ON agent_memberships(starred_at) WHERE starred_at IS NOT NULL;

-- Apply updated_at triggers
CREATE TRIGGER update_projects_updated_at BEFORE UPDATE ON projects
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_project_workspaces_updated_at BEFORE UPDATE ON project_workspaces
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_project_memberships_updated_at BEFORE UPDATE ON project_memberships
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_agent_memberships_updated_at BEFORE UPDATE ON agent_memberships
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
