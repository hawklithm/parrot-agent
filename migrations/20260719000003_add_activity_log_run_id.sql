-- Link activity entries to heartbeat runs so costs without a direct project can
-- inherit the project of the issue that was worked on by that run.
ALTER TABLE activity_logs
    ADD COLUMN IF NOT EXISTS run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_activity_logs_run_id ON activity_logs(run_id)
    WHERE run_id IS NOT NULL;
