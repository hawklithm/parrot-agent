-- P0.1 Decision domain: training examples (decision snapshot capture for fine-tuning export)
-- Ported from Paperclip packages/db/src/schema/decision_training_examples.ts

CREATE TABLE IF NOT EXISTS decision_training_examples (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_id UUID NOT NULL,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    cutoff_at TIMESTAMPTZ NOT NULL,
    notes TEXT NOT NULL DEFAULT '',
    notes_history JSONB NOT NULL DEFAULT '[]'::jsonb,
    decision_outcome TEXT,
    retention_policy TEXT NOT NULL DEFAULT 'scrub_deleted_comments_v1',
    snapshot JSONB NOT NULL,
    created_by_user_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decision_training_examples_source_kind_check
        CHECK (source_kind IN ('interaction', 'approval', 'execution_decision')),
    CONSTRAINT uniq_decision_training_examples_source_author
        UNIQUE (source_kind, source_id, created_by_user_id)
);

CREATE INDEX IF NOT EXISTS idx_decision_training_examples_company_created
    ON decision_training_examples(company_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_decision_training_examples_issue
    ON decision_training_examples(issue_id);
CREATE INDEX IF NOT EXISTS idx_decision_training_examples_author
    ON decision_training_examples(company_id, created_by_user_id);
