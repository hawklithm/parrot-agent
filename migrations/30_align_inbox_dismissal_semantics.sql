ALTER TABLE inbox_dismissals
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

UPDATE inbox_dismissals
SET updated_at = COALESCE(updated_at, dismissed_at, created_at, NOW())
WHERE updated_at IS NULL;

DO $$ BEGIN
    ALTER TABLE inbox_dismissals
        ADD CONSTRAINT inbox_dismissals_kind_check
        CHECK (kind IN ('dismiss', 'snooze'));
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE inbox_dismissals
        ADD CONSTRAINT inbox_dismissals_kind_snooze_until_check
        CHECK ((kind = 'dismiss' AND snoozed_until IS NULL) OR (kind = 'snooze' AND snoozed_until IS NOT NULL));
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS idx_inbox_dismissals_company_user_updated
    ON inbox_dismissals(company_id, user_id, updated_at DESC);
