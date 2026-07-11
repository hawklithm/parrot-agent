-- Migration 010: Create Issue Tree Control and Auxiliary Tables
-- This migration creates tables for issue tree control, work products, and attachments

-- Create issue_tree_holds table
CREATE TABLE IF NOT EXISTS issue_tree_holds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    root_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,

    mode TEXT NOT NULL, -- 'pause' | 'resume' | 'cancel' | 'restore'
    reason TEXT,

    -- Release policy
    release_policy_strategy TEXT NOT NULL DEFAULT 'manual', -- 'manual' | 'all_done' | 'first_done'
    release_policy_note TEXT,

    metadata JSONB,

    -- Actor
    actor_agent_id UUID,
    actor_user_id UUID,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    released_at TIMESTAMPTZ
);

-- Indexes for issue_tree_holds
CREATE INDEX IF NOT EXISTS issue_tree_holds_company_id_idx ON issue_tree_holds(company_id);
CREATE INDEX IF NOT EXISTS issue_tree_holds_root_issue_id_idx ON issue_tree_holds(root_issue_id);
CREATE INDEX IF NOT EXISTS issue_tree_holds_mode_idx ON issue_tree_holds(mode);
CREATE INDEX IF NOT EXISTS issue_tree_holds_released_at_idx ON issue_tree_holds(released_at);

-- Create issue_tree_hold_members table
CREATE TABLE IF NOT EXISTS issue_tree_hold_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hold_id UUID NOT NULL REFERENCES issue_tree_holds(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,

    previous_status TEXT NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for issue_tree_hold_members
CREATE INDEX IF NOT EXISTS issue_tree_hold_members_hold_id_idx ON issue_tree_hold_members(hold_id);
CREATE INDEX IF NOT EXISTS issue_tree_hold_members_issue_id_idx ON issue_tree_hold_members(issue_id);

-- Unique constraint: hold + issue (prevent duplicates)
CREATE UNIQUE INDEX IF NOT EXISTS issue_tree_hold_members_hold_issue_idx
    ON issue_tree_hold_members(hold_id, issue_id);

-- Create work_products table
CREATE TABLE IF NOT EXISTS work_products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    company_id UUID NOT NULL,

    name TEXT NOT NULL,
    description TEXT,
    artifact JSONB,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for work_products
CREATE INDEX IF NOT EXISTS work_products_issue_id_idx ON work_products(issue_id);
CREATE INDEX IF NOT EXISTS work_products_company_id_idx ON work_products(company_id);

-- Create attachments table (polymorphic: issue or case)
CREATE TABLE IF NOT EXISTS attachments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    parent_type TEXT NOT NULL, -- 'issue' | 'case'
    parent_id UUID NOT NULL,
    company_id UUID NOT NULL,

    asset_id UUID,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size BIGINT NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for attachments
CREATE INDEX IF NOT EXISTS attachments_parent_type_parent_id_idx ON attachments(parent_type, parent_id);
CREATE INDEX IF NOT EXISTS attachments_company_id_idx ON attachments(company_id);
CREATE INDEX IF NOT EXISTS attachments_created_at_idx ON attachments(created_at);

-- Create issue_approvals table (links to external approval system)
CREATE TABLE IF NOT EXISTS ise_approvals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    company_id UUID NOT NULL,

    approval_id UUID NOT NULL, -- External approval ID

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for issue_approvals
CREATE INDEX IF NOT EXISTS issue_approvals_issue_id_idx ON issue_approvals(issue_id);
CREATE INDEX IF NOT EXISTS issue_approvals_company_id_idx ON issue_approvals(company_id);
CREATE INDEX IF NOT EXISTS issue_approvals_approval_id_idx ON issue_approvals(approval_id);

-- Unique constraint: issue + approval
CREATE UNIQUE INDEX IF NOT EXISTS issue_approvals_issue_approval_idx
    ON issue_approvals(issue_id, approval_id);

-- Create issue_watchdogs table
CREATE TABLE IF NOT EXISTS issue_watchdogs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    company_id UUID NOT NULL,

    config JSONB NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for issue_watchdogs
CREATE INDEX IF NOT EXISTS issue_watchdogs_issue_id_idx ON issue_watchdogs(issue_ATE INDEX IF NOT EXISTS issue_watchdogs_company_id_idx ON issue_watchdogs(company_id);

-- Unique constraint: one watchdog per issue
CREATE UNIQUE INDEX IF NOT EXISTS issue_watchdogs_issue_id_unique_idx ON issue_watchdogs(issue_id);

-- Create labels table (company-wide)
CREATE TABLE IF NOT EXISTS labels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,

    name TEXT NOT NULL,
    color TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for labels
CREATE INDEX IF NOT EXISTS labels_company_id_idx ON labels(company_id);
CREATE UNIQUE INX IF NOT EXISTS labels_company_name_idx ON labels(company_id, name);

-- Create issue_labels junction table
CREATE TABLE IF NOT EXISTS issue_labels (
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    label_id UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (issue_id, label_id)
);

-- Indexes for issue_labels
CREATE INDEX IF NOT EXISTS issue_labels_issue_id_idx ON issue_labels(issue_id);
CREATE INDEX IF NOT EXISTS issue_labels_label_id_idx ON issue_labels(label_id);

-- Creates junction table
CREATE TABLE IF NOT EXISTS case_labels (
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    label_id UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (case_id, label_id)
);

-- Indexes for case_labels
CREATE INDEX IF NOT EXISTS case_labels_case_id_idx ON case_labels(case_id);
CREATE INDEX IF NOT EXISTS case_labels_label_id_idx ON case_labels(label_id);
