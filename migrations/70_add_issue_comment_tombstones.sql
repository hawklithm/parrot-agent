-- Keep deleted issue comments as redacted tombstones so references and audit
-- history retain a stable comment id without exposing the original content.
ALTER TABLE issue_comments
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS deleted_by_type TEXT,
    ADD COLUMN IF NOT EXISTS deleted_by_agent_id UUID,
    ADD COLUMN IF NOT EXISTS deleted_by_user_id TEXT,
    ADD COLUMN IF NOT EXISTS deleted_by_run_id UUID;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'issue_comments_deleted_by_agent_id_fkey'
    ) THEN
        ALTER TABLE issue_comments
            ADD CONSTRAINT issue_comments_deleted_by_agent_id_fkey
            FOREIGN KEY (deleted_by_agent_id) REFERENCES agents(id) ON DELETE SET NULL;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'issue_comments_deleted_by_run_id_fkey'
    ) THEN
        ALTER TABLE issue_comments
            ADD CONSTRAINT issue_comments_deleted_by_run_id_fkey
            FOREIGN KEY (deleted_by_run_id) REFERENCES heartbeat_runs(id) ON DELETE SET NULL;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS issue_comments_issue_created_idx
    ON issue_comments(issue_id, created_at ASC, id ASC);

