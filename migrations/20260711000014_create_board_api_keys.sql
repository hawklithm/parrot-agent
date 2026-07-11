-- Create board_api_keys table
CREATE TABLE board_api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    key_hash VARCHAR(255) NOT NULL,
    key_prefix VARCHAR(20) NOT NULL,
    name VARCHAR(255),
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes
CREATE INDEX idx_board_api_keys_company_id ON board_api_keys(company_id);
CREATE INDEX idx_board_api_keys_user_id ON board_api_keys(user_id);
CREATE INDEX idx_board_api_keys_key_hash ON board_api_keys(key_hash);
CREATE INDEX idx_board_api_keys_expires_at ON board_api_keys(expires_at) WHERE expires_at IS NOT NULL;
