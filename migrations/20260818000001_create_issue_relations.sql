-- Create issue_relations table for tracking relationships between issues
-- Migration: 20260818000001_create_issue_relations
-- Aligned with Paperclip's issue_relations table

CREATE TABLE IF NOT EXISTS issue_relations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    related_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL CHECK (relation_type IN ('blocks', 'blocked_by', 'relates_to', 'duplicates', 'duplicate_of')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    metadata JSONB,
    
    -- Prevent self-references
    CONSTRAINT issue_relations_no_self_reference CHECK (issue_id != related_issue_id),
    
    -- Prevent duplicate relations
    CONSTRAINT issue_relations_unique UNIQUE (issue_id, related_issue_id, relation_type)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_issue_relations_issue_id ON issue_relations(issue_id);
CREATE INDEX IF NOT EXISTS idx_issue_relations_related_issue_id ON issue_relations(related_issue_id);
CREATE INDEX IF NOT EXISTS idx_issue_relations_company_id ON issue_relations(company_id);
CREATE INDEX IF NOT EXISTS idx_issue_relations_type ON issue_relations(relation_type);
CREATE INDEX IF NOT EXISTS idx_issue_relations_created_at ON issue_relations(created_at DESC);

COMMENT ON TABLE issue_relations IS 'Tracks relationships between issues (blocks, relates to, duplicates, etc.)';
COMMENT ON COLUMN issue_relations.relation_type IS 'Type of relationship: blocks, blocked_by, relates_to, duplicates, duplicate_of';
COMMENT ON COLUMN issue_relations.metadata IS 'Additional metadata about the relationship';
