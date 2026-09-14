-- Migration: Align inbox archive attribution + inbox-agent policy modes with
--            Paperclip
--
-- 修复两张表相对 Paperclip 源码（paperclip/packages/db/src/schema/*.ts）的漂移。
-- 两者都是 Paperclip 的 `inbox:manage` 决策路径直接依赖的形状，漂移会让部分
-- 合法请求无法表达、部分非法请求无法拒绝。
--
--   issue_inbox_archives
--     缺列 archived_by_actor_type / archived_by_agent_id / archived_by_run_id。
--     Paperclip 的 POST /issues/:id/inbox-archive 会把发起归档的 actor 一并落库
--     （routes/issues.ts:7929 传入 archivedByActorType/AgentId/RunId），响应体
--     svc.archiveInbox() 的返回行也包含这三列。Parrot 缺列导致「谁归档了这条
--     issue」不可追溯，且响应形状无法与 Paperclip 对齐。
--
--   user_inbox_agent_policies
--     CHECK 约束只允许 mode IN ('open','allowlist')，而 Paperclip 的
--     inboxAgentPolicyModeSchema 是 z.enum(["open","allowlist","disabled"])
--     （packages/shared/src/validators/inbox-agent-policy.ts:3）。
--     'disabled' 是 authorization.ts:2049 的 inbox_management_disabled 拒绝码的
--     唯一触发条件——约束缺失使该拒绝路径永远不可达，用户无法关闭自己收件箱的
--     agent 代管。同时补上 Paperclip 的 allowed_agent_ids GIN 索引。
--
-- 幂等性：所有语句均可在「列已存在」「约束已存在」等情况下重复执行。

-- ---------------------------------------------------------------------------
-- 1. issue_inbox_archives —— 补齐归档人归属列
-- ---------------------------------------------------------------------------

ALTER TABLE issue_inbox_archives
    ADD COLUMN IF NOT EXISTS archived_by_actor_type TEXT NOT NULL DEFAULT 'user',
    ADD COLUMN IF NOT EXISTS archived_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS archived_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL;

-- 历史行没有归属信息，一律按 Paperclip 的默认值 'user' 处理（见上方 DEFAULT）。
-- CHECK 与 DEFAULT 一起保证「agent 归档必须写出 agent id」这一不变量。
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'issue_inbox_archives_archived_by_actor_type_check'
    ) THEN
        ALTER TABLE issue_inbox_archives
            ADD CONSTRAINT issue_inbox_archives_archived_by_actor_type_check
            CHECK (archived_by_actor_type IN ('user', 'agent'));
    END IF;
END
$$;

-- ---------------------------------------------------------------------------
-- 2. user_inbox_agent_policies —— 放开 'disabled' 模式并补 GIN 索引
-- ---------------------------------------------------------------------------

-- 旧约束名为 _mode_check 且只允许两种取值；先删后加，避免与新语义冲突。
ALTER TABLE user_inbox_agent_policies
    DROP CONSTRAINT IF EXISTS user_inbox_agent_policies_mode_check;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'user_inbox_agent_policies_mode_check'
    ) THEN
        ALTER TABLE user_inbox_agent_policies
            ADD CONSTRAINT user_inbox_agent_policies_mode_check
            CHECK (mode IN ('open', 'allowlist', 'disabled'));
    END IF;
END
$$;

-- Paperclip 用 GIN 索引支撑 allowlist 的包含查询。
CREATE INDEX IF NOT EXISTS user_inbox_agent_policies_allowed_agent_ids_idx
    ON user_inbox_agent_policies USING gin (allowed_agent_ids);
