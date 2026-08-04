-- Paperclip MCP agents can add approval comments. Preserve existing board
-- comments while allowing a gateway Agent actor to be recorded without
-- fabricating a Board user identity.
ALTER TABLE approval_comments
    ALTER COLUMN author_user_id DROP NOT NULL;

ALTER TABLE approval_comments
    ADD COLUMN IF NOT EXISTS author_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

UPDATE approval_comments
   SET author_user_id = NULL
 WHERE author_user_id IS NOT NULL AND author_agent_id IS NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conname = 'approval_comments_author_check'
           AND conrelid = 'approval_comments'::regclass
    ) THEN
        ALTER TABLE approval_comments
            ADD CONSTRAINT approval_comments_author_check
            CHECK ((author_user_id IS NOT NULL) <> (author_agent_id IS NOT NULL));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_approval_comments_agent
    ON approval_comments (author_agent_id)
 WHERE author_agent_id IS NOT NULL;
