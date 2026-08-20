-- Align issue work products with Paperclip's typed deliverable contract while
-- keeping the legacy name/description/artifact columns for compatibility.
ALTER TABLE issue_work_products
    ADD COLUMN IF NOT EXISTS project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS execution_workspace_id UUID,
    ADD COLUMN IF NOT EXISTS runtime_service_id UUID,
    ADD COLUMN IF NOT EXISTS type TEXT NOT NULL DEFAULT 'artifact',
    ADD COLUMN IF NOT EXISTS provider TEXT NOT NULL DEFAULT 'parrot',
    ADD COLUMN IF NOT EXISTS external_id TEXT,
    ADD COLUMN IF NOT EXISTS title TEXT NOT NULL DEFAULT 'work product',
    ADD COLUMN IF NOT EXISTS url TEXT,
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active',
    ADD COLUMN IF NOT EXISTS review_state TEXT NOT NULL DEFAULT 'none',
    ADD COLUMN IF NOT EXISTS is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS health_status TEXT NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS summary TEXT,
    ADD COLUMN IF NOT EXISTS metadata JSONB,
    ADD COLUMN IF NOT EXISTS source_trust JSONB;

UPDATE issue_work_products
SET title = COALESCE(NULLIF(name, ''), 'work product'),
    summary = description,
    metadata = artifact
WHERE title = 'work product';

CREATE INDEX IF NOT EXISTS issue_work_products_company_issue_type_idx
    ON issue_work_products(company_id, issue_id, type);
CREATE INDEX IF NOT EXISTS issue_work_products_company_updated_idx
    ON issue_work_products(company_id, updated_at);
