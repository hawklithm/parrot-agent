-- Create pipeline_cases table
CREATE TABLE pipeline_cases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    pipeline_id UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    stage_id UUID NOT NULL REFERENCES pipeline_stages(id) ON DELETE RESTRICT,
    case_key VARCHAR(50) NOT NULL,
    title VARCHAR(500) NOT NULL,
    summary TEXT,
    fields JSONB NOT NULL DEFAULT '{}',
    terminal_kind terminal_kind,
    version INTEGER NOT NULL DEFAULT 1,
    pending_suggestion JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_case_key_per_company UNIQUE (company_id, case_key)
);

-- Create case_events table for event sourcing
CREATE TABLE case_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES pipeline_cases(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    payload JSONB NOT NULL,
    actor_type VARCHAR(50),
    actor_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for pipeline_cases
CREATE INDEX idx_pipeline_cases_company_id ON pipeline_cases(company_id);
CREATE INDEX idx_pipeline_cases_pipeline_stage ON pipeline_cases(pipeline_id, stage_id);
CREATE INDEX idx_pipeline_cases_terminal_kind ON pipeline_cases(terminal_kind) WHERE terminal_kind IS NOT NULL;
CREATE INDEX idx_pipeline_cases_case_key ON pipeline_cases(case_key);

-- Create indexes for case_events
CREATE INDEX idx_case_events_case_id ON case_events(case_id, created_at DESC);
CREATE INDEX idx_case_events_event_type ON case_events(event_type);

-- Create trigger for updated_at
CREATE TRIGGER update_pipeline_cases_updated_at
    BEFORE UPDATE ON pipeline_cases
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
