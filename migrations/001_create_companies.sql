-- Create company status enum
CREATE TYPE company_status AS ENUM ('active', 'paused', 'archived');

-- Create principal type enum
CREATE TYPE principal_type AS ENUM ('user', 'agent');

-- Create membership role enum
CREATE TYPE membership_role AS ENUM ('owner', 'member');

-- Create company membership status enum
CREATE TYPE company_membership_status AS ENUM ('active', 'inactive');

-- Create companies table
CREATE TABLE companies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    status company_status NOT NULL DEFAULT 'active',
    pause_reason TEXT,
    paused_at TIMESTAMPTZ,
    issue_prefix VARCHAR(10) NOT NULL,
    issue_counter INTEGER NOT NULL DEFAULT 0,
    budget_monthly_cents BIGINT,
    spent_monthly_cents BIGINT NOT NULL DEFAULT 0,
    attachment_max_bytes BIGINT NOT NULL DEFAULT 10485760, -- 10MB default
    default_responsible_user_id UUID,
    require_board_approval_for_new_agents BOOLEAN NOT NULL DEFAULT false,
    feedback_data_sharing_enabled BOOLEAN NOT NULL DEFAULT false,
    feedback_data_sharing_consent_at TIMESTAMPTZ,
    feedback_data_sharing_consent_by_user_id UUID,
    feedback_data_sharing_terms_version VARCHAR(50),
    brand_color VARCHAR(7), -- hex color
    logo_asset_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_prefix)
);

-- Create company memberships table
CREATE TABLE company_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCEmpanies(id) ON DELETE CASCADE,
    principal_type principal_type NOT NULL,
    principal_id UUID NOT NULL,
    status company_membership_status NOT NULL DEFAULT 'active',
    membership_role membership_role NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, principal_type, principal_id)
);

-- Create indexes
CREATE INDEX idx_companies_status ON companies(status);
CREATE INDEX idx_company_memberships_company_id ON company_memberships(company_id);
CREATE INDEX idx_company_memberships_principal ON company_memberships(principal_type, principal_id);
CREATE INDEX idx_company_memberships_role ON company_memberships(membership_role);

-- Create updated_at trigger function
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Apply updated_at trigger to companies
CREATE TRIGGER update_companies_updated_at BEFORE UPDATE ON companies
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Apply updated_at trigger to company_memberships
CREATE TRIGGER update_company_memberships_updated_at BEFORE UPDATny_memberships
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
