-- Create issue status enum
CREATE TYPE issue_status AS ENUM ('backlog', 'todo', 'in_progress', 'in_review', 'blocked', 'done', 'cancelled');

-- Create issue priority enum
CREATE TYPE issue_priority AS ENUM ('critical', 'high', 'medium', 'low');

-- Create issue work mode enum
CREATE TYPE issue_work_mode AS ENUM ('standard', 'ask', 'planning', 'skill_test');

-- Create issue monitor scheduled by enum
CREATE TYPE issue_monitor_scheduled_by AS ENUM ('assignee', 'board');

-- Create issues table
CREATE TABLE issues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    project_id UUID REFERENCES projects(id),
    project_workspace_id UUID,
    goal_id UUID,
    parent_id UUID REFERENCES issues(id),

    title TEXT NOT NULL,
    description TEXT,
    status issue_status NOT NULL DEFAULT 'backlog',
    work_mode issue_work_mode NOT NULL DEFAULT 'standard',
    priority issue_priority NOT NULL DEFAULT 'medium',

    assignee_agent_id UUID REFERENCES agents(id),
    assignee_user_id UUID,
    checkout_run_id UUID,
    execution_run_id UUID,
    execution_agent_name_key TEXT,
    execution_locked_at TIMESTAMPTZ,

    created_by_agent_id UUID REFERENCES agents(id),
    created_by_user_id UUID,
    responsible_user_id UUID,

    issue_number INTEGER,
    identifier TEXT UNIQUE,

    origin_kind TEXT NOT NULL DEFAULT 'manual',
    origin_id TEXT,
    origin_run_id UUID,
    origin_fingerprint TEXT NOT NULL DEFAULT 'default',
    request_depth INTEGER NOT NULL DEFAULT 0,

    billing_code TEXT,
    assignee_adapter_overrides JSONB,
    execution_policy JSONB,
    execution_state JSONB,

    monitor_next_check_at TIMESTAMPTZ,
    monitor_last_triggered_at TIMESTAMPTZ,
    monitor_attempt_count INTEGER NOT NULL DEFAULT 0,
    monitor_notes TEXT,
    monitor_scheduled_by issue_monitor_scheduled_by,

    execution_workspace_id UUID,
    execution_workspace_preference TEXT,
    execution_workspace_settings JSONB,

    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    hidden_at TIMESTAMPTZ,
    source_trust JSONB,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for issues table
CREATE INDEX issues_company_status_idx ON issues(company_id, status);
CREATE INDEX issues_company_assignee_status_idx ON issues(company_id, assignee_agent_id, status);
CREATE INDEX issues_company_assignee_user_status_idx ON issues(company_id, assignee_user_id, status);
CREATE INDEX issues_company_responsible_user_idx ON issues(company_id, responsible_user_id);
CREATE INDEX issues_company_parent_idx ON issues(company_id, parent_id);
CREATE INDEX issues_company_project_idx ON issues(company_id, project_id);
CREATE INDEX issues_company_origin_idx ON issues(company_id, origin_kind, origin_id);
CREATE INDEX issues_company_execution_workspace_idx ON issues(company_id, execution_workspace_id);
CREATE INDEX issues_company_monitor_due_idx ON issues(company_id, monitor_next_check_at);
CREATE INDEX issues_company_updated_idx ON issues(company_id, updated_at);
CREATE INDEX issues_company_created_idx ON issues(company_id, created_at);
CREATE INDEX issues_company_priority_idx ON issues(company_id, priority);

-- Enable pg_trgm extension for text search
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Create text search indexes
CREATE INDEX issues_title_search_idx ON issues USING gin(title gin_trgm_ops);
CREATE INDEX issues_identifier_search_idx ON issues USING gin(identifier gin_trgm_ops);
CREATE INDEX issues_description_search_idx ON issues USING gin(description gin_trgm_ops);

-- Create comment actor type enum
CREATE TYPE comment_actor_type AS ENUM ('user', 'agent', 'system');

-- Create issue_comments table
CREATE TABLE issue_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    actor_type comment_actor_type NOT NULL,
    actor_id UUID,
    actor_run_id UUID,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX issue_comments_issue_idx ON issue_comments(issue_id);
CREATE INDEX issue_comments_company_idx ON issue_comments(company_id);

-- Create interaction type enum
CREATE TYPE interaction_type AS ENUM ('question', 'clarification', 'approval', 'feedback');

-- Create thread_interactions table
CREATE TABLE thread_interactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    interaction_type interaction_type NOT NULL,
    actor_type comment_actor_type NOT NULL,
    actor_id UUID,
    body TEXT NOT NULL,
    metadata JSONB,
    resolved_at TIMESTAMPTZ,
    resolved_by_type comment_actor_type,
    resolved_by_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX thread_interactions_issue_idx ON thread_interactions(issue_id);
CREATE INDEX thread_interactions_resolved_idx ON thread_interactions(issue_id, resolved_at);

-- Create documents table (shared between issues and cases)
CREATE TABLE documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    content TEXT NOT NULL,
    content_type TEXT,
    locked_by_type TEXT,
    locked_by_id UUID,
    locked_at TIMESTAMPTZ,
    locked_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX documents_company_idx ON documents(company_id);
CREATE INDEX documents_locked_idx ON documents(locked_by_id, locked_at);

-- Create issue_documents table (link issues to documents)
CREATE TABLE issue_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, key)
);

CREATE INDEX issue_documents_issue_idx ON issue_documents(issue_id);
CREATE INDEX issue_documents_document_idx ON issue_documents(document_id);

-- Create annotation thread status enum
CREATE TYPE annotation_thread_status AS ENUM ('open', 'resolved');

-- Create annotation_threads table
CREATE TABLE annotation_threads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    position JSONB NOT NULL,
    status annotation_thread_status NOT NULL DEFAULT 'open',
    created_by_type TEXT,
    created_by_id UUID,
    resolved_by_type TEXT,
    resolved_by_id UUID,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX annotation_threads_document_idx ON annotation_threads(document_id);
CREATE INDEX annotation_threads_status_idx ON annotation_threads(document_id, status);

-- Create annotation_comments table
CREATE TABLE annotation_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    thread_id UUID NOT NULL REFERENCES annotation_threads(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    actor_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX annotation_comments_thread_idx ON annotation_comments(thread_id);

-- Create issue_work_products table
CREATE TABLE issue_work_products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    artifact JSONB NOT NULL,
    created_by_agent_id UUID REFERENCES agents(id),
    created_by_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX issue_work_products_issue_idx ON issue_work_products(issue_id);
CREATE INDEX issue_work_products_company_idx ON issue_work_products(company_id);

-- Create labels table
CREATE TABLE labels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    name TEXT NOT NULL,
    color TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, name)
);

CREATE INDEX labels_company_idx ON labels(company_id);

-- Create issue_labels table (many-to-many)
CREATE TABLE issue_labels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    label_id UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, label_id)
);

CREATE INDEX issue_labels_issue_idx ON issue_labels(issue_id);
CREATE INDEX issue_labels_label_idx ON issue_labels(label_id);

-- Create attachments table
CREATE TABLE attachments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    parent_type TEXT NOT NULL,
    parent_id UUID NOT NULL,
    asset_id UUID NOT NULL,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    created_by_type TEXT,
    created_by_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX attachments_parent_idx ON attachments(parent_type, parent_id);
CREATE INDEX attachments_company_idx ON attachments(company_id);

-- Create updated_at trigger function
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Add updated_at triggers
CREATE TRIGGER update_issues_updated_at BEFORE UPDATE ON issues
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_issue_comments_updated_at BEFORE UPDATE ON issue_comments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_documents_updated_at BEFORE UPDATE ON documents
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_issue_documents_updated_at BEFORE UPDATE ON issue_documents
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_annotation_threads_updated_at BEFORE UPDATE ON annotation_threads
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_issue_work_products_updated_at BEFORE UPDATE ON issue_work_products
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_attachments_updated_at BEFORE UPDATE ON attachments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
