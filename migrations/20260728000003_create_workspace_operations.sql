CREATE TABLE IF NOT EXISTS workspace_operations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    execution_workspace_id UUID REFERENCES execution_workspaces(id) ON DELETE SET NULL,
    heartbeat_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    phase TEXT NOT NULL,
    command TEXT,
    cwd TEXT,
    status TEXT NOT NULL DEFAULT 'running',
    exit_code INTEGER,
    log_store TEXT,
    log_ref TEXT,
    log_bytes BIGINT,
    log_sha256 TEXT,
    log_compressed BOOLEAN NOT NULL DEFAULT FALSE,
    stdout_excerpt TEXT,
    stderr_excerpt TEXT,
    metadata JSONB,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS workspace_operations_company_run_started_idx ON workspace_operations(company_id, heartbeat_run_id, started_at);
CREATE INDEX IF NOT EXISTS workspace_operations_company_workspace_started_idx ON workspace_operations(company_id, execution_workspace_id, started_at);
