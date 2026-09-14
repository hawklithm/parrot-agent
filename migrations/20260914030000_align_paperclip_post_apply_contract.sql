-- Preserve the schema adjustments that were added after
-- 20260914000001 had already been applied to development databases.

ALTER TABLE invites
    ALTER COLUMN company_id DROP NOT NULL;

DROP INDEX IF EXISTS heartbeat_run_events_run_seq_uq;

ALTER TABLE plugin_config
    DROP CONSTRAINT IF EXISTS plugin_config_plugin_id_company_id_key;
CREATE UNIQUE INDEX IF NOT EXISTS plugin_config_plugin_company_idx
    ON plugin_config(plugin_id, company_id);

ALTER TABLE plugin_state
    ALTER COLUMN value_json DROP DEFAULT;
DROP INDEX IF EXISTS plugin_state_unique_entry_uq;
CREATE UNIQUE INDEX IF NOT EXISTS plugin_state_unique_entry_idx
    ON plugin_state(plugin_id, scope_kind, scope_id, namespace, state_key)
    NULLS NOT DISTINCT;

DROP INDEX IF EXISTS plugin_entities_external_uq;
CREATE UNIQUE INDEX IF NOT EXISTS plugin_entities_external_idx
    ON plugin_entities(company_id, plugin_id, entity_type, external_id)
    NULLS NOT DISTINCT;
DROP INDEX IF EXISTS plugin_entities_plugin_company_type_idx;
CREATE INDEX IF NOT EXISTS plugin_entities_plugin_idx
    ON plugin_entities(plugin_id);
CREATE INDEX IF NOT EXISTS plugin_entities_company_idx
    ON plugin_entities(company_id);
CREATE INDEX IF NOT EXISTS plugin_entities_type_idx
    ON plugin_entities(entity_type);

ALTER TABLE plugin_database_namespaces
    DROP CONSTRAINT IF EXISTS plugin_database_namespaces_plugin_id_namespace_name_key;
CREATE UNIQUE INDEX IF NOT EXISTS plugin_database_namespaces_plugin_idx
    ON plugin_database_namespaces(plugin_id);
CREATE UNIQUE INDEX IF NOT EXISTS plugin_database_namespaces_namespace_idx
    ON plugin_database_namespaces(namespace_name);

UPDATE plugin_migrations
   SET plugin_version = ''
 WHERE plugin_version IS NULL;
ALTER TABLE plugin_migrations
    ALTER COLUMN plugin_version SET NOT NULL,
    ALTER COLUMN plugin_version DROP DEFAULT,
    ALTER COLUMN status DROP DEFAULT;
ALTER TABLE plugin_migrations
    DROP CONSTRAINT IF EXISTS plugin_migrations_plugin_id_namespace_name_migration_key_key;
CREATE UNIQUE INDEX IF NOT EXISTS plugin_migrations_plugin_key_idx
    ON plugin_migrations(plugin_id, migration_key);
CREATE INDEX IF NOT EXISTS plugin_migrations_status_idx
    ON plugin_migrations(status);

DROP INDEX IF EXISTS plugin_webhook_deliveries_plugin_created_idx;
DROP INDEX IF EXISTS plugin_webhook_deliveries_company_created_idx;
CREATE INDEX IF NOT EXISTS plugin_webhook_deliveries_plugin_idx
    ON plugin_webhook_deliveries(plugin_id);
CREATE INDEX IF NOT EXISTS plugin_webhook_deliveries_company_idx
    ON plugin_webhook_deliveries(company_id);
CREATE INDEX IF NOT EXISTS plugin_webhook_deliveries_status_idx
    ON plugin_webhook_deliveries(status);
CREATE INDEX IF NOT EXISTS plugin_webhook_deliveries_key_idx
    ON plugin_webhook_deliveries(webhook_key);

ALTER TABLE plugin_company_settings
    DROP CONSTRAINT IF EXISTS plugin_company_settings_company_id_plugin_id_key;
CREATE INDEX IF NOT EXISTS plugin_company_settings_company_idx
    ON plugin_company_settings(company_id);
CREATE INDEX IF NOT EXISTS plugin_company_settings_plugin_idx
    ON plugin_company_settings(plugin_id);
CREATE UNIQUE INDEX IF NOT EXISTS plugin_company_settings_company_plugin_uq
    ON plugin_company_settings(company_id, plugin_id);

DROP INDEX IF EXISTS company_transfer_runs_idempotency_direction_uq;
CREATE INDEX IF NOT EXISTS company_transfer_runs_idempotency_direction_idx
    ON company_transfer_runs(idempotency_key, direction);

DROP INDEX IF EXISTS document_annotation_anchor_snapshots_company_thread_idx;
CREATE INDEX IF NOT EXISTS document_annotation_anchor_snapshots_company_thread_created_at_idx
    ON document_annotation_anchor_snapshots(company_id, thread_id, created_at DESC);
