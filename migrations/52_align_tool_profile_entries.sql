-- Align tool_profile_entries with Paperclip (packages/db/src/schema/tool_access.ts).
-- The uncommitted tool_access.rs INSERT expects (company_id, profile_id, selector_type, effect, tool_name)
-- and paperclip has NO selector_value column. Bring the table to parity.
ALTER TABLE tool_profile_entries
    ADD COLUMN company_id UUID REFERENCES companies(id) ON DELETE CASCADE;

UPDATE tool_profile_entries
    SET company_id = (SELECT tp.company_id FROM tool_profiles tp WHERE tp.id = tool_profile_entries.profile_id)
    WHERE company_id IS NULL;

ALTER TABLE tool_profile_entries
    ALTER COLUMN company_id SET NOT NULL;

ALTER TABLE tool_profile_entries
    ADD COLUMN application_id UUID REFERENCES tool_applications(id) ON DELETE CASCADE;

ALTER TABLE tool_profile_entries
    ADD COLUMN catalog_entry_id UUID REFERENCES tool_catalog_entries(id) ON DELETE CASCADE;

ALTER TABLE tool_profile_entries
    ADD COLUMN risk_level TEXT;

ALTER TABLE tool_profile_entries
    ADD COLUMN conditions JSONB;

-- Paperclip effect domain is 'include' | 'exclude'; normalize legacy 'allow' rows.
UPDATE tool_profile_entries SET effect = 'include' WHERE effect = 'allow';
ALTER TABLE tool_profile_entries
    ALTER COLUMN effect SET DEFAULT 'include';

-- Paperclip has no selector_value column; the route no longer writes it.
-- SAFETY: selector_value column existed in earlier Parrot schema versions
-- but was never part of Paperclip's tool_profile_entries definition.
-- If the column does not exist (fresh install from 00_init_schema_unified),
-- this is a safe no-op.
ALTER TABLE tool_profile_entries
    DROP COLUMN IF EXISTS selector_value;
