-- 对齐 Paperclip invites 设计：补 revoked_at（invite.revoked 审计所需）
-- 幂等：重复执行安全
ALTER TABLE invites ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ;

-- company 级 invite 列表/撤销查询索引（对齐 Paperclip）
CREATE INDEX IF NOT EXISTS idx_invites_company_id ON invites(company_id);
