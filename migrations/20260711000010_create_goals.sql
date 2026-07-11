-- Create enum types for goals
CREATE TYPE goal_level AS ENUM ('company', 'project', 'task');
CREATE TYPE goal_status AS ENUM ('planned', 'active', 'completed', 'archived');

-- Create goals table
CREATE TABLE goals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    level goal_level NOT NULL,
    status goal_status NOT NULL DEFAULT 'planned',
    parent_id UUID REFERENCES goals(id) ON DELETE SET NULL,
    owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for goals
CREATE INDEX idx_goals_company_id ON goals(company_id);
CREATE INDEX idx_goals_parent_id ON goals(parent_id) WHERE parent_id IS NOT NULL;
CREATE INDEX idx_goals_owner_agent_id ON goals(owner_agent_id) WHERE owner_agent_id IS NOT NULL;
CREATE INDEX idx_goals_level_status ON goals(level, status);
CREATE INDEX idx_goals_created_at ON goals(created_at DESC);

-- Create trigger for updated_at
CREATE TRIGGER update_goals_updated_at
    BEFORE UPDATE ON goals
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
