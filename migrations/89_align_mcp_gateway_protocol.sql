-- Paperclip parity: named gateway tokens are durable MCP credentials and do
-- not require an agent heartbeat run. Per-run sessions still use the same
-- table, so only their existing run-scoped validation remains conditional in
-- the gateway session loader.
ALTER TABLE tool_gateway_sessions
    ALTER COLUMN agent_id DROP NOT NULL,
    ALTER COLUMN run_id DROP NOT NULL;

-- Durable protocol-level throttling for the hosted MCP gateway. This is
-- separate from tool policy rate limits: initialize/session traffic must be
-- bounded even when no tool policy exists.
CREATE TABLE IF NOT EXISTS tool_gateway_rate_limit_counters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    counter_key TEXT NOT NULL,
    window_start_at TIMESTAMPTZ NOT NULL,
    window_ms INTEGER NOT NULL,
    "limit" INTEGER NOT NULL,
    count INTEGER NOT NULL DEFAULT 0,
    reset_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (window_ms > 0),
    CHECK ("limit" > 0),
    CHECK (count >= 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS tool_gateway_rate_limit_counters_window_uq
    ON tool_gateway_rate_limit_counters(company_id, counter_key, window_start_at);

CREATE INDEX IF NOT EXISTS tool_gateway_rate_limit_counters_reset_idx
    ON tool_gateway_rate_limit_counters(reset_at);
