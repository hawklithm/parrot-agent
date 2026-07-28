CREATE TABLE IF NOT EXISTS environment_custom_image_setup_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    template_id UUID,
    promoted_template_id UUID,
    provider TEXT NOT NULL,
    provider_lease_id TEXT,
    environment_lease_id UUID REFERENCES environment_leases(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'starting',
    started_by_user_id TEXT,
    started_by_agent_id UUID,
    base_template_ref TEXT,
    expires_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    failure_reason TEXT,
    connection_summary JSONB,
    connection_secret_ref TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS environment_custom_image_setup_sessions_environment_status_idx ON environment_custom_image_setup_sessions(environment_id, status);
CREATE INDEX IF NOT EXISTS environment_custom_image_setup_sessions_expires_idx ON environment_custom_image_setup_sessions(expires_at);
