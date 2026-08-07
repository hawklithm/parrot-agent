-- Add missing fields to project_workspaces table to align with paperclip
-- Migration from paperclip server/src/services/projects.ts:937-962

-- Add company_id column (required for multi-tenancy)
ALTER TABLE project_workspaces 
ADD COLUMN company_id UUID;

-- Backfill company_id from projects table
UPDATE project_workspaces pw
SET company_id = p.company_id
FROM projects p
WHERE pw.project_id = p.id;

-- Make company_id NOT NULL after backfill
ALTER TABLE project_workspaces 
ALTER COLUMN company_id SET NOT NULL;

-- Add foreign key constraint
ALTER TABLE project_workspaces
ADD CONSTRAINT fk_project_workspaces_company
FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE;

-- Add workspace source and repo fields
ALTER TABLE project_workspaces
ADD COLUMN source_type VARCHAR(50),
ADD COLUMN cwd TEXT,
ADD COLUMN repo_url TEXT,
ADD COLUMN repo_ref VARCHAR(255),
ADD COLUMN default_ref VARCHAR(255),
ADD COLUMN visibility VARCHAR(50) DEFAULT 'default',
ADD COLUMN setup_command TEXT,
ADD COLUMN cleanup_command TEXT,
ADD COLUMN remote_provider VARCHAR(100),
ADD COLUMN remote_workspace_ref TEXT,
ADD COLUMN shared_workspace_key VARCHAR(255),
ADD COLUMN metadata JSONB;

-- Migrate existing config.cwd to cwd column
UPDATE project_workspaces
SET cwd = config->>'cwd'
WHERE config ? 'cwd';

-- Create indexes for common queries
CREATE INDEX idx_project_workspaces_company_id ON project_workspaces(company_id);
CREATE INDEX idx_project_workspaces_source_type ON project_workspaces(source_type);
CREATE INDEX idx_project_workspaces_shared_key ON project_workspaces(shared_workspace_key) 
WHERE shared_workspace_key IS NOT NULL;
