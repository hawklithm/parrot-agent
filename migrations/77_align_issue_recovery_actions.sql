-- Paperclip canonical issue recovery action schema.
-- The historical recovery_actions table is intentionally removed from the
-- fresh schema; current code must use source/recovery issue semantics.
BEGIN;

CREATE TABLE IF NOT EXISTS issue_recovery_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    source_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    recovery_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    kind TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    owner_type TEXT NOT NULL DEFAULT 'agent',
    owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    owner_user_id TEXT,
    previous_owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    return_owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    cause TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    evidence JSONB NOT NULL DEFAULT '{}',
    next_action TEXT NOT NULL,
    wake_policy JSONB,
    monitor_policy JSONB,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_attempts INTEGER CHECK (max_attempts IS NULL OR max_attempts > 0),
    timeout_at TIMESTAMPTZ,
    last_attempt_at TIMESTAMPTZ,
    outcome TEXT,
    resolution_note TEXT,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS issue_recovery_actions_company_source_status_idx
    ON issue_recovery_actions (company_id, source_issue_id, status);

CREATE INDEX IF NOT EXISTS issue_recovery_actions_company_owner_status_idx
    ON issue_recovery_actions (company_id, owner_agent_id, status);

CREATE INDEX IF NOT EXISTS issue_recovery_actions_company_recovery_issue_idx
    ON issue_recovery_actions (company_id, recovery_issue_id);

CREATE UNIQUE INDEX IF NOT EXISTS issue_recovery_actions_active_source_uq
    ON issue_recovery_actions (company_id, source_issue_id)
    WHERE status IN ('active', 'escalated');

CREATE UNIQUE INDEX IF NOT EXISTS issue_recovery_actions_active_fingerprint_uq
    ON issue_recovery_actions (company_id, source_issue_id, cause, fingerprint)
    WHERE status IN ('active', 'escalated');

DROP TABLE IF EXISTS recovery_actions;

COMMIT;
