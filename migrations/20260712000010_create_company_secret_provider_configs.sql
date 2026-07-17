-- Company secret provider configs (paperclip-aligned).
-- Mirrors paperclip 0083_company_secret_provider_configs.sql.

CREATE TABLE IF NOT EXISTS company_secret_provider_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'ready',
    is_default BOOLEAN NOT NULL DEFAULT false,
    config JSONB NOT NULL DEFAULT '{}',
    health_status TEXT,
    health_checked_at TIMESTAMPTZ,
    health_message TEXT,
    health_details JSONB,
    disabled_at TIMESTAMPTZ,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS company_secret_provider_configs_company_idx
    ON company_secret_provider_configs(company_id);
CREATE INDEX IF NOT EXISTS company_secret_provider_configs_company_provider_idx
    ON company_secret_provider_configs(company_id, provider);
CREATE UNIQUE INDEX IF NOT EXISTS company_secret_provider_configs_default_uq
    ON company_secret_provider_configs(company_id, provider) WHERE is_default = true;

-- Wire company_secrets.provider_config_id -> provider_configs (column exists since 20260711000022).
ALTER TABLE company_secrets
    ADD CONSTRAINT company_secrets_provider_config_id_fk
    FOREIGN KEY (provider_config_id)
    REFERENCES company_secret_provider_configs(id) ON DELETE SET NULL;
