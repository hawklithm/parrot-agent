-- Paperclip-compatible issue blocker relations.
-- A row means issue_id blocks related_issue_id.  The company column is kept
-- explicitly so every relation query can enforce tenant scope.
CREATE TABLE IF NOT EXISTS issue_relations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    related_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    type TEXT NOT NULL DEFAULT 'blocks' CHECK (type = 'blocks'),
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT issue_relations_company_edge_uq
        UNIQUE (company_id, issue_id, related_issue_id, type),
    CONSTRAINT issue_relations_no_self_edge CHECK (issue_id <> related_issue_id)
);

CREATE INDEX IF NOT EXISTS issue_relations_company_issue_idx
    ON issue_relations(company_id, issue_id);
CREATE INDEX IF NOT EXISTS issue_relations_company_related_issue_idx
    ON issue_relations(company_id, related_issue_id);
CREATE INDEX IF NOT EXISTS issue_relations_company_type_idx
    ON issue_relations(company_id, type);
