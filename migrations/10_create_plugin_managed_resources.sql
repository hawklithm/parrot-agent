-- Migration: Create plugin_managed_resources table
-- Purpose: Track resources (projects, agents, tools, etc.) managed by plugins
-- Reference: Paperclip plugin_managed_resources.ts

CREATE TABLE IF NOT EXISTS plugin_managed_resources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    plugin_key TEXT NOT NULL,
    resource_kind TEXT NOT NULL,
    resource_key TEXT NOT NULL,
    resource_id UUID NOT NULL,
    defaults_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS plugin_managed_resources_company_idx ON plugin_managed_resources(company_id);
CREATE INDEX IF NOT EXISTS plugin_managed_resources_plugin_idx ON plugin_managed_resources(plugin_id);
CREATE INDEX IF NOT EXISTS plugin_managed_resources_resource_idx ON plugin_managed_resources(resource_kind, resource_id);

-- Unique constraint: one plugin can manage one resource key per company
CREATE UNIQUE INDEX IF NOT EXISTS plugin_managed_resources_company_plugin_resource_uq 
    ON plugin_managed_resources(company_id, plugin_id, resource_kind, resource_key);

-- Comments
COMMENT ON TABLE plugin_managed_resources IS 'Resources (projects, agents, tools) managed by plugins';
COMMENT ON COLUMN plugin_managed_resources.plugin_key IS 'Plugin identifier (e.g., @acme/github-integration)';
COMMENT ON COLUMN plugin_managed_resources.resource_kind IS 'Resource type (project, agent, tool, routine, etc.)';
COMMENT ON COLUMN plugin_managed_resources.resource_key IS 'Plugin-scoped resource key';
COMMENT ON COLUMN plugin_managed_resources.resource_id IS 'UUID of the managed resource';
COMMENT ON COLUMN plugin_managed_resources.defaults_json IS 'Plugin-specific default configuration for this resource';
