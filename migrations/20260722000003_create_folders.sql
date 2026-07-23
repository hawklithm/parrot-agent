-- Paperclip-compatible company folders used by routines and skills.
CREATE TABLE IF NOT EXISTS folders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('routine', 'skill')),
    parent_id UUID REFERENCES folders(id) ON DELETE RESTRICT,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    system_key TEXT,
    color TEXT,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE routines ADD COLUMN IF NOT EXISTS folder_id UUID REFERENCES folders(id) ON DELETE SET NULL;
ALTER TABLE company_skills ADD COLUMN IF NOT EXISTS folder_id UUID REFERENCES folders(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS folders_company_kind_position_idx ON folders(company_id, kind, position, name);
CREATE INDEX IF NOT EXISTS folders_company_kind_parent_idx ON folders(company_id, kind, parent_id, position, name);
CREATE INDEX IF NOT EXISTS routines_company_folder_idx ON routines(company_id, folder_id);
CREATE INDEX IF NOT EXISTS company_skills_company_folder_idx ON company_skills(company_id, folder_id);
