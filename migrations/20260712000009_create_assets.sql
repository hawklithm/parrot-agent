-- Assets (paperclip-aligned). Mirrors paperclip 0010_stale_justin_hammer.sql
-- (assets table only; issue_attachments omitted as out of scope).

CREATE TABLE IF NOT EXISTS assets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE NO ACTION,
    provider TEXT NOT NULL,
    object_key TEXT NOT NULL,
    content_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    original_filename TEXT,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE NO ACTION,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS assets_company_created_idx ON assets(company_id, created_at);
CREATE INDEX IF NOT EXISTS assets_company_provider_idx ON assets(company_id, provider);
CREATE UNIQUE INDEX IF NOT EXISTS assets_company_object_key_uq ON assets(company_id, object_key);
