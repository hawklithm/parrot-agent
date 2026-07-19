CREATE TABLE IF NOT EXISTS cloud_upstream_connections (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    remote_url TEXT NOT NULL,
    status TEXT NOT NULL,
    source_instance_id TEXT,
    source_instance_fingerprint TEXT,
    source_public_key TEXT,
    private_key_pem TEXT,
    token_status TEXT NOT NULL DEFAULT 'pending',
    scopes TEXT[] NOT NULL DEFAULT '{}',
    target_stack_id TEXT,
    target_stack_slug TEXT,
    target_company_id TEXT,
    target_origin TEXT,
    pending_state TEXT,
    pending_code_verifier TEXT,
    pending_redirect_uri TEXT,
    pending_token_url TEXT,
    access_token TEXT,
    token_id TEXT,
    token_expires_at TIMESTAMPTZ,
    authorized_global_user_id TEXT,
    last_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS cloud_upstream_runs (
    id UUID PRIMARY KEY,
    connection_id UUID NOT NULL REFERENCES cloud_upstream_connections(id) ON DELETE CASCADE,
    company_id UUID REFERENCES companies(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    active_step TEXT,
    progress_percent INTEGER NOT NULL DEFAULT 0,
    dry_run BOOLEAN NOT NULL DEFAULT false,
    summary JSONB NOT NULL DEFAULT '{}',
    warnings JSONB NOT NULL DEFAULT '[]',
    conflicts JSONB NOT NULL DEFAULT '[]',
    events JSONB NOT NULL DEFAULT '[]',
    report JSONB NOT NULL DEFAULT '{}',
    idempotency_key TEXT,
    manifest_hash TEXT,
    target_url TEXT,
    remote_run_id TEXT,
    retry_of_run_id UUID REFERENCES cloud_upstream_runs(id),
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS cloud_upstream_connections_company_idx ON cloud_upstream_connections(company_id);
CREATE INDEX IF NOT EXISTS cloud_upstream_runs_connection_idx ON cloud_upstream_runs(connection_id);
