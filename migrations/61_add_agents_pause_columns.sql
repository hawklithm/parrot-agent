-- Migration: Add pause_reason / paused_at to agents
--
-- Closes a schema drift that breaks the budget-pause path at runtime:
-- cost_service.rs writes pause_reason='budget' + paused_at on agents (and reads
-- agents.pause_reason in check_scope_paused), but the agents table never received
-- the columns that companies and projects already have — every budget pause/resume
-- UPDATE on an agent fails with column-not-exist (42703).
--
-- PAPERCLIP_MIGRATION_PLAN §4B.2 line 316 (Agent lifecycle: pause/resume/terminate,
-- error reasons).

ALTER TABLE agents
ADD COLUMN IF NOT EXISTS pause_reason TEXT,
ADD COLUMN IF NOT EXISTS paused_at TIMESTAMPTZ;

COMMENT ON COLUMN agents.pause_reason IS 'Why the agent is paused (e.g. budget); NULL when not paused';
COMMENT ON COLUMN agents.paused_at IS 'When the agent was paused; NULL when not paused';
