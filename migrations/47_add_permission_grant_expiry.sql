-- Keep permission-grant storage aligned with the repository's expiry checks.
ALTER TABLE principal_permission_grants
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_principal_permission_grants_expires_at
    ON principal_permission_grants(expires_at)
    WHERE expires_at IS NOT NULL;
