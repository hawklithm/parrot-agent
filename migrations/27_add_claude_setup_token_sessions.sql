-- Durable, non-secret state for company-owned Claude setup-token login flows.
-- Login URLs, browser codes, tokens and process output are intentionally not
-- stored here; the transport layer owns those values in memory.
CREATE TABLE IF NOT EXISTS claude_setup_token_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id TEXT NOT NULL,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    owner_user_id TEXT NOT NULL,
    adapter_type TEXT NOT NULL,
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    state TEXT NOT NULL DEFAULT 'starting',
    deadline_at TIMESTAMPTZ NOT NULL,
    bound_at TIMESTAMPTZ,
    failure_reason TEXT,
    failure_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS claude_setup_token_sessions_session_id_uq
    ON claude_setup_token_sessions(session_id);
CREATE UNIQUE INDEX IF NOT EXISTS claude_setup_token_sessions_active_uq
    ON claude_setup_token_sessions(company_id, owner_user_id, adapter_type, environment_id)
    WHERE state IN ('starting', 'awaiting_code', 'submitting', 'persisting');
CREATE INDEX IF NOT EXISTS claude_setup_token_sessions_deadline_idx
    ON claude_setup_token_sessions(deadline_at);
