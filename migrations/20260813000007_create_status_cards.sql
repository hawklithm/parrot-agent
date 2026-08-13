-- 对齐 Paperclip status-cards 域（status_cards + updates + summary revisions）
CREATE TABLE IF NOT EXISTS status_cards (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    created_by_user_id TEXT,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    title TEXT,
    title_pinned BOOLEAN NOT NULL DEFAULT false,
    interest_prompt TEXT NOT NULL DEFAULT '',
    queries JSONB NOT NULL DEFAULT '[]',
    query_version INTEGER NOT NULL DEFAULT 0,
    query_compiled_at TIMESTAMPTZ,
    query_compiled_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    refresh_policy JSONB NOT NULL DEFAULT '{}',
    state TEXT NOT NULL DEFAULT 'compiling',   -- compiling|active|error|paused_budget|paused_hours
    pending_change_count INTEGER NOT NULL DEFAULT 0,
    archived_at TIMESTAMPTZ,
    archived_by_user_id TEXT,
    generating_issue_id UUID,
    summary_markdown TEXT,
    summary_compiled_at TIMESTAMPTZ,
    summary_compiled_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_status_cards_company ON status_cards(company_id);

CREATE TABLE IF NOT EXISTS status_card_updates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    card_id UUID NOT NULL REFERENCES status_cards(id) ON DELETE CASCADE,
    issue_id UUID,
    identifier TEXT,
    from_status TEXT,
    to_status TEXT,
    change_kind TEXT NOT NULL DEFAULT 'status',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_status_card_updates_card ON status_card_updates(card_id);

CREATE TABLE IF NOT EXISTS status_card_summary_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    card_id UUID NOT NULL REFERENCES status_cards(id) ON DELETE CASCADE,
    markdown TEXT NOT NULL,
    compiled_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_status_card_summary_revisions_card ON status_card_summary_revisions(card_id);
