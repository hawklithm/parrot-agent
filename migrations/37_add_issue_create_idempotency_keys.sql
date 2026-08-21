CREATE TABLE IF NOT EXISTS issue_create_idempotency_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT issue_create_idempotency_keys_company_key_uq UNIQUE (company_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS issue_create_idempotency_keys_issue_idx
    ON issue_create_idempotency_keys(issue_id);

CREATE INDEX IF NOT EXISTS issue_create_idempotency_keys_company_created_at_idx
    ON issue_create_idempotency_keys(company_id, created_at);
