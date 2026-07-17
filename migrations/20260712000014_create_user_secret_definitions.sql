-- User secret definitions (paperclip-aligned). Mirrors paperclip 0128_user_specific_secrets.sql
-- (user_secret_definitions table).

CREATE TABLE IF NOT EXISTS user_secret_definitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    provider TEXT NOT NULL DEFAULT 'local_encrypted',
    managed_mode TEXT NOT NULL DEFAULT 'paperclip_managed',
    provider_config_id UUID REFERENCES company_secret_provider_configs(id) ON DELETE SET NULL,
    provider_metadata JSONB,
    usage_guidance TEXT,
    required BOOLEAN NOT NULL DEFAULT false,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id UUID,
    updated_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    updated_by_user_id UUID,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS user_secret_definitions_company_status_idx
    ON user_secret_definitions(company_id, status);
CREATE INDEX IF NOT EXISTS user_secret_definitions_company_provider_idx
    ON user_secret_definitions(company_id, provider);
CREATE INDEX IF NOT EXISTS user_secret_definitions_provider_config_idx
    ON user_secret_definitions(provider_config_id);
CREATE UNIQUE INDEX IF NOT EXISTS user_secret_definitions_company_key_uq
    ON user_secret_definitions(company_id, key) WHERE deleted_at IS NULL;
