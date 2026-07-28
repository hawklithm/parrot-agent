CREATE TABLE IF NOT EXISTS routine_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    routine_id UUID NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    document_key TEXT NOT NULL,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    UNIQUE (routine_id, document_key)
);

CREATE TABLE IF NOT EXISTS document_annotation_threads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    routine_id UUID REFERENCES routines(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    document_key TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    anchor_state TEXT NOT NULL DEFAULT 'active',
    original_revision_id UUID,
    original_revision_number INTEGER NOT NULL,
    current_revision_id UUID,
    current_revision_number INTEGER NOT NULL,
    selected_text TEXT NOT NULL,
    prefix_text TEXT NOT NULL DEFAULT '',
    suffix_text TEXT NOT NULL DEFAULT '',
    normalized_start INTEGER NOT NULL,
    normalized_end INTEGER NOT NULL,
    markdown_start INTEGER NOT NULL,
    markdown_end INTEGER NOT NULL,
    anchor_confidence TEXT NOT NULL DEFAULT 'exact',
    anchor_selector JSONB NOT NULL,
    created_by_agent_id UUID,
    created_by_user_id TEXT,
    resolved_by_agent_id UUID,
    resolved_by_user_id TEXT,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS document_annotation_threads_routine_idx ON document_annotation_threads(company_id, routine_id, status);

CREATE TABLE IF NOT EXISTS document_annotation_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    thread_id UUID NOT NULL REFERENCES document_annotation_threads(id) ON DELETE CASCADE,
    routine_id UUID REFERENCES routines(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    author_type TEXT NOT NULL DEFAULT 'user',
    author_agent_id UUID,
    author_user_id TEXT,
    created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS document_annotation_comments_thread_idx ON document_annotation_comments(company_id, thread_id, created_at);
