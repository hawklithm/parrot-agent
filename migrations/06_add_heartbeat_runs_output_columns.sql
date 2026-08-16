-- Migration: Add missing output and result_json columns to heartbeat_runs
-- Addresses: BROWSER_TEST_ERROR_REPORT.md - Error 1
-- 
-- Issue: API endpoints /api/companies/.../live-runs and /api/companies/.../heartbeat-runs
-- were failing with "column 'output' does not exist" error.
--
-- The heartbeat_runs table was missing two columns that the API code expects:
-- 1. output: TEXT - stores stdout excerpt from the run
-- 2. result_json: JSONB - stores structured result data
--
-- Reference: crates/api/src/routes/heartbeat_runs.rs:142-143, 176

-- Add output column (TEXT type for stdout excerpt)
ALTER TABLE heartbeat_runs 
ADD COLUMN IF NOT EXISTS output TEXT;

-- Add result_json column (JSONB type for structured results)
ALTER TABLE heartbeat_runs 
ADD COLUMN IF NOT EXISTS result_json JSONB;

-- Add helpful comment
COMMENT ON COLUMN heartbeat_runs.output IS 'Stdout excerpt from the heartbeat run execution';
COMMENT ON COLUMN heartbeat_runs.result_json IS 'Structured JSON result data from the run';
