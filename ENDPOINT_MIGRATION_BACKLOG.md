# Endpoint Migration Backlog (Paperclip → Parrot, 全量)

自动生成：`scripts/gen_endpoint_backlog.py`。按用户决策：**by-design-candidate + missing 全部迁移**。

- 待迁移端点: **62**（by-design-candidate: 6，missing: 56）
- 实现每个端点时同步：handler + service + schema（如需）+ 权限 + activity log 事件（见 audit event 列）。

| # | Method | Path | Paperclip source | 状态 | 域 | 建议 audit event |
|---|---|---|---|---|---|---|
| 1 | DELETE | `/api/issues/:param/comments/:param` | `routes/issues.ts:10741` | missing | issues | `issue.deleted` |
| 2 | DELETE | `/api/issues/:param/watchdog` | `routes/issues.ts:6113` | missing | issues | `issue.deleted` |
| 3 | DELETE | `/api/tool-profile-entries/:param` | `routes/tool-access.ts:1027` | missing | tool-profile-entries | `tool-profile-entries.deleted` |
| 4 | GET | `/api/_plugins/:param/ui/:param` | `routes/plugin-ui-static.ts:230` | by-design-candidate | _plugins | `_plugins.updated` |
| 5 | GET | `/api/cases/:param/automation/retry-plan` | `routes/pipelines.ts:2217` | missing | cases | `cases.updated` |
| 6 | GET | `/api/cloud/stacks` | `routes/cloud.ts:30` | by-design-candidate | cloud | `cloud.updated` |
| 7 | GET | `/api/companies/:param/audit/agent-actions` | `routes/activity.ts:238` | missing | companies | `company.updated` |
| 8 | GET | `/api/companies/:param/audit/agent-actions.csv` | `routes/activity.ts:274` | by-design-candidate | companies | `company.updated` |
| 9 | GET | `/api/companies/:param/export/fidelity` | `routes/companies.ts:410` | missing | companies | `company.updated` |
| 10 | GET | `/api/companies/:param/recovery-observability` | `routes/dashboard.ts:34` | missing | companies | `company.updated` |
| 11 | GET | `/api/companies/:param/search/extract` | `routes/issues.ts:5195` | missing | companies | `company.updated` |
| 12 | GET | `/api/companies/:param/users/:param/inbox-agent-policy` | `routes/inbox-agent-policy.ts:84` | missing | companies | `company.updated` |
| 13 | GET | `/api/companies/:param/users/me/inbox-agent-policy` | `routes/inbox-agent-policy.ts:68` | missing | companies | `company.updated` |
| 14 | GET | `/api/companies/import/jobs/:param` | `routes/companies.ts:425` | missing | companies | `company.updated` |
| 15 | GET | `/api/environments/:param/leases` | `routes/environments.ts:951` | missing | environments | `environments.updated` |
| 16 | GET | `/api/environments/:param/secret-refs` | `routes/environments.ts:935` | missing | environments | `environments.updated` |
| 17 | GET | `/api/health` | `routes/health.ts:129` | missing | health | `health.updated` |
| 18 | GET | `/api/issues/:param/watchdog` | `routes/issues.ts:6064` | missing | issues | `issue.updated` |
| 19 | GET | `/api/skills/catalog/:param/files` | `routes/company-skills.ts:295` | missing | skills | `skills.updated` |
| 20 | GET | `/api/tool-connections/:param/test-agents` | `routes/tool-access.ts:659` | missing | tool-connections | `tool-connections.updated` |
| 21 | GET | `/api/tool-connections/:param/test-calls/:param` | `routes/tool-access.ts:720` | missing | tool-connections | `tool-connections.updated` |
| 22 | GET | `/api/tools/oauth/callback` | `routes/tool-access.ts:320` | missing | tools | `tools.updated` |
| 23 | PATCH | `/api/companies/:param/tools/policies/:param` | `routes/tool-access.ts:1206` | missing | companies | `company.updated` |
| 24 | PATCH | `/api/pipelines/:param` | `routes/pipelines.ts:1171` | missing | pipelines | `pipelines.updated` |
| 25 | PATCH | `/api/tool-profile-entries/:param` | `routes/tool-access.ts:1011` | missing | tool-profile-entries | `tool-profile-entries.updated` |
| 26 | POST | `/api/agents/me/connections/:param/start-authorization` | `routes/tool-access.ts:178` | missing | agents | `agent.created` |
| 27 | POST | `/api/agents/me/connections/:param/token` | `routes/tool-access.ts:196` | missing | agents | `agent.created` |
| 28 | POST | `/api/agents/me/secrets/:param/value` | `routes/secrets.ts:355` | missing | agents | `agent.created` |
| 29 | POST | `/api/board-claim/:param/claim` | `routes/access.ts:2666` | by-design-candidate | board-claim | `board-claim.created` |
| 30 | POST | `/api/cases/:param/claim` | `routes/pipelines.ts:1927` | missing | cases | `cases.created` |
| 31 | POST | `/api/cases/:param/release` | `routes/pipelines.ts:1936` | missing | cases | `cases.created` |
| 32 | POST | `/api/cases/:param/transition` | `routes/pipelines.ts:1944` | missing | cases | `cases.created` |
| 33 | POST | `/api/companies/:param/agents` | `routes/agents.ts:2702` | missing | companies | `agent.created` |
| 34 | POST | `/api/companies/:param/tools/apps/:param/finish` | `routes/tool-access.ts:364` | missing | companies | `company.created` |
| 35 | POST | `/api/companies/:param/tools/apps/connect` | `routes/tool-access.ts:255` | missing | companies | `company.created` |
| 36 | POST | `/api/companies/:param/tools/connections` | `routes/tool-access.ts:534` | missing | companies | `company.created` |
| 37 | POST | `/api/companies/:param/tools/examples/:param/install` | `routes/tool-access.ts:413` | missing | companies | `company.created` |
| 38 | POST | `/api/companies/:param/tools/examples/:param/smoke` | `routes/tool-access.ts:435` | missing | companies | `company.created` |
| 39 | POST | `/api/companies/:param/tools/mcp/import-json` | `routes/tool-access.ts:1347` | missing | companies | `company.created` |
| 40 | POST | `/api/companies/:param/tools/policies/:param/duplicate` | `routes/tool-access.ts:1176` | missing | companies | `company.created` |
| 41 | POST | `/api/companies/:param/tools/policies/reorder` | `routes/tool-access.ts:1137` | missing | companies | `company.created` |
| 42 | POST | `/api/companies/:param/tools/policy/test` | `routes/tool-access.ts:1364` | missing | companies | `company.created` |
| 43 | POST | `/api/companies/:param/tools/profiles` | `routes/tool-access.ts:882` | missing | companies | `company.created` |
| 44 | POST | `/api/companies/:param/tools/runtime-slots/:param/restart` | `routes/tool-access.ts:1101` | missing | companies | `company.created` |
| 45 | POST | `/api/companies/:param/tools/runtime-slots/:param/stop` | `routes/tool-access.ts:1095` | missing | companies | `company.created` |
| 46 | POST | `/api/companies/:param/tools/stdio-templates` | `routes/tool-access.ts:1305` | missing | companies | `company.created` |
| 47 | POST | `/api/companies/:param/tools/trust-rules/:param/revoke` | `routes/tool-access.ts:1278` | missing | companies | `company.created` |
| 48 | POST | `/api/companies/import/preview` | `routes/companies.ts:417` | missing | companies | `company.created` |
| 49 | POST | `/api/health/dev-server/restart` | `routes/health.ts:95` | missing | health | `health.created` |
| 50 | POST | `/api/projects/:param/workspaces/:param/runtime-commands/:param` | `routes/projects.ts:635` | missing | projects | `projects.created` |
| 51 | POST | `/api/projects/:param/workspaces/:param/runtime-services/:param` | `routes/projects.ts:634` | missing | projects | `projects.created` |
| 52 | POST | `/api/tool-connections/:param/catalog/refresh` | `routes/tool-access.ts:843` | missing | tool-connections | `tool-connections.created` |
| 53 | POST | `/api/tool-connections/:param/grants/installations` | `routes/tool-access.ts:575` | missing | tool-connections | `tool-connections.created` |
| 54 | POST | `/api/tool-connections/:param/health-check` | `routes/tool-access.ts:812` | missing | tool-connections | `tool-connections.created` |
| 55 | POST | `/api/tool-connections/:param/test-calls` | `routes/tool-access.ts:696` | missing | tool-connections | `tool-connections.created` |
| 56 | POST | `/api/tool-gateway/runtime-slots/:param/restart` | `routes/tool-gateway.ts:609` | by-design-candidate | tool-gateway | `tool-gateway.created` |
| 57 | POST | `/api/tool-gateway/runtime-slots/:param/stop` | `routes/tool-gateway.ts:582` | by-design-candidate | tool-gateway | `tool-gateway.created` |
| 58 | POST | `/api/tool-profiles/:param/duplicate` | `routes/tool-access.ts:929` | missing | tool-profiles | `tool-profiles.created` |
| 59 | POST | `/api/tool-profiles/:param/entries` | `routes/tool-access.ts:995` | missing | tool-profiles | `tool-profiles.created` |
| 60 | POST | `/api/tool-profiles/:param/new-tools/review` | `routes/tool-access.ts:975` | missing | tool-profiles | `tool-profiles.created` |
| 61 | POST | `/api/tools/oauth/:param/start` | `routes/tool-access.ts:310` | missing | tools | `tools.created` |
| 62 | PUT | `/api/issues/:param/watchdog` | `routes/issues.ts:6072` | missing | issues | `issue.updated` |