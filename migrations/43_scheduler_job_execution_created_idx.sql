-- Support global scheduler execution history reads and retention cleanup.
CREATE INDEX IF NOT EXISTS scheduler_job_executions_created_idx
    ON scheduler_job_executions(created_at ASC);
