-- Complete the access-control persistence contract used by the Paperclip UI.
--
-- Migration 86 adds the membership enum values. This migration is separate so
-- PostgreSQL has committed those enum values before the backfill below uses
-- `archived`.

UPDATE company_memberships
SET status = 'archived'::company_membership_status,
    updated_at = COALESCE(updated_at, NOW())
WHERE status = 'inactive'::company_membership_status;

ALTER TABLE invites
    ADD COLUMN IF NOT EXISTS human_role membership_role,
    ADD COLUMN IF NOT EXISTS defaults_payload JSONB,
    ADD COLUMN IF NOT EXISTS invite_message TEXT,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

ALTER TABLE join_requests
    ADD COLUMN IF NOT EXISTS invite_id UUID REFERENCES invites(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS request_ip TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS request_email_snapshot TEXT,
    ADD COLUMN IF NOT EXISTS agent_name TEXT,
    ADD COLUMN IF NOT EXISTS adapter_type TEXT,
    ADD COLUMN IF NOT EXISTS capabilities TEXT,
    ADD COLUMN IF NOT EXISTS agent_defaults_payload JSONB,
    ADD COLUMN IF NOT EXISTS rejected_by_user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS rejected_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_join_requests_invite_id
    ON join_requests(invite_id);

CREATE INDEX IF NOT EXISTS idx_join_requests_company_status_created
    ON join_requests(company_id, status, created_at DESC);
