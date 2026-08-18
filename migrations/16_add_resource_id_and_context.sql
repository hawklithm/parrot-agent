-- 修复更多的 schema 问题

-- 1. 为 activity_log 表添加 resource_id 列
ALTER TABLE activity_log ADD COLUMN IF NOT EXISTS resource_id UUID;

-- 2. 为 decisions 表添加 context 列
ALTER TABLE decisions ADD COLUMN IF NOT EXISTS context JSONB NOT NULL DEFAULT '{}';

-- 3. 添加 'terminated' 状态到 issue_status enum
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'terminated' AND enumtypid = 'issue_status'::regtype) THEN
        ALTER TYPE issue_status ADD VALUE 'terminated';
    END IF;
END $$;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_activity_log_resource_id ON activity_log(resource_id);
