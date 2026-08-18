-- 为 workspaces 表添加缺失的列

ALTER TABLE workspaces ADD COLUMN IF NOT EXISTS created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;
ALTER TABLE workspaces ADD COLUMN IF NOT EXISTS assigned_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_workspaces_created_by_agent_id ON workspaces(created_by_agent_id);
CREATE INDEX IF NOT EXISTS idx_workspaces_assigned_agent_id ON workspaces(assigned_agent_id);
