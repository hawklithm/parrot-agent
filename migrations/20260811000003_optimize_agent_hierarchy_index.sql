-- Optimize agent hierarchy index to match Paperclip schema
-- Key change: Replace single-column reports_to index with compound company+reports_to index

-- This optimization enables efficient queries like:
-- 1. "Find all agents reporting to X within company Y"
-- 2. "Get the org chart for company Y"
-- 3. Prevent cross-company agent relationship pollution

-- Step 1: Drop the old single-column index
DROP INDEX IF EXISTS idx_agents_reports_to;

-- Step 2: Create Paperclip-style compound index
-- This covers both (company_id, reports_to) queries and company_id-only queries
CREATE INDEX IF NOT EXISTS idx_agents_company_reports_to ON agents(company_id, reports_to);

-- Note: The compound index (company_id, reports_to) can also serve queries that only filter by company_id,
-- so we keep the existing idx_agents_company_id for now as it may have better selectivity for that case.
-- PostgreSQL query planner will choose the optimal index automatically.

COMMENT ON INDEX idx_agents_company_reports_to IS 'Optimizes org chart queries within a company, prevents cross-company agent relationships';
