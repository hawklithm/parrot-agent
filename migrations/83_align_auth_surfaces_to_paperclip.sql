-- Migration: Align auth surfaces (board_api_keys / cli_auth_challenges /
--            instance_user_roles) with Paperclip
--
-- 本迁移修复三张表相对 Paperclip 源码（paperclip/packages/db/src/schema/*.ts）
-- 的 schema 漂移。三张表在统一初始化脚本 00_init_schema_unified.sql 中建成了
-- Paperclip 的早期形状，而 Rust 侧（crates/repositories/*）已按 Paperclip 形状读写：
--
--   board_api_keys
--     live（0 行）缺列：is_revoked、revoked_at、revoked_by_user_id、updated_at。
--     并且残留 Paperclip 没有的 `company_id UUID NOT NULL`——Rust 的
--     PgBoardApiKeyRepository::create() 的 INSERT 根本不绑定该列，导致每次创建
--     API Key 都以 NOT NULL 违约 500；同时 `name` 允许 NULL，Paperclip 为 NOT NULL。
--     Paperclip 的索引 board_api_keys_key_hash_idx / board_api_keys_user_idx 也缺失。
--
--   cli_auth_challenges
--     live（0 行）是旧版单机设备码形状，缺列：secret_hash、command、client_name、
--     requested_access、requested_company_id、pending_key_hash、pending_key_name、
--     approved_by_user_id、board_api_key_id、approved_at、cancelled_at、updated_at；
--     并残留 Paperclip 已废弃的 challenge_code / user_id / approved。
--     其中 challenge_code 是 NOT NULL UNIQUE，会直接阻塞 Paperclip 形状的 INSERT。
--
--   instance_user_roles
--     live（1 行）缺列：updated_at。
--
-- 幂等性：所有语句均可在「表 0 行」「列已存在」「约束已存在」等情况下重复执行。
-- 非破坏性优先：board_api_keys.company_id 只解除 NOT NULL 约束，不 DROP COLUMN。
-- cli_auth_challenges 为 0 行，故直接删除废弃列以腾出唯一形状。
-- 注意：Parrot 的 auth_users.id 是 UUID（Paperclip 中是 text），因此所有指向
-- auth_users 的 FK 一律使用 UUID，approved_by_user_id 亦不例外。

-- ---------------------------------------------------------------------------
-- 1. board_api_keys —— 补齐撤回/更新时间列，解除 company_id NOT NULL 违约
-- ---------------------------------------------------------------------------

ALTER TABLE board_api_keys
    ADD COLUMN IF NOT EXISTS is_revoked BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS revoked_by_user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

-- Paperclip 的 board_api_keys 没有 company_id，Rust 的 INSERT 也不绑定它。
-- 保留列（非破坏性）但去掉 NOT NULL，避免 create() 直接违约。
ALTER TABLE board_api_keys
    ALTER COLUMN company_id DROP NOT NULL;

-- Paperclip 的 name 为 NOT NULL；表 0 行，可安全收紧。
ALTER TABLE board_api_keys
    ALTER COLUMN name SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS board_api_keys_key_hash_idx
    ON board_api_keys(key_hash);

CREATE INDEX IF NOT EXISTS board_api_keys_user_idx
    ON board_api_keys(user_id);

-- ---------------------------------------------------------------------------
-- 2. cli_auth_challenges —— 从旧版单机挑战形状切换到 Paperclip 设备授权形状
-- ---------------------------------------------------------------------------

-- 先移除 Paperclip 已废弃的旧列。表为 0 行，无需数据迁移。
-- DROP COLUMN 时索引/唯一约束随之自动删除。
ALTER TABLE cli_auth_challenges
    DROP COLUMN IF EXISTS challenge_code,
    DROP COLUMN IF EXISTS user_id,
    DROP COLUMN IF EXISTS approved;

-- 再按 Paperclip 补列。NOT NULL 的新列先带 DEFAULT 再统一 DROP DEFAULT，
-- 以保证语句在任何行数下都安全。
ALTER TABLE cli_auth_challenges
    ADD COLUMN IF NOT EXISTS secret_hash TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS command TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS client_name TEXT,
    ADD COLUMN IF NOT EXISTS requested_access TEXT NOT NULL DEFAULT 'board',
    ADD COLUMN IF NOT EXISTS requested_company_id UUID REFERENCES companies(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS pending_key_hash TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS pending_key_name TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS approved_by_user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS board_api_key_id UUID REFERENCES board_api_keys(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS approved_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS cancelled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT now();

-- Paperclip 中 secret_hash / command / pending_key_hash / pending_key_name 无默认值。
ALTER TABLE cli_auth_challenges
    ALTER COLUMN secret_hash DROP DEFAULT,
    ALTER COLUMN command DROP DEFAULT,
    ALTER COLUMN pending_key_hash DROP DEFAULT,
    ALTER COLUMN pending_key_name DROP DEFAULT,
    ALTER COLUMN expires_at DROP DEFAULT;
-- requested_access 保留 DEFAULT 'board'（Paperclip 语义：board / instance_admin_required）。
-- created_at 保留 DEFAULT now()（Paperclip 亦为 notNull defaultNow）。

CREATE INDEX IF NOT EXISTS cli_auth_challenges_secret_hash_idx
    ON cli_auth_challenges(secret_hash);

CREATE INDEX IF NOT EXISTS cli_auth_challenges_approved_by_idx
    ON cli_auth_challenges(approved_by_user_id);

CREATE INDEX IF NOT EXISTS cli_auth_challenges_requested_company_idx
    ON cli_auth_challenges(requested_company_id);

-- ---------------------------------------------------------------------------
-- 3. instance_user_roles —— 补齐 Paperclip 的 updated_at
-- ---------------------------------------------------------------------------

-- 保留既有唯一索引 unique_user_role (user_id, role)；
-- Paperclip 亦无 granted_by_user_id / granted_at 列，故不添加。
ALTER TABLE instance_user_roles
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
