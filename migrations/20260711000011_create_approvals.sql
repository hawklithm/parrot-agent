-- Create enum types for approvals
CREATE TYPE approval_type AS ENUM ('hire_agent', 'spend_credits', 'create_resource', 'deploy_agent');
CREATE TYPE approval_status AS ENUM ('pending', 'approved', 'rejected', 'revision_requested');

-- Create approvals table
CREATE TABLE approvals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    approval_type approval_type NOT NULL,
    requested_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    requested_by_user_id UUID,
    status approval_status NOT NULL DEFAULT 'pending',
    payload JSONB NOT NULL,
    decision_note TEXT,
    decided_by_user_id UUID,
    decided_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create issue_approvals link table
CREATE TABLE issue_approvals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    approval_id UUID NOT NULL REFERENCES approvals(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    CONSTRAINT unique_approval_issue UNIQUE (approval_id, issue_id)
);

-- Create indexes for approvals
CREATE INDEX idx_approvals_company_id ON approvals(company_id);
CREATE INDEX idx_approvals_status ON approvals(status);
CREATE INDEX idx_approvals_requested_by_agent_id ON approvals(requested_by_agent_id) WHERE requested_by_agent_id IS NOT NULL;
CREATE INDEX idx_approvals_created_at ON approvals(created_at DESC);

-- Create indexes for issue_approvals
CREATE INDEX idx_issue_approvals_approval_id ON issue_approvals(approval_id);
CREATE INDEX idx_issue_approvals_issue_id ON issue_approvals(issue_id);

-- Create trigger for updated_at
CREATE TRIGGER update_approvals_updated_at
    BEFORE UPDATE ON approvals
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
