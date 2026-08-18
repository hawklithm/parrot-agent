-- Migration: Add Plugin System
-- Description: Adds 12 tables for plugin architecture, management, and runtime
-- Date: 2026-08-18
-- Tables: plugins, config, settings, database, jobs, logs, state, entities, webhooks

-- ============================================================================
-- Core Plugin Tables
-- ============================================================================

-- Plugins (installed plugin registry)
CREATE TABLE plugins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_key TEXT NOT NULL,
    package_name TEXT NOT NULL,
    version TEXT NOT NULL,
    api_version INTEGER NOT NULL DEFAULT 1,
    categories JSONB NOT NULL DEFAULT '[]',
    manifest_json JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'installed',
    install_order INTEGER,
    package_path TEXT,
    last_error TEXT,
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT plugins_status_check CHECK (status IN ('installed', 'active', 'error', 'disabled'))
);

CREATE UNIQUE INDEX plugins_plugin_key_idx ON plugins(plugin_key);
CREATE INDEX plugins_status_idx ON plugins(status);

-- Plugin config (instance-level operator configuration)
CREATE TABLE plugin_config (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    config_json JSONB NOT NULL DEFAULT '{}',
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX plugin_config_plugin_company_idx ON plugin_config(plugin_id, company_id);

-- Plugin company settings (per-company enable/disable and settings)
CREATE TABLE plugin_company_settings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT true,
    settings_json JSONB NOT NULL DEFAULT '{}',
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX plugin_company_settings_company_idx ON plugy_settings(company_id);
CREATE INDEX plugin_company_settings_plugin_idx ON plugin_company_settings(plugin_id);
CREATE UNIQUE INDEX plugin_company_settings_company_plugin_uq ON plugin_company_settings(company_id, plugin_id);

-- ============================================================================
-- Plugin Database Management
-- ============================================================================

-- Plugin database namespaces (isolated database schemas for plugins)
CREATE TABLE plugin_database_namespaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugi NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    plugin_key TEXT NOT NULL,
    namespace_name TEXT NOT NULL,
    namespace_mode TEXT NOT NULL DEFAULT 'schema',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT plugin_database_namespaces_mode_check CHECK (namespace_mode IN ('schema', 'prefix')),
    CONSTRAINT plugin_database_namespaces_status_check CHECK (status IN ('active', 'migrating', 'error', 'dropped'))
);

CREATE UNIQUE INDEX plugin_database_namespaces_plugin_idx ON plugin_database_namespaces(plugin_id);
CREATE UNIQUE INDEX plugin_database_namespaces_namespace_idx ON plugin_database_namespaces(namespace_name);
CREATE INDEX plugin_database_namespaces_status_idx ON plugin_database_namespaces(status);

-- Plugin migrations (migration history with checksums)
CREATE TABLE plugin_migrations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    plugin_key TEXT NOT NULL,
    namespace_name TEXT NOT NULL,
    migration_key TEXT NOT NULL,
    checksum TEXT NOT NULL,
    plugin_version TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    applied_at TIMESTAMPTZ,
    error_message TEXT,
    
    CONSTRAINT plugin_migrations_status_check CHECK (status IN ('pending', 'applied', 'failed', 'rolled_back'))
);

CREATE UNIQUE INDEX plugin_migrations_plugin_key_idx ON plugin_migrations(plugin_id, migration_key);
CREATE INDEX plugin_migrations_plugin_idx ON plugin_migrations(plugin_id);
CREATE INDEX plugin_migrations_status_idx ON plugin_migrations(status);

-- ============================================================================
-- Plugin Runtime: Jobs and Logs
-- ============================================================================

-- Plugin jobs (scheduled job definitions)
CREATE TABLE plugin_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    job_key TEXT NOT NULL,
    schedule TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    last_run_at TIMESTAMPTZ,
    next_run_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT plugin_jobs_status_check CHECK (status IN ('active', 'paused', 'error'))
);

CREATE UNIQUE INDEX plugin_jobs_plugin_key_idx ON plugin_jobs(plugin_id, job_key);
CREATE INDEX plugin_jobs_next_run_idx ON plugin_jobs(next_run_at) WHERE status = 'active';

-- Plugin job runs (job execution history)
CREATE TABLE plugin_job_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_job_id UUID NOT NULL REFERENCES plugin_jobs(id) ON DELETE CASCADE,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    company_id UUID REFERENCES companies(id) ON DELETE CASCADE,
    trigger TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    duration_ms INTEGER,
    error TEXT,
    output JSONB,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT plugin_job_runs_trigger_check CHECK (trigger IN ('scheduled', 'manual', 'retry')),
    CONSTRAINT plugin_job_runs_status_check CHECKIN ('pending', 'running', 'succeeded', 'failed', 'cancelled'))
);

CREATE INDEX plugin_job_runs_plugin_job_idx ON plugin_job_runs(plugin_job_id, created_at DESC);
CREATE INDEX plugin_job_runs_plugin_idx ON plugin_job_runs(plugin_id);
CREATE INDEX plugin_job_runs_status_idx ON plugin_job_runs(status);

-- Plugin logs (structured log storage)
CREATE TABLE plugin_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    company_id UUID REFERENCES companies(id) ON DELETE CASCADE,
    level TEXT NOT NULL DEFAULT 'info',
    message T NULL,
    meta JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT plugin_logs_level_check CHECK (level IN ('debug', 'info', 'warn', 'error'))
);

CREATE INDEX plugin_logs_plugin_time_idx ON plugin_logs(plugin_id, created_at DESC);
CREATE INDEX plugin_logs_company_idx ON plugin_logs(company_id);
CREATE INDEX plugin_logs_level_idx ON plugin_logs(level);

-- ============================================================================
-- Plugin State and Entity Management
-- ============================================================================

-- Plugin state (scoped key-value storage)
CREATE TABLE plugin_state (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    scope_kind TEXT NOT NULL,
    scope_id TEXT,
    namespace TEXT NOT NULL DEFAULT 'default',
    state_key TEXT NOT NULL,
    value_json JSONB NOT NULL,
    updated_at TIMESTAMPTZ EFAULT now(),
    
    CONSTRAINT plugin_state_scope_kind_check CHECK (scope_kind IN ('instance', 'company', 'project', 'project_workspace', 'agent', 'issue', 'goal', 'run'))
);

-- PostgreSQL 15+ feature: nullsNotDistinct() for proper NULL handling
CREATE UNIQUE INDEX plugin_state_unique_entry_idx ON plugin_state(plugin_id, scope_kind, scope_id, namespace, state_key) NULLS NOT DISTINCT;
CREATE INDEX plugin_state_plugin_scope_idx ON plugin_state(plugin_id, scope_kind);

-- Plugin entities (external entity mapping)
CREATE TABLE plugin_entities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_idNULL REFERENCES plugins(id) ON DELETE CASCADE,
    company_id UUID REFERENCES companies(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    scope_kind TEXT NOT NULL,
    scope_id TEXT,
    external_id TEXT,
    title TEXT,
    status TEXT,
    data JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT plugin_entities_scope_kind_check CHECK (scope_kind IN ('instance', 'company', 'project', 'project_workspace', 'agent', 'issue', 'goal', 'run'))
);

CREATE INDEX plugin_entities_plugin_idx ON plugin_entities(plugin_id);
CREATE INDEX plugin_entities_company_idx ON plugin_entities(company_id);
CREATE INDEX plugin_entities_type_idx ON plugin_entities(entity_type);
CREATE INDEX plugin_entities_scope_idx ON plugin_entities(scope_kind, scope_id);
CREATE UNIQUE INDEX plugin_entities_external_idx ON plugin_entities(company_id, plugin_id, entity_type, external_id) NULLS NOT DISTINCT;

-- Plugin managed resources (resource ownership tracking)
CREATE TABLE plugin_managed_resources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    plugin_key TEXT NOT NULL,
    resource_kind TEXT NOT NULL,
    resource_key TEXT NOT NULL,
    resource_id UUID NOT NULL,
    defaults_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX plugin_managed_resources_company_idx ON plugin_managed_resources(company_id);
CREATE INDEX plugin_managed_resources_plugin_idx ON plugin_managed_resources(plugin_id);
CREATE INDEX plugin_managed_resources_resource_idx ON plugin_managed_resources(resource_kind, resource_id);
CREATE UNIQUE INDEX plugin_managed_resources_company_plugin_resource_uq ON plugin_managed_resources(company_id, plugin_id, resource_kind, resource_key);

-- ============================================================================
-- Plugin Webhooks
-- ============================================================================

-- Plugin webhook deliveries (inbound webhook history)
CREATE TABLE plugin_webhook_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    company_id UUID REFERENCES companies(id) ON DELETE CASCADE,
    webhook_key TEXT NOT NULL,
    external_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    duration_ms INTEGER,
    error TEXT,
    payload JSONB NOT NULL,
    headers JSONB NOT NULL DEFAULT '{}',
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT plugin_webhook_deliveries_status_check CHECK (status IN ('pending', 'processing', 'succeeded', 'failed'))
);

CREATE INDEX plugin_webhook_deliveries_plugin_idx ON plugin_webhook_deliveries(plugin_id);
CREATE INDEX plugin_webhook_deliveries_company_idx ON plugin_webhook_deliveries(company_id);
CREATE INDEX plugin_webhook_deliveries_status_idx ON plugin_webhook_deliveries(status);
CREATE INDEX plugin_webhook_deliveries_key_idx ON plugin_webhook_deliveries(webhook_key);

-- ============================================================================
-- Triggers
-- ============================================================================

CREATE TRIGGER update_plugins_updated_at BEFORE UPDATE ON plugins
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_plugin_config_updated_at BEFORE UPDATE ON plugin_config
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_plugin_company_settings_updated_at BEFORE UPDATE ON plugin_company_settings
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_plugin_database_namespaces_updated_at BEFORE UPDATE ON plugin_database_namespaces
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_plugin_jobs_updated_at BEFORE UPDATE ON plugin_jobs
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_plugin_entities_updated_at BEFORE UPDATE ON plugin_entities
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_plugin_managed_resources_updated_at BEFORE UPDATE ON plugin_managed_resources
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON TABLE plugins IS 'Installed plugin registry with manifests';
COMMENT ON TABLE plugin_config IS 'Instance-level plugin configuration per company';
COMMENT ON TABLE plugin_company_settings IS 'Per-company plugin enable/disable and settings';
COMMENT ON TABLE plugin_database_namespaces IS 'Isolated database schemas for plugin data';
COMMENT ON TABLE plugin_migrations IS 'Plugin migration history with checksums';
COMMENT ON TABLE plugin_jobs IS 'Scheduled job definitions from plugin manifests';
COMMENT ON TABLE plugin_job_runs IS 'Job execution history with results';
COMMENT ON TABLE plugin_logs IS 'Structured log storage from plugin workers';
COMMENT ON TABLE plugin_state IS 'Scoped key-value storage for plugins';
COMMENT ON TABLE plugin_entities IS 'External entity mapping (e.g. GitHub issues, Linear tickets)';
COMMENT ON TABLE plugin_managed_resources IS 'Resource ownership tracking for cleanup';
COMMENT ON TABLE plugin_webhook_deliveries IS 'Inbound webhook delivery history';

COMMENT ON COLUMN plugins.manifest_json IS 'Full plugin manifest (PaperclipPluginManifestV1)';
COMMENT ON COLUMN plugins.status IS 'Plugin state: installed, active, error, disabled';
COMMENT ON COLUMN plugin_state.scope_kind IS 'Storage scope: instance, company, project, agent, issue, etc.';
COMMENT ON COLUMN plugin_entities.scope_kind IS 'Entity scope: instance, company, project, agent, issue, etc.';
COMMENT ON COLUMN plugin_entities.external_id IS 'ID in the external system (e.g. GitHub issue number)';
