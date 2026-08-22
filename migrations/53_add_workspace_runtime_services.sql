-- Migration 53: add workspace_runtime_services
--
-- 对齐 Paperclip `packages/db/src/schema/workspace_runtime_services.ts`，为
-- Execution Workspace 提供运行时服务状态 read model（provision status /
-- service ports / runtime service state，#165）。provider 实际进程执行仍属
-- Provider 能力矩阵边界；本表先承载声明式状态与 read model。
--
-- 索引与索引名对齐 Paperclip drizzle 定义（company + execution_workspace +
-- status、company + updated_at）。

CREATE TABLE IF NOT EXISTS workspace_runtime_services (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    project_workspace_id UUID REFERENCES project_workspaces(id) ON DELETE SET NULL,
    execution_workspace_id UUID REFERENCES execution_workspaces(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    scope_type TEXT NOT NULL,
    scope_id TEXT,
    service_name TEXT NOT NULL,
    status TEXT NOT NULL,
    lifecycle TEXT NOT NULL,
    reuse_key TEXT,
    command TEXT,
    cwd TEXT,
    port INTEGER,
    url TEXT,
    provider TEXT NOT NULL,
    provider_ref TEXT,
    owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    started_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    stopped_at TIMESTAMPTZ,
    stop_policy JSONB,
    exposure JSONB,
    exposure_handle TEXT,
    backend_url TEXT,
    health_status TEXT NOT NULL DEFAULT 'unknown',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT workspace_runtime_services_status_check
        CHECK (status IN ('provisioning', 'starting', 'running', 'stopped', 'failed')),
    CONSTRAINT workspace_runtime_services_lifecycle_check
        CHECK (lifecycle IN ('shared', 'ephemeral')),
    CONSTRAINT workspace_runtime_services_provider_check
        CHECK (provider IN ('local_process', 'adapter_managed')),
    CONSTRAINT workspace_runtime_services_health_check
        CHECK (health_status IN ('unknown', 'healthy', 'unhealthy'))
);

CREATE INDEX IF NOT EXISTS workspace_runtime_services_company_execution_workspace_status_idx
    ON workspace_runtime_services(company_id, execution_workspace_id, status);
CREATE INDEX IF NOT EXISTS workspace_runtime_services_company_updated_idx
    ON workspace_runtime_services(company_id, updated_at);
