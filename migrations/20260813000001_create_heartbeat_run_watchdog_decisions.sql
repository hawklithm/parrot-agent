-- Heartbeat-run watchdog decisions (Paperclip parity:
-- POST/GET /heartbeat-runs/:runId/watchdog-decisions).
-- Replaces the previous stub that always returned an empty list.

CREATE TABLE IF NOT EXISTS heartbeat_run_watchdog_decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES heartbeat_runs(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    decision TEXT NOT NULL
        CHECK (decision IN ('snooze', 'continue', 'dismissed_false_positive')),
    evaluation_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    reason TEXT,
    snoozed_until TIMESTAMPTZ,
    created_by_type TEXT,
    created_by_id UUID,
    created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS heartbeat_run_watchdog_decisions_run_idx
    ON heartbeat_run_watchdog_decisions(run_id);
CREATE INDEX IF NOT EXISTS heartbeat_run_watchdog_decisions_company_idx
    ON heartbeat_run_watchdog_decisions(company_id);
