-- 68_add_item_verdict_withdraw_interactions.sql
-- Add support for item_verdict and withdraw interaction kinds
-- The CHECK constraint was already dropped in 00_init_schema_unified, so no ALTER needed.
-- This migration documents the new values for posterity.

-- No schema changes needed: kind is already TEXT without CHECK constraint.
-- item_verdict and withdraw are accepted by issue_thread_interaction_service.
SELECT 1;
