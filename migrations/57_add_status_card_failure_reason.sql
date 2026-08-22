-- #110: status-card refresh failure reason + last generation meta
-- (Paperclip status_cards.ts failureReason / lastGeneratedAt / lastModel).
-- Tracks the last failed refresh for retry/error surface; split into its own
-- migration so migration 56's checksum stays stable.
ALTER TABLE status_cards
    ADD COLUMN IF NOT EXISTS failure_reason TEXT,
    ADD COLUMN IF NOT EXISTS last_generated_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_model TEXT,
    ADD COLUMN IF NOT EXISTS last_update_run_kind TEXT;
