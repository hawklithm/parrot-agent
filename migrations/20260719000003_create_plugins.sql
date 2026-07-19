CREATE TABLE IF NOT EXISTS plugins (
    id UUID PRIMARY KEY,
    plugin_key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '0.0.0',
    api_version INTEGER NOT NULL DEFAULT 1,
    categories JSONB NOT NULL DEFAULT '[]',
    install_order INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'installed',
    package_name TEXT,
    install_path TEXT,
    manifest JSONB NOT NULL DEFAULT '{}',
    config JSONB NOT NULL DEFAULT '{}',
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS plugins_status_idx ON plugins(status);
ALTER TABLE plugins ADD COLUMN IF NOT EXISTS api_version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE plugins ADD COLUMN IF NOT EXISTS categories JSONB NOT NULL DEFAULT '[]';
ALTER TABLE plugins ADD COLUMN IF NOT EXISTS install_order INTEGER NOT NULL DEFAULT 0;
ALTER TABLE plugins ADD COLUMN IF NOT EXISTS config JSONB NOT NULL DEFAULT '{}';
CREATE UNIQUE INDEX IF NOT EXISTS issue_inbox_archives_company_issue_user_idx ON issue_inbox_archives(company_id, issue_id, user_id);

CREATE TABLE IF NOT EXISTS plugin_jobs (
    id UUID PRIMARY KEY,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    job_key TEXT NOT NULL,
    name TEXT NOT NULL,
    schedule TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    definition JSONB NOT NULL DEFAULT '{}',
    UNIQUE(plugin_id, job_key)
);
CREATE TABLE IF NOT EXISTS plugin_job_runs (
    id UUID PRIMARY KEY,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    job_id UUID NOT NULL REFERENCES plugin_jobs(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'queued',
    result JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);
CREATE TABLE IF NOT EXISTS plugin_data (
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    data_key TEXT NOT NULL,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(plugin_id, data_key)
);
CREATE TABLE IF NOT EXISTS plugin_logs (
    id UUID PRIMARY KEY,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    level TEXT NOT NULL DEFAULT 'info',
    message TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
