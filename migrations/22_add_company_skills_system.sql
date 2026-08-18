-- Migration: Add Company Skills System
-- Description: Adds 7 tables for company skill management, sharing, and testing
-- Date: 2026-08-18
-- Tables: skills, versions, stars, comments, test inputs/templates/runs

-- ============================================================================
-- Core Skill Tables
-- ============================================================================

-- Company skills (skill definitions)
CREATE TABLE company_skills (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    folder_id UUID REFERENCES folders(id) ON DELETE SET NULL,
    key TEXT NOT NULL,
    slug TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    markdown TEXT NOT NULL,
    source_type TEXT NOT NULL DEFAULT 'local_path',
    source_locator TEXT,
    source_ref TEXT,
    trust_level TEXT NOT NULL DEFAULT 'markdown_only',
    compatibility TEXT NOT NULL DEFAULT 'compatible',
    file_inventory JSONB NOT NULL DEFAULT '[]',
    icon_url TEXT,
    color TEXT,
    tagline TEXT,
    author_name TEXT,
    homepage_url TEXT,
    categories TEXT[] NOT NULL DEFAULT '{}',
    sharing_scope TEXT NOT NULL DEFAULT 'company',
    public_share_token TEXT,
    forked_from_skill_id UUID REFERENCES company_skills(id) ON DELETE SET NULL,
    forked_from_company_id UUID REFERENCES companies(id) ON DELETE SET NULL,
    star_count INTEGER NOT NULL DEFAULT 0,
    install_count INTEGER NOT NULL DEFAULT 0,
    fork_count INTEGER NOT NULL DEFAULT 0,
    current_version_id UUID,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT company_skills_source_type_check CHECK (source_type IN ('local_path', 'git', 'npm', 'url', 'inline')),
    CONSTRAINT company_skills_trust_level_check CHECK (trust_level IN ('markdown_only', 'safe_tools', 'full_tools')),
    CONSTRAINT company_skills_compatibility_check CHECK (compatibility IN ('compatible', 'deprecated', 'incompatible')),
    CONSTRAINT company_skills_sharing_scope_check CHECK (sharing_scope IN ('company', 'public', 'unlisted'))
);

CREATE UNIQUE INDEX company_skills_company_key_idx ON company_skills(company_id, key);
CREATE INDEX company_skills_company_name_idx ON company_skills(company_id, name);
CREATE INDEX company_skills_company_folder_idx ON company_skills(company_id, folder_id);
CREATE INDEX company_skills_company_categories_idx ON company_skills USING gin(categories);
CREATE INDEX company_skills_company_sharing_scope_idx ON company_skills(company_id, sharing_scope);
CREATE INDEX company_skills_company_current_version_idx ON company_skills(company_id, current_version_id);
CREATE INDEX company_skills_company_forked_from_idx ON company_skills(company_id, forked_from_skill_id);

-- Company skill versions (version control)
CREATE TABLE company_skill_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    company_skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL,
    label TEXT,
    release_id TEXT,
    release_name TEXT,
    released_at TIMESTAMPTZ,
    file_inventory JSONB NOT NULL DEFAULT '[]',
    author_agent_id UUID REFERENCES ON DELETE SET NULL,
    author_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT company_skill_versions_revision_positive CHECK (revision_number > 0)
);

CREATE UNIQUE INDEX company_skill_versions_skill_revision_idx ON company_skill_versions(company_skill_id, revision_number);
CREATE UNIQUE INDEX company_skill_versions_skill_release_idx ON company_skill_versions(company_skill_id, release_id) WHERE release_id IS NOT NULL;
CREATE INDEX company_skill_versions_company_skill_created_idx ON company_skill_versions(company_id, company_skill_id, created_at);

-- Add foreign key for current_version_id after company_skill_versions is created
ALTER TABLE company_skills 
    ADD CONSTRAINT company_skills_current_version_fk 
    FOREIGN KEY (current_version_id) REFERENCES company_skill_versions(id) ON DELETE SET NULL;

-- Company skill stars (user favorites)
CREATE TABLE company_skill_stars (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    company_skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX company_skill_stars_skill_user_idx ON company_skill_stars(company_skill_id, user_id);
CREATE INDEX company_skill_stars_company_skill_idx ON company_skill_stars(company_id, company_skill_id);
CREATE INDEX company_skill_stars_user_idx ON company_skill_stars(user_id);

-- Company skill comments (discussion)
CREATE TABLE company_skill_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    company_skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    parent_comment_id UUID REFERENCES company_skill_comments(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    author_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    author_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX company_skill_comments_company_skill_idx ON company_skill_comments(company_id, company_skill_id, ed_at DESC);
CREATE INDEX company_skill_comments_parent_idx ON company_skill_comments(parent_comment_id);

-- ============================================================================
-- Skill Testing Tables
-- ============================================================================

-- Company skill test inputs (test data)
CREATE TABLE company_skill_test_inputs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    company_skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
  description TEXT,
    input_payload JSONB NOT NULL,
    expected_output JSONB,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX company_skill_test_inputs_company_skill_idx ON company_skill_test_inputs(company_id, company_skill_id);
CREATE UNIQUE INDEX company_skill_test_inputs_skill_name_idx ON company_skill_test_inputs(company_skill_id, name);

-- Company skill test run templates (test configurations)
CREATE TA company_skill_test_run_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
CREATE TABLE company_skill_test_run_templates (
    name TEXT NOT NULL,
    description TEXT,
    test_input_ids UUID[] NOT NULL DEFAULT '{}',
    adapter_type TEXT,
    adapter_config JSONB,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX company_skill_test_run_templates_company_skill_idx ON company_skill_test_run_templates(company_id, company_skill_id);
CREATE UNIQUE INDEX company_skill_test_run_templates_skill_name_idx ON company_skill_test_run_templates(company_skill_id, name);

-- Company skill test runs (test execution records)
CREATE TABLE company_skill_test_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    company_skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    company_skill_version_id UUID REFERENCES company_skill_versions(id) ON DELETE SET NULL,
    template_id UUID REFERENCES company_skill_test_run_templates(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    results JSONB NOT NULL DEFAULT '[]',
    summary JSONB,
    test_input_ids UUID[] NOT NULL DEFAULT '{}',
    adapter_type TEXT,
    adapter_config JSONB,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    triggered_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    triggered_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT company_skill_test_runs_status_check CHECK (status IN ('pending', 'running', 'passed', 'failed', 'error', 'cancelled'))
);

CREATE INDEX company_skill_test_runs_company_skill_idx ON company_skill_test_runs(company_id, company_skill_id, created_at DESC);
CREATE INDEX company_skill_test_runs_version_idx ON company_skill_test_runs(company_skill_version_id);
CREATE INDEX company_skill_test_runs_status_idx ON company_skill_test_runs(company_id, status);
CREATE INDEX company_skill_test_runs_issue_idx ON company_skill_test_runs(issue_id);

-- ============================================================================
-- Triggers
-- ============================================================================

CREATE TRIGGER update_company_skills_updated_at BEFORE UPDATE ON company_skills
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_company_skill_comments_updated_at BEFORE UPDATE ON company_skill_comments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_company_skill_test_inputs_updated_at BEFORE UPDATE ON company_skill_test_inputs
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_company_skill_test_run_templates_updated_at BEFORE UPDATE ON company_skill_test_run_templates
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_company_skill_test_runs_updated_at BEFORE UPDATE ON company_skill_test_runs
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON TABLE company_skills IS 'Company skill definitions with versioning and sharing';
COMMENT ON TABLE company_skill_versions IS 'Version history for company skills';
COMMENT ON TABLE company_skill_stars IS 'User favorites for skills';
COMMENT ON TABLE company_skill_comments IS 'Discussion threads on skills';
COMMENT ON TABLE company_skill_test_inputs IS 'Test data for skill validation';
COMMENT ON TABLE company_skill_test_run_templates IS 'Reusable test configurations';
COMMENT ON TABLE company_skill_test_runs IS 'Skill test execution records';

COMMENT ON COLUMN company_skills.source_type IS 'Skill source: local_path, git, npm, url, inline';
COMMENT ON COLUMN company_skills.trust_level IS 'Security level: markdown_only, safe_tools, full_tools';
COMMENT ON COLUMN company_skills.compatibility IS 'Compatibility status: compatible, deprecated, incompatible';
COMMENT ON COLUMN company_skills.sharing_scope IS 'Visibility: company, public, unlisted';
COMMENT ON COLUMN company_skills.file_inventory IS 'List of skill files with metadata';
COMMENT ON COLUMN company_skill_versions.file_inventory IS 'Snapshot of skill files with content';
COMMENT ON COLUMN company_skill_test_runs.results IS 'Array of test case results';
COMMENT ON COLUMN company_skill_test_runs.summary IS 'Aggregate test metrics';
