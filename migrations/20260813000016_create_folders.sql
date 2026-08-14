-- 对齐 Paperclip folders 域（kind: routine|skill，含 folder items 归属）
CREATE TABLE IF NOT EXISTS folders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,                        -- routine | skill
    parent_id UUID REFERENCES folders(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    system_key TEXT,
    path TEXT NOT NULL,
    depth INTEGER NOT NULL DEFAULT 1,
    color TEXT,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (company_id, kind, slug)
);

CREATE INDEX IF NOT EXISTS idx_folders_company_kind ON folders(company_id, kind);

CREATE TABLE IF NOT EXISTS folder_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    folder_id UUID NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    item_kind TEXT NOT NULL,                   -- routine | skill
    item_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (folder_id, item_kind, item_id)
);

CREATE INDEX IF NOT EXISTS idx_folder_items_item ON folder_items(item_kind, item_id);
