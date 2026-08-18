-- Add missing interaction_resolver_governance column to companies table
-- This field stores governance rules for interaction resolvers (JSONB format)

ALTER TABLE companies 
  ADD COLUMN IF NOT EXISTS interaction_resolver_governance JSONB NOT NULL DEFAULT '{}'::JSONB;

COMMENT ON COLUMN companies.interaction_resolver_governance IS 'Governance rules for interaction resolvers in JSON format';
