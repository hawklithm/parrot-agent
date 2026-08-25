-- Migration: Pipeline runs / triggers / logs / case outputs tables
--
-- Closes the P16–P27 stub surface (PAPERCLIP_MIGRATION_PLAN §4B.3 line 342):
-- pipeline runs (Automation Retry / cancel / delete), triggers, run logs and
-- case outputs were registered as routes but backed by no tables — handlers
-- returned hardcoded empty/fake payloads, and pipeline_case_outputs (the table
-- PipelineCaseOutputsService already targets) was never migrated, so
-- GET /cases/:id/outputs failed with 42P01 at runtime.

-- pipeline_runs: one row per pipeline execution. retry_of_run_id links an
-- automation-retry run to the run it retries (mirrors heartbeat_runs semantics).
CREATE TABLE IF NOT EXISTS pipeline_runs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id      UUID NOT NULL REFERENCES companies (id) ON DELETE CASCADE,
    pipeline_id     UUID NOT NULL REFERENCES pipelines (id) ON DELETE CASCADE,
    stage_id        UUID REFERENCES pipeline_stages (id) ON DELETE SET NULL,
    case_id         UUID REFERENCES pipeline_cases (id) ON DELETE SET NULL,
    status          TEXT NOT NULL DEFAULT 'queued',
    attempt         INTEGER NOT NULL DEFAULT 1,
    retry_of_run_id UUID REFERENCES pipeline_runs (id) ON DELETE SET NULL,
    trigger_type    TEXT,
    trigger_detail  TEXT,
    error           TEXT,
    started_at      TIMESTAMPTZ,
    finished_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS pipeline_runs_pipeline_idx ON pipeline_runs (pipeline_id, created_at DESC);
CREATE INDEX IF NOT EXISTS pipeline_runs_status_idx ON pipeline_runs (status);

-- pipeline_triggers: declarative pipeline triggers (schedule/event/webhook).
CREATE TABLE IF NOT EXISTS pipeline_triggers (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id    UUID NOT NULL REFERENCES companies (id) ON DELETE CASCADE,
    pipeline_id   UUID NOT NULL REFERENCES pipelines (id) ON DELETE CASCADE,
    trigger_type  TEXT NOT NULL,
    config        JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS pipeline_triggers_pipeline_idx ON pipeline_triggers (pipeline_id);

-- pipeline_logs: append-only execution log lines for a pipeline run.
CREATE TABLE IF NOT EXISTS pipeline_logs (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies (id) ON DELETE CASCADE,
    run_id     UUID REFERENCES pipeline_runs (id) ON DELETE CASCADE,
    level      TEXT NOT NULL DEFAULT 'info',
    message    TEXT NOT NULL,
    metadata   JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS pipeline_logs_run_idx ON pipeline_logs (run_id, created_at DESC);

-- pipeline_case_outputs: case output ledger (PipelineCaseOutputsService target).
CREATE TABLE IF NOT EXISTS pipeline_case_outputs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id     UUID NOT NULL REFERENCES pipeline_cases (id) ON DELETE CASCADE,
    output_type TEXT NOT NULL,
    content     JSONB NOT NULL DEFAULT '{}'::jsonb,
    metadata    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS pipeline_case_outputs_case_idx ON pipeline_case_outputs (case_id, created_at DESC);
