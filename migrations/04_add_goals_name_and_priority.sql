-- Add missing name and priority columns to goals table
-- These fields are required by the Goal model but were missing from the initial schema

-- Add name column (similar to title, but may serve a different purpose in the application)
ALTER TABLE goals 
  ADD COLUMN IF NOT EXISTS name TEXT NOT NULL DEFAULT '';

-- Add priority column (Paperclip doesn't have this, but Parrot's model requires it)
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'goal_priority') THEN
        CREATE TYPE goal_priority AS ENUM ('low', 'medium', 'high', 'critical');
    END IF;
END $$;

ALTER TABLE goals 
  ADD COLUMN IF NOT EXISTS priority goal_priority NOT NULL DEFAULT 'medium';

-- Add indexes for common queries
CREATE INDEX IF NOT EXISTS idx_goals_name ON goals(name) WHERE name != '';
CREATE INDEX IF NOT EXISTS idx_goals_priority ON goals(priority);

COMMENT ON COLUMN goals.name IS 'Goal name (may differ from title for internal reference)';
COMMENT ON COLUMN goals.priority IS 'Goal priority level for scheduling and resource allocation';
