-- Create pipeline_stages table
CREATE TABLE pipeline_stages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    key VARCHAR(100) NOT NULL,
    name VARCHAR(255) NOT NULL,
    kind pipeline_stage_kind NOT NULL,
    position INTEGER NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_stage_key_per_pipeline UNIQUE (pipeline_id, key),
    CONSTRAINT unique_stage_position_per_pipeline UNIQUE (pipeline_id, position)
);

-- Create indexes
CREATE INDEX idx_pipeline_stages_pipeline_id ON pipeline_stages(pipeline_id);
CREATE INDEX idx_pipeline_stages_position ON pipeline_stages(pipeline_id, position);
CREATE INDEX idx_pipeline_stages_kind ON pipeline_stages(kind);

-- Create trigger for updated_at
CREATE TRIGGER update_pipeline_stages_updated_at
    BEFORE UPDATE ON pipeline_stages
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
