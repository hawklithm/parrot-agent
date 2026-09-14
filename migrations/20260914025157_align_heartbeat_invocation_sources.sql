-- Align `heartbeat_runs.invocation_source` with Paperclip's canonical set.
--
-- The original check only allowed ('on_demand', 'scheduled', 'watchdog'), but
-- production code wakes agents with 'automation' and 'assignment' -- both of
-- which Paperclip lists in HEARTBEAT_INVOCATION_SOURCES
-- (packages/shared/src/constants.ts):
--
--     ["timer", "assignment", "on_demand", "automation"]
--
-- Every such wakeup failed with `new row for relation "heartbeat_runs"
-- violates check constraint "valid_invocation_source"`, surfacing as
-- HeartbeatError::WakeupFailed on comment-driven, interaction and recovery
-- wakes. Neither 'scheduled' nor 'watchdog' is written anywhere.
--
-- 'schedule' is kept as a tolerated alias for the legacy scheduled-retry
-- promotion path. Paperclip itself has no DB-level check on this column
-- (plain `text`, default 'on_demand'), so this check is a Parrot addition and
-- is deliberately kept loose to the union of both sides' values.

ALTER TABLE heartbeat_runs DROP CONSTRAINT IF EXISTS valid_invocation_source;

ALTER TABLE heartbeat_runs
    ADD CONSTRAINT valid_invocation_source
    CHECK (invocation_source IN (
        'timer',
        'assignment',
        'on_demand',
        'automation',
        'scheduled',
        'schedule',
        'watchdog'
    ));
