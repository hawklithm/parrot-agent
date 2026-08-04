-- Extend issue thread interactions to the Paperclip MCP interaction contract.
-- This migration is additive and keeps the existing question/approval/review rows valid.
ALTER TABLE issue_thread_interactions
    ADD COLUMN IF NOT EXISTS payload JSONB,
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT,
    ADD COLUMN IF NOT EXISTS continuation_policy TEXT NOT NULL DEFAULT 'wake_assignee',
    ADD COLUMN IF NOT EXISTS source_comment_id UUID;

ALTER TABLE issue_thread_interactions
    DROP CONSTRAINT IF EXISTS valid_interaction_kind;

ALTER TABLE issue_thread_interactions
    ADD CONSTRAINT valid_interaction_kind CHECK (
        kind IN (
            'question', 'approval', 'review',
            'suggest_tasks', 'ask_user_questions',
            'request_confirmation', 'request_checkbox_confirmation'
        )
    );

CREATE UNIQUE INDEX IF NOT EXISTS issue_thread_interactions_idempotency_idx
    ON issue_thread_interactions (issue_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
