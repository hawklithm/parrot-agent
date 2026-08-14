-- P1.4 Teams Catalog：记录公司已安装的 catalog team，用于 installed 查询与幂等安装
CREATE TABLE IF NOT EXISTS company_team_installs (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    catalog_id TEXT NOT NULL,
    catalog_key TEXT,
    content_hash TEXT NOT NULL,
    agent_ids UUID[] NOT NULL DEFAULT '{}',
    agent_count INTEGER NOT NULL DEFAULT 0,
    installed_by_user_id UUID,
    installed_by_agent_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT company_team_installs_company_catalog_unique UNIQUE (company_id, catalog_id)
);

CREATE INDEX IF NOT EXISTS idx_company_team_installs_company
    ON company_team_installs (company_id);
