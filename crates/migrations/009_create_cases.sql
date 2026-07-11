-- Migration 009: Create Cases and Related Tables
-- This migration creates the cases table and case-issue linking structures

-- Create cases table
CREATE TABLE IF NOT EXISTS cases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    project_id UUID,

    -- Case identification
    case_number INTEGER NOT NULL,
    identifier TEXT NOT NULL,
    case_type TEXT NOT NULL,
    key TEXT,

    -- Basic fields
    title TEXT NOT NULL,
    summary TEXT,
    status TEXT NOT NULL DEFAULT 'draft',

    -- Custom fields (JSONB)
    fields JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Hierarchy
    parent_case_id UUID REFERENCES cases(id) ON DELETE SET NULL,

    -- Creation tracking
    created_by_agent_id UUID,
    created_by_user_id UUID,

    -- Lifecycle timestamps
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for cases table
CREATE INDEX IF NOT EXISTS cases_company_id_idx ON cases(company_id);
CREATE INDEX IF NOT EXISTS cases_project_id_idx ON cases(project_id);
CREATE INDEX IF NOT EXISTS cases_case_type_idx ON cases(case_type);
CREATE INDEX IF NOT EXISTS cases_status_idx ON cases(status);
CREATE INDEX IF NOT EXISTS cases_parent_case_id_idx ON cases(parent_case_id);
CREATE INDEX IF NOT EXISTS cases_created_at_idx ON cases(created_at);

-- Unique constraint: company + case_type + key
CREATE UNIQUE INDEX IF NOT EXISTS cases_company_type_key_idx
    ON cases(company_id, case_type, key)
    WHERE key IS NOT NULL;

-- Unique constraint: company + identifier
CREATE UNIQUE INDEX IF NOT EXISTS cases_company_identifier_idx
    ON cases(company_id, identifier);

-- Create case_issue_links table (many-to-many)
CREATE TABLE IF NOT EXISTS case_issue_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,

    role TEXT NOT NULL, -- 'origin' | 'work' | 'reference'

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for case_issue_links
CREATE INDEX IF NOT EXISTS case_issue_links_case_id_idx ON case_issue_links(case_id);
CREATE INDEX IF NOT EXISTS case_issue_links_issue_id_idx ON case_issue_links(issue_id);
CREATE INDEX IF NOT EXISTS case_issue_links_company_id_idx ON case_issue_links(company_id);
CREATE INDEX IF NOT EXISTS case_issue_links_role_idx ON case_issue_links(role);

-- Unique constraint: case + issue (prevent duplicate links)
CREATE UNIQUE INDEX IF NOT EXISTS case_issue_links_case_issue_idx
    ON case_issue_links(case_id, issue_id);

-- Create case_events table (event sourcing)
CATE TABLE IF NOT EXISTS case_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    company_id UUID NOT NULL,

    kind TEXT NOT NULL, -- 'created' | 'updated' | 'status_changed' | 'document_revised' | 'issue_linked' | 'issue_unlinked'
    metadata JSONB,

    actor_agent_id UUID,
    actor_user_id UUID,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for case_events
CREATE INDEX IF NOT EXISTS case_events_case_id_idx ON case_events(case_id);
CREATE INDEX IF NOT EXISTS case_events_company_id_idx ON case_events(company_id);
CREATE INDEX IF NOT EXISTS case_events_kind_idx ON case_events(kind);
CREATE INDEX IF NOT EXISTS case_events_created_at_idx ON case_events(created_at);

-- Create case_documents table
CREATE TABLE IF NOT EXISTS case_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
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

-- Indexes for case_documents
CREATE INDEX IF NOT EXISTS case_documents_case_id_idx ON case_documents(case_id);
CREATE INDEX IF NOT EXISTS case_documents_company_id_idx ON case_documents(company_id);
CREATE UNIQUE INDEX IF NOT EXISTS case_documents_case_key_idx ON case_documents(case_id, key);
