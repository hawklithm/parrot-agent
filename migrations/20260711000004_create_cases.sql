-- Create case status enum
CREATE TYPE case_status AS ENUM ('draft', 'in_progress', 'in_review', 'approved', 'done', 'cancelled');

-- Create case issue link role enum
CREATE TYPE case_issue_link_role AS ENUM ('origin', 'work', 'reference');

-- Create case event kind enum
CREATE TYPE case_event_kind AS ENUM ('created', 'updated', 'status_changed', 'document_revised', 'issue_linked', 'issue_unlinked');

-- Create cases table
CREATE TABLE cases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    project_id UUID REFERENCES projects(id),
    case_number INTEGER NOT NULL,
    identifier TEXT NOT NULL,
    case_type TEXT NOT NULL,
    key TEXT,
    title TEXT NOT NULL,
    summary TEXT,
    status case_status NOT NULL DEFAULT 'draft',
    fields JSONB NOT NULL DEFAULT '{}'::jsonb,
    parent_case_id UUID REFERENCES cases(id),
    created_by_agent_id UUID REFERENCES agents(id),
    created_by_user_id UUID,
    created_by_run_id UUID,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Unique constraint: company_id + case_type + key (when key is not null)
    UNIQUE(company_id, case_type, key),
    UNIQUE(company_id, case_number)
);

CREATE INDEX cases_company_idx ON cases(company_id);
CREATE INDEX cases_company_status_idx ON cases(company_id, status);
CREATE INDEX cases_company_type_idx ON cases(company_id, case_type);
CREATE INDEX cases_project_idx ON cases(project_id);
CREATE INDEX cases_parent_case_idx ON cases(parent_case_id);
CREATE INDEX cases_identifier_idx ON cases(identifier);
CREATE INDEX cases_company_updated_idx ON cases(company_id, updated_at);

-- Create case_events table (event sourcing)
CREATE TABLE case_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    kind case_event_kind NOT NULL,
    actor_type TEXT,
    actor_id UUID,
    actor_run_id UUID,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX case_events_case_idx ON case_events(case_id, created_at DESC);
CREATE INDEX case_events_kind_idx ON case_events(case_id, kind);

-- Create case_issue_links table (many-to-many relationship)
CREATE TABLE case_issue_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    role case_issue_link_role NOT NULL,
    created_by_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(case_id, issue_id)
);

CREATE INDEX case_issue_links_case_idx ON case_issue_links(case_id);
CREATE INDEX case_issue_links_issue_idx ON case_issue_links(issue_id);
CREATE INDEX case_issue_links_role_idx ON case_issue_links(case_id, role);

-- Create case_documents table (link cases to documents)
CREATE TABLE case_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(case_id, key)
);

CREATE INDEX case_documents_case_idx ON case_documents(case_id);
CREATE INDEX case_documents_document_idx ON case_documents(document_id);

-- Create document_revisions table (for case document versioning)
CREATE TABLE document_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL,
    content TEXT NOT NULL,
    created_by_type TEXT,
    created_by_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(document_id, revision_number)
);

CREATE INDEX document_revisions_document_idx ON document_revisions(document_id, revision_number DESC);

-- Create case_attachments table
CREATE TABLE case_attachments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    asset_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX case_attachments_case_idx ON case_attachments(case_id);
CREATE INDEX case_attachments_asset_idx ON case_attachments(asset_id);

-- Create case_labels table (many-to-many)
CREATE TABLE case_labels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    label_id UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(case_id, label_id)
);

CREATE INDEX case_labels_case_idx ON case_labels(case_id);
CREATE INDEX case_labels_label_idx ON case_labels(label_id);

-- Add updated_at triggers
CREATE TRIGGER update_cases_updated_at BEFORE UPDATE ON cases
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_case_issue_links_updated_at BEFORE UPDATE ON case_issue_links
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_case_documents_updated_at BEFORE UPDATE ON case_documents
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_case_attachments_updated_at BEFORE UPDATE ON case_attachments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
