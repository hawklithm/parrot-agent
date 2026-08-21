-- Durable scheduler execution history for restart-safe observability.
CREATE TABLE IF NOT EXISTS scheduler_job_executions (
    id UUID PRIMARY KEY,
    job_name TEXT NOT NULL,
    owner_id UUID NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    status TEXT NOT NULL CHECK (status IN ('idle', 'running', 'succeeded', 'failed', 'disabled')),
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS scheduler_job_executions_job_created_idx
    ON scheduler_job_executions(job_name, created_at DESC);
