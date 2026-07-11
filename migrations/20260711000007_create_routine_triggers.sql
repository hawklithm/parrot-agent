-- Create enum types for triggers
CREATE TYPE trigger_kind AS ENUM ('schedule', 'webhook', 'manual');

-- Create routine_triggers table
CREATE TABLE routine_triggers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    routine_id UUID NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    kind trigger_kind NOT NULL,
    label VARCHAR(255),
    enabled BOOLEAN NOT NULL DEFAULT true,
    cron_expression VARCHAR(255),
    timezone VARCHAR(100),
    next_run_at TIMESTAMPTZ,
    last_fired_at TIMESTAMPTZ,
    public_id VARCHAR(100) UNIQUE,
    secret_id VARCHAR(100),
    signing_mode VARCHAR(50),
    replay_window_sec INTEGER,
    last_rotated_at TIMESTAMPTZ,
    last_result JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for routine_triggers
CREATE INDEX idx_routine_triggers_routine_id ON routine_triggers(routine_id);
CREATE INDEX idx_routine_triggers_company_id ON routine_triggers(company_id);
CREATE INDEX idx_routine_triggers_next_run_at ON routine_triggers(next_run_at) WHERE enabled = true AND kind = 'schedule';
CREATE INDEX idx_routine_triggers_public_id ON routine_triggers(public_id) WHERE public_id IS NOT NULL;

-- Create trigger for updated_at
CREATE TRIGGER update_routine_triggers_updated_at
    BEFORE UPDATE ON routine_triggers
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
