CREATE TABLE IF NOT EXISTS environment_custom_image_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    template_kind TEXT NOT NULL DEFAULT 'snapshot',
    template_ref TEXT NOT NULL,
    source_template_ref TEXT,
    source_environment_config_fingerprint TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    superseded_by_template_id UUID REFERENCES environment_custom_image_templates(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_by_agent_id UUID,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE environment_custom_image_setup_sessions
    ADD COLUMN IF NOT EXISTS connection_payload JSONB;

CREATE INDEX IF NOT EXISTS environment_custom_image_templates_environment_status_idx
    ON environment_custom_image_templates(environment_id, status);
CREATE UNIQUE INDEX IF NOT EXISTS environment_custom_image_templates_environment_active_uq
    ON environment_custom_image_templates(environment_id) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS environment_custom_image_setup_sessions_environment_status_idx
    ON environment_custom_image_setup_sessions(environment_id, status);
CREATE UNIQUE INDEX IF NOT EXISTS environment_custom_image_setup_sessions_environment_active_uq
    ON environment_custom_image_setup_sessions(environment_id)
    WHERE status IN ('pending', 'running', 'starting', 'waiting_for_user', 'capturing');
