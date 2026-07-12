-- Create issue_read_status table for tracking which users have read which issues
CREATE TABLE issue_read_status (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    read_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, user_id)
);

CREATE INDEX issue_read_status_company_idx ON issue_read_status(company_id);
CREATE INDEX issue_read_status_issue_idx ON issue_read_status(issue_id);
CREATE INDEX issue_read_status_user_idx ON issue_read_status(company_id, user_id);

-- Create issue_inbox_archives table for tracking inbox archive state per user
CREATE TABLE issue_inbox_archives (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, user_id)
);

CREATE INDEX issue_inbox_archives_company_idx ON issue_inbox_archives(company_id);
CREATE INDEX issue_inbox_archives_issue_idx ON issue_inbox_archives(issue_id);
CREATE INDEX issue_inbox_archives_user_idx ON issue_inbox_archives(company_id, user_id);
CREATE INDEX issue_inbox_archives_company_issue_idx ON issue_inbox_archives(company_id, issue_id);

-- Create feedback_votes table
CREATE TABLE feedback_votes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    voter_id UUID NOT NULL,
    voter_type TEXT NOT NULL DEFAULT 'user',
    vote TEXT NOT NULL CHECK (vote IN ('up', 'down')),
    reason TEXT,
    shared_with_labs BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, voter_id, voter_type)
);

CREATE INDEX feedback_votes_company_idx ON feedback_votes(company_id);
CREATE INDEX feedback_votes_issue_idx ON feedback_votes(issue_id);
CREATE INDEX feedback_votes_voter_idx ON feedback_votes(company_id, voter_id);

-- Create feedback_traces table
CREATE TABLE feedback_traces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    vote_id UUID NOT NULL REFERENCES feedback_votes(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,
    target_id UUID,
    payload JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'local_only' CHECK (status IN ('local_only', 'pending', 'sent', 'failed')),
    failure_reason TEXT,
    shared_with_labs BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX feedback_traces_company_idx ON feedback_traces(company_id);
CREATE INDEX feedback_traces_issue_idx ON feedback_traces(issue_id);
CREATE INDEX feedback_traces_vote_idx ON feedback_traces(vote_id);
CREATE INDEX feedback_traces_status_idx ON feedback_traces(status);

-- Create recovery_actions table
CREATE TABLE recovery_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    action_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'in_progress', 'resolved', 'failed')),
    description TEXT,
    metadata JSONB,
    triggered_by_issue_id UUID REFERENCES issues(id),
    triggered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX recovery_actions_company_idx ON recovery_actions(company_id);
CREATE INDEX recovery_actions_issue_idx ON recovery_actions(issue_id);
CREATE INDEX recovery_actions_status_idx ON recovery_actions(status);
CREATE INDEX recovery_actions_triggered_idx ON recovery_actions(company_id, status, triggered_at);

-- Create plan_decompositions table
CREATE TABLE plan_decompositions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    plan JSONB NOT NULL,
    accepted_at TIMESTAMPTZ,
    accepted_by_type TEXT,
    accepted_by_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX plan_decompositions_company_idx ON plan_decompositions(company_id);
CREATE INDEX plan_decompositions_issue_idx ON plan_decompositions(issue_id);

-- Add updated_at triggers
CREATE TRIGGER update_issue_read_status_updated_at BEFORE UPDATE ON issue_read_status
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_issue_inbox_archives_updated_at BEFORE UPDATE ON issue_inbox_archives
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_feedback_votes_updated_at BEFORE UPDATE ON feedback_votes
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_feedback_traces_updated_at BEFORE UPDATE ON feedback_traces
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_recovery_actions_updated_at BEFORE UPDATE ON recovery_actions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_plan_decompositions_updated_at BEFORE UPDATE ON plan_decompositions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
