-- Add missing columns to auth_users table
-- These columns are required by the authentication code but were missing from the initial schema

ALTER TABLE auth_users 
    ADD COLUMN IF NOT EXISTS password_hash TEXT,
    ADD COLUMN IF NOT EXISTS email_verified_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS oauth_provider VARCHAR(50),
    ADD COLUMN IF NOT EXISTS oauth_provider_id VARCHAR(255),
    ADD COLUMN IF NOT EXISTS cloud_tenant_id UUID,
    ADD COLUMN IF NOT EXISTS is_active BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN IF NOT EXISTS last_login_at TIMESTAMPTZ;

-- Create index for OAuth lookup
CREATE INDEX IF NOT EXISTS idx_auth_users_oauth 
    ON auth_users(oauth_provider, oauth_provider_id) 
    WHERE oauth_provider IS NOT NULL;

-- Create index for cloud tenant lookup
CREATE INDEX IF NOT EXISTS idx_auth_users_cloud_tenant 
    ON auth_users(cloud_tenant_id) 
    WHERE cloud_tenant_id IS NOT NULL;

-- Create index for active users
CREATE INDEX IF NOT EXISTS idx_auth_users_active 
    ON auth_users(is_active) 
    WHERE is_active = true;

COMMENT ON COLUMN auth_users.password_hash IS 'Bcrypt hashed password for email/password authentication';
COMMENT ON COLUMN auth_users.email_verified_at IS 'Timestamp when email was verified';
COMMENT ON COLUMN auth_users.oauth_provider IS 'OAuth provider name (e.g., github, google)';
COMMENT ON COLUMN auth_users.oauth_provider_id IS 'User ID from OAuth provider';
COMMENT ON COLUMN auth_users.cloud_tenant_id IS 'Reference to cloud tenant for multi-tenancy';
COMMENT ON COLUMN auth_users.is_active IS 'Whether the user account is active';
COMMENT ON COLUMN auth_users.last_login_at IS 'Timestamp of last successful login';
