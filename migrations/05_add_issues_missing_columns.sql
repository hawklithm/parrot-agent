-- Add missing columns to issues table
-- These fields are required by the Issue model but were missing from the initial schema

-- Add harness_kind column (specifies the execution harness type)
ALTER TABLE issues 
  ADD COLUMN IF NOT EXISTS harness_kind TEXT;

-- Add cancelled_at timestamp (tracks when an issue was cancelled)
ALTER TABLE issues 
  ADD COLUMN IF NOT EXISTS cancelled_at TIMESTAMPTZ;

-- Add hidden_at timestamp (tracks when an issue was hidden from views)
ALTER TABLE issues 
  ADD COLUMN IF NOT EXISTS hidden_at TIMESTAMPTZ;

-- Add source_trust JSONB (stores trust/verification metadata about the issue source)
ALTER TABLE issues 
  ADD COLUMN IF NOT EXISTS source_trust JSONB;

-- Add indexes for common queries
CREATE INDEX IF NOT EXISTS idx_issues_harness_kind 
  ON issues(harness_kind) 
  WHERE harness_kind IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_issues_cancelled_at 
  ON issues(cancelled_at) 
  WHERE cancelled_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_issues_hidden_at 
  ON issues(hidden_at) 
  WHERE hidden_at IS NOT NULL;

-- Add comments for documentation
COMMENT ON COLUMN issues.harness_kind IS 'Type of execution harness (e.g., docker, kubernetes, local)';
COMMENT ON COLUMN issues.cancelled_at IS 'Timestamp when the issue was cancelled';
COMMENT ON COLUMN issues.hidden_at IS 'Timestamp when the issue was hidden from default views';
COMMENT ON COLUMN issues.source_trust IS 'Trust and verification metadata about the issue source in JSON format';
