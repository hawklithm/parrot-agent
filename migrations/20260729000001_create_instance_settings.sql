CREATE TABLE IF NOT EXISTS instance_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    instance_name TEXT NOT NULL DEFAULT 'Parrot Agent',
    version TEXT NOT NULL DEFAULT '0.1.0',
    general JSONB NOT NULL DEFAULT '{"timezone":"UTC","language":"en"}',
    experimental JSONB NOT NULL DEFAULT '{"issueGraphLivenessAutoRecovery":false,"enableCloudSync":false,"enableBuiltInAgents":true,"enableCases":true}',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO instance_settings (id) VALUES (1) ON CONFLICT (id) DO NOTHING;
