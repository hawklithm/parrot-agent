-- 对齐 Paperclip summary-slots 域
CREATE TABLE IF NOT EXISTS summary_slots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    scope_kind TEXT NOT NULL,
    scope_id UUID,
    slot_key TEXT NOT NULL,
    document_id UUID,
    status TEXT NOT NULL DEFAULT 'idle',       -- idle|generating|error
    failure_reason TEXT,
    generating_issue_id UUID,
    last_generated_at TIMESTAMPTZ,
    last_generated_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    last_model TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (company_id, scope_kind, scope_id, slot_key)
);

CREATE INDEX IF NOT EXISTS idx_summary_slots_company ON summary_slots(company_id, scope_kind, scope_id);

CREATE TABLE IF NOT EXISTS summary_slot_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slot_id UUID NOT NULL REFERENCES summary_slots(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL DEFAULT 1,
    markdown TEXT NOT NULL,
    title TEXT,
    change_summary TEXT,
    base_revision_id UUID,
    generation_issue_id UUID,
    model TEXT,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (slot_id, revision_number)
);
