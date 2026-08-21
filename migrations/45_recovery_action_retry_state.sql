ALTER TABLE recovery_actions
    ADD COLUMN IF NOT EXISTS retry_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS next_retry_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS last_attempt_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_error TEXT;

ALTER TABLE recovery_actions
    DROP CONSTRAINT IF EXISTS recovery_actions_retry_count_check;

ALTER TABLE recovery_actions
    ADD CONSTRAINT recovery_actions_retry_count_check CHECK (retry_count >= 0);

CREATE INDEX IF NOT EXISTS recovery_actions_retry_due_idx
    ON recovery_actions (next_retry_at, status)
    WHERE status IN ('pending', 'in_progress');
