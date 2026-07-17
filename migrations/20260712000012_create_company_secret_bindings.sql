-- Company secret bindings (paperclip-aligned). Mirrors paperclip 0082_dry_vision.sql
-- (company_secret_bindings table).

CREATE TABLE IF NOT EXISTS company_secret_bindings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE NO ACTION,
    secret_id UUID NOT NULL REFERENCES company_secrets(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    config_path TEXT NOT NULL,
    version_selector TEXT NOT NULL DEFAULT 'latest',
    required BOOLEAN NOT NULL DEFAULT true,
    label TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS company_secret_bindings_company_idx ON company_secret_bindings(company_id);
CREATE INDEX IF NOT EXISTS company_secret_bindings_secret_idx ON company_secret_bindings(secret_id);
CREATE INDEX IF NOT EXISTS company_secret_bindings_target_idx
    ON company_secret_bindings(company_id, target_type, target_id);
CREATE UNIQUE INDEX IF NOT EXISTS company_secret_bindings_target_path_uq
    ON company_secret_bindings(company_id, target_type, target_id, config_path);
