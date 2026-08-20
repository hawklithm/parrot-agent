-- Durable workspace cleanup task state.
-- Cleanup is intentionally auditable: a workspace is marked inactive and its
-- metadata records the cleanup completion instead of deleting the workspace
-- row or its history.

ALTER TABLE workspaces
    ADD COLUMN IF NOT EXISTS last_accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

CREATE TABLE IF NOT EXISTS workspace_cleanup_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    cleanup_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Pending',
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    details JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS workspace_cleanup_tasks_status_idx
    ON workspace_cleanup_tasks(status, started_at);

CREATE INDEX IF NOT EXISTS workspace_cleanup_tasks_workspace_idx
    ON workspace_cleanup_tasks(workspace_id, created_at DESC);
