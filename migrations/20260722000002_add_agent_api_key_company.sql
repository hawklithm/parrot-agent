ALTER TABLE agent_api_keys
    ADD COLUMN IF NOT EXISTS company_id UUID;

UPDATE agent_api_keys k
SET company_id = a.company_id
FROM agents a
WHERE a.id = k.agent_id AND k.company_id IS NULL;

ALTER TABLE agent_api_keys
    ALTER COLUMN company_id SET NOT NULL;

ALTER TABLE agent_api_keys
    ADD CONSTRAINT agent_api_keys_company_fk
    FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE;

UPDATE agent_api_keys SET name = 'Agent key' WHERE name IS NULL;
ALTER TABLE agent_api_keys ALTER COLUMN name SET DEFAULT 'Agent key';
ALTER TABLE agent_api_keys ALTER COLUMN name SET NOT NULL;
