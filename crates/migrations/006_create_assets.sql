-- Migration: Create Assets Management Tables
-- Description: Asset storage for images, files, and attachments

-- Assets Table
CREATE TABLE assets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    provider TEXT NOT NULL,
    object_key TEXT NOT NULL,
    content_type TEXT NOT NULL,
    byte_size BIGINT NOT NULL,
    sha256 TEXT NOT NULL,
    original_filename TEXT,
    created_by_agent_id UUID,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX assets_company_id_idx ON assets(company_id);
CREATE INDEX assets_created_by_agent_id_idx ON assets(created_by_agent_id);
CREATE INDEX assets_created_by_user_id_idx ON assets(created_by_user_id);
CREATE INDEX assets_sha256_idx ON assets(sha256);
CREATE INDEX assets_created_at_idx ON assets(created_at DESC);
CREATE UNIQUE INDEX assets_provider_object_key_idx ON assets(provider, object_key);

COMMENT ON TABLE assets IS 'Asset storage metadata for images, files, and attachments';
COMMENT ON COLUMN assets.provider IS 'Storage provider: local_fs / s3 / gcs';
COMMENT ON COLUMN assets.object_key IS 'Provider-specific object key or path';
COMMENT ON COLUMN assets.content_type IS 'MIME type (e.g., image/png, image/svg+xml)';
COMMENT ON COLUMN assets.byte_size IS 'File size in bytes';
COMMENT ON COLUMN assets.sha256 IS 'SHA-256 hash for deduplication and integrity';
COMMENT ON COLUMN assets.original_filename IS 'Original filename from upload';
COMMENT ON COLUMN assets.created_by_agent_id IS 'Agent that created this asset (if applicable)';
COMMENT ON COLUMN assets.created_by_user_id IS 'User that created this asset (if applicable)';
