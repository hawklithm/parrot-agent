-- Restore the Issue annotation scope used by the canonical issue document API.
-- Routine annotations already use routine_id; issue annotations need an
-- explicit foreign key so rows cannot be attached to another issue.

ALTER TABLE document_annotation_threads
    ADD COLUMN IF NOT EXISTS issue_id UUID REFERENCES issues(id) ON DELETE CASCADE;

ALTER TABLE document_annotation_comments
    ADD COLUMN IF NOT EXISTS issue_id UUID REFERENCES issues(id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS document_annotation_threads_issue_document_idx
    ON document_annotation_threads(issue_id, document_id, status);

CREATE INDEX IF NOT EXISTS document_annotation_comments_issue_thread_idx
    ON document_annotation_comments(issue_id, thread_id, created_at);
