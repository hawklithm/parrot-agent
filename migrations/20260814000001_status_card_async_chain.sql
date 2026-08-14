-- 后台 worker 迁移：status-card / summary-slot 异步任务链（对齐 Paperclip）
--
-- Paperclip 的后台执行链不依赖独立 worker 进程，而是：
--   1. scheduler tick 扫描 next_eval_at 到期卡片（乐观锁 claim 5 分钟窗口）
--   2. compile/refresh 通过「创建 hidden issue + 唤醒 Summarizer 内置 agent」执行
--   3. agent 执行后经 PUT /status-cards/:id/query 与 /summary 写回（强校验 writer）
--   4. issue 终态（done/cancelled/blocked）触发 finalization 释放 generating_issue_id
-- 本迁移补齐 Parrot status_cards 的调度列与执行记录表。

-- 1) status_cards 补调度/指纹/状态列
ALTER TABLE status_cards
    ADD COLUMN IF NOT EXISTS next_eval_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS fingerprint JSONB,
    ADD COLUMN IF NOT EXISTS fingerprint_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS mentioned_issue_ids JSONB NOT NULL DEFAULT '[]',
    ADD COLUMN IF NOT EXISTS pending_change_hash TEXT,
    ADD COLUMN IF NOT EXISTS last_change_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_update_run_kind TEXT,  -- full|incremental|null
    ADD COLUMN IF NOT EXISTS last_generated_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_model TEXT,
    ADD COLUMN IF NOT EXISTS failure_reason TEXT,
    ADD COLUMN IF NOT EXISTS archived_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_status_cards_next_eval
    ON status_cards(archived_at, generating_issue_id, next_eval_at);

-- 2) 执行记录表（对应 Paperclip statusCardUpdates：kind/trigger/generationIssueId/runId/
--    changes/inputTokens/outputTokens/costCents/model/queryVersion/changeSummary/
--    startedAt/finishedAt/status/error）
CREATE TABLE IF NOT EXISTS status_card_update_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    card_id UUID NOT NULL REFERENCES status_cards(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,                    -- compile|full|incremental
    trigger TEXT NOT NULL DEFAULT 'manual',-- manual|interval|reactive|restore
    generation_issue_id UUID,
    run_id UUID,
    changes JSONB NOT NULL DEFAULT '[]',
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cost_cents INTEGER NOT NULL DEFAULT 0,
    model TEXT,
    query_version INTEGER,
    change_summary TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'running',-- running|ok|failed
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_status_card_update_runs_card
    ON status_card_update_runs(card_id, started_at);
CREATE INDEX IF NOT EXISTS idx_status_card_update_runs_gen_issue
    ON status_card_update_runs(generation_issue_id);

-- 3) summary_slots 补 last_model / failure_reason 幂等对齐（Paperclip 已有同名列，
--    此处仅保证 Parrot 存量库不缺列）
ALTER TABLE summary_slots
    ADD COLUMN IF NOT EXISTS last_model TEXT;
