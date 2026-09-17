-- Align profile assignments with Paperclip's target model.
--
-- Parrot's original table only allowed one profile per target and stored the
-- target as UUID. Paperclip permits multiple ordered profiles and also uses
-- string target ids for company/gateway/external scopes.
ALTER TABLE tool_profile_bindings
    ADD COLUMN IF NOT EXISTS priority INTEGER NOT NULL DEFAULT 100,
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS created_by_user_id TEXT,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    SELECT c.conname
      INTO constraint_name
      FROM pg_constraint c
      JOIN pg_class t ON t.oid = c.conrelid
     WHERE t.relname = 'tool_profile_bindings'
       AND c.contype = 'u'
       AND pg_get_constraintdef(c.oid) LIKE 'UNIQUE (company_id, target_type, target_id)%'
     LIMIT 1;
    IF constraint_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE tool_profile_bindings DROP CONSTRAINT %I', constraint_name);
    END IF;
END $$;

ALTER TABLE tool_profile_bindings
    ALTER COLUMN target_id TYPE TEXT USING target_id::text;

CREATE UNIQUE INDEX IF NOT EXISTS tool_profile_bindings_target_profile_uq
    ON tool_profile_bindings(company_id, target_type, target_id, profile_id);
CREATE INDEX IF NOT EXISTS tool_profile_bindings_company_target_idx
    ON tool_profile_bindings(company_id, target_type, target_id);

ALTER TABLE tool_profiles
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;
