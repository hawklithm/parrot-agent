-- Create enum types
CREATE TYPE invite_type AS ENUM ('company_join', 'bootstrap_ceo');
CREATE TYPE allowed_join_types AS ENUM ('human', 'agent', 'both');
CREATE TYPE join_request_status AS ENUM ('pending_approval', 'approved', 'rejected');

-- Create invites table
CREATE TABLE invites (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    invite_type invite_type NOT NULL,
    invited_email VARCHAR(255),
    invited_by_user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    token VARCHAR(255) NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    accepted BOOLEAN NOT NULL DEFAULT false,
    accepted_by_user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    accepted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create join_requests table
CREATE TABLE join_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    requester_user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    status join_request_status NOT NULL DEFAULT 'pending_approval',
    message TEXT,
    reviewed_by_user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    reviewed_at TIMESTAMPTZ,
    rejection_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for invites
CREATE INDEX idx_invites_company_id ON invites(company_id);
CREATE INDEX idx_invites_token ON invites(token);
CREATE INDEX idx_invites_expires_at ON invites(expires_at);
CREATE INDEX idx_invites_accepted ON invites(accepted);

-- Create indexes for join_requests
CREATE INDEX idx_join_requests_company_id ON join_requests(company_id);
CREATE INDEX idx_join_requests_requester_user_id ON join_requests(requester_user_id);
CREATE INDEX idx_join_requests_status ON join_requests(status);

-- Create trigger for updated_at
CREATE TRIGGER update_join_requests_updated_at
    BEFORE UPDATE ON join_requests
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
