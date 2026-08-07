-- Create plugin_managed_resources table
-- Migrated from paperclip: packages/db/src/schema/plugin_managed_resources.ts
-- Tracks resources (projects, issues, etc.) managed by plugins

CREATE TABLE plugin_managed_resources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    plugin_key TEXT NOT NULL,
    resource_kind TEXT NOT NULL,  -- e.g., 'project', 'issue', 'agent'
    resource_key TEXT NOT NULL,    -- plugin-specific identifier
    resource_id UUID NOT NULL,     -- actual resource UUID in our DB
    defaults_json JSONB NOT NULL DEFAULT '{}',  -- plugin-specific metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for efficient queries
CREATE INDEX idx_plugin_managed_resources_company_id ON plugin_managed_resources(company_id);
CREATE INDEX idx_plugin_managed_resources_plugin_id ON plugin_managed_resources(plugin_id);
CREATE INDEX idx_plugin_managed_resources_resource_id ON plugin_managed_resources(resource_id);
CREATE UNIQUE INDEX idx_plugin_managed_resources_lookup 
    ON plugin_managed_resources(company_id, plugin_id, resource_kind, resource_key);

-- Create trigger for updated_at
CREATE TRIGGER update_plugin_managed_resources_updated_at
    BEFORE UPDATE ON plugin_managed_resources
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Add comment
COMMENT ON TABLE plugin_managed_resources IS 'Tracks resources managed by plugins (e.g., GitHub issues, Linear projects)';
COMMENT ON COLUMN plugin_managed_resources.plugin_key IS 'Plugin identifier for validation';
COMMENT ON COLUMN plugin_managed_resources.resource_kind IS 'Type of resource: project, issue, agent, etc.';
COMMENT ON COLUMN plugin_managed_resources.resource_key IS 'Plugin-specific key (e.g., GitHub repo name)';
COMMENT ON COLUMN plugin_managed_resources.resource_id IS 'UUID of the actual resource in our database';
COMMENT ON COLUMN plugin_managed_resources.defaults_json IS 'Plugin-specific configuration and metadata';
