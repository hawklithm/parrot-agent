CREATE TABLE IF NOT EXISTS user_inbox_agent_policies (
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    mode TEXT NOT NULL DEFAULT 'open',
    allowed_agent_ids UUID[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (company_id, user_id),
    CONSTRAINT user_inbox_agent_policies_mode_check CHECK (mode IN ('open', 'allowlist'))
);

CREATE INDEX IF NOT EXISTS idx_user_inbox_agent_policies_company
    ON user_inbox_agent_policies(company_id);
