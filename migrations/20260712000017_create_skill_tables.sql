-- Skill Registry tables for Phase 2 (P2) Skills domain
-- Covers SK1-SK38 endpoints

-- SK1-SK3: Skill catalogs (pre-defined skill collections)
CREATE TABLE IF NOT EXISTS skill_catalogs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    category VARCHAR(255),
    metadata JSONB NOT NULL DEFAULT '{}',
    is_paperclip_managed BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- SK4-SK6, SK23-SK27, SK35-SK38: Company skills (installed/forked skills)
CREATE TABLE IF NOT EXISTS company_skills (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    catalog_id UUID REFERENCES skill_catalogs(id) ON DELETE SET NULL,
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    category VARCHAR(255),
    version VARCHAR(50) NOT NULL DEFAULT '1.0.0',
    tags JSONB NOT NULL DEFAULT '[]',
    config JSONB NOT NULL DEFAULT '{}',
    is_paperclip_managed BOOLEAN NOT NULL DEFAULT false,
    is_fork BOOLEAN NOT NULL DEFAULT false,
    forked_from_skill_id UUID REFERENCES company_skills(id) ON DELETE SET NULL,
    forked_from_catalog_id UUID REFERENCES skill_catalogs(id) ON DELETE SET NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    update_available BOOLEAN NOT NULL DEFAULT false,
    latest_version VARCHAR(50),
    created_by_agent_id UUID,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, slug)
);

-- SK7-SK8: Skill versions
CREATE TABLE IF NOT EXISTS skill_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    version VARCHAR(50) NOT NULL,
    files JSONB NOT NULL DEFAULT '{}',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_by_agent_id UUID,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(skill_id, version)
);

-- SK9-SK12: Skill test inputs
CREATE TABLE IF NOT EXISTS skill_test_inputs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_by_agent_id UUID,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- SK13-SK16: Skill test run templates
CREATE TABLE IF NOT EXISTS skill_test_run_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    created_by_agent_id UUID,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- SK17-SK20: Skill test runs
CREATE TABLE IF NOT EXISTS skill_test_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    template_id UUID REFERENCES skill_test_run_templates(id) ON DELETE SET NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    result JSONB,
    started_by_agent_id UUID,
    started_by_user_id UUID,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- SK21-SK22: Skill stars (favorites)
CREATE TABLE IF NOT EXISTS skill_stars (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    agent_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, skill_id, user_id),
    UNIQUE(company_id, skill_id, agent_id)
);

-- SK28-SK31: Skill comments
CREATE TABLE IF NOT EXISTS skill_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    parent_comment_id UUID REFERENCES skill_comments(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    author_agent_id UUID,
    author_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- SK32-SK34: Skill files
CREATE TABLE IF NOT EXISTS skill_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    path VARCHAR(1024) NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    mime_type VARCHAR(255),
    size_bytes BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(skill_id, path)
);

-- Indexes for performance
CREATE INDEX idx_company_skills_company_id ON company_skills(company_id);
CREATE INDEX idx_company_skills_catalog_id ON company_skills(catalog_id);
CREATE INDEX idx_company_skills_name ON company_skills(name);
CREATE INDEX idx_skill_versions_skill_id ON skill_versions(skill_id);
CREATE INDEX idx_skill_test_inputs_skill_id ON skill_test_inputs(skill_id);
CREATE INDEX idx_skill_test_runs_skill_id ON skill_test_runs(skill_id);
CREATE INDEX idx_skill_stars_skill_id ON skill_stars(skill_id);
CREATE INDEX idx_skill_comments_skill_id ON skill_comments(skill_id);
CREATE INDEX idx_skill_files_skill_id ON skill_files(skill_id);
