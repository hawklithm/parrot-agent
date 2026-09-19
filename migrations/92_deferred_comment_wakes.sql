-- Deferred comment wakes.
--
-- A comment that arrives while its issue's run is already executing cannot
-- reach that turn: the prompt is already on the child's stdin. It also cannot
-- get a follow-up run of its own, because
-- `idx_heartbeat_runs_unique_active_agent_issue` (migration 18) allows only one
-- `queued`/`running` run per agent + issue — the exact duplicates that index
-- was added to delete.
--
-- Such a comment is therefore parked on its `agent_wakeup_requests` row
-- (`status = 'queued'`, `run_id` pointing at the run that blocked it) and
-- replayed by `reconcile_deferred_comment_wakes` once that run is gone.
--
-- `attempt_count` bounds the retries: a replay that keeps failing must not
-- loop forever, so the row is failed once it exhausts its attempts.

ALTER TABLE agent_wakeup_requests
    ADD COLUMN IF NOT EXISTS attempt_count integer NOT NULL DEFAULT 0;

-- Drives the reconciler's scan for parked rows that are ready to replay.
CREATE INDEX IF NOT EXISTS idx_agent_wakeup_requests_deferred_comment
    ON agent_wakeup_requests (requested_at)
    WHERE status = 'queued' AND run_id IS NOT NULL;
