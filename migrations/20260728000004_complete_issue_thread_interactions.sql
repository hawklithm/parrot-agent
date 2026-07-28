ALTER TABLE issue_thread_interactions
    ADD COLUMN IF NOT EXISTS question TEXT,
    ADD COLUMN IF NOT EXISTS response JSONB,
    ADD COLUMN IF NOT EXISTS resolved_by_type TEXT,
    ADD COLUMN IF NOT EXISTS resolved_by_id TEXT;
