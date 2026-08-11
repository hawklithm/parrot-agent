-- Upgrade company_memberships to match Paperclip schema
-- Key changes: principal_id UUID -> TEXT for user ID compatibility, add optimized indexes

-- Step 1: Change principal_id from UUID to TEXT
-- This is needed because Paperclip supports both UUID agent_ids and TEXT user_ids
ALTER TABLE company_memberships
  ALTER COLUMN principal_id TYPE TEXT USING principal_id::TEXT;

-- Step 2: Drop old indexes
DROP INDEX IF EXISTS idx_company_memberships_company_id;
DROP INDEX IF EXISTS idx_company_memberships_principal;
DROP INDEX IF EXISTS idx_company_memberships_role;

-- Step 3: Add Paperclip-style optimized indexes
CREATE UNIQUE INDEX IF NOT EXISTS company_memberships_company_principal_unique_idx 
  ON company_memberships (company_id, principal_type, principal_id);

CREATE INDEX IF NOT EXISTS company_memberships_principal_status_idx 
  ON company_memberships (principal_type, principal_id, status);

CREATE INDEX IF NOT EXISTS company_memberships_company_status_idx 
  ON company_memberships (company_id, status);

-- Step 4: Keep the UNIQUE constraint at table level for compatibility
-- The UNIQUE(company_id, principal_type, principal_id) already exists in the table definition
-- and is now backed by the unique index above

COMMENT ON COLUMN company_memberships.principal_id IS 'Principal ID - TEXT to support both UUID agent IDs and string user IDs (e.g., auth0|123456)';
COMMENT ON INDEX company_memberships_company_principal_unique_idx IS 'Ensures one membership per principal per company';
COMMENT ON INDEX company_memberships_principal_status_idx IS 'Optimizes principal membership lookups with status filtering';
COMMENT ON INDEX company_memberships_company_status_idx IS 'Optimizes company member list queries by status';
