-- Versioned owner-bound user secret values for setup-token compare-and-set.
ALTER TABLE user_secret_declarations
    ADD COLUMN IF NOT EXISTS latest_version INTEGER NOT NULL DEFAULT 1;

ALTER TABLE claude_setup_token_sessions
    ADD COLUMN IF NOT EXISTS expected_secret_id UUID,
    ADD COLUMN IF NOT EXISTS expected_latest_version INTEGER;
