-- Restore the project_workspaces source/repo columns.
--
-- `00_init_schema_unified.sql` creates `project_workspaces` with only
-- (id, project_id, name, config, is_primary, created_at, updated_at) and later
-- backfills `company_id`. The remaining columns — added by
-- `archive_old_migrations_20260815_224741/20260807000002_add_project_workspaces_fields.sql`,
-- which no longer runs — are still required by
-- `ProjectWorkspace` (`crates/models/src/project.rs:87`),
-- `PgProjectRepository::create_workspace` (`crates/repositories/src/project_repository.rs:260`),
-- and `scan_project_skill_workspaces` (`crates/api/src/routes/skills.rs:846`).
--
-- Without them every `SELECT * FROM project_workspaces` fails at row mapping
-- and `POST /projects/:project_id/workspaces` fails at INSERT, so project
-- workspaces cannot be created or read at all.

ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS source_type VARCHAR(50);
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS cwd TEXT;
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS repo_url TEXT;
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS repo_ref VARCHAR(255);
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS default_ref VARCHAR(255);
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS visibility VARCHAR(50) DEFAULT 'default';
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS setup_command TEXT;
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS cleanup_command TEXT;
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS remote_provider VARCHAR(100);
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS remote_workspace_ref TEXT;
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS shared_workspace_key VARCHAR(255);
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS metadata JSONB;

-- Existing rows predate the `cwd` column; their working directory only lives in
-- `config`. Backfill so a workspace created before this migration is still
-- browsable.
UPDATE project_workspaces
SET cwd = config->>'cwd'
WHERE cwd IS NULL AND config ? 'cwd';

CREATE INDEX IF NOT EXISTS idx_project_workspaces_company_id ON project_workspaces(company_id);
CREATE INDEX IF NOT EXISTS idx_project_workspaces_source_type ON project_workspaces(source_type);
CREATE INDEX IF NOT EXISTS idx_project_workspaces_shared_key ON project_workspaces(shared_workspace_key)
WHERE shared_workspace_key IS NOT NULL;
