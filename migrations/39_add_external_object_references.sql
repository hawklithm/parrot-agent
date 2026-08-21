CREATE TABLE IF NOT EXISTS external_objects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    provider_key TEXT NOT NULL,
    plugin_id UUID REFERENCES plugins(id) ON DELETE SET NULL,
    object_type TEXT NOT NULL,
    external_id TEXT NOT NULL,
    sanitized_canonical_url TEXT,
    canonical_identity_hash TEXT,
    display_key TEXT,
    icon_key TEXT,
    display_title TEXT,
    status_key TEXT,
    status_label TEXT,
    status_icon_key TEXT,
    status_category TEXT NOT NULL DEFAULT 'unknown',
    status_tone TEXT NOT NULL DEFAULT 'neutral',
    liveness TEXT NOT NULL DEFAULT 'unknown',
    is_terminal BOOLEAN NOT NULL DEFAULT FALSE,
    data JSONB NOT NULL DEFAULT '{}'::jsonb,
    remote_version TEXT,
    etag TEXT,
    last_resolved_at TIMESTAMPTZ,
    last_changed_at TIMESTAMPTZ,
    last_error_at TIMESTAMPTZ,
    next_refresh_at TIMESTAMPTZ,
    refresh_started_at TIMESTAMPTZ,
    refresh_token UUID,
    last_error_code TEXT,
    last_error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (company_id, provider_key, object_type, external_id),
    UNIQUE (company_id, provider_key, object_type, canonical_identity_hash)
);

CREATE TABLE IF NOT EXISTS external_object_mentions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_record_id UUID,
    document_key TEXT,
    property_key TEXT,
    matched_text_redacted TEXT,
    sanitized_display_url TEXT,
    canonical_identity_hash TEXT,
    canonical_identity JSONB,
    object_id UUID REFERENCES external_objects(id) ON DELETE SET NULL,
    provider_key TEXT,
    detector_key TEXT,
    object_type TEXT,
    confidence TEXT NOT NULL DEFAULT 'exact',
    created_by_plugin_id UUID REFERENCES plugins(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS external_objects_company_provider_object_idx
    ON external_objects(company_id, provider_key, object_type);
CREATE INDEX IF NOT EXISTS external_objects_company_provider_status_idx
    ON external_objects(company_id, provider_key, status_category);
CREATE INDEX IF NOT EXISTS external_objects_company_refresh_idx
    ON external_objects(company_id, next_refresh_at);
CREATE INDEX IF NOT EXISTS external_object_mentions_company_source_issue_idx
    ON external_object_mentions(company_id, source_issue_id);
CREATE INDEX IF NOT EXISTS external_object_mentions_company_object_idx
    ON external_object_mentions(company_id, object_id);
CREATE INDEX IF NOT EXISTS external_object_mentions_company_provider_idx
    ON external_object_mentions(company_id, provider_key, object_type);

-- Compatibility projection for the older Parrot Issue read routes. New code
-- should read the Paperclip-shaped tables directly.
CREATE OR REPLACE VIEW issue_external_objects AS
SELECT
    mention.source_issue_id AS issue_id,
    object.id,
    object.object_type,
    object.external_id AS object_id,
    jsonb_build_object(
        'providerKey', object.provider_key,
        'title', object.display_title,
        'url', object.sanitized_canonical_url,
        'statusKey', object.status_key,
        'statusLabel', object.status_label,
        'statusCategory', object.status_category,
        'statusTone', object.status_tone,
        'liveness', object.liveness,
        'data', object.data
    ) AS summary,
    object.created_at,
    object.updated_at
FROM external_object_mentions mention
JOIN external_objects object ON object.id = mention.object_id;
