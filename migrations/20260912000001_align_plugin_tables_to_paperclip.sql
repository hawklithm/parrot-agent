-- Align plugin_jobs / plugin_job_runs with the paperclip schema contract.
-- Strategy: keep parrot-owned columns (name, enabled, definition, result,
-- completed_at) and ADD the columns paperclip requires that parrot is missing,
-- so both code paths keep working and the tables satisfy the paperclip contract.

-- ---------------------------------------------------------------------------
-- plugin_jobs
-- ---------------------------------------------------------------------------
ALTER TABLE plugin_jobs ADD COLUMN IF NOT EXISTS status text NOT NULL DEFAULT 'active';
ALTER TABLE plugin_jobs ADD COLUMN IF NOT EXISTS last_run_at timestamptz;
ALTER TABLE plugin_jobs ADD COLUMN IF NOT EXISTS next_run_at timestamptz;
ALTER TABLE plugin_jobs ADD COLUMN IF NOT EXISTS created_at timestamptz NOT NULL DEFAULT now();
ALTER TABLE plugin_jobs ADD COLUMN IF NOT EXISTS updated_at timestamptz NOT NULL DEFAULT now();

-- paperclip requires schedule NOT NULL; table is currently empty so this is safe.
ALTER TABLE plugin_jobs ALTER COLUMN schedule SET NOT NULL;

CREATE INDEX IF NOT EXISTS plugin_jobs_plugin_idx ON plugin_jobs(plugin_id);
CREATE INDEX IF NOT EXISTS plugin_jobs_next_run_idx ON plugin_jobs(next_run_at);

-- ---------------------------------------------------------------------------
-- plugin_job_runs
-- ---------------------------------------------------------------------------
ALTER TABLE plugin_job_runs ADD COLUMN IF NOT EXISTS trigger text NOT NULL DEFAULT 'manual';
ALTER TABLE plugin_job_runs ADD COLUMN IF NOT EXISTS duration_ms integer;
ALTER TABLE plugin_job_runs ADD COLUMN IF NOT EXISTS error text;
ALTER TABLE plugin_job_runs ADD COLUMN IF NOT EXISTS logs jsonb NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE plugin_job_runs ADD COLUMN IF NOT EXISTS started_at timestamptz;
ALTER TABLE plugin_job_runs ADD COLUMN IF NOT EXISTS finished_at timestamptz;

CREATE INDEX IF NOT EXISTS plugin_job_runs_job_idx ON plugin_job_runs(job_id);
CREATE INDEX IF NOT EXISTS plugin_job_runs_status_idx ON plugin_job_runs(status);
