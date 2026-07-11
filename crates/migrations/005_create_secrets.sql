-- Migration: Create Secrets Management Tables
-- Description: Company secrets, secret provider configs, user secret definitions, user secrets, and secret bindings

-- Company Secrets Table
CREATE TABLE company_secrets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    name TEXT NOT NULL,
    key TEXT NOT NULL,
    provider TEXT,
    provider_config_id UUID,
    managed_mode TEXT NOT NULL DEFAULT 'paperclip_managed',
    scope TEXT NOT NULL DEFAULT 'company',
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX company_secrets_company_id_idx ON company_secrets(company_id);
CREATE INDEX company_secrets_status_idx ON company_secrets(status);
CREATE UNIQUE INDEX company_secrets_company_key_idx ON company_secrets(company_id, key) WHERE status = 'active';

COMMENT ON TABLE company_secrets IS 'Company-level secrets for API keys, credentials, and configuration';
COMMENT ON COLUMN company_secrets.managed_mode IS 'paperclip_managed: stored in DB; external: reference to external provider';
COMMENT ON COLUMN company_secrets.scope IS 'company: shared across company; user: user-specific';
COMMENT ON COLUMN company_secrets.status IS 'active: in use; archived: soft deleted';

-- Secret Provider Configs Table
CREATE TABLE secret_provider_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    provider_type TEXT NOT NULL,
    name TEXT NOT NULL,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_default BOOLEAN NOT NULL DEFAULT false,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX secret_provider_configs_company_id_idx ON secret_provider_configs(company_id);
CREATE UNIQUE INDEX secret_provider_configs_company_default_idx ON secret_provider_configs(company_id)
    WHERE is_default = true AND status = 'active';

COMMENT ON TABLE secret_provider_configs IS 'Configuration for external secret providers (AWS, GCP, Vault)';
COMMENT ON COLUMN secret_provider_configs.provider_type IS 'local_encrypted / aws_secrets_manager / gcp_secret_manager / vault';
COMMENT ON COLUMN secret_provider_configs.is_default IS 'Default provider for new secrets';

-- User Secret Definitions Table
CREATE TABLE user_secret_definitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    name TEXT NOT NULL,
    key TEXT NOT NULL,
    description TEXT,
    required BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX user_secret_definitions_company_id_idx ON user_secret_definitions(company_id);
CREATE UNIQUE INDEX user_secret_definitions_company_key_idx ON user_secret_definitions(company_id, key);

COMMENT ON TABLE user_secret_definitions IS 'User-level secret definitions (e.g., personal GitHub token)';
COMMENT ON COLUMN user_secret_definitions.required IS 'Whether users must provide this secret';

-- User Secrets Table
CREATE TABLE user_secrets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    definition_id UUID NOT NULL REFERENCES user_secret_definitions(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    value_ref TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX user_secrets_definition_id_idx ON user_secrets(definition_id);
CREATE INDEX user_secrets_user_id_idx ON user_secrets(user_id);
CREATE UNIQUE INDEX user_secrets_definition_user_idx ON user_secrets(definition_id, user_id) WHERE status = 'active';

COMMENT ON TABLE user_secrets IS 'User-specific secret values';
COMMENT ON COLUMN user_secrets.value_ref IS 'Encrypted value or reference to external provider';

-- Secret Bindings Table
CREATE TABLE secret_bindings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    secret_id UUID NOT NULL REFERENCES company_secrets(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,
    target_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX secret_bindings_secret_id_idx ON secret_bindings(secret_id);
CREATE INDEX secret_bindings_target_idx ON secret_bindings(target_type, target_id);
CREATE UNIQUE INDEX secret_bindings_secret_target_idx ON secret_bindings(secret_id, target_type, target_id);

COMMENT ON TABLE secret_bindings IS 'Bindings between secrets and target entities (agents, environments, projects, routines)';
COMMENT ON COLUMN secret_bindings.target_type IS 'agent / environment / project / routine';

-- Add foreign key constraint for provider_config_id
ALTER TABLE company_secrets
    ADD CONSTRAINT company_secrets_provider_config_id_fkey
    FOREIGN KEY (provider_config_id) REFERENCES secret_provider_configs(id) ON DELETE SET NULL;
