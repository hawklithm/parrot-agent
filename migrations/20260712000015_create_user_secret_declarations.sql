-- User secret declarations (paperclip-aligned). Mirrors paperclip 0128_user_specific_secrets.sql
-- (user_secret_declarations table). Replaces parrot-agent's old user_secrets model.

CREATE TABLE IF NOT EXISTS user_secret_declarations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_secret_definition_id UUID NOT NULL REFERENCES user_secret_definitions(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    config_path TEXT NOT NULL,
    env_key TEXT NOT NULL,
    version_selector TEXT NOT NULL DEFAULT 'latest',
    required BOOLEAN NOT NULL DEFAULT true,
    allow_missing_override BOOLEAN NOT NULL DEFAULT false,
    label TEXT,
    -- parrot extension: encrypted user-provided value (paperclip stores values via provider/UI).
    value_material JSONB,
    value_sha256 TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Wire company_secrets.user_secret_definition_id -> definitions (column exists since 20260711000022).
ALTER TABLE company_secrets
    ADD CONSTRAINT company_secrets_user_secret_definition_id_fk
    FOREIGN KEY (user_secret_definition_id)
    REFERENCES user_secret_definitions(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS user_secret_declarations_company_idx
    ON user_secret_declarations(company_id);
CREATE INDEX IF NOT EXISTS user_secret_declarations_definition_idx
    ON user_secret_declarations(user_secret_definition_id);
CREATE INDEX IF NOT EXISTS user_secret_declarations_target_idx
    ON user_secret_declarations(company_id, target_type, target_id);
CREATE INDEX IF NOT EXISTS user_secret_declarations_company_required_idx
    ON user_secret_declarations(company_id, required);
CREATE UNIQUE INDEX IF NOT EXISTS user_secret_declarations_target_path_uq
    ON user_secret_declarations(company_id, target_type, target_id, config_path);
CREATE INDEX IF NOT EXISTS user_secret_declarations_required_override_idx
    ON user_secret_declarations(company_id, allow_missing_override) WHERE allow_missing_override = true;
