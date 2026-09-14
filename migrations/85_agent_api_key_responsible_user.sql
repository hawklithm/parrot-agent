-- 85_agent_api_key_responsible_user.sql
--
-- 对齐 Paperclip `packages/db/src/schema/agent_api_keys.ts:14`
-- （`responsibleUserId: text("responsible_user_id")`）。
--
-- 背景：Parrot 此前把 `agents.reports_to` 当作 responsible user 来解析
-- (`crates/services/src/auth/middleware.rs` 的 `resolve_agent_key`)，
-- 但 `agents.reports_to` 引用的是 `agents(id)` —— 它是**组织架构的上级 agent**，
-- 不是用户。两者类型都是 UUID，所以这个错位不会在编译期或 DDL 层暴露，
-- 只会在「agent 用 API key 调用公司级接口」时静默地取到一个 agent id 当 user id，
-- 进而让 `on_behalf_of_memberships` 查询落空、公司级写操作被 403 拒绝。
--
-- 本迁移把 responsible user 挪回它唯一正确的家：key 自己身上。
-- 存量 key 的 responsible user 无法从旧数据可靠回填（旧值本就可能是 agent id），
-- 因此留空；`resolve_agent_key` 会按 Paperclip `RESPONSIBLE_USER_UNAVAILABLE`
-- 的语义拒绝这类 key，直到它们被重新签发。

ALTER TABLE agent_api_keys
    ADD COLUMN IF NOT EXISTS responsible_user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL;

-- Paperclip 用 text 存 user id；Parrot 全库 `user_id` 都是 uuid
-- （`auth_users.id` 即 uuid），沿用 uuid 以保持库内一致。
COMMENT ON COLUMN agent_api_keys.responsible_user_id IS
    'Responsible human user for this key; mirrors Paperclip agent_api_keys.responsible_user_id.';

CREATE INDEX IF NOT EXISTS idx_agent_api_keys_responsible_user_id
    ON agent_api_keys(responsible_user_id)
    WHERE responsible_user_id IS NOT NULL;
