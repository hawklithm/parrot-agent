-- Create enum types for routines
CREATE TYPE routine_status AS ENUM ('active', 'paused', 'draft');
CREATE TYPE concurrency_policy AS ENUM ('coalesce_if_active', 'parallel', 'skip_if_active');
CREATE TYPE catch_up_policy AS ENUM ('run_missed', 'skip_missed');

-- Create routines table
CREATE TABLE routines (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    goal_id UUID REFERENCES goals(id) ON DELETE SET NULL,
    parent_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    assignee_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    priority INTEGER NOT NULL DEFAULT 50,
    status routine_status NOT NULL DEFAULT 'draft',
    concurrency_policy concurrency_policy NOT NULL DEFAULT 'coalesce_if_active',
    catch_up_policy catch_up_policy NOT NULL DEFAULT 'skip_missed',
    variables JSONB NOT NULL DEFAULT '[]',
    env JSONB NOT NULL DEFAULT '{}',
    latest_revision_id UUID,
    latest_revision_number INTEGER NOT NULL DEFAULT 0,
    responsible_user_id UUID,
    last_triggered_at TIMESTAMPTZ,
    last_enqueued_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for routines
CREATE INDEX idx_routines_company_id ON routines(company_id);
CREATE INDEX idx_routines_project_id ON routines(project_id) WHERE project_id IS NOT NULL;
CREATE INDEX idx_routines_goal_id ON routines(goal_id) WHERE goal_id IS NOT NULL;
CREATE INDEX idx_routines_assignee_agent_id ON routines(assignee_agent_id);
CREATE INDEX idx_routines_status ON routines(status);
CREATE INDEX idx_routines_created_at ON routines(created_at DESC);

-- Create trigger for updated_at
CREATE TRIGGER update_routines_updated_at
    BEFORE UPDATE ON routines
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
