-- Company Skill Policy: per-company governance over which skills may be
-- installed / executed, by agent, role, action, source and protected-skill set.
-- Aligned with paperclip company-skill-policy.

CREATE TABLE IF NOT EXISTS company_skill_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    policy JSONB NOT NULL DEFAULT '{}'::jsonb,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (company_id)
);

CREATE INDEX IF NOT EXISTS company_skill_policies_company_idx
    ON company_skill_policies(company_id);
