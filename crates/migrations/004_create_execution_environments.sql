-- Migration 004: Create Execution Environments and Runtime Leases
-- This migration creates the tables for environment management and runtime leases

-- Create environments table
CREATE TABLE IF NOT EXISTS environments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT,
    driver TEXT NOT NULL DEFAULT 'local',
    status TEXT NOT NULL DEFAULT 'active',
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    env_vars JSONB NOT NULL DEFAULT '{}'::jsonb,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create unique constraint on environment name
CREATE UNIQUE INDEX IF NOT EXISTS environments_name_idx ON environments(name);

-- Create index on status for filtering
CREATE INDEX IF NOT EXISTS environments_status_idx ON environments(status);

-- Create index on driver for filtering
CREATE INDEX IF NOT EXISTS environments_driver_idx ON environments(driver);

-- Create unique index for local driver (only one local environment allowed)
CREATE UNIQUE INDEX IF NOT EXISTS environments_local_driver_idx
    ON environments(driver)
    WHERE driver = 'local';

-- Create environment_leases table
CREATE TABLE IF NOT EXISTS environment_leases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    agent_id UUID,
    run_id UUID,
    issue_id UUID,
    status TEXT NOT NULL DEFAULT 'active',
    policy TEXT NOT NULL DEFAULT 'ephemeral',
    workspace_id TEXT,
    lease_metadata JSONB,
    cleanup_status TEXT,
    cleanup_error TEXT,
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    released_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create index on environment_id for fast lookups
CREATE INDEX IF NOT EXISTS environment_leases_environment_id_idx ON environment_leases(environment_id);

-- Create index on status for filtering active leases
CREATE INDEX IF NOT EXISTS environment_leases_status_idx ON environment_leases(status);

-- Create index on agent_id for filtering by agent
CREATE INDEX IF NOT EXISTS environment_leases_agent_id_idx ON environment_leases(agent_id);

-- Create index on run_id for filtering by run
CREATE INDEX IF NOT EXISTS environment_leases_run_id_idx ON environment_leases(run_id);

-- Create index on issue_id for filtering by issue
CREATE INDEX IF NOT EXISTS environment_leases_issue_id_idx ON environment_leases(issue_id);

-- Create index on expires_at for cleanup queries
CREATE INDEX IF NOT EXISTS environment_leases_expires_at_idx ON environment_leases(expires_at) WHERE expires_at IS NOT NULL;

-- Create composite index for finding reusable leases
CREATE INDEX IF NOT EXISTS environment_leases_reusable_idx
    ON environment_leases(environment_id, status, policy)
    WHERE status = 'active' AND policy = 'reusable';

-- Insert default local environment
INSERT INTO environments (name, description, driver, status, config, env_vars)
VALUES (
    'Local',
    'Default execution environment for agent runs on this machine',
    'local',
    'active',
    '{}'::jsonb,
    '{}'::jsonb
)
ON CONFLICT (name) DO NOTHING;

-- Add comments for documentation
COMMENT ON TABLE environments IS 'Execution environments for agent runs (local, SSH, sandbox, plugin)';
COMMENT ON TABLE environment_leases IS 'Runtime leases tracking environment allocation to agent runs';

COMMENT ON COLUMN environments.driver IS 'Environment driver type: local, ssh, sandbox, plugin';
COMMENT ON COLUMN environments.status IS 'Environment status: active, archived';
COMMENT ON COLUMN environments.config IS 'Driver-specific configuration (JSONB)';
COMMENT ON COLUMN environments.env_vars IS 'Environment variables for this environment (JSONB)';
COMMENT ON COLUMN environments.metadata IS 'Additional metadata (JSONB)';

COMMENT ON COLUMN environment_leases.policy IS 'Lease policy: ephemeral (single-use), reusable (multi-run)';
COMMENT ON COLUMN environment_leases.status IS 'Lease status: active, expired, released, failed';
COMMENT ON COLUMN environment_leases.cleanup_status IS 'Cleanup status: pending, in_progress, completed, failed';
COMMENT ON COLUMN environment_leases.workspace_id IS 'External workspace identifier from environment provider';
COMMENT ON COLUMN environment_leases.lease_metadata IS 'Lease-specific metadata (JSONB)';
