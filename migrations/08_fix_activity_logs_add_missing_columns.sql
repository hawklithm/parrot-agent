-- Migration: Fix activity_logs table - add missing run_id and agent_id columns
-- Issue: These columns were in 07_add_activity_logs_run_columns.sql but never executed
--        because that migration wasn't included in lib.rs ALL_MIGRATIONS list
-- 
-- This migration MUST be added to lib.rs to execute!

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
COMMENT ON COLUMN activity_logs.agent_id IS 'Agent that performed this activity (NULL for user/system activities)';
