-- Environments (paperclip-aligned). Mirrors paperclip 0065_environments.sql.

CREATE TABLE IF NOT EXISTS environments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    driver TEXT NOT NULL DEFAULT 'local',
    status TEXT NOT NULL DEFAULT 'active',
    config JSONB NOT NULL DEFAULT '{}',
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS environments_company_status_idx ON environments(company_id, status);
CREATE UNIQUE INDEX IF NOT EXISTS environments_company_driver_idx ON environments(company_id, driver);
CREATE INDEX IF NOT EXISTS environments_company_name_idx ON environments(company_id, name);
