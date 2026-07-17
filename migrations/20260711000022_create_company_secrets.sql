-- Company secrets (paperclip-aligned, versioned model).
-- Values live in company_secret_versions (material jsonb); this table holds
-- only metadata. No flat `value` column.
-- Mirrors paperclip packages/db/src/migrations/0009_fast_jackal.sql +
-- 0082_dry_vision.sql + 0128_user_specific_secrets.sql (company_secrets part).

CREATE TABLE IF NOT EXISTS company_secrets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE NO ACTION,
    scope TEXT NOT NULL DEFAULT 'company',
    owner_user_id TEXT,
    user_secret_definition_id UUID,
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    provider TEXT NOT NULL DEFAULT 'local_encrypted',
    status TEXT NOT NULL DEFAULT 'active',
    managed_mode TEXT NOT NULL DEFAULT 'paperclip_managed',
    external_ref TEXT,
    provider_config_id UUID,
    provider_metadata JSONB,
    latest_version INTEGER NOT NULL DEFAULT 1,
    description TEXT,
    last_resolved_at TIMESTAMPTZ,
    last_rotated_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT company_secrets_scope_shape_check CHECK (
        (scope = 'company' AND owner_user_id IS NULL AND user_secret_definition_id IS NULL)
        OR
        (scope = 'user' AND owner_user_id IS NOT NULL AND user_secret_definition_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_company_secrets_company ON company_secrets(company_id);
CREATE INDEX IF NOT EXISTS idx_company_secrets_company_provider ON company_secrets(company_id, provider);
CREATE INDEX IF NOT EXISTS idx_company_secrets_company_scope ON company_secrets(company_id, scope);
CREATE INDEX IF NOT EXISTS idx_company_secrets_company_owner ON company_secrets(company_id, owner_user_id);
CREATE INDEX IF NOT EXISTS idx_company_secrets_user_definition_owner ON company_secrets(company_id, user_secret_definition_id, owner_user_id);
CREATE INDEX IF NOT EXISTS idx_company_secrets_provider_config ON company_secrets(provider_config_id);

-- Unique per company by key / name, only for active company-scoped secrets.
CREATE UNIQUE INDEX IF NOT EXISTS company_secrets_company_key_uq
    ON company_secrets(company_id, key) WHERE scope = 'company' AND deleted_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS company_secrets_company_name_uq
    ON company_secrets(company_id, name) WHERE scope = 'company' AND deleted_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS company_secrets_user_definition_owner_uq
    ON company_secrets(company_id, user_secret_definition_id, owner_user_id) WHERE scope = 'user' AND deleted_at IS NULL;
