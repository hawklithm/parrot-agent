-- Issue comment attribution: acting on behalf of a user and derived
-- non-human author attribution.
--
-- Paperclip's `issueComments` distinguishes *who wrote a comment* from *who the
-- comment was written for*:
--
--   * `on_behalf_of_user_id` — an agent comment posted on behalf of a
--     responsible user, resolved from an explicit request value or derived from
--     the creating heartbeat run's responsible user.
--   * `derived_author_agent_id` / `derived_created_by_run_id` /
--     `derived_author_source` — best-effort attribution for comments whose
--     stored author is a non-human sentinel (e.g. `local-board`). Paperclip
--     backfills this once and derives lazily on read so the load path does not
--     re-scan run logs.
--
-- Parrot persisted neither, so agent comments could not be attributed to a
-- responsible user and sentinel-authored comments had no recoverable author.

-- Paperclip stores user ids as text (matching `heartbeat_runs.responsible_user_id`),
-- so the on-behalf-of reference is text rather than uuid.
ALTER TABLE issue_comments
    ADD COLUMN IF NOT EXISTS on_behalf_of_user_id TEXT;

ALTER TABLE issue_comments
    ADD COLUMN IF NOT EXISTS derived_author_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

ALTER TABLE issue_comments
    ADD COLUMN IF NOT EXISTS derived_created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL;

ALTER TABLE issue_comments
    ADD COLUMN IF NOT EXISTS derived_author_source TEXT;

ALTER TABLE issue_comments
    ADD COLUMN IF NOT EXISTS source_trust JSONB;

ALTER TABLE issue_comments
    ADD COLUMN IF NOT EXISTS author_type TEXT;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'issue_comments_derived_author_source_check'
    ) THEN
        ALTER TABLE issue_comments
            ADD CONSTRAINT issue_comments_derived_author_source_check
            CHECK (derived_author_source IS NULL
                   OR derived_author_source IN ('run', 'log_scan', 'best_effort'));
    END IF;
END
$$;

-- Backfill `author_type` from the legacy `actor_type` enum so readers can rely
-- on the Paperclip field name without a second source of truth.
UPDATE issue_comments
   SET author_type = actor_type::text
 WHERE author_type IS NULL;

-- Backfill `on_behalf_of_user_id` for agent comments from the creating run.
-- Parrot stores the creating run in `actor_run_id` and has no
-- `created_by_run_id`/`author_user_id` columns, so the backfill joins on the
-- legacy column and the actor type.
UPDATE issue_comments c
   SET on_behalf_of_user_id = r.responsible_user_id
  FROM heartbeat_runs r
 WHERE c.on_behalf_of_user_id IS NULL
   AND c.actor_run_id = r.id
   AND c.actor_type = 'agent'
   AND r.responsible_user_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS issue_comments_company_issue_created_idx
    ON issue_comments (company_id, issue_id, created_at);

CREATE INDEX IF NOT EXISTS issue_comments_on_behalf_of_user_idx
    ON issue_comments (company_id, on_behalf_of_user_id)
    WHERE on_behalf_of_user_id IS NOT NULL;
