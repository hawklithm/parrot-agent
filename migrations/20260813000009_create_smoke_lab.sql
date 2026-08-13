-- 对齐 Paperclip smoke-lab 域（smoke_runs + smoke_run_steps）
CREATE TABLE IF NOT EXISTS smoke_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    trigger TEXT NOT NULL DEFAULT 'manual',
    status TEXT NOT NULL DEFAULT 'running',   -- running|passed|failed|cancelled
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    summary JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_smoke_runs_company ON smoke_runs(company_id, started_at);

CREATE TABLE IF NOT EXISTS smoke_run_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES smoke_runs(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    scenario_step TEXT NOT NULL,
    status TEXT NOT NULL,                    -- passed|failed|skipped|running
    detail TEXT,
    screenshot_artifact_ref JSONB,
    duration_ms INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_smoke_run_steps_company_run ON smoke_run_steps(company_id, run_id);
