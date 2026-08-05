ALTER TABLE issues
    ADD COLUMN IF NOT EXISTS harness_kind TEXT;

CREATE INDEX IF NOT EXISTS issues_company_harness_kind_idx
    ON issues(company_id, harness_kind)
    WHERE harness_kind IS NOT NULL;
