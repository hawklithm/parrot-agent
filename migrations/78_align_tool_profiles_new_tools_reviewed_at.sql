-- Align `tool_profiles` with Paperclip canonical schema
-- (packages/db/src/schema/tool_access.ts: newToolsReviewedAt).
--
-- Paperclip persists the moment a profile's new-tools queue was last reviewed
-- (`reviewProfileNewTools` sets it inside the review transaction). Parrot's
-- per-entry review state (tool_catalog_entries.reviewed_at/attribution)
-- exists, but the profile-level timestamp was missing.
ALTER TABLE tool_profiles
    ADD COLUMN IF NOT EXISTS new_tools_reviewed_at timestamptz;
