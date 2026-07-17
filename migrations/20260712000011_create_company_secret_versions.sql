-- Company secret versions (paperclip-aligned). Mirrors paperclip 0009_fast_jackal.sql
-- (company_secret_versions table). Holds the encrypted material (jsonb) + sha256.

CREATE TABLE IF NOT EXISTS company_secret_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    secret_id UUID NOT NULL REFERENCES company_secrets(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    material JSONB NOT NULL,
    value_sha256 TEXT NOT NULL,
    provider_version_ref TEXT,
    status TEXT NOT NULL DEFAULT 'current',
    fingerprint_sha256 TEXT,
    rotation_job_id TEXT,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS company_secret_versions_secret_idx
    ON company_secret_versions(secret_id, created_at);
CREATE INDEX IF NOT EXISTS company_secret_versions_value_sha256_idx
    ON company_secret_versions(value_sha256);
CREATE UNIQUE INDEX IF NOT EXISTS company_secret_versions_secret_version_uq
    ON company_secret_versions(secret_id, version);
CREATE INDEX IF NOT EXISTS company_secret_versions_fingerprint_idx
    ON company_secret_versions(fingerprint_sha256);

-- Backfill fingerprint_sha256 from value_sha256 for rows created before the column existed.
UPDATE company_secret_versions SET fingerprint_sha256 = value_sha256
    WHERE fingerprint_sha256 IS NULL;
ALTER TABLE company_secret_versions ALTER COLUMN fingerprint_sha256 SET NOT NULL;
