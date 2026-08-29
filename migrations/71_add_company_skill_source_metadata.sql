-- Align company skills with Paperclip's local project-scan representation.
-- Existing rows receive compatibility defaults; project scans persist their
-- source locator, markdown entrypoint, file inventory, and provenance.

ALTER TABLE company_skills
    ADD COLUMN IF NOT EXISTS key TEXT,
    ADD COLUMN IF NOT EXISTS markdown TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS source_type TEXT NOT NULL DEFAULT 'local_path',
    ADD COLUMN IF NOT EXISTS source_locator TEXT,
    ADD COLUMN IF NOT EXISTS source_ref TEXT,
    ADD COLUMN IF NOT EXISTS trust_level TEXT NOT NULL DEFAULT 'markdown_only',
    ADD COLUMN IF NOT EXISTS compatibility TEXT NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS file_inventory JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS categories JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS sharing_scope TEXT NOT NULL DEFAULT 'company',
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

UPDATE company_skills
SET key = format('company/%s/%s', company_id, slug)
WHERE key IS NULL OR btrim(key) = '';

ALTER TABLE company_skills
    ALTER COLUMN key SET NOT NULL;

-- Legacy callers and fixtures do not yet provide a key. Keep those writes
-- valid while the application write paths migrate to deterministic keys.
ALTER TABLE company_skills
    ALTER COLUMN key SET DEFAULT ('legacy/' || gen_random_uuid()::text);

CREATE UNIQUE INDEX IF NOT EXISTS company_skills_company_key_idx
    ON company_skills(company_id, key);
