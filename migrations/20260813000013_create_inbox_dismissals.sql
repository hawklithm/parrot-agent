-- 对齐 Paperclip inbox-dismissals 域：用户对公司 inbox 项的 dismiss/snooze
CREATE TABLE IF NOT EXISTS inbox_dismissals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    item_key TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'dismiss',          -- dismiss | snooze
    dismissed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    snoozed_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (company_id, user_id, item_key)
);

CREATE INDEX IF NOT EXISTS idx_inbox_dismissals_company_user ON inbox_dismissals(company_id, user_id);
