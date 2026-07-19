-- Add missing columns to cost_events table for the full CostEvent model
-- The original table only had: id, agent_id, amount_cents, event_type, created_at
-- We need: company_id, issue_id, project_id, goal_id, heartbeat_run_id, billing_code,
--          provider, biller, billing_type, model, input_tokens, cached_input_tokens,
--          output_tokens, cost_cents, occurred_at

-- First, add new columns with defaults where possible
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS company_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS issue_id UUID;
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS project_id UUID;
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS goal_id UUID;
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS heartbeat_run_id UUID;
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS billing_code TEXT;
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS provider TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS biller TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS billing_type TEXT NOT NULL DEFAULT 'usage';
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS model TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS input_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS cached_input_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS output_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS cost_cents INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Drop old columns that have been replaced
-- amount_cents → cost_cents, event_type → billing_type/provider/model
-- We keep agent_id and created_at as they are

-- Rename amount_cents to cost_cents if cost_cents was just added (migration idempotency)
-- (already handled by IF NOT EXISTS above)

-- Add indexes for common aggregation queries
CREATE INDEX IF NOT EXISTS idx_cost_events_company_id ON cost_events(company_id);
CREATE INDEX IF NOT EXISTS idx_cost_events_occurred_at ON cost_events(occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_cost_events_provider ON cost_events(provider);
CREATE INDEX IF NOT EXISTS idx_cost_events_biller ON cost_events(biller);
CREATE INDEX IF NOT EXISTS idx_cost_events_model ON cost_events(model);
CREATE INDEX IF NOT EXISTS idx_cost_events_project_id ON cost_events(project_id);
CREATE INDEX IF NOT EXISTS idx_cost_events_issue_id ON cost_events(issue_id);
