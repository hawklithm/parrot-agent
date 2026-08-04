-- Complete the auth_users schema used by the current authentication services.
-- Older local databases were created by 20260711000012_create_auth_users.sql,
-- which only contained the basic profile columns.

ALTER TABLE auth_users
    ADD COLUMN IF NOT EXISTS password_hash TEXT,
    ADD COLUMN IF NOT EXISTS email_verified_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS oauth_provider TEXT,
    ADD COLUMN IF NOT EXISTS oauth_provider_id TEXT,
    ADD COLUMN IF NOT EXISTS cloud_tenant_id TEXT,
    ADD COLUMN IF NOT EXISTS is_active BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS last_login_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS auth_users_cloud_tenant_id_idx
    ON auth_users(cloud_tenant_id);
