-- Create cli_auth_challenges table
CREATE TABLE cli_auth_challenges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    challenge_code VARCHAR(50) NOT NULL UNIQUE,
    user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    approved BOOLEAN NOT NULL DEFAULT false,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create instance_user_roles table
CREATE TABLE instance_user_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_user_role UNIQUE (user_id, role)
);

-- Create indexes
CREATE INDEX idx_cli_auth_challenges_challenge_code ON cli_auth_challenges(challenge_code);
CREATE INDEX idx_cli_auth_challenges_expires_at ON cli_auth_challenges(expires_at);
CREATE INDEX idx_instance_user_roles_user_id ON instance_user_roles(user_id);
