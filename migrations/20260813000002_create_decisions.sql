-- P0.1 Decision domain: decision bundles, decisions, target issues, effect executions
-- Ported from Paperclip packages/db/src/schema/decisions.ts

CREATE TABLE IF NOT EXISTS decision_bundles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    summary TEXT,
    origin_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    origin_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    origin_run_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_decision_bundles_company_created
    ON decision_bundles(company_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_decision_bundles_origin_issue
    ON decision_bundles(origin_issue_id);

CREATE TABLE IF NOT EXISTS decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    bundle_id UUID REFERENCES decision_bundles(id) ON DELETE SET NULL,
    origin_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    origin_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    origin_run_id UUID NOT NULL,
    rule_key TEXT,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    options JSONB NOT NULL,
    inputs JSONB,
    status TEXT NOT NULL DEFAULT 'open',
    execution_status TEXT,
    chosen_option_id TEXT,
    input_values JSONB,
    decided_by_user_id TEXT,
    decided_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL,
    idempotency_key TEXT,
    signed_spec TEXT NOT NULL DEFAULT '',
    target_snapshots JSONB NOT NULL DEFAULT '{}'::jsonb,
    continuation_policy TEXT NOT NULL DEFAULT 'none',
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decisions_status_check
        CHECK (status IN ('open', 'decided', 'cancelled', 'expired')),
    CONSTRAINT decisions_continuation_policy_check
        CHECK (continuation_policy IN ('none', 'wake_origin_agent'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uniq_decisions_company_idempotency
    ON decisions(company_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_decisions_company_status_expires
    ON decisions(company_id, status, expires_at);
CREATE INDEX IF NOT EXISTS idx_decisions_bundle
    ON decisions(bundle_id);
CREATE INDEX IF NOT EXISTS idx_decisions_origin_issue
    ON decisions(origin_issue_id);
CREATE INDEX IF NOT EXISTS idx_decisions_origin_agent
    ON decisions(origin_agent_id);
CREATE INDEX IF NOT EXISTS idx_decisions_company_rule_key
    ON decisions(company_id, rule_key);

CREATE TABLE IF NOT EXISTS decision_target_issues (
    decision_id UUID NOT NULL REFERENCES decisions(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    PRIMARY KEY (decision_id, issue_id)
);

CREATE INDEX IF NOT EXISTS idx_decision_target_issues_issue
    ON decision_target_issues(issue_id);
CREATE INDEX IF NOT EXISTS idx_decision_target_issues_company
    ON decision_target_issues(company_id);

CREATE TABLE IF NOT EXISTS decision_effect_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    decision_id UUID NOT NULL REFERENCES decisions(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    effect_index INTEGER NOT NULL,
    effect_type TEXT NOT NULL,
    target_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'claimed',
    result JSONB,
    error TEXT,
    activity_log_id UUID,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decision_effect_executions_status_check
        CHECK (status IN ('claimed', 'succeeded', 'failed', 'skipped'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uniq_decision_effect_executions_decision_index
    ON decision_effect_executions(decision_id, effect_index);
CREATE INDEX IF NOT EXISTS idx_decision_effect_executions_company
    ON decision_effect_executions(company_id);
CREATE INDEX IF NOT EXISTS idx_decision_effect_executions_target_issue
    ON decision_effect_executions(target_issue_id);
