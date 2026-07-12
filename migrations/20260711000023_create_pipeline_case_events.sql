-- Create pipeline_case_events table (event sourcing for pipeline cases)
CREATE TABLE pipeline_case_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES pipeline_cases(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    actor_type VARCHAR(50),
    actor_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pipeline_case_events_case_id ON pipeline_case_events(case_id, created_at DESC);
CREATE INDEX idx_pipeline_case_events_event_type ON pipeline_case_events(event_type);
