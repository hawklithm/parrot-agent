-- Create routine_revisions table
CREATE TABLE routine_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    routine_id UUID NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    snapshot JSONB NOT NULL,
    change_summary TEXT,
    restored_from_revision_id UUID REFERENCES routine_revisions(id) ON DELETE SET NULL,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_routine_revision_number UNIQUE (routine_id, revision_number)
);

-- Create indexes for routine_revisions
CREATE INDEX idx_routine_revisions_routine_id ON routine_revisions(routine_id, revision_number DESC);
CREATE INDEX idx_routine_revisions_company_id ON routine_revisions(company_id);
CREATE INDEX idx_routine_revisions_created_at ON routine_revisions(created_at DESC);

-- Add foreign key from routines to routine_revisions
ALTER TABLE routines
    ADD CONSTRAINT fk_routines_latest_revision
    FOREIGN KEY (latest_revision_id)
    REFERENCES routine_revisions(id)
    ON DELETE SET NULL;
