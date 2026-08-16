-- Migration: Add run_id columns to activity_logs
-- Addresses: API error "column al.run_id does not exist"
--
-- Issue: /api/heartbeat-runs/:id/issues endpoint fails with 500 error
-- The activity_logs table is missing columns that link activities to runs:
-- 1. run_id: UUID - links activity to a heartbeat_run
-- 2. agent_id: UUID - tracks which agent performed the activity
--
-- These columns are used to:
-- - Find all issues touched during a specific run
-- - Track agent actions across runs
-- - Support cost attribution and audit trails
--
-- Reference: crates/api/src/routes/heartbeat_runs.rs:467

-- Add run_id column (nullable, as not all activities are part of a run)
ALTER TABLE activity_logs 
ADD COLUMN IF NOT EXISTS run_id UUID REFERENCES heartbeat_runs(id) ON DELETE CASCADE;

-- Add agent_id column (nullable, as not all activities are performed by agents)
ALTER TABLE activity_logs 
ADD COLUMN IF NOT EXISTS agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

-- Add index for efficient run-based queries
CREATE INDEX IF NOT EXISTS idx_activity_logs_run_id 
ON activity_logs(run_id) 
WHERE run_id IS NOT NULL;

-- Add index for efficient agent-based queries
CREATE INDEX IF NOT EXISTS idx_activity_logs_agent_id 
ON activity_logs(agent_id) 
WHERE agent_id IS NOT NULL;

-- Add composite index for the common query pattern in list_run_issues
CREATE INDEX IF NOT EXISTS idx_activity_logs_run_resource 
ON activity_logs(company_id, run_id, resource_type, resource_id) 
WHERE run_id IS NOT NULL;

-- Add helpful comments
COMMENT ON COLUMN activity_logs.run_id IS 'Links this activity to a heartbeat run (NULL for non-run activities)';
COMMENT ON COLUMN activity_logs.agent_id IS 'Agent that performed this activity (NULL for user activities)';
