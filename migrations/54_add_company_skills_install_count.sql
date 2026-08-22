-- Migration 54: add install_count to company_skills
--
-- 对齐 Paperclip `packages/db/src/schema/company_skills.ts` 的
-- `installCount`（install_count integer not null default 0）：安装计数随
-- install/移除 ±1，作为公司级 Skill 的来源安装统计（#124）。既有行默认 0，
-- 维护点（install/移除递增）随后续安装流程接入。

ALTER TABLE company_skills
    ADD COLUMN IF NOT EXISTS install_count INTEGER NOT NULL DEFAULT 0;
