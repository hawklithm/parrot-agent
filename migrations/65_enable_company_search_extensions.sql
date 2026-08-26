-- Company Search uses pg_trgm for identifier similarity and fuzzystrmatch for
-- title-token Levenshtein matching. Install both at migration time so request
-- handlers never need DDL privileges on the application connection.
CREATE EXTENSION IF NOT EXISTS "pg_trgm";
CREATE EXTENSION IF NOT EXISTS "fuzzystrmatch";

-- Search always scopes by company and frequently combines these filters with a
-- recent-update sort. The existing single-column indexes remain useful for
-- other endpoints; these composite indexes cover the migration's query shape.
CREATE INDEX IF NOT EXISTS issues_company_status_priority_updated_idx
    ON issues(company_id, status, priority, updated_at DESC);
CREATE INDEX IF NOT EXISTS issue_comments_company_issue_created_idx
    ON issue_comments(company_id, issue_id, created_at ASC);
CREATE INDEX IF NOT EXISTS issue_documents_company_issue_key_idx
    ON issue_documents(company_id, issue_id, key);
CREATE INDEX IF NOT EXISTS documents_company_updated_idx
    ON documents(company_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS issue_work_products_company_issue_updated_idx
    ON issue_work_products(company_id, issue_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS attachments_company_parent_updated_idx
    ON attachments(company_id, parent_type, parent_id, updated_at DESC);
