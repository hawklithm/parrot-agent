ALTER TABLE agent_api_keys
    ADD COLUMN IF NOT EXISTS scope JSONB NOT NULL DEFAULT '{}'::jsonb;

CREATE INDEX IF NOT EXISTS agent_api_keys_scope_type_idx
    ON agent_api_keys ((scope->>'scope_type'));
