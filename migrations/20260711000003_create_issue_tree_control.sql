-- Create issue tree control mode enum
CREATE TYPE issue_tree_control_mode AS ENUM ('pause', 'resume', 'cancel', 'restore');

-- Create issue tree hold status enum
CREATE TYPE issue_tree_hold_status AS ENUM ('active', 'released');

-- Create hold release policy strategy enum
CREATE TYPE hold_release_strategy AS ENUM ('manual', 'all_done', 'first_done');

-- Create issue_tree_holds table
CREATE TABLE issue_tree_holds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    root_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    mode issue_tree_control_mode NOT NULL,
    status issue_tree_hold_status NOT NULL DEFAULT 'active',
    reason TEXT,
    release_policy JSONB NOT NULL DEFAULT '{"strategy":"manual"}'::jsonb,
    metadata JSONB,
    actor_type TEXT,
    actor_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    released_at TIMESTAMPTZ,
    released_by_type TEXT,
    released_by_id UUID
);

CREATE INDEX issue_tree_holds_root_issue_idx ON issue_tree_holds(root_issue_id);
CREATE INDEX issue_tree_holds_company_status_idx ON issue_tree_holds(company_id, status);
CREATE INDEX issue_tree_holds_mode_idx ON issue_tree_holds(mode, status);

-- Create issue_tree_hold_members table
CREATE TABLE issue_tree_hold_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    hold_id UUID NOT NULL REFERENCES issue_tree_holds(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    parent_issue_id UUID REFERENCES issues(id),
    depth INTEGER NOT NULL,
    issue_identifier TEXT,
    issue_title TEXT NOT NULL,
    issue_status TEXT NOT NULL,
    assignee_agent_id UUID,
    assignee_user_id UUID,
    active_run_id UUID,
    active_run_status TEXT,
    skipped BOOLEAN NOT NULL DEFAULT false,
    skip_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(hold_id, issue_id)
);

CREATE INDEX issue_tree_hold_members_hold_idx ON issue_tree_hold_members(hold_id);
CREATE INDEX issue_tree_hold_members_issue_idx ON issue_tree_hold_members(issue_id);
CREATE INDEX issue_tree_hold_members_company_idx ON issue_tree_hold_members(company_id);

-- Add updated_at triggers
CREATE TRIGGER update_issue_tree_holds_updated_at BEFORE UPDATE ON issue_tree_holds
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
