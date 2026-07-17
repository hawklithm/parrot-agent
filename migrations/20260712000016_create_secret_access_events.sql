-- Secret access events (paperclip-aligned). Mirrors paperclip 0082_dry_vision.sql
-- (secret_access_events table) + 0128_user_specific_secrets.sql additions
-- (user_secret_definition_id / scope / responsible columns).

CREATE TABLE IF NOT EXISTS secret_access_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE NO ACTION,
    secret_id UUID REFERENCES company_secrets(id) ON DELETE CASCADE,
    user_secret_definition_id UUID REFERENCES user_secret_definitions(id) ON DELETE SET NULL,
    version INTEGER,
    provider TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    actor_id TEXT,
    consumer_type TEXT NOT NULL,
    consumer_id TEXT NOT NULL,
    config_path TEXT,
    secret_scope TEXT NOT NULL DEFAULT 'company',
    responsible_user_id TEXT,
    credential_owner_user_id TEXT,
    credential_subject_type TEXT,
    credential_subject_id TEXT,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    heartbeat_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    plugin_id UUID,
    outcome TEXT NOT NULL,
    error_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS secret_access_events_company_created_idx
    ON secret_access_events(company_id, created_at);
CREATE INDEX IF NOT EXISTS secret_access_events_secret_created_idx
    ON secret_access_events(secret_id, created_at);
CREATE INDEX IF NOT EXISTS secret_access_events_consumer_idx
    ON secret_access_events(company_id, consumer_type, consumer_id);
CREATE INDEX IF NOT EXISTS secret_access_events_run_idx
    ON secret_access_events(heartbeat_run_id);
CREATE INDEX IF NOT EXISTS secret_access_events_user_definition_created_idx
    ON secret_access_events(user_secret_definition_id, created_at);
CREATE INDEX IF NOT EXISTS secret_access_events_company_credential_owner_idx
    ON secret_access_events(company_id, credential_owner_user_id, created_at);
