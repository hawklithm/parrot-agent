-- Managed-resource binding ledger for built-in agents.
--
-- Paperclip binds built-in agents to independent, company-scoped Skill and
-- Routine resources. Parrot previously only persisted a managed bundle as an
-- Agent-metadata snapshot. This table is the source of truth for the binding
-- between a company + built-in key + canonical resource key and the managed
-- resource rows, and tracks stock/current versions so reconcile can detect and
-- repair drift.
CREATE TABLE IF NOT EXISTS builtin_managed_resources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    built_in_key TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    canonical_resource_key TEXT NOT NULL,
    target_resource_id UUID,
    stock_version TEXT NOT NULL,
    current_version TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    drift_detected BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (company_id, built_in_key, resource_type, canonical_resource_key)
);

CREATE INDEX IF NOT EXISTS idx_builtin_managed_resources_company_key
    ON builtin_managed_resources (company_id, built_in_key);

CREATE INDEX IF NOT EXISTS idx_builtin_managed_resources_company
    ON builtin_managed_resources (company_id);
