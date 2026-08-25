-- Migration: Extend approval_type enum with budget_override_required
--
-- DefaultBudgetService::create_budget_override_approval inserts an approval of
-- type 'budget_override_required' when a hard-stop budget threshold is crossed,
-- but the live approval_type enum never received the value — the INSERT fails
-- with "invalid input value for enum approval_type" the first time a budget
-- incident fires. PAPERCLIP_MIGRATION_PLAN §4B.2 line 316 / line 327.

ALTER TYPE approval_type ADD VALUE IF NOT EXISTS 'budget_override_required';
