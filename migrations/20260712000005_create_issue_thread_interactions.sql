-- Task watchdog support: issue thread interactions.
-- Mirrors paperclip issue_thread_interactions: pending interactions/approvals
-- that keep a stopped watchdog issue in the in_review review path.

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'issue_thread_interaction_status') THEN
        CREATE TYPE issue_thread_interaction_status AS ENUM ('pending', 'resolved', 'cancelled');
    END IF;
END
$$;

CREATE TABLE issue_thread_interactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'question',
    status issue_thread_interaction_status NOT NULL DEFAULT 'pending',
    source_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_interaction_kind CHECK (kind IN ('question', 'approval', 'review'))
);

CREATE INDEX issue_thread_interactions_issue_idx ON issue_thread_interactions(issue_id);
CREATE INDEX issue_thread_interactions_company_issue_idx ON issue_thread_interactions(company_id, issue_id);
