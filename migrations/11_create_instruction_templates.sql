-- Migration: Create instruction_templates table
-- Purpose: Store Agent instruction templates with variable substitution and versioning
-- This is a Parrot-specific feature for managing Agent instructions

CREATE TABLE IF NOT EXISTS instruction_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    content TEXT NOT NULL,
    variables TEXT[] NOT NULL DEFAULT '{}',
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ
);

-- Indexes
CREATE INDEX IF NOT EXISTS instruction_templates_name_idx ON instruction_templates(name);
CREATE INDEX IF NOT EXISTS instruction_templates_created_at_idx ON instruction_templates(created_at DESC);
CREATE INDEX IF NOT EXISTS instruction_templates_version_idx ON instruction_templates(version);

-- Comments
COMMENT ON TABLE instruction_templates IS 'Agent instruction templates with variable substitution support';
COMMENT ON COLUMN instruction_templates.name IS 'Unique template name';
COMMENT ON COLUMN instruction_templates.content IS 'Template content with {{variable}} placeholders';
COMMENT ON COLUMN instruction_templates.variables IS 'List of variable names used in the template';
COMMENT ON COLUMN instruction_templates.version IS 'Template version number';
