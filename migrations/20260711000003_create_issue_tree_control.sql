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


-- Create issue watchdog status enum
CREATE TYPE issue_watchdog_status AS ENUM ('active', 'disabled');

-- Create issue_watchdogs table
CREATE TABLE issue_watchdogs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    watchdog_agent_id UUID NOT NULL REFERENCES agents(id),
    instructions TEXT,
    status issue_watchdog_status NOT NULL DEFAULT 'active',
    watchdog_issue_id UUID REFERENCES issues(id),
    last_observed_fingerprint TEXT,
    last_reviewed_fingerprint TEXT,
    last_triggered_at TIMESTAMPTZ,
    last_completed_at TIMESTAMPTZ,
    trigger_count INTEGER NOT NULL DEFAULT 0,
    created_by_agent_id UUID REFERENCES agents(id),
    created_by_user_id UUID,
    created_by_run_id UUID,
    updated_by_agent_id UUID REFERENCES agents(id),
    updated_by_user_id UUID,
    updated_by_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id)
);

CREATE INDEX issue_watchdogs_issue_idx ON issue_watchdogs(issue_id);
CREATE INDEX issue_watchdogs_watchdog_agent_idx ON issue_watchdogs(watchdog_agent_id);
CREATE INDEX issue_watchdogs_status_idx ON issue_watchdogs(company_id, status);

-- Create recovery action status enum
CREATE TYPE recovery_action_status AS ENUM ('pending', 'in_progress', 'resolved', 'failed');

-- Create issue_recovery_actions table
CREATE TABLE issue_recovery_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    action_type TEXT NOT NULL,
    status recovery_action_status NOT NULL DEFAULT 'pending',
    details JSONB,
    resolved_at TIMESTAMPTZ,
    resolved_by_type TEXT,
    resolved_by_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX issue_recovery_actions_issue_idx ON issue_recovery_actions(issue_id);
CREATE INDEX issue_recovery_actions_status_idx ON issue_recovery_actions(issue_id, status);

-- Create issue_read_status table (track user read status)
CREATE TABLE issue_read_status (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    read_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, user_id)
);

CREATE INDEX issue_read_status_issue_idx ON issue_read_status(issue_id);
CREATE INDEX issue_read_status_user_idx ON issue_read_status(user_id);

-- Create issue_inbox_archives table (track user inbox archive status)
CREATE TABLE issue_inbox_archives (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, user_id)
);

CREATE INDEX issue_inbox_archives_issue_idx ON issue_inbox_archives(issue_id);
CREATE INDEX issue_inbox_archives_user_idx ON issue_inbox_archives(user_id);

-- Create feedback_votes table
CREATE TABLE feedback_votes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    voter_id UUID NOT NULL,
    vote TEXT NOT NULL,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, voter_id)
);

CREATE INDEX feedback_votes_issue_idx ON feedback_votes(issue_id);

-- Create feedback_traces table
CREATE TABLE feedback_traces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    trace_data JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX feedback_traces_issue_idx ON feedback_traces(issue_id);

-- Create accepted_plan_decompositions table
CREATE TABLE accepted_plan_decompositions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    plan JSONB NOT NULL,
    created_by_agent_id UUID REFERENCES agents(id),
    created_by_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX accepted_plan_decompositions_issue_idx ON accepted_plan_decompositions(issue_id);

-- Add updated_at triggers
CREATE TRIGGER update_issue_tree_holds_updated_at BEFORE UPDATE ON issue_tree_holds
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_issue_watchdogs_updated_at BEFORE UPDATE ON issue_watchdogs
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_issue_recovery_actions_updated_at BEFORE UPDATE ON issue_recovery_actions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
