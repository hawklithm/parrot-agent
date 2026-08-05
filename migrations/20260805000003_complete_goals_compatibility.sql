-- Keep the goal model/repository compatible with databases created before the
-- current Paperclip goal contract. This migration is intentionally idempotent
-- so an existing parrot_agent_dev can be upgraded in place.
DO $$
BEGIN
    CREATE TYPE goal_priority AS ENUM ('low', 'medium', 'high', 'critical');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

ALTER TYPE goal_status ADD VALUE IF NOT EXISTS 'achieved';

ALTER TABLE goals
    ADD COLUMN IF NOT EXISTS name VARCHAR(255),
    ADD COLUMN IF NOT EXISTS priority goal_priority NOT NULL DEFAULT 'medium';

UPDATE goals
SET name = title
WHERE name IS NULL;

ALTER TABLE goals
    ALTER COLUMN name SET NOT NULL;

