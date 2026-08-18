-- Migration: Add missing columns to issue_thread_interactions table
-- Issue: API endpoint /api/issues/:id/interactions fails with 500
-- Error: "column continuation_policy does not exist"
--
-- This table was missing many columns that exist in Paperclip:
-- - continuation_policy, requested_resolver_policy, effective_resolver_policy
-- - idempotency_key, source_comment_id, title, summary
-- - created_by_agent_id, addressee_agent_id, created_by_user_id
-- - resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id
-- - payload, result, resolved_at

-- Add continuation_policy
ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS continuation_policy TEXT NOT NULL DEFAULT 'wake_assignee';

-- Add resolver policy columns
ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS requested_resolver_policy TEXT NOT NULL DEFAULT 'board_only';

ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS effective_resolver_policy TEXT NOT NULL DEFAULT 'board_only';

-- Add idempotency_key
ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS idempotency_key TEXT;

-- Add source_comment_id
ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS source_comment_id UUID REFERENCES issue_comments(id) ON DELETE SET NULL;

-- Add title and summary
ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS title TEXT;

ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS summary TEXT;

-- Add creator columns
ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS addressee_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS created_by_user_id TEXT;

-- Add resolver columns
ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS resolved_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS resolved_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL;

ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS resolved_by_user_id TEXT;

-- Add payload and result
ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS payload JSONB NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS result JSONB;

-- Add resolved_at
ALTER TABLE issue_thread_interactions 
ADD COLUMN IF NOT EXISTS resolved_at TIMESTAMPTZ;

-- Add indexes for better query performance
CREATE INDEX IF NOT EXISTS idx_issue_thread_interactions_issue_id 
ON issue_thread_interactions(issue_id);

CREATE INDEX IF NOT EXISTS idx_issue_thread_interactions_company_issue_created_at 
ON issue_thread_interactions(company_id, issue_id, created_at);
CREATE INDEX IF NOT EXISTS idx_issue_thread_interactions_company_issue_status 
ON issue_thread_interactions(company_id, issue_id, status);

CREATE UNIQUE INDEX IF NOT EXISTS idx_issue_thread_interactions_company_issue_idempotency 
ON issue_thread_interactions(company_id, issue_id, idempotency_key) 
WHERE idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_issue_thread_interactions_source_comment_id 
ON issue_thread_interactions(source_comment_id);

CREATE INDEX IF NOT EXISTS idx_issue_thread_interactions_addressee_agent_id 
ON issue_thread_interactions(addressee_agent_id);

-- Add helpful comments
COMMENT ON COLUMN issue_thread_interactions.continuation_policy IS 'Policy for what happens after interaction is resolved';
COMMENT ON COLUMN issue_thread_interactions.requested_resolver_policy IS 'Who is allowed to resolve this interaction (requested)';
COMMENT ON COLUMN issue_thread_interactions.effective_resolver_policy IS 'Who is allowed to resolve this interaction (effective after governance)';
COMMENT ON COLUMN issue_thread_interactions.idempotency_key IS 'Unique key to prevent duplicate interactions';
COMMENT ON COLUMN issue_thread_interactions.payload IS 'Interaction-specific data (question content, approval details, etc)';
COMMENT ON COLUMN issue_thread_interactions.result IS 'Resolution result data';
