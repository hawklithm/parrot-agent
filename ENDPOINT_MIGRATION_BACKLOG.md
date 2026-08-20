# Endpoint Migration Backlog (Paperclip → Parrot, 全量)

自动生成：`scripts/gen_endpoint_backlog.py`。按用户决策：**by-design-candidate + missing 全部迁移**。

- 待迁移端点: **7**（by-design-candidate: 0，missing: 7）
- 实现每个端点时同步：handler + service + schema（如需）+ 权限 + activity log 事件（见 audit event 列）。

| # | Method | Path | Paperclip source | 状态 | 域 | 建议 audit event |
|---|---|---|---|---|---|---|
| 1 | GET | `/api/companies/:param/claude-oauth-token-status` | `routes/agents.ts:4616` | missing | companies | `company.updated` |
| 2 | GET | `/api/companies/:param/setup-token-login-sessions/:param` | `routes/agents.ts:4702` | missing | companies | `company.updated` |
| 3 | GET | `/api/companies/:param/setup-token-login-sessions/:param/prompt` | `routes/agents.ts:4721` | missing | companies | `company.updated` |
| 4 | POST | `/api/companies/:param/setup-token-login-sessions` | `routes/agents.ts:4632` | missing | companies | `company.created` |
| 5 | POST | `/api/companies/:param/setup-token-login-sessions/:param/cancel` | `routes/agents.ts:4812` | missing | companies | `company.created` |
| 6 | POST | `/api/companies/:param/setup-token-login-sessions/:param/code` | `routes/agents.ts:4754` | missing | companies | `company.created` |
| 7 | POST | `/api/companies/:param/setup-token-login-sessions/:param/completion` | `routes/agents.ts:4790` | missing | companies | `company.created` |