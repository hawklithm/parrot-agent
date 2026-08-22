-- #110: status-card refresh scheduling columns (Paperclip status_cards.ts)
-- StatusCardWorker.request_refresh / scheduler ticks read these columns, but
-- the unified baseline status_cards table never had them, so every refresh
-- request 500'd on a missing column.
ALTER TABLE status_cards
    ADD COLUMN IF NOT EXISTS fingerprint JSONB,
    ADD COLUMN IF NOT EXISTS fingerprint_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS pending_change_hash TEXT,
    ADD COLUMN IF NOT EXISTS last_change_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS next_eval_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS status_cards_company_next_eval_idx
    ON status_cards (company_id, next_eval_at);
