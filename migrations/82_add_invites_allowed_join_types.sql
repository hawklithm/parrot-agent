-- Migration: Add allowed_join_types column to invites
--
-- Aligns with Paperclip invites schema which includes:
--   allowed_join_types TEXT NOT NULL DEFAULT 'both' — which principal kinds
--   (human / agent / both) the invite admits.
--
-- The enum type `allowed_join_types` was created in the unified init schema but
-- the matching column was never added to `invites` (its index was dropped as
-- `[REMOVED]` at 00_init_schema_unified.sql:3382), while 31 code references
-- still read and write it. Every `GET /companies/:company_id/invites` failed
-- with `column "allowed_join_types" does not exist`, as did invite creation.
--
-- Paperclip's column is NOT NULL with default 'both', so existing rows stay
-- valid without a backfill.

ALTER TABLE invites
    ADD COLUMN IF NOT EXISTS allowed_join_types allowed_join_types NOT NULL DEFAULT 'both';
