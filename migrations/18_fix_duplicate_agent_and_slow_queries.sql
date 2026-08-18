-- 修复1: 防止Agent重复启动 - 添加唯一约束
-- 清理重复记录
WITH duplicates AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY agent_id, 
                           COALESCE(context_snapshot->>'issueId', context_snapshot->>'taskId'),
                           company_id
               ORDER BY created_at DESC
           ) as rn
    FROM heartbeat_runs
    WHERE status IN ('queued', 'running')
      AND (context_snapshot->>'issueId' IS NOT NULL OR context_snapshot->>'taskId' IS NOT NULL)
)
DELETE FROM heartbeat_runs
WHERE id IN (SELECT id FROM duplicates WHERE rn > 1);

-- 创建唯一索引防止future重复
CREATE UNIQUE INDEX IF NOT EXISTS idx_heartbeat_runs_unique_active_agent_issue
ON heartbeat_runs (
    agent_id, 
    company_id,
    COALESCE(context_snapshot->>'issueId', context_snapshot->>'taskId')
)
WHERE status IN ('queued', 'running')
  AND (context_snapshot->>'issueId' IS NOT NULL OR context_snapshot->>'taskId' IS NOT NULL);

-- 修复3: 添加性能索引优化慢SQL查询
-- issues表索引
CREATE INDEX IF NOT EXISTS idx_issues_assignee_agent_id ON issues(assignee_agent_id) WHERE assignee_agent_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_issues_status ON issues(status);
CREATE INDEX IF NOT EXISTS idx_issues_company_status ON issues(company_id, status);

-- approvals表索引
CREATE INDEX IF NOT EXISTS idx_approvals_status ON approvals(status);
CREATE INDEX IF NOT EXISTS idx_approvals_company_status ON approvals(company_id, status);

-- project_memberships表索引
CREATE INDEX IF NOT EXISTS idx_project_memberships_project_id ON project_memberships(project_id);
CREATE INDEX IF NOT EXISTS idx_project_memberships_user_id ON project_memberships(user_id);

-- agent_memberships表索引
CREATE INDEX IF NOT EXISTS idx_agent_memberships_agent_id ON agent_memberships(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_memberships_company_id ON agent_memberships(company_id);

-- heartbeat_runs查询性能索引
CREATE INDEX IF NOT EXISTS idx_heartbeat_runs_agent_status 
    ON heartbeat_runs(agent_id, status, created_at DESC) 
    WHERE status IN ('queued', 'running');
