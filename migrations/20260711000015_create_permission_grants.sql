-- Create enum types
CREATE TYPE membership_role AS ENUM ('owner', 'admin', 'operator', 'viewer');
CREATE TYPE principal_type AS ENUM ('user', 'agent');
CREATE TYPE membership_status AS ENUM ('active', 'archived');

-- Create principal_permission_grants table
CREATE TABLE principal_permission_grants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    principal_type principal_type NOT NULL,
    principal_id UUID NOT NULL,
    permission_key VARCHAR(100) NOT NULL,
    scope JSONB NOT NULL DEFAULT '{}',
    granted_by_user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_principal_permission UNIQUE (company_id, principal_type, principal_id, permission_key)
);

-- Create indexes
CREATE INDEX idx_principal_permission_grants_company_id ON principal_permission_grants(company_id);
CREATE INDEX idx_principal_permission_grants_principal ON principal_permission_grants(principal_type, principal_id);
CREATE INDEX idx_principal_permission_grants_permission_key ON principal_permission_grants(permission_key);

-- Create trigger for updated_at
CREATE TRIGGER update_principal_permission_grants_updated_at
    BEFORE UPDATE ON principal_permission_grants
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
