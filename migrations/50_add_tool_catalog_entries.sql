-- Paperclip parity: tool_catalog_entries
-- Baseline: packages/db/src/schema/tool_access.ts (toolCatalogEntries)
CREATE TABLE IF NOT EXISTS tool_catalog_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    application_id UUID REFERENCES tool_applications(id) ON DELETE CASCADE,
    connection_id UUID NOT NULL REFERENCES tool_connections(id) ON DELETE CASCADE,
    entry_kind TEXT NOT NULL DEFAULT 'tool',
    name TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    title TEXT,
    description TEXT,
    input_schema JSONB NOT NULL DEFAULT '{}',
    output_schema JSONB,
    annotations JSONB NOT NULL DEFAULT '{}',
    risk_level TEXT NOT NULL DEFAULT 'read',
    is_read_only BOOLEAN NOT NULL DEFAULT true,
    is_write BOOLEAN NOT NULL DEFAULT false,
    is_destructive BOOLEAN NOT NULL DEFAULT false,
    status TEXT NOT NULL DEFAULT 'active',
    version TEXT,
    version_hash TEXT NOT NULL,
    schema_hash TEXT,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reviewed_at TIMESTAMPTZ,
    reviewed_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    reviewed_by_user_id TEXT,
    quarantined_at TIMESTAMPTZ,
    quarantine_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(connection_id, name)
);

CREATE INDEX IF NOT EXISTS tool_catalog_entries_company_idx ON tool_catalog_entries(company_id);
CREATE INDEX IF NOT EXISTS tool_catalog_entries_application_idx ON tool_catalog_entries(application_id);
CREATE INDEX IF NOT EXISTS tool_catalog_entries_connection_idx ON tool_catalog_entries(connection_id);
CREATE INDEX IF NOT EXISTS tool_catalog_entries_company_status_idx ON tool_catalog_entries(company_id, status);
