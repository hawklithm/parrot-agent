-- Upgrade issue_tree_holds to match Paperclip's complete schema
-- Adds comprehensive audit tracking, release metadata, and updated_at

-- Add missing columns to issue_tree_holds
ALTER TABLE issue_tree_holds
  ADD COLUMN IF NOT EXISTS created_by_actor_type TEXT DEFAULT 'system' NOT NULL,
  ADD COLUMN IF NOT EXISTS created_by_user_id TEXT,
  ADD COLUMN IF NOT EXISTS created_by_run_id UUID,
  ADD COLUMN IF NOT EXISTS released_by_actor_type TEXT,
  ADD COLUMN IF NOT EXISTS released_by_user_id TEXT,
  ADD COLUMN IF NOT EXISTS released_by_run_id UUID,
  ADD COLUMN IF NOT EXISTS release_reason TEXT,
  ADD COLUMN IF NOT EXISTS release_metadata JSONB,
  ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ DEFAULT NOW() NOT NULL;

-- Migrate existing actor_type and actor_id to created_by fields
UPDATE issue_tree_holds
SET created_by_actor_type = COALESCE(actor_type, 'system')
WHERE actor_type IS NOT NULL;

-- Add foreign key constraints for run references
DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'issue_tree_holds_created_by_run_id_fkey'
  ) THEN
    ALTER TABLE issue_tree_holds
      ADD CONSTRAINT issue_tree_holds_created_by_run_id_fkey
      FOREIGN KEY (created_by_run_id) REFERENCES heartbeat_runs(id) ON DELETE SET NULL;
  END IF;
END $$;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'issue_tree_holds_released_by_run_id_fkey'
  ) THEN
    ALTER TABLE issue_tree_holds
      ADD CONSTRAINT issue_tree_holds_released_by_run_id_fkey
      FOREIGN KEY (released_by_run_id) REFERENCES heartbeat_runs(id) ON DELETE SET NULL;
  END IF;
END $$;

-- Add missing indexes from Paperclip
CREATE INDEX IF NOT EXISTS issue_tree_holds_company_root_status_idx 
  ON issue_tree_holds(company_id, root_issue_id, status);

CREATE INDEX IF NOT EXISTS issue_tree_holds_company_status_mode_idx 
  ON issue_tree_holds(company_id, status, mode);

-- Add missing indexes to issue_tree_hold_members
CREATE UNIQUE INDEX IF NOT EXISTS issue_tree_hold_members_hold_issue_uq 
  ON issue_tree_hold_members(hold_id, issue_id);

CREATE INDEX IF NOT EXISTS issue_tree_hold_members_company_issue_idx 
  ON issue_tree_hold_members(company_id, issue_id);

CREATE INDEX IF NOT EXISTS issue_tree_hold_members_hold_depth_idx 
  ON issue_tree_hold_members(hold_id, depth);

-- Add foreign key for active_run_id to issue_tree_hold_members
DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'issue_tree_hold_members_active_run_id_fkey'
  ) THEN
    ALTER TABLE issue_tree_hold_members
      ADD CONSTRAINT issue_tree_hold_members_active_run_id_fkey
      FOREIGN KEY (active_run_id) REFERENCES heartbeat_runs(id) ON DELETE SET NULL;
  END IF;
END $$;

-- Update the existing trigger to also handle updated_at
DROP TRIGGER IF EXISTS update_issue_tree_holds_updated_at ON issue_tree_holds;
CREATE TRIGGER update_issue_tree_holds_updated_at 
  BEFORE UPDATE ON issue_tree_holds
  FOR EACH ROW 
  EXECUTE FUNCTION update_updated_at_column();
