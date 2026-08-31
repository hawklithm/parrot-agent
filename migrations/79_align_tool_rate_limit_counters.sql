-- Align with Paperclip canonical schema
-- (packages/db/src/schema/tool_access.ts: toolRateLimitCounters).
--
-- Backs the Tool Gateway policy ladder's rate_limit stage: per-policy
-- sliding-window counters keyed by (company, policy, bucket, window kind,
-- window start). Paperclip increments atomically via INSERT ... ON CONFLICT
-- with remaining > 0 guard.
CREATE TABLE IF NOT EXISTS tool_rate_limit_counters (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id uuid NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    policy_id uuid NOT NULL REFERENCES tool_policies(id) ON DELETE CASCADE,
    counter_key text NOT NULL,
    scope_type text NOT NULL,
    scope_id text NOT NULL,
    window_kind text NOT NULL,
    window_start_at timestamptz NOT NULL,
    "limit" integer NOT NULL,
    remaining integer NOT NULL,
    reset_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT NOW(),
    updated_at timestamptz NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS tool_rate_limit_counters_company_idx
    ON tool_rate_limit_counters (company_id);
CREATE UNIQUE INDEX IF NOT EXISTS tool_rate_limit_counters_window_uq
    ON tool_rate_limit_counters (company_id, policy_id, counter_key, window_kind, window_start_at);
