-- Cross-process scheduler lease for crash-safe job ownership.
CREATE TABLE IF NOT EXISTS scheduler_job_leases (
    job_name TEXT PRIMARY KEY,
    owner_id UUID NOT NULL,
    leased_until TIMESTAMPTZ NOT NULL,
    heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS scheduler_job_leases_expiry_idx
    ON scheduler_job_leases(leased_until);
