-- Paperclip parity: tool_stdio_command_templates
-- Baseline: packages/db/src/schema/tool_access.ts (toolStdioCommandTemplates)
CREATE TABLE IF NOT EXISTS tool_stdio_command_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    template_key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
    command TEXT NOT NULL,
    args JSONB NOT NULL DEFAULT '[]',
    env_keys JSONB NOT NULL DEFAULT '[]',
    tools JSONB NOT NULL DEFAULT '[]',
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    disabled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT tool_stdio_command_templates_company_key_uq UNIQUE (company_id, template_key)
);

CREATE INDEX IF NOT EXISTS tool_stdio_command_templates_company_idx
    ON tool_stdio_command_templates(company_id);
CREATE INDEX IF NOT EXISTS tool_stdio_command_templates_company_status_idx
    ON tool_stdio_command_templates(company_id, status);
