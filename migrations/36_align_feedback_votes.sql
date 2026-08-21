-- Align issue feedback votes with Paperclip's target/author upsert contract.
-- Keep the legacy voter columns for existing Parrot callers during migration.
ALTER TABLE feedback_votes
    ADD COLUMN IF NOT EXISTS target_type TEXT,
    ADD COLUMN IF NOT EXISTS target_id TEXT,
    ADD COLUMN IF NOT EXISTS author_user_id TEXT;

UPDATE feedback_votes
SET target_type = COALESCE(target_type, 'issue'),
    target_id = COALESCE(target_id, issue_id::text),
    author_user_id = COALESCE(author_user_id, voter_id::text);

ALTER TABLE feedback_votes
    ALTER COLUMN target_type SET NOT NULL,
    ALTER COLUMN target_id SET NOT NULL,
    ALTER COLUMN author_user_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS feedback_votes_company_issue_idx
    ON feedback_votes(company_id, issue_id);
CREATE INDEX IF NOT EXISTS feedback_votes_issue_target_idx
    ON feedback_votes(issue_id, target_type, target_id);
CREATE UNIQUE INDEX IF NOT EXISTS feedback_votes_company_target_author_idx
    ON feedback_votes(company_id, target_type, target_id, author_user_id);
