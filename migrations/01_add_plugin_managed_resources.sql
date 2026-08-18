-- Add plugin_managed_resources table
CREATE TABLE IF NOT EXISTS plugin_managed_resources (
    id UUID PRIMARY KEY,
    plugin_id UUID NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_plugin_managed_resources_plugin_id 
    ON plugin_managed_resources(plugin_id);

CREATE INDEX IF NOT EXISTS idx_plugin_managed_resources_resource_id 
    ON plugin_managed_resources(resource_id);
