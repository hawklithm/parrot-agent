-- 对齐 Paperclip company_secret_proposals 域（agent 提案 secret/binding，board 审批）
CREATE TABLE IF NOT EXISTS company_secret_proposals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,                          -- secret | binding
    status TEXT NOT NULL DEFAULT 'pending',      -- pending|approved|rejected|withdrawn|expired
    proposed_name TEXT,
    proposed_key TEXT,
    proposed_description TEXT,
    justification TEXT NOT NULL,
    value_ciphertext JSONB,
    value_fingerprint_sha256 TEXT,
    value_length INTEGER,
    secret_id UUID REFERENCES company_secrets(id) ON DELETE SET NULL,
    target_type TEXT,
    target_id UUID,
    config_path TEXT,
    proposed_by_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    origin_issue_id UUID,
    origin_run_id UUID,
    resolved_by_user_id TEXT,
    resolved_at TIMESTAMPTZ,
    resolution_reason TEXT,
    created_secret_id UUID REFERENCES company_secrets(id) ON DELETE SET NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_secret_proposals_company_status
    ON company_secret_proposals(company_id, status);
CREATE INDEX IF NOT EXISTS idx_secret_proposals_proposer_status
    ON company_secret_proposals(proposed_by_agent_id, status);
