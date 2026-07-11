-- Create pipeline_transitions table
CREATE TABLE pipeline_transitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    from_stage_id UUID NOT NULL REFERENCES pipeline_stages(id) ON DELETE CASCADE,
    to_stage_id UUID NOT NULL REFERENCES pipeline_stages(id) ON DELETE CASCADE,
    label VARCHAR(255),
    conditions JSONB NOT NULL DEFAULT '{}',
    CONSTRAINT unique_pipeline_transition UNIQUE (pipeline_id, from_stage_id, to_stage_id)
);

-- Create indexes
CREATE INDEX idx_pipeline_transitions_pipeline_id ON pipeline_transitions(pipeline_id);
CREATE INDEX idx_pipeline_transitions_from_stage ON pipeline_transitions(from_stage_id);
CREATE INDEX idx_pipeline_transitions_to_stage ON pipeline_transitions(to_stage_id);
