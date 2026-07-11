-- Create enum types for runs
CREATE TYPE run_source AS ENUM ('schedule', 'manual', 'webhook', 'api');
CREATE TYPE run_status AS ENUM ('received', 'queued', 'dispatched', 'coalesced', 'skipped', 'succeeded', 'failed');

-- Create routine_runs table
CREATE TABLE routine_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    routine_id UUID NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    trigger_id UUID REFERENCES routine_triggers(id) ON DELETE SET NULL,
    source run_source NOT NULL,
    status run_status NOT NULL DEFAULT 'received',
    triggered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    routine_revision_id UUID REFERENCES routine_revisions(id) ON DELETE SET NULL,
    idempotency_key VARCHAR(255),
    trigger_payload JSONB,
    dispatch_fingerprint VARCHAR(255),
    linked_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    coalesced_into_run_id UUID REFERENCES routine_runs(id) ON DELETE SET NULL,
    failure_reason TEXT,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_trigger_idempotency UNIQUE (trigger_id, idempotency_key)
);

-- Create indexes for routine_runs
CREATE INDEX idx_routine_runs_routine_id ON routine_runs(routine_id, created_at DESC);
CREATE INDEX idx_routine_runs_company_id ON routine_runs(company_id);
CREATE INDEX idx_routine_runs_status ON routine_runs(status);
CREATE INDEX idx_routine_runs_trigger_id ON routine_runs(trigger_id) WHERE trigger_id IS NOT NULL;
CREATE INDEX idx_routine_runs_linked_issue_id ON routine_runs(linked_issue_id) WHERE linked_issue_id IS NOT NULL;
CREATE INDEX idx_routine_runs_dispatch_fingerprint ON routine_runs(dispatch_fingerprint) WHERE dispatch_fingerprint IS NOT NULL;

-- Create trigger for updated_at
CREATE TRIGGER update_routine_runs_updated_at
    BEFORE UPDATE ON routine_runs
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
