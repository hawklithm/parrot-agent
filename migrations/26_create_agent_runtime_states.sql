-- 创建 agent_runtime_states 表
-- 对齐 Paperclip 的 agent_runtime_state 表结构

CREATE TABLE IF NOT EXISTS agent_runtime_states (
    agent_id UUID PRIMARY KEY REFERENCES agents(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    adapter_type TEXT NOT NULL,
    session_id TEXT,
    session_display_id TEXT,
    session_params_json JSONB,
    state_json JSONB NOT NULL DEFAULT '{}',
    last_run_id UUID,
    last_run_status TEXT,
    total_input_tokens BIGINT NOT NULL DEFAULT 0,
    total_output_tokens BIGINT NOT NULL DEFAULT 0,
    total_cached_input_tokens BIGINT NOT NULL DEFAULT 0,
    total_cost_cents BIGINT NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 索引
CREATE INDEX IF NOT EXISTS agent_runtime_states_company_agent_idx ON agent_runtime_states(company_id, agent_id);
CREATE INDEX IF NOT EXISTS agent_runtime_states_company_updated_idx ON agent_runtime_states(company_id, updated_at);

-- 初始化现有 agents 的 runtime state
INSERT INTO agent_runtime_states (
    agent_id, 
    company_id, 
    adapter_type,
    total_input_tokens,
    total_output_tokens,
    total_cached_input_tokens,
    total_cost_cents
)
SELECT 
    a.id,
    a.company_id,
    a.adapter_type,
    0,
    0,
    0,
    0
FROM agents a
WHERE NOT EXISTS (
    SELECT 1 FROM agent_runtime_states ars WHERE ars.agent_id = a.id
);
