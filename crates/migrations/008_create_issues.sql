-- Migration 008: Create Issues and Related Tables
-- This migration creates the core issues table and related structures

-- Create issues table
CREATE TABLE IF NOT EXISTS issues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    project_id UUID,
    project_workspace_id UUID,
    goal_id UUID,
    parent_id UUID REFERENCES issues(id) ON DELETE SET NULL,

    -- Basic fields
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'backlog',
    work_mode TEXT NOT NULL DEFAULT 'standard',
    priority TEXT NOT NULL DEFAULT 'medium',

    -- Assignment
    assignee_agent_id UUID,
    assignee_user_id UUID,
    checkout_run_id UUID,

    -- Execution tracking
    execution_run_id UUID,
    execution_agent_name_key TEXT,
    execution_locked_at TIMESTAMPTZ,
    execution_policy JSONB,
    execution_state JSONB,

    -- Creation tracking
    created_by_agent_id UUID,
    created_by_user_id UUID,
    responsible_user_id UUID,

    -- Identifiers
    issue_number INTEGER,
    identifier TEXT,

    -- Origin tracking
    origin_kind TEXT,
    origin_id TEXT,
    origin_run_id UUID,
    origin_fingerprint TEXT,
    request_depth INTEGER NOT NULL DEFAULT 0,

    -- Billing
    billing_code TEXT,

    -- Monitor fields
    monitor_next_check_at TIMESTAMPTZ,
    monitor_last_triggered_at TIMESTAMPTZ,
    monitor_attempt_count INTEGER DEFAULT 0,
    monitor_notes TEXT,
    monitor_scheduled_by TEXT,

    -- Workspace preference
    execution_workspace_id UUID,
    execution_workspace_preference TEXT,

    -- Lifecycle timestamps
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    hidden_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for issues table
CREATE INDEX IF NOT EXISTS issues_company_id_idx ON issues(company_id);
CREATE INDEX IF NOT EXISTS issues_project_id_idx ON issues(project_id);
CREATE INDEX IF NOT EXISTS issues_goal_id_idx ON issues(goal_id);
CREATE INDEX IF NOT EXISTS issues_parent_id_idx ON issues(parent_id);
CREATE INDEX IF NOT EXISTS issues_status_idx ON issues(status);
CREATE INDEX IF NOT EXISTS issues_priority_idx ON issues(priority);
CREATE INDEX IF NOT EXISTS issues_assignee_agent_id_idx ON issues(assignee_agent_id);
CREATE INDEX IF NOT EXISTS issues_assignee_user_id_idx ON issues(assignee_user_id);
CREATE INDEX IF NOT EXISTS issues_checkout_run_id_idx ON issues(checkout_run_id);
CREATE INDEX IF NOT EXISTS issues_execution_run_id_idx ON issues(execution_run_id);
CREATE INDEX IF NOT ISTS issues_created_at_idx ON issues(created_at);
CREATE INDEX IF NOT EXISTS issues_monitor_next_check_at_idx ON issues(monitor_next_check_at) WHERE monitor_next_check_at IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS issues_company_identifier_idx ON issues(company_id, identifier) WHERE identifier IS NOT NULL;

-- Create issue_comments table
CREATE TABLE IF NOT EXISTS issue_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    company_id UUID NOT NULL,

    body TEXT NOT NULL,
    author_type TEXT NOT NULL,
    author_agent_id UUID,
    authd UUID,
    created_by_run_id UUID,

    follow_up_requested BOOLEAN DEFAULT FALSE,
    metadata JSONB,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for issue_comments
CREATE INDEX IF NOT EXISTS issue_comments_issue_id_idx ON issue_comments(issue_id);
CREATE INDEX IF NOT EXISTS issue_comments_company_id_idx ON issue_comments(company_id);
CREATE INDEX IF NOT EXISTS issue_comments_author_agent_id_idx ON issue_comments(author_agent_id);
CREATE INDEX IF NOT EXISTS issue_comments_author_user_id_idx ON issue_comments(author_user_id);
CREATE INDEX IF NOT EXISTS issue_comments_created_at_idx ON issue_comments(created_at);

-- Create issue_documents table
CREATE TABLE IF NOT EXISTS issue_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    company_id UUID NOT NULL,

    key TEXT NOT NULL,
    content TEXT NOT NULL,
    content_type TEXT,

    locked_by_agent_id UUID,
    locked_by_user_id UUID,
    locked_at TIMESTAMPTZ,
    locked_run_id UUID,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for issue_documents
CREATE INDEX IF NOT EXISTS issue_documents_issue_id_idx ON issue_documents(issue_id);
CREATE INDEX IF NOT EXISTS issue_documents_company_id_idx ON issue_documents(company_id);
CREATE UNIQUE INDEX IF NOT EXISTS issue_documents_issue_key_idx ON issue_documents(issue_id, key);

-- Create issue_thread_interactions table
CREATE TABLE IF NOT EXISTS issue_thread_interactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    company_id UUID NOT NULL,

    interaction_type TEXT NOT NULL,
    actor_agent_id UUID,
    actor_user_id UUID,

    metadata JSONB,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ
);

-- Indexes for issue_thread_interactions
CREATE INDEX IF NOT EXISTS issue_thread_interactions_issue_id_idx ON issue_thread_interactions(issue_id);
CREATE INDEX IF NOT EXISTS issue_thread_interactions_company_id_idx ON issue_thread_interactions(company_id);
CREATE INDEX IF NOT EXISTS issue_thread_interactions_interaction_type_idx ON issue_thread_interactions(interaction_type);
CREATE INDEX IF NOT EXISTS issue_thread_interactions_resolved_at_idx ON issue_thread_interactions(resolved_at);
