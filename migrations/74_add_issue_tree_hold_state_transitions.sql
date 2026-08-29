-- Issue tree control: hold state-transition persistence and Paperclip-aligned
-- lifecycle attribution.
--
-- Two defects are fixed here:
--
-- 1. `create_tree_hold` never persisted the issue status observed *before* the
--    hold was applied, so the release path could not restore members to their
--    pre-hold status; paused/cancelled issues were left in their held status
--    after release. `previous_status`/`restored_at` record the transition.
--
-- 2. The `IssueTreeHold` model declared `actor_agent_id`/`actor_user_id`, which
--    do not exist in the table DDL — every `query_as::<_, IssueTreeHold>` failed
--    at runtime with a decoding error. The canonical Paperclip attribution
--    columns (created/released actor type, agent, user, run, reason, metadata)
--    are added and backfilled from the legacy `actor_type`/`actor_id` pair.

ALTER TABLE issue_tree_hold_members
    ADD COLUMN IF NOT EXISTS previous_status TEXT;

ALTER TABLE issue_tree_hold_members
    ADD COLUMN IF NOT EXISTS restored_at TIMESTAMPTZ;

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS created_by_actor_type TEXT NOT NULL DEFAULT 'system';

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS created_by_user_id TEXT;

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL;

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS released_by_actor_type TEXT;

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS released_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS released_by_user_id TEXT;

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS released_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL;

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS release_reason TEXT;

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS release_metadata JSONB;

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

ALTER TABLE issue_tree_holds
    ADD COLUMN IF NOT EXISTS apply_error TEXT;

-- Backfill legacy attribution written by the pre-Paperclip hold creation path.
UPDATE issue_tree_holds
   SET created_by_actor_type = COALESCE(actor_type, 'system'),
       created_by_agent_id = CASE WHEN actor_type = 'agent' THEN actor_id ELSE NULL END,
       created_by_user_id = CASE WHEN actor_type = 'user' THEN actor_id::text ELSE NULL END
 WHERE created_by_actor_type = 'system'
   AND (actor_type IS NOT NULL OR actor_id IS NOT NULL);

UPDATE issue_tree_holds
   SET released_by_actor_type = COALESCE(released_by_type, released_by_actor_type),
       released_by_agent_id = CASE
           WHEN released_by_type = 'agent' THEN released_by_id ELSE released_by_agent_id
       END,
       released_by_user_id = CASE
           WHEN released_by_type = 'user' THEN released_by_id::text ELSE released_by_user_id
       END
 WHERE released_by_type IS NOT NULL;

CREATE INDEX IF NOT EXISTS issue_tree_hold_members_restore_idx
    ON issue_tree_hold_members (hold_id, skipped)
    WHERE skipped = false;

CREATE INDEX IF NOT EXISTS issue_tree_holds_company_root_status_idx
    ON issue_tree_holds (company_id, root_issue_id, status);

CREATE INDEX IF NOT EXISTS issue_tree_holds_company_status_mode_idx
    ON issue_tree_holds (company_id, status, mode);
