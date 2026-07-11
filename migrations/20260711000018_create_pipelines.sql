-- Create enum types
CREATE TYPE pipeline_stage_kind AS ENUM ('open', 'working', 'review', 'done', 'cancelled');
CREATE TYPE terminal_kind AS ENUM ('done', 'cancelled');

-- Create pipelines table
CREATE TABLE pipelines (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    key VARCHAR(100) NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    enforce_transitions BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_pipeline_key_per_company UNIQUE (company_id, key)
);

-- Create indexes
CREATE INDEX idx_pipelines_company_id ON pipelines(company_id);
CREATE INDEX idx_pipelines_project_id ON pipelines(project_id) WHERE project_id IS NOT NULL;
CREATE INDEX idx_pipelines_key ON pipelines(key);

-- Create trigger for updated_at
CREATE TRIGGER update_pipelines_updated_at
    BEFORE UPDATE ON pipelines
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
