-- Create project_goals association table
-- Migrated from paperclip: packages/db/src/schema/project_goals.ts

CREATE TABLE project_goals (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    goal_id UUID NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (project_id, goal_id)
);

-- Create indexes for efficient queries
CREATE INDEX idx_project_goals_project_id ON project_goals(project_id);
CREATE INDEX idx_project_goals_goal_id ON project_goals(goal_id);
CREATE INDEX idx_project_goals_company_id ON project_goals(company_id);

-- Create trigger for updated_at
CREATE TRIGGER update_project_goals_updated_at
    BEFORE UPDATE ON project_goals
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Add comment
COMMENT ON TABLE project_goals IS 'Many-to-many relationship between projects and goals';
