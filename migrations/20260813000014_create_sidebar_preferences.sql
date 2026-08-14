-- 对齐 Paperclip sidebar-preferences 域（user 级 company 顺序 + company 级 project 顺序）
CREATE TABLE IF NOT EXISTS user_sidebar_preferences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    company_order JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id)
);

CREATE TABLE IF NOT EXISTS company_user_sidebar_preferences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    project_order JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (company_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_company_user_sidebar_company
    ON company_user_sidebar_preferences(company_id);
