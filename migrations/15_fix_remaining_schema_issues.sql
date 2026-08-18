-- 修复剩余的数据库 schema 问题

-- 1. 为 activity_log 表添加缺失的列
ALTER TABLE activity_log ADD COLUMN IF NOT EXISTS resource_type TEXT;
ALTER TABLE activity_log ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}';

-- 2. 为 decisions 表添加 made_by_agent_id 列
ALTER TABLE decisions ADD COLUMN IF NOT EXISTS made_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

-- 3. 为 workspaces 表添加 issue_id 列（如果需要的话）
ALTER TABLE workspaces ADD COLUMN IF NOT EXISTS issue_id UUID REFERENCES issues(id) ON DELETE SET NULL;

-- 4. 为 runs 表添加 issue_id 列
ALTER TABLE runs ADD COLUMN IF NOT EXISTS issue_id UUID REFERENCES issues(id) ON DELETE SET NULL;

-- 5. 更新 issue_status enum 以包含 pending 和 completed
-- 先检查是否需要添加这些值
DO $$ 
BEGIN
    -- 添加 'pending' 状态（如果不存在）
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'pending' AND enumtypid = 'issue_status'::regtype) THEN
        ALTER TYPE issue_status ADD VALUE 'pending';
    END IF;
    
    -- 添加 'completed' 状态（如果不存在）
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'completed' AND enumtypid = 'issue_status'::regtype) THEN
        ALTER TYPE issue_status ADD VALUE 'completed';
    END IF;
END $$;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_workspaces_issue_id ON workspaces(issue_id);
CREATE INDEX IF NOT EXISTS idx_runs_issue_id ON runs(issue_id);
CREATE INDEX IF NOT EXISTS idx_activity_log_resource_type ON activity_log(resource_type);
CREATE INDEX IF NOT EXISTS idx_decisions_made_by_agent_id ON decisions(made_by_agent_id);
