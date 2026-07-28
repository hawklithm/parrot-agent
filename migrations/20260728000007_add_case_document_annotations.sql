ALTER TABLE document_annotation_threads
    ADD COLUMN IF NOT EXISTS case_id UUID REFERENCES cases(id) ON DELETE CASCADE;

ALTER TABLE document_annotation_comments
    ADD COLUMN IF NOT EXISTS case_id UUID REFERENCES cases(id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS document_annotation_threads_case_document_idx
    ON document_annotation_threads(case_id, document_id, status);

CREATE INDEX IF NOT EXISTS document_annotation_comments_case_thread_idx
    ON document_annotation_comments(case_id, thread_id, created_at);
