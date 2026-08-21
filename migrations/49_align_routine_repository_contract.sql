-- Keep the existing Rust Routine/RoutineTrigger persistence contract usable while
-- preserving the Paperclip-compatible columns already present in the unified schema.

DO $$ BEGIN
    CREATE TYPE trigger_type AS ENUM ('schedule', 'webhook', 'manual', 'event', 'cron');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE trigger_status AS ENUM ('enabled', 'disabled', 'paused', 'active', 'failed', 'configuration');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

ALTER TABLE routines ADD COLUMN IF NOT EXISTS name TEXT;
ALTER TABLE routines ADD COLUMN IF NOT EXISTS agent_id UUID REFERENCES agents(id) ON DELETE CASCADE;
ALTER TABLE routines ADD COLUMN IF NOT EXISTS trigger_config JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE routines ADD COLUMN IF NOT EXISTS created_by_user_id UUID;
ALTER TABLE routines ADD COLUMN IF NOT EXISTS last_run_at TIMESTAMPTZ;
ALTER TABLE routines ADD COLUMN IF NOT EXISTS next_run_at TIMESTAMPTZ;
ALTER TABLE routines ADD COLUMN IF NOT EXISTS run_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE routines ADD COLUMN IF NOT EXISTS success_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE routines ADD COLUMN IF NOT EXISTS failure_count INTEGER NOT NULL DEFAULT 0;

UPDATE routines
SET name = title
WHERE name IS NULL;

UPDATE routines
SET agent_id = assignee_agent_id
WHERE agent_id IS NULL;

ALTER TABLE routines ALTER COLUMN name SET NOT NULL;
ALTER TABLE routines ALTER COLUMN agent_id SET NOT NULL;

ALTER TABLE routine_triggers ADD COLUMN IF NOT EXISTS trigger_type trigger_type;
ALTER TABLE routine_triggers ADD COLUMN IF NOT EXISTS config JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE routine_triggers ADD COLUMN IF NOT EXISTS status trigger_status;
ALTER TABLE routine_triggers ADD COLUMN IF NOT EXISTS next_trigger_at TIMESTAMPTZ;
ALTER TABLE routine_triggers ADD COLUMN IF NOT EXISTS last_triggered_at TIMESTAMPTZ;

UPDATE routine_triggers
SET trigger_type = CASE kind::text
    WHEN 'schedule' THEN 'schedule'::trigger_type
    WHEN 'webhook' THEN 'webhook'::trigger_type
    ELSE 'manual'::trigger_type
END
WHERE trigger_type IS NULL;

UPDATE routine_triggers
SET status = CASE
    WHEN enabled THEN 'active'::trigger_status
    ELSE 'disabled'::trigger_status
END
WHERE status IS NULL;

UPDATE routine_triggers
SET config = jsonb_strip_nulls(jsonb_build_object(
    'cron_expression', cron_expression,
    'timezone', timezone
))
WHERE config = '{}'::jsonb AND (cron_expression IS NOT NULL OR timezone IS NOT NULL);

UPDATE routine_triggers
SET next_trigger_at = next_run_at
WHERE next_trigger_at IS NULL;

UPDATE routine_triggers
SET last_triggered_at = last_fired_at
WHERE last_triggered_at IS NULL;

ALTER TABLE routine_triggers ALTER COLUMN trigger_type SET NOT NULL;
ALTER TABLE routine_triggers ALTER COLUMN status SET NOT NULL;
