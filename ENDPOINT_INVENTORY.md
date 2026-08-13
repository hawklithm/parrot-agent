# Paperclip / Parrot Endpoint Inventory

自动生成：`scripts/endpoint_inventory.py`（P2.1 脚本化检查）。

## 方法

- **Mount-aware**：Paperclip 按 `app.ts` 的 `api.use("/prefix", xxxRoutes)` 挂载前缀解析完整路径（`auth.ts` → `/api/auth`，其余默认 `/api`）；Parrot 按 `app_state.rs` 的 `/api` nest 解析。
- **Status**：`implemented`（方法+规范化路径匹配）；`partial`（路径匹配但方法不同）；`by-design-candidate`（命中 Paperclip 平台/UI 专属域，需产品确认）；`missing`（无结构匹配）。
- **Limitation**：状态为结构性判定；`partial` 语义、`by-design-candidate` 最终清单需人工复核。

- Paperclip endpoints: **597**
- Parrot endpoints: **703**
- implemented: **534**
- partial: **8**
- by-design-candidate: **6**
- missing: **49**
- Parrot-only (extension): **169**

## 1. Implemented

| Method+Path | Source |
|---|---|
| `DELETE /api/adapters/:param` | `routes/adapters.ts:461` |
| `DELETE /api/agents/:param` | `routes/agents.ts:3478` |
| `DELETE /api/agents/:param/instructions-bundle/file` | `routes/agents.ts:3079` |
| `DELETE /api/agents/:param/keys/:param` | `routes/agents.ts:3542` |
| `DELETE /api/agents/me/secret-proposals/:param` | `routes/secrets.ts:282` |
| `DELETE /api/attachments/:param` | `routes/issues.ts:11923` |
| `DELETE /api/board-api-keys/:param` | `routes/access.ts:2912` |
| `DELETE /api/cases/:param/documents/:param` | `routes/cases.ts:1209` |
| `DELETE /api/cases/:param/issue-links/:param` | `routes/pipelines.ts:2123` |
| `DELETE /api/companies/:param` | `routes/companies.ts:747` |
| `DELETE /api/companies/:param/folders/:param` | `routes/folders.ts:149` |
| `DELETE /api/companies/:param/inbox-dismissals/:param` | `routes/inbox-dismissals.ts:98` |
| `DELETE /api/companies/:param/me/user-secrets/:param` | `routes/secrets.ts:866` |
| `DELETE /api/companies/:param/skill-policy` | `routes/company-skill-policy.ts:99` |
| `DELETE /api/companies/:param/skill-test-run-templates/:param` | `routes/company-skills.ts:530` |
| `DELETE /api/companies/:param/skills/:param` | `routes/company-skills.ts:1261` |
| `DELETE /api/companies/:param/skills/:param/comments/:param` | `routes/company-skills.ts:961` |
| `DELETE /api/companies/:param/skills/:param/star` | `routes/company-skills.ts:804` |
| `DELETE /api/companies/:param/skills/:param/test-inputs/:param` | `routes/company-skills.ts:445` |
| `DELETE /api/companies/:param/skills/:param/test-runs/:param` | `routes/company-skills.ts:716` |
| `DELETE /api/companies/:param/tools/policies/:param` | `routes/tool-access.ts:1230` |
| `DELETE /api/companies/:param/user-secret-definitions/:param` | `routes/secrets.ts:698` |
| `DELETE /api/decision-training/:param` | `routes/decision-training.ts:221` |
| `DELETE /api/environments/:param` | `routes/environments.ts:1089` |
| `DELETE /api/environments/:param/custom-image-template` | `routes/environments.ts:845` |
| `DELETE /api/goals/:param` | `routes/goals.ts:76` |
| `DELETE /api/issues/:param` | `routes/issues.ts:9907` |
| `DELETE /api/issues/:param/approvals/:param` | `routes/issues.ts:7580` |
| `DELETE /api/issues/:param/documents/:param` | `routes/issues.ts:7014` |
| `DELETE /api/issues/:param/inbox-archive` | `routes/issues.ts:7513` |
| `DELETE /api/issues/:param/read` | `routes/issues.ts:7406` |
| `DELETE /api/labels/:param` | `routes/issues.ts:5673` |
| `DELETE /api/pipelines/:param/stages/:param` | `routes/pipelines.ts:1240` |
| `DELETE /api/plugins/:param` | `routes/plugins.ts:1969` |
| `DELETE /api/projects/:param` | `routes/projects.ts:666` |
| `DELETE /api/projects/:param/workspaces/:param` | `routes/projects.ts:637` |
| `DELETE /api/routine-triggers/:param` | `routes/routines.ts:543` |
| `DELETE /api/secret-provider-configs/:param` | `routes/secrets.ts:514` |
| `DELETE /api/secrets/:param` | `routes/secrets.ts:1113` |
| `DELETE /api/status-cards/:param` | `routes/status-cards.ts:199` |
| `DELETE /api/tool-applications/:param` | `routes/tool-access.ts:511` |
| `DELETE /api/tool-connections/:param` | `routes/tool-access.ts:783` |
| `DELETE /api/tool-connections/:param/grants/:param` | `routes/tool-access.ts:601` |
| `DELETE /api/tool-profiles/:param` | `routes/tool-access.ts:954` |
| `DELETE /api/work-products/:param` | `routes/issues.ts:7339` |
| `GET /api/adapters` | `routes/adapters.ts:222` |
| `GET /api/adapters/:param` | `routes/adapters.ts:375` |
| `GET /api/adapters/:param/config-schema` | `routes/adapters.ts:663` |
| `GET /api/adapters/:param/ui-parser.js` | `routes/adapters.ts:703` |
| `GET /api/admin/users` | `routes/access.ts:4765` |
| `GET /api/admin/users/:param/company-access` | `routes/access.ts:4836` |
| `GET /api/agents/:param` | `routes/agents.ts:2366` |
| `GET /api/agents/:param/config-revisions` | `routes/agents.ts:2404` |
| `GET /api/agents/:param/config-revisions/:param` | `routes/agents.ts:2416` |
| `GET /api/agents/:param/configuration` | `routes/agents.ts:2393` |
| `GET /api/agents/:param/instructions-bundle` | `routes/agents.ts:2963` |
| `GET /api/agents/:param/instructions-bundle/file` | `routes/agents.ts:3017` |
| `GET /api/agents/:param/keys` | `routes/agents.ts:3502` |
| `GET /api/agents/:param/runtime-state` | `routes/agents.ts:2466` |
| `GET /api/agents/:param/skills` | `routes/agents.ts:2004` |
| `GET /api/agents/:param/task-sessions` | `routes/agents.ts:2477` |
| `GET /api/agents/me` | `routes/agents.ts:2265` |
| `GET /api/agents/me/inbox-lite` | `routes/agents.ts:2299` |
| `GET /api/agents/me/inbox/mine` | `routes/agents.ts:2348` |
| `GET /api/agents/me/secret-proposals` | `routes/secrets.ts:267` |
| `GET /api/agents/me/secrets` | `routes/secrets.ts:336` |
| `GET /api/approvals/:param` | `routes/approvals.ts:216` |
| `GET /api/approvals/:param/comments` | `routes/approvals.ts:501` |
| `GET /api/approvals/:param/issues` | `routes/approvals.ts:279` |
| `GET /api/assets/:param/content` | `routes/assets.ts:314` |
| `GET /api/attachments/:param/content` | `routes/issues.ts:11862` |
| `GET /api/auth/get-session` | `routes/auth.ts:40` |
| `GET /api/auth/profile` | `routes/auth.ts:55` |
| `GET /api/board-api-keys` | `routes/access.ts:2859` |
| `GET /api/board-claim/:param` | `routes/access.ts:2655` |
| `GET /api/cases/:param` | `routes/cases.ts:1450` |
| `GET /api/cases/:param/children` | `routes/pipelines.ts:1894` |
| `GET /api/cases/:param/children/tree` | `routes/pipelines.ts:2171` |
| `GET /api/cases/:param/context-pack` | `routes/pipelines.ts:2183` |
| `GET /api/cases/:param/documents/:param` | `routes/cases.ts:785` |
| `GET /api/cases/:param/documents/:param/annotations` | `routes/cases.ts:794` |
| `GET /api/cases/:param/documents/:param/annotations/:param` | `routes/cases.ts:806` |
| `GET /api/cases/:param/documents/:param/revisions` | `routes/cases.ts:1370` |
| `GET /api/cases/:param/events` | `routes/cases.ts:1343` |
| `GET /api/cases/:param/issue-links` | `routes/pipelines.ts:2070` |
| `GET /api/cases/:param/outputs` | `routes/pipelines.ts:2086` |
| `GET /api/cases/:param/rollup` | `routes/pipelines.ts:2177` |
| `GET /api/cli-auth/challenges/:param` | `routes/access.ts:2751` |
| `GET /api/cli-auth/me` | `routes/access.ts:2843` |
| `GET /api/companies` | `routes/companies.ts:264` |
| `GET /api/companies/:param` | `routes/companies.ts:359` |
| `GET /api/companies/:param/activity` | `routes/activity.ts:222` |
| `GET /api/companies/:param/adapters/:param/detect-model` | `routes/agents.ts:1841` |
| `GET /api/companies/:param/adapters/:param/model-profiles` | `routes/agents.ts:1833` |
| `GET /api/companies/:param/adapters/:param/models` | `routes/agents.ts:1809` |
| `GET /api/companies/:param/agent-configurations` | `routes/agents.ts:2258` |
| `GET /api/companies/:param/agents` | `routes/agents.ts:2144` |
| `GET /api/companies/:param/approvals` | `routes/approvals.ts:207` |
| `GET /api/companies/:param/artifacts` | `routes/companies.ts:296` |
| `GET /api/companies/:param/attention` | `routes/attention.ts:18` |
| `GET /api/companies/:param/budgets/overview` | `routes/costs.ts:288` |
| `GET /api/companies/:param/built-in-agents` | `routes/built-in-agents.ts:148` |
| `GET /api/companies/:param/built-in-agents/:param/status` | `routes/built-in-agents.ts:156` |
| `GET /api/companies/:param/case-events` | `routes/pipelines.ts:880` |
| `GET /api/companies/:param/cases` | `routes/cases.ts:720` |
| `GET /api/companies/:param/costs/by-agent` | `routes/costs.ts:192` |
| `GET /api/companies/:param/costs/by-agent-model` | `routes/costs.ts:201` |
| `GET /api/companies/:param/costs/by-biller` | `routes/costs.ts:219` |
| `GET /api/companies/:param/costs/by-project` | `routes/costs.ts:321` |
| `GET /api/companies/:param/costs/by-provider` | `routes/costs.ts:210` |
| `GET /api/companies/:param/costs/finance-by-biller` | `routes/costs.ts:237` |
| `GET /api/companies/:param/costs/finance-by-kind` | `routes/costs.ts:246` |
| `GET /api/companies/:param/costs/finance-events` | `routes/costs.ts:255` |
| `GET /api/companies/:param/costs/finance-summary` | `routes/costs.ts:228` |
| `GET /api/companies/:param/costs/quota-windows` | `routes/costs.ts:273` |
| `GET /api/companies/:param/costs/summary` | `routes/costs.ts:173` |
| `GET /api/companies/:param/costs/window-spend` | `routes/costs.ts:265` |
| `GET /api/companies/:param/dashboard` | `routes/dashboard.ts:27` |
| `GET /api/companies/:param/decision-queue-seed-rules` | `routes/decision-queues.ts:71` |
| `GET /api/companies/:param/decision-queues` | `routes/decision-queues.ts:77` |
| `GET /api/companies/:param/decision-queues/:param/items` | `routes/decision-queues.ts:107` |
| `GET /api/companies/:param/decision-training` | `routes/decision-training.ts:114` |
| `GET /api/companies/:param/decision-training/export.jsonl` | `routes/decision-training.ts:136` |
| `GET /api/companies/:param/decision-triage/:param/:param` | `routes/decision-queues.ts:153` |
| `GET /api/companies/:param/decisions` | `routes/decisions.ts:148` |
| `GET /api/companies/:param/decisions/stats` | `routes/decisions.ts:164` |
| `GET /api/companies/:param/environments` | `routes/environments.ts:592` |
| `GET /api/companies/:param/environments/capabilities` | `routes/environments.ts:611` |
| `GET /api/companies/:param/execution-workspaces` | `routes/execution-workspaces.ts:75` |
| `GET /api/companies/:param/feedback-traces` | `routes/companies.ts:374` |
| `GET /api/companies/:param/folders` | `routes/folders.ts:26` |
| `GET /api/companies/:param/goals` | `routes/goals.ts:14` |
| `GET /api/companies/:param/heartbeat-runs` | `routes/agents.ts:3772` |
| `GET /api/companies/:param/inbox-dismissals` | `routes/inbox-dismissals.ts:52` |
| `GET /api/companies/:param/invites` | `routes/access.ts:4131` |
| `GET /api/companies/:param/issues` | `routes/issues.ts:5271` |
| `GET /api/companies/:param/issues/count` | `routes/issues.ts:5558` |
| `GET /api/companies/:param/join-requests` | `routes/access.ts:4139` |
| `GET /api/companies/:param/labels` | `routes/issues.ts:5646` |
| `GET /api/companies/:param/live-runs` | `routes/agents.ts:3783` |
| `GET /api/companies/:param/me/user-secrets` | `routes/secrets.ts:734` |
| `GET /api/companies/:param/members` | `routes/access.ts:4434` |
| `GET /api/companies/:param/org` | `routes/agents.ts:2226` |
| `GET /api/companies/:param/org.png` | `routes/agents.ts:2246` |
| `GET /api/companies/:param/org.svg` | `routes/agents.ts:2234` |
| `GET /api/companies/:param/pipelines` | `routes/pipelines.ts:821` |
| `GET /api/companies/:param/pipelines-attention` | `routes/pipelines.ts:870` |
| `GET /api/companies/:param/projects` | `routes/projects.ts:129` |
| `GET /api/companies/:param/resource-memberships/me` | `routes/resource-memberships.ts:60` |
| `GET /api/companies/:param/review-cases` | `routes/pipelines.ts:934` |
| `GET /api/companies/:param/routines` | `routes/routines.ts:149` |
| `GET /api/companies/:param/search` | `routes/issues.ts:5229` |
| `GET /api/companies/:param/secret-proposals` | `routes/secrets.ts:288` |
| `GET /api/companies/:param/secret-provider-configs` | `routes/secrets.ts:395` |
| `GET /api/companies/:param/secret-providers` | `routes/secrets.ts:380` |
| `GET /api/companies/:param/secret-providers/health` | `routes/secrets.ts:387` |
| `GET /api/companies/:param/secrets` | `routes/secrets.ts:601` |
| `GET /api/companies/:param/sidebar-badges` | `routes/sidebar-badges.ts:33` |
| `GET /api/companies/:param/sidebar-preferences/me` | `routes/sidebar-preferences.ts:33` |
| `GET /api/companies/:param/skill-policy` | `routes/company-skill-policy.ts:71` |
| `GET /api/companies/:param/skill-test-run-templates` | `routes/company-skills.ts:471` |
| `GET /api/companies/:param/skills` | `routes/company-skills.ts:308` |
| `GET /api/companies/:param/skills/:param` | `routes/company-skills.ts:339` |
| `GET /api/companies/:param/skills/:param/comments` | `routes/company-skills.ts:903` |
| `GET /api/companies/:param/skills/:param/files` | `routes/company-skills.ts:995` |
| `GET /api/companies/:param/skills/:param/fork-precheck` | `routes/company-skills.ts:351` |
| `GET /api/companies/:param/skills/:param/test-inputs` | `routes/company-skills.ts:383` |
| `GET /api/companies/:param/skills/:param/test-runs` | `routes/company-skills.ts:555` |
| `GET /api/companies/:param/skills/:param/test-runs/:param` | `routes/company-skills.ts:565` |
| `GET /api/companies/:param/skills/:param/update-status` | `routes/company-skills.ts:983` |
| `GET /api/companies/:param/skills/:param/versions` | `routes/company-skills.ts:363` |
| `GET /api/companies/:param/skills/:param/versions/:param` | `routes/company-skills.ts:370` |
| `GET /api/companies/:param/skills/categories` | `routes/company-skills.ts:333` |
| `GET /api/companies/:param/smoke-lab/oauth/authorize` | `routes/smoke-lab.ts:58` |
| `GET /api/companies/:param/smoke-lab/oauth/userinfo` | `routes/smoke-lab.ts:99` |
| `GET /api/companies/:param/smoke-lab/runs` | `routes/smoke-lab.ts:191` |
| `GET /api/companies/:param/smoke-lab/runs/:param` | `routes/smoke-lab.ts:219` |
| `GET /api/companies/:param/smoke-lab/services` | `routes/smoke-lab.ts:115` |
| `GET /api/companies/:param/status-cards` | `routes/status-cards.ts:142` |
| `GET /api/companies/:param/summary-slots/:param/:param` | `routes/summary-slots.ts:76` |
| `GET /api/companies/:param/summary-slots/:param/:param/revisions` | `routes/summary-slots.ts:89` |
| `GET /api/companies/:param/teams/catalog/installed` | `routes/teams-catalog.ts:88` |
| `GET /api/companies/:param/timeline` | `routes/companies.ts:305` |
| `GET /api/companies/:param/tools/action-requests` | `routes/tool-access.ts:402` |
| `GET /api/companies/:param/tools/applications` | `routes/tool-access.ts:464` |
| `GET /api/companies/:param/tools/apps/attention` | `routes/tool-access.ts:395` |
| `GET /api/companies/:param/tools/connections` | `routes/tool-access.ts:527` |
| `GET /api/companies/:param/tools/examples` | `routes/tool-access.ts:388` |
| `GET /api/companies/:param/tools/gallery` | `routes/tool-access.ts:235` |
| `GET /api/companies/:param/tools/gateways` | `routes/tool-gateway.ts:284` |
| `GET /api/companies/:param/tools/policies` | `routes/tool-access.ts:1128` |
| `GET /api/companies/:param/tools/profiles` | `routes/tool-access.ts:867` |
| `GET /api/companies/:param/tools/profiles/effective/agents/:param` | `routes/tool-access.ts:902` |
| `GET /api/companies/:param/tools/runs/:param/decisions` | `routes/tool-access.ts:1114` |
| `GET /api/companies/:param/tools/runtime-health` | `routes/tool-access.ts:1107` |
| `GET /api/companies/:param/tools/runtime-slots` | `routes/tool-access.ts:1089` |
| `GET /api/companies/:param/tools/stdio-templates` | `routes/tool-access.ts:1299` |
| `GET /api/companies/:param/tools/trust-rules` | `routes/tool-access.ts:1121` |
| `GET /api/companies/:param/user-directory` | `routes/access.ts:4447` |
| `GET /api/companies/:param/user-secret-definitions` | `routes/secrets.ts:609` |
| `GET /api/companies/:param/user-secret-definitions/:param/coverage` | `routes/secrets.ts:727` |
| `GET /api/companies/:param/users/:param/profile` | `routes/user-profiles.ts:304` |
| `GET /api/companies/:param/workspace-overview` | `routes/execution-workspaces.ts:92` |
| `GET /api/companies/issues` | `routes/companies.ts:290` |
| `GET /api/companies/stats` | `routes/companies.ts:275` |
| `GET /api/decision-training/:param` | `routes/decision-training.ts:163` |
| `GET /api/decisions/:param` | `routes/decisions.ts:174` |
| `GET /api/environment-custom-image-setup-sessions/:param` | `routes/environments.ts:684` |
| `GET /api/environment-leases/:param` | `routes/environments.ts:964` |
| `GET /api/environments/:param` | `routes/environments.ts:925` |
| `GET /api/environments/:param/custom-image-template` | `routes/environments.ts:647` |
| `GET /api/environments/:param/delete-blast-radius` | `routes/environments.ts:601` |
| `GET /api/execution-workspaces/:param` | `routes/execution-workspaces.ts:110` |
| `GET /api/execution-workspaces/:param/close-readiness` | `routes/execution-workspaces.ts:118` |
| `GET /api/execution-workspaces/:param/workspace-operations` | `routes/execution-workspaces.ts:131` |
| `GET /api/feedback-traces/:param` | `routes/issues.ts:10932` |
| `GET /api/feedback-traces/:param/bundle` | `routes/issues.ts:10947` |
| `GET /api/goals/:param` | `routes/goals.ts:21` |
| `GET /api/heartbeat-runs/:param` | `routes/agents.ts:3868` |
| `GET /api/heartbeat-runs/:param/events` | `routes/agents.ts:3946` |
| `GET /api/heartbeat-runs/:param/issues` | `routes/activity.ts:359` |
| `GET /api/heartbeat-runs/:param/log` | `routes/agents.ts:3964` |
| `GET /api/heartbeat-runs/:param/workspace-operations` | `routes/agents.ts:3980` |
| `GET /api/instance/scheduler-heartbeats` | `routes/agents.ts:2163` |
| `GET /api/instance/settings` | `routes/instance-settings.ts:33` |
| `GET /api/instance/settings/experimental` | `routes/instance-settings.ts:132` |
| `GET /api/instance/settings/general` | `routes/instance-settings.ts:76` |
| `GET /api/invites/:param` | `routes/access.ts:3390` |
| `GET /api/invites/:param/logo` | `routes/access.ts:3422` |
| `GET /api/invites/:param/onboarding` | `routes/access.ts:3476` |
| `GET /api/invites/:param/onboarding.txt` | `routes/access.ts:3495` |
| `GET /api/invites/:param/skills/:param` | `routes/access.ts:3540` |
| `GET /api/invites/:param/skills/index` | `routes/access.ts:3518` |
| `GET /api/invites/:param/test-resolution` | `routes/access.ts:3559` |
| `GET /api/issues` | `routes/issues.ts:5189` |
| `GET /api/issues/:param` | `routes/issues.ts:5978` |
| `GET /api/issues/:param/accepted-plan-decompositions` | `routes/issues.ts:8043` |
| `GET /api/issues/:param/active-run` | `routes/agents.ts:4062` |
| `GET /api/issues/:param/activity` | `routes/activity.ts:341` |
| `GET /api/issues/:param/approvals` | `routes/issues.ts:7539` |
| `GET /api/issues/:param/attachments` | `routes/issues.ts:11757` |
| `GET /api/issues/:param/cases` | `routes/cases.ts:1424` |
| `GET /api/issues/:param/comments` | `routes/issues.ts:10106` |
| `GET /api/issues/:param/comments/:param` | `routes/issues.ts:10727` |
| `GET /api/issues/:param/cost-summary` | `routes/costs.ts:182` |
| `GET /api/issues/:param/diagnostics/blockers` | `routes/issues.ts:5852` |
| `GET /api/issues/:param/diagnostics/subtree` | `routes/issues.ts:5928` |
| `GET /api/issues/:param/diagnostics/wakes` | `routes/issues.ts:5883` |
| `GET /api/issues/:param/documents` | `routes/issues.ts:6430` |
| `GET /api/issues/:param/documents/:param` | `routes/issues.ts:6441` |
| `GET /api/issues/:param/documents/:param/annotations` | `routes/issues.ts:6467` |
| `GET /api/issues/:param/documents/:param/annotations/:param` | `routes/issues.ts:6537` |
| `GET /api/issues/:param/documents/:param/revisions` | `routes/issues.ts:6876` |
| `GET /api/issues/:param/external-object-summary` | `routes/issues.ts:6369` |
| `GET /api/issues/:param/external-objects` | `routes/issues.ts:6360` |
| `GET /api/issues/:param/feedback-traces` | `routes/issues.ts:10902` |
| `GET /api/issues/:param/feedback-votes` | `routes/issues.ts:10889` |
| `GET /api/issues/:param/file-resources/content` | `routes/file-resources.ts:556` |
| `GET /api/issues/:param/file-resources/list` | `routes/file-resources.ts:334` |
| `GET /api/issues/:param/file-resources/resolve` | `routes/file-resources.ts:443` |
| `GET /api/issues/:param/heartbeat-context` | `routes/issues.ts:5698` |
| `GET /api/issues/:param/interactions` | `routes/issues.ts:10137` |
| `GET /api/issues/:param/live-runs` | `routes/agents.ts:4007` |
| `GET /api/issues/:param/recovery-actions` | `routes/issues.ts:6150` |
| `GET /api/issues/:param/runs` | `routes/activity.ts:350` |
| `GET /api/issues/:param/tree-control/state` | `routes/issue-tree-control.ts:301` |
| `GET /api/issues/:param/tree-holds` | `routes/issue-tree-control.ts:310` |
| `GET /api/issues/:param/tree-holds/:param` | `routes/issue-tree-control.ts:328` |
| `GET /api/issues/:param/work-products` | `routes/issues.ts:6351` |
| `GET /api/llms/agent-configuration.txt` | `routes/llms.ts:33` |
| `GET /api/llms/agent-configuration/:param.txt` | `routes/llms.ts:84` |
| `GET /api/llms/agent-icons.txt` | `routes/llms.ts:69` |
| `GET /api/mcp/gateways/:param` | `routes/tool-gateway.ts:163` |
| `GET /api/openapi.json` | `routes/openapi.ts:7361` |
| `GET /api/pipelines/:param` | `routes/pipelines.ts:970` |
| `GET /api/pipelines/:param/cases` | `routes/pipelines.ts:1496` |
| `GET /api/pipelines/:param/documents/:param` | `routes/pipelines.ts:1279` |
| `GET /api/pipelines/:param/documents/:param/revisions` | `routes/pipelines.ts:1389` |
| `GET /api/pipelines/:param/health` | `routes/pipelines.ts:1018` |
| `GET /api/pipelines/:param/intake-form` | `routes/pipelines.ts:1153` |
| `GET /api/plugins` | `routes/plugins.ts:838` |
| `GET /api/plugins/:param` | `routes/plugins.ts:1939` |
| `GET /api/plugins/:param/bridge/stream/:param` | `routes/plugins.ts:1751` |
| `GET /api/plugins/:param/companies/:param/local-folders` | `routes/plugins.ts:2807` |
| `GET /api/plugins/:param/companies/:param/local-folders/:param/status` | `routes/plugins.ts:2838` |
| `GET /api/plugins/:param/config` | `routes/plugins.ts:2254` |
| `GET /api/plugins/:param/dashboard` | `routes/plugins.ts:2965` |
| `GET /api/plugins/:param/health` | `routes/plugins.ts:2084` |
| `GET /api/plugins/:param/jobs` | `routes/plugins.ts:2519` |
| `GET /api/plugins/:param/jobs/:param/runs` | `routes/plugins.ts:2565` |
| `GET /api/plugins/:param/logs` | `routes/plugins.ts:2152` |
| `GET /api/plugins/examples` | `routes/plugins.ts:862` |
| `GET /api/plugins/tools` | `routes/plugins.ts:950` |
| `GET /api/plugins/ui-contributions` | `routes/plugins.ts:907` |
| `GET /api/projects/:param` | `routes/projects.ts:137` |
| `GET /api/projects/:param/external-object-summary` | `routes/projects.ts:145` |
| `GET /api/projects/:param/workspaces` | `routes/projects.ts:277` |
| `GET /api/routines/:param` | `routes/routines.ts:194` |
| `GET /api/routines/:param/description/annotations` | `routes/routines.ts:210` |
| `GET /api/routines/:param/description/annotations/:param` | `routes/routines.ts:224` |
| `GET /api/routines/:param/revisions` | `routes/routines.ts:200` |
| `GET /api/routines/:param/runs` | `routes/routines.ts:456` |
| `GET /api/secret-provider-configs/:param` | `routes/secrets.ts:472` |
| `GET /api/secrets/:param/access-events` | `routes/secrets.ts:1098` |
| `GET /api/secrets/:param/usage` | `routes/secrets.ts:1083` |
| `GET /api/sidebar-preferences/me` | `routes/sidebar-preferences.ts:21` |
| `GET /api/skills/:param` | `routes/access.ts:3273` |
| `GET /api/skills/available` | `routes/access.ts:3247` |
| `GET /api/skills/catalog` | `routes/company-skills.ts:285` |
| `GET /api/skills/catalog/:param` | `routes/company-skills.ts:302` |
| `GET /api/skills/index` | `routes/access.ts:3252` |
| `GET /api/status-cards/:param` | `routes/status-cards.ts:170` |
| `GET /api/status-cards/:param/dry-run` | `routes/status-cards.ts:251` |
| `GET /api/status-cards/:param/summary-revisions` | `routes/status-cards.ts:216` |
| `GET /api/status-cards/:param/updates` | `routes/status-cards.ts:209` |
| `GET /api/teams/catalog` | `routes/teams-catalog.ts:65` |
| `GET /api/teams/catalog/:param` | `routes/teams-catalog.ts:82` |
| `GET /api/teams/catalog/:param/files` | `routes/teams-catalog.ts:75` |
| `GET /api/tool-connections/:param` | `routes/tool-access.ts:559` |
| `GET /api/tool-connections/:param/activity` | `routes/tool-access.ts:857` |
| `GET /api/tool-connections/:param/catalog` | `routes/tool-access.ts:849` |
| `GET /api/tool-connections/:param/grants` | `routes/tool-access.ts:567` |
| `GET /api/tool-connections/:param/installs` | `routes/tool-access.ts:628` |
| `GET /api/tool-connections/:param/usage` | `routes/tool-access.ts:618` |
| `GET /api/tool-gateway/audit` | `routes/tool-gateway.ts:636` |
| `GET /api/tool-gateway/gateways/:param/mcp` | `routes/tool-gateway.ts:375` |
| `GET /api/tool-gateway/runtime-slots` | `routes/tool-gateway.ts:567` |
| `GET /api/tool-gateway/tools` | `routes/tool-gateway.ts:469` |
| `GET /api/tool-profiles/:param/new-tools` | `routes/tool-access.ts:874` |
| `GET /api/workspace-operations/:param/log` | `routes/agents.ts:3991` |
| `PATCH /api/adapters/:param` | `routes/adapters.ts:398` |
| `PATCH /api/adapters/:param/override` | `routes/adapters.ts:433` |
| `PATCH /api/agents/:param` | `routes/agents.ts:3111` |
| `PATCH /api/agents/:param/budgets` | `routes/costs.ts:364` |
| `PATCH /api/agents/:param/instructions-bundle` | `routes/agents.ts:2971` |
| `PATCH /api/agents/:param/instructions-path` | `routes/agents.ts:2883` |
| `PATCH /api/agents/:param/permissions` | `routes/agents.ts:2824` |
| `PATCH /api/auth/profile` | `routes/auth.ts:63` |
| `PATCH /api/cases/:param` | `routes/cases.ts:1456` |
| `PATCH /api/companies/:param` | `routes/companies.ts:633` |
| `PATCH /api/companies/:param/branding` | `routes/companies.ts:710` |
| `PATCH /api/companies/:param/budgets` | `routes/costs.ts:330` |
| `PATCH /api/companies/:param/decision-queues/:param` | `routes/decision-queues.ts:95` |
| `PATCH /api/companies/:param/folders/:param` | `routes/folders.ts:79` |
| `PATCH /api/companies/:param/smoke-lab/runs/:param` | `routes/smoke-lab.ts:226` |
| `PATCH /api/decision-training/:param` | `routes/decision-training.ts:187` |
| `PATCH /api/environments/:param` | `routes/environments.ts:974` |
| `PATCH /api/execution-workspaces/:param` | `routes/execution-workspaces.ts:582` |
| `PATCH /api/goals/:param` | `routes/goals.ts:51` |
| `PATCH /api/issues/:param` | `routes/issues.ts:8420` |
| `PATCH /api/pipelines/:param/stages/:param` | `routes/pipelines.ts:1211` |
| `PATCH /api/pipelines/:param/stages/:param/automation-env` | `routes/pipelines.ts:1224` |
| `PATCH /api/projects/:param` | `routes/projects.ts:221` |
| `PATCH /api/routine-triggers/:param` | `routes/routines.ts:500` |
| `PATCH /api/routines/:param` | `routes/routines.ts:363` |
| `PATCH /api/secret-provider-configs/:param` | `routes/secrets.ts:479` |
| `PATCH /api/secrets/:param` | `routes/secrets.ts:1039` |
| `PATCH /api/status-cards/:param` | `routes/status-cards.ts:177` |
| `PATCH /api/tool-applications/:param` | `routes/tool-access.ts:491` |
| `PATCH /api/tool-connections/:param` | `routes/tool-access.ts:740` |
| `PATCH /api/tool-gateway/gateways/:param` | `routes/tool-gateway.ts:313` |
| `PATCH /api/tool-profiles/:param` | `routes/tool-access.ts:909` |
| `PATCH /api/work-products/:param` | `routes/issues.ts:7282` |
| `POST /api/adapters/:param/reinstall` | `routes/adapters.ts:589` |
| `POST /api/adapters/:param/reload` | `routes/adapters.ts:537` |
| `POST /api/adapters/install` | `routes/adapters.ts:251` |
| `POST /api/agents/:param/approve` | `routes/agents.ts:3354` |
| `POST /api/agents/:param/claude-login` | `routes/agents.ts:3737` |
| `POST /api/agents/:param/clear-error` | `routes/agents.ts:3322` |
| `POST /api/agents/:param/config-revisions/:param/rollback` | `routes/agents.ts:2433` |
| `POST /api/agents/:param/heartbeat/invoke` | `routes/agents.ts:3657` |
| `POST /api/agents/:param/keys` | `routes/agents.ts:3513` |
| `POST /api/agents/:param/pause` | `routes/agents.ts:3265` |
| `POST /api/agents/:param/resume` | `routes/agents.ts:3291` |
| `POST /api/agents/:param/runtime-state/reset-session` | `routes/agents.ts:2493` |
| `POST /api/agents/:param/terminate` | `routes/agents.ts:3408` |
| `POST /api/agents/:param/wakeup` | `routes/agents.ts:3650` |
| `POST /api/agents/me/secret-proposals` | `routes/secrets.ts:243` |
| `POST /api/approvals/:param/approve` | `routes/approvals.ts:288` |
| `POST /api/approvals/:param/comments` | `routes/approvals.ts:509` |
| `POST /api/approvals/:param/reject` | `routes/approvals.ts:404` |
| `POST /api/approvals/:param/resubmit` | `routes/approvals.ts:466` |
| `POST /api/board/chat/stream` | `routes/board-chat.ts:97` |
| `POST /api/bootstrap/claim` | `routes/access.ts:2703` |
| `POST /api/cases/:param/acknowledge-drift` | `routes/pipelines.ts:1984` |
| `POST /api/cases/:param/attachments` | `routes/cases.ts:1275` |
| `POST /api/cases/:param/automation/current-stage/rerun` | `routes/pipelines.ts:2266` |
| `POST /api/cases/:param/automation/retry` | `routes/pipelines.ts:2233` |
| `POST /api/cases/:param/automations/:param/retry` | `routes/pipelines.ts:2257` |
| `POST /api/cases/:param/breakdown` | `routes/pipelines.ts:1487` |
| `POST /api/cases/:param/documents/:param/lock` | `routes/cases.ts:1075` |
| `POST /api/cases/:param/documents/:param/revisions/:param/restore` | `routes/cases.ts:1120` |
| `POST /api/cases/:param/documents/:param/unlock` | `routes/cases.ts:1098` |
| `POST /api/cases/:param/issue-links` | `routes/pipelines.ts:2092` |
| `POST /api/cases/:param/links` | `routes/cases.ts:1230` |
| `POST /api/cases/:param/open-conversation` | `routes/pipelines.ts:2010` |
| `POST /api/cases/:param/resolve-suggestion` | `routes/pipelines.ts:1968` |
| `POST /api/cases/:param/review` | `routes/pipelines.ts:1996` |
| `POST /api/cases/:param/suggest-transition` | `routes/pipelines.ts:1961` |
| `POST /api/cli-auth/revoke-current` | `routes/access.ts:2948` |
| `POST /api/companies` | `routes/companies.ts:584` |
| `POST /api/companies/:param/activity` | `routes/activity.ts:329` |
| `POST /api/companies/:param/agent-hires` | `routes/agents.ts:2519` |
| `POST /api/companies/:param/approvals` | `routes/approvals.ts:224` |
| `POST /api/companies/:param/archive` | `routes/companies.ts:735` |
| `POST /api/companies/:param/assets/images` | `routes/assets.ts:110` |
| `POST /api/companies/:param/built-in-agents/:param/reconcile` | `routes/built-in-agents.ts:164` |
| `POST /api/companies/:param/built-in-agents/:param/reset` | `routes/built-in-agents.ts:215` |
| `POST /api/companies/:param/cases` | `routes/cases.ts:643` |
| `POST /api/companies/:param/cost-events` | `routes/costs.ts:114` |
| `POST /api/companies/:param/decision-bundles` | `routes/decisions.ts:143` |
| `POST /api/companies/:param/decision-queues` | `routes/decision-queues.ts:83` |
| `POST /api/companies/:param/decision-retention/:param/:param/archive` | `routes/decision-queues.ts:206` |
| `POST /api/companies/:param/decision-retention/:param/:param/revive` | `routes/decision-queues.ts:217` |
| `POST /api/companies/:param/decisions` | `routes/decisions.ts:138` |
| `POST /api/companies/:param/environments` | `routes/environments.ts:863` |
| `POST /api/companies/:param/export` | `routes/companies.ts:402` |
| `POST /api/companies/:param/exports` | `routes/companies.ts:522` |
| `POST /api/companies/:param/exports/preview` | `routes/companies.ts:514` |
| `POST /api/companies/:param/finance-events` | `routes/costs.ts:143` |
| `POST /api/companies/:param/folders` | `routes/folders.ts:32` |
| `POST /api/companies/:param/folders/:param/move` | `routes/folders.ts:124` |
| `POST /api/companies/:param/folders/items/move` | `routes/folders.ts:104` |
| `POST /api/companies/:param/goals` | `routes/goals.ts:28` |
| `POST /api/companies/:param/imports/apply` | `routes/companies.ts:547` |
| `POST /api/companies/:param/imports/preview` | `routes/companies.ts:530` |
| `POST /api/companies/:param/issues` | `routes/issues.ts:7608` |
| `POST /api/companies/:param/issues/:param/attachments` | `routes/issues.ts:11766` |
| `POST /api/companies/:param/issues/external-object-summaries` | `routes/issues.ts:6378` |
| `POST /api/companies/:param/labels` | `routes/issues.ts:5653` |
| `POST /api/companies/:param/logo` | `routes/assets.ts:214` |
| `POST /api/companies/:param/pipelines` | `routes/pipelines.ts:891` |
| `POST /api/companies/:param/projects` | `routes/projects.ts:153` |
| `POST /api/companies/:param/review-cases/bulk` | `routes/pipelines.ts:942` |
| `POST /api/companies/:param/routines` | `routes/routines.ts:157` |
| `POST /api/companies/:param/secret-proposals/:param/approve` | `routes/secrets.ts:303` |
| `POST /api/companies/:param/secret-proposals/:param/reject` | `routes/secrets.ts:319` |
| `POST /api/companies/:param/secret-provider-configs` | `routes/secrets.ts:437` |
| `POST /api/companies/:param/secrets` | `routes/secrets.ts:894` |
| `POST /api/companies/:param/skills/:param/star` | `routes/company-skills.ts:783` |
| `POST /api/companies/:param/skills/:param/test-runs/:param/cancel` | `routes/company-skills.ts:673` |
| `POST /api/companies/:param/smoke-lab/install-fixtures` | `routes/smoke-lab.ts:164` |
| `POST /api/companies/:param/smoke-lab/oauth/authorize` | `routes/smoke-lab.ts:72` |
| `POST /api/companies/:param/smoke-lab/oauth/revoke` | `routes/smoke-lab.ts:107` |
| `POST /api/companies/:param/smoke-lab/oauth/token` | `routes/smoke-lab.ts:87` |
| `POST /api/companies/:param/smoke-lab/reset` | `routes/smoke-lab.ts:268` |
| `POST /api/companies/:param/smoke-lab/runs` | `routes/smoke-lab.ts:198` |
| `POST /api/companies/:param/smoke-lab/runs/:param/steps` | `routes/smoke-lab.ts:247` |
| `POST /api/companies/:param/smoke-lab/services/start` | `routes/smoke-lab.ts:122` |
| `POST /api/companies/:param/smoke-lab/services/stop` | `routes/smoke-lab.ts:143` |
| `POST /api/companies/:param/status-cards` | `routes/status-cards.ts:150` |
| `POST /api/companies/:param/tools/applications` | `routes/tool-access.ts:471` |
| `POST /api/companies/:param/tools/gateways` | `routes/tool-gateway.ts:293` |
| `POST /api/companies/:param/tools/policies` | `routes/tool-access.ts:1156` |
| `POST /api/decisions/:param/cancel` | `routes/decisions.ts:193` |
| `POST /api/decisions/:param/decide` | `routes/decisions.ts:181` |
| `POST /api/decisions/:param/dismiss` | `routes/decisions.ts:187` |
| `POST /api/environments/:param/custom-image-template/rollback` | `routes/environments.ts:825` |
| `POST /api/environments/:param/probe` | `routes/environments.ts:1152` |
| `POST /api/execution-workspaces/:param/reconcile-branch` | `routes/execution-workspaces.ts:493` |
| `POST /api/execution-workspaces/:param/runtime-commands/:param` | `routes/execution-workspaces.ts:491` |
| `POST /api/execution-workspaces/:param/runtime-services/:param` | `routes/execution-workspaces.ts:490` |
| `POST /api/heartbeat-runs/:param/cancel` | `routes/agents.ts:3884` |
| `POST /api/heartbeat-runs/:param/watchdog-decisions` | `routes/agents.ts:3914` |
| `POST /api/instance/database-backups` | `routes/instance-database-backups.ts:25` |
| `POST /api/invites/:param/revoke` | `routes/access.ts:4090` |
| `POST /api/issues/:param/accepted-plan-decompositions` | `routes/issues.ts:8051` |
| `POST /api/issues/:param/admin/force-release` | `routes/issues.ts:10063` |
| `POST /api/issues/:param/approvals` | `routes/issues.ts:7549` |
| `POST /api/issues/:param/checkout` | `routes/issues.ts:9945` |
| `POST /api/issues/:param/children` | `routes/issues.ts:7869` |
| `POST /api/issues/:param/comments` | `routes/issues.ts:10961` |
| `POST /api/issues/:param/documents/:param/lock` | `routes/issues.ts:6792` |
| `POST /api/issues/:param/documents/:param/unlock` | `routes/issues.ts:6837` |
| `POST /api/issues/:param/external-objects/refresh` | `routes/issues.ts:6401` |
| `POST /api/issues/:param/feedback-votes` | `routes/issues.ts:11659` |
| `POST /api/issues/:param/inbox-archive` | `routes/issues.ts:7482` |
| `POST /api/issues/:param/interactions` | `routes/issues.ts:10172` |
| `POST /api/issues/:param/low-trust/promotions` | `routes/issues.ts:7140` |
| `POST /api/issues/:param/monitor/check-now` | `routes/issues.ts:8252` |
| `POST /api/issues/:param/read` | `routes/issues.ts:7377` |
| `POST /api/issues/:param/recovery-actions/resolve` | `routes/issues.ts:6166` |
| `POST /api/issues/:param/release` | `routes/issues.ts:10029` |
| `POST /api/issues/:param/scheduled-retry/retry-now` | `routes/issues.ts:8269` |
| `POST /api/issues/:param/tree-control/preview` | `routes/issue-tree-control.ts:46` |
| `POST /api/issues/:param/tree-holds` | `routes/issue-tree-control.ts:73` |
| `POST /api/issues/:param/work-products` | `routes/issues.ts:7093` |
| `POST /api/mcp/gateways/:param` | `routes/tool-gateway.ts:170` |
| `POST /api/pipelines/:param/cases` | `routes/pipelines.ts:1470` |
| `POST /api/pipelines/:param/cases/batch` | `routes/pipelines.ts:1479` |
| `POST /api/pipelines/:param/documents/:param/revisions/:param/restore` | `routes/pipelines.ts:1397` |
| `POST /api/pipelines/:param/stages` | `routes/pipelines.ts:1189` |
| `POST /api/plugins/:param/actions/:param` | `routes/plugins.ts:1659` |
| `POST /api/plugins/:param/bridge/action` | `routes/plugins.ts:1475` |
| `POST /api/plugins/:param/bridge/data` | `routes/plugins.ts:1382` |
| `POST /api/plugins/:param/companies/:param/local-folders/:param/validate` | `routes/plugins.ts:2861` |
| `POST /api/plugins/:param/config` | `routes/plugins.ts:2286` |
| `POST /api/plugins/:param/config/test` | `routes/plugins.ts:2418` |
| `POST /api/plugins/:param/data/:param` | `routes/plugins.ts:1569` |
| `POST /api/plugins/:param/disable` | `routes/plugins.ts:2043` |
| `POST /api/plugins/:param/enable` | `routes/plugins.ts:2005` |
| `POST /api/plugins/:param/jobs/:param/trigger` | `routes/plugins.ts:2613` |
| `POST /api/plugins/:param/upgrade` | `routes/plugins.ts:2205` |
| `POST /api/plugins/:param/webhooks/:param` | `routes/plugins.ts:2670` |
| `POST /api/plugins/install` | `routes/plugins.ts:1126` |
| `POST /api/plugins/tools/execute` | `routes/plugins.ts:997` |
| `POST /api/projects/:param/workspaces` | `routes/projects.ts:285` |
| `POST /api/routine-triggers/public/:param/fire` | `routes/routines.ts:656` |
| `POST /api/routines/:param/revisions/:param/restore` | `routes/routines.ts:421` |
| `POST /api/routines/:param/run` | `routes/routines.ts:629` |
| `POST /api/routines/:param/triggers` | `routes/routines.ts:464` |
| `POST /api/secret-provider-configs/:param/default` | `routes/secrets.ts:543` |
| `POST /api/secret-provider-configs/:param/health` | `routes/secrets.ts:572` |
| `POST /api/secrets/:param/rotate` | `routes/secrets.ts:999` |
| `POST /api/status-cards/:param/recompile` | `routes/status-cards.ts:223` |
| `POST /api/status-cards/:param/refresh` | `routes/status-cards.ts:236` |
| `POST /api/tool-gateway/action-requests/:param/approve` | `routes/tool-gateway.ts:517` |
| `POST /api/tool-gateway/action-requests/:param/decline` | `routes/tool-gateway.ts:542` |
| `POST /api/tool-gateway/gateway-tokens/:param/revoke` | `routes/tool-gateway.ts:360` |
| `POST /api/tool-gateway/gateways/:param/mcp` | `routes/tool-gateway.ts:383` |
| `POST /api/tool-gateway/gateways/:param/tokens` | `routes/tool-gateway.ts:334` |
| `POST /api/tool-gateway/sessions` | `routes/tool-gateway.ts:387` |
| `POST /api/tool-gateway/sessions/:param/revoke` | `routes/tool-gateway.ts:432` |
| `POST /api/tool-gateway/tools/call` | `routes/tool-gateway.ts:483` |
| `PUT /api/agents/:param/instructions-bundle/file` | `routes/agents.ts:3032` |
| `PUT /api/cases/:param/blockers` | `routes/pipelines.ts:2003` |
| `PUT /api/cases/:param/documents/:param` | `routes/cases.ts:934` |
| `PUT /api/issues/:param/documents/:param` | `routes/issues.ts:6659` |
| `PUT /api/pipelines/:param/documents/:param` | `routes/pipelines.ts:1288` |
| `PUT /api/pipelines/:param/transitions` | `routes/pipelines.ts:1256` |
| `PUT /api/plugins/:param/companies/:param/local-folders/:param` | `routes/plugins.ts:2894` |
| `PUT /api/sidebar-preferences/me` | `routes/sidebar-preferences.ts:27` |
| `PUT /api/status-cards/:param/query` | `routes/status-cards.ts:271` |
| `PUT /api/status-cards/:param/summary` | `routes/status-cards.ts:290` |

## 2. Partial（路径存在、方法不同）

| Method+Path | Source | Note |
|---|---|---|
| `DELETE /api/issues/:param/comments/:param` | `routes/issues.ts:10741` | path exists with method(s) ['GET'] |
| `GET /api/cases/:param/automation/retry-plan` | `routes/pipelines.ts:2217` | path exists with method(s) ['POST'] |
| `PATCH /api/companies/:param/tools/policies/:param` | `routes/tool-access.ts:1206` | path exists with method(s) ['DELETE'] |
| `PATCH /api/pipelines/:param` | `routes/pipelines.ts:1171` | path exists with method(s) ['GET'] |
| `POST /api/companies/:param/agents` | `routes/agents.ts:2702` | path exists with method(s) ['GET'] |
| `POST /api/companies/:param/tools/connections` | `routes/tool-access.ts:534` | path exists with method(s) ['GET'] |
| `POST /api/companies/:param/tools/profiles` | `routes/tool-access.ts:882` | path exists with method(s) ['GET'] |
| `POST /api/companies/:param/tools/stdio-templates` | `routes/tool-access.ts:1305` | path exists with method(s) ['GET'] |

## 3. By-design candidates（平台/UI 专属，需产品确认）

| Method+Path | Source | Note |
|---|---|---|
| `GET /api/_plugins/:param/ui/:param` | `routes/plugin-ui-static.ts:230` | Paperclip-only domain '/_plugins' |
| `GET /api/cloud/stacks` | `routes/cloud.ts:30` | Paperclip-only domain '/cloud' |
| `GET /api/companies/:param/audit/agent-actions.csv` | `routes/activity.ts:274` | export artifact suffix |
| `POST /api/board-claim/:param/claim` | `routes/access.ts:2666` | Paperclip-only domain '/board-claim' |
| `POST /api/tool-gateway/runtime-slots/:param/restart` | `routes/tool-gateway.ts:609` | Paperclip-only domain '/tool-gateway' |
| `POST /api/tool-gateway/runtime-slots/:param/stop` | `routes/tool-gateway.ts:582` | Paperclip-only domain '/tool-gateway' |

## 4. Missing

| Method+Path | Source | Note |
|---|---|---|
| `ALL /api/auth/{:param}` | `app.ts:321` |  |
| `DELETE /api/issues/:param/watchdog` | `routes/issues.ts:6113` |  |
| `DELETE /api/tool-profile-entries/:param` | `routes/tool-access.ts:1027` |  |
| `GET /api/companies/:param/audit/agent-actions` | `routes/activity.ts:238` |  |
| `GET /api/companies/:param/export/fidelity` | `routes/companies.ts:410` |  |
| `GET /api/companies/:param/recovery-observability` | `routes/dashboard.ts:34` |  |
| `GET /api/companies/:param/search/extract` | `routes/issues.ts:5195` |  |
| `GET /api/companies/:param/users/:param/inbox-agent-policy` | `routes/inbox-agent-policy.ts:84` |  |
| `GET /api/companies/:param/users/me/inbox-agent-policy` | `routes/inbox-agent-policy.ts:68` |  |
| `GET /api/companies/import/jobs/:param` | `routes/companies.ts:425` |  |
| `GET /api/environments/:param/leases` | `routes/environments.ts:951` |  |
| `GET /api/environments/:param/secret-refs` | `routes/environments.ts:935` |  |
| `GET /api/health` | `routes/health.ts:129` |  |
| `GET /api/issues/:param/watchdog` | `routes/issues.ts:6064` |  |
| `GET /api/skills/catalog/:param/files` | `routes/company-skills.ts:295` |  |
| `GET /api/tool-connections/:param/test-agents` | `routes/tool-access.ts:659` |  |
| `GET /api/tool-connections/:param/test-calls/:param` | `routes/tool-access.ts:720` |  |
| `GET /api/tools/oauth/callback` | `routes/tool-access.ts:320` |  |
| `PATCH /api/tool-profile-entries/:param` | `routes/tool-access.ts:1011` |  |
| `POST /api/agents/me/connections/:param/start-authorization` | `routes/tool-access.ts:178` |  |
| `POST /api/agents/me/connections/:param/token` | `routes/tool-access.ts:196` |  |
| `POST /api/agents/me/secrets/:param/value` | `routes/secrets.ts:355` |  |
| `POST /api/cases/:param/claim` | `routes/pipelines.ts:1927` |  |
| `POST /api/cases/:param/release` | `routes/pipelines.ts:1936` |  |
| `POST /api/cases/:param/transition` | `routes/pipelines.ts:1944` |  |
| `POST /api/companies/:param/tools/apps/:param/finish` | `routes/tool-access.ts:364` |  |
| `POST /api/companies/:param/tools/apps/connect` | `routes/tool-access.ts:255` |  |
| `POST /api/companies/:param/tools/examples/:param/install` | `routes/tool-access.ts:413` |  |
| `POST /api/companies/:param/tools/examples/:param/smoke` | `routes/tool-access.ts:435` |  |
| `POST /api/companies/:param/tools/mcp/import-json` | `routes/tool-access.ts:1347` |  |
| `POST /api/companies/:param/tools/policies/:param/duplicate` | `routes/tool-access.ts:1176` |  |
| `POST /api/companies/:param/tools/policies/reorder` | `routes/tool-access.ts:1137` |  |
| `POST /api/companies/:param/tools/policy/test` | `routes/tool-access.ts:1364` |  |
| `POST /api/companies/:param/tools/runtime-slots/:param/restart` | `routes/tool-access.ts:1101` |  |
| `POST /api/companies/:param/tools/runtime-slots/:param/stop` | `routes/tool-access.ts:1095` |  |
| `POST /api/companies/:param/tools/trust-rules/:param/revoke` | `routes/tool-access.ts:1278` |  |
| `POST /api/companies/import/preview` | `routes/companies.ts:417` |  |
| `POST /api/health/dev-server/restart` | `routes/health.ts:95` |  |
| `POST /api/projects/:param/workspaces/:param/runtime-commands/:param` | `routes/projects.ts:635` |  |
| `POST /api/projects/:param/workspaces/:param/runtime-services/:param` | `routes/projects.ts:634` |  |
| `POST /api/tool-connections/:param/catalog/refresh` | `routes/tool-access.ts:843` |  |
| `POST /api/tool-connections/:param/grants/installations` | `routes/tool-access.ts:575` |  |
| `POST /api/tool-connections/:param/health-check` | `routes/tool-access.ts:812` |  |
| `POST /api/tool-connections/:param/test-calls` | `routes/tool-access.ts:696` |  |
| `POST /api/tool-profiles/:param/duplicate` | `routes/tool-access.ts:929` |  |
| `POST /api/tool-profiles/:param/entries` | `routes/tool-access.ts:995` |  |
| `POST /api/tool-profiles/:param/new-tools/review` | `routes/tool-access.ts:975` |  |
| `POST /api/tools/oauth/:param/start` | `routes/tool-access.ts:310` |  |
| `PUT /api/issues/:param/watchdog` | `routes/issues.ts:6072` |  |

## 5. Parrot-only（Parrot 扩展端点）

| Method+Path | Source |
|---|---|
| `DELETE /api/comments/:param` | `issue_comments.rs` |
| `DELETE /api/companies/:param/decision-queues/:param/items/:param/:param` | `decisions.rs` |
| `DELETE /api/companies/:param/skills/:param/files` | `skills.rs` |
| `DELETE /api/mcp/gateways/:param` | `tools.rs` |
| `DELETE /api/routines/:param` | `routines.rs` |
| `DELETE /api/tool-gateway/gateways/:param/mcp` | `tools.rs` |
| `DELETE /api/tool-gateway/mcp` | `tools.rs` |
| `GET /api/agents/:param/config-revisions/:param/diff` | `config_revisions.rs` |
| `GET /api/cases/:param/detail` | `cases.rs` |
| `GET /api/cloud-upstreams` | `cloud_upstreams.rs` |
| `GET /api/cloud-upstreams/:param/push-runs/:param` | `cloud_upstreams.rs` |
| `GET /api/comments/:param` | `issue_comments.rs` |
| `GET /api/companies/:param/adapters` | `adapters.rs` |
| `GET /api/companies/:param/adapters/:param` | `adapters.rs` |
| `GET /api/companies/:param/budgets/invocation-block/:param` | `costs.rs` |
| `GET /api/companies/:param/budgets/policies` | `costs.rs` |
| `GET /api/companies/:param/events/:param` | `sse.rs` |
| `GET /api/companies/:param/events/:param/stats` | `sse.rs` |
| `GET /api/companies/:param/events/ws` | `websocket.rs` |
| `GET /api/companies/:param/issues/:param/watchdog` | `watchdogs.rs` |
| `GET /api/companies/:param/issues/search` | `issues.rs` |
| `GET /api/companies/:param/me/user-secrets/:param` | `user_secret_definitions.rs` |
| `GET /api/companies/:param/me/user-secrets/:param/bindings` | `user_secret_definitions.rs` |
| `GET /api/companies/:param/org-chart.svg` | `org_chart.rs` |
| `GET /api/companies/:param/teams-catalog` | `companies.rs` |
| `GET /api/companies/:param/user-secret-definitions/:param` | `user_secret_definitions.rs` |
| `GET /api/goals/:param/children` | `goals.rs` |
| `GET /api/goals/:param/hierarchy` | `goals.rs` |
| `GET /api/goals/:param/progress` | `goals.rs` |
| `GET /api/heartbeat-runs/:param/watchdog-decisions` | `heartbeat_runs.rs` |
| `GET /api/issues/:param/cost-tree-summary` | `costs.rs` |
| `GET /api/issues/:param/interactions/:param` | `interactions.rs` |
| `GET /api/issues/:param/tree-holds/:param/members` | `issue_tree_control.rs` |
| `GET /api/issues/low-trust` | `low_trust.rs` |
| `GET /api/pipelines/:param/health-warnings` | `pipelines.rs` |
| `GET /api/pipelines/:param/stages` | `pipelines.rs` |
| `GET /api/pipelines/:param/transitions` | `pipelines.rs` |
| `GET /api/plugins/:param/ui/:param` | `plugins.rs` |
| `GET /api/routines/:param/triggers` | `routines.rs` |
| `GET /api/runs/:param` | `routines.rs` |
| `GET /api/secrets/:param` | `secrets.rs` |
| `GET /api/secrets/:param/bindings` | `user_secret_definitions.rs` |
| `GET /api/skills/catalog/files` | `skills.rs` |
| `GET /api/stats` | `llms.rs` |
| `GET /api/tool-gateway/mcp` | `tools.rs` |
| `PATCH /api/cases/:param/advance` | `cases.rs` |
| `PATCH /api/cases/:param/documents/:param/annotations/:param` | `cases.rs` |
| `PATCH /api/companies/:param/decision-retention/:param/:param` | `decisions.rs` |
| `PATCH /api/companies/:param/me/user-secrets/:param` | `user_secret_definitions.rs` |
| `PATCH /api/companies/:param/members/:param` | `access_control.rs` |
| `PATCH /api/companies/:param/members/:param/permissions` | `companies.rs` |
| `PATCH /api/companies/:param/members/:param/role-and-grants` | `access_control.rs` |
| `PATCH /api/companies/:param/skill-test-run-templates/:param` | `skills.rs` |
| `PATCH /api/companies/:param/skills/:param/comments/:param` | `skills.rs` |
| `PATCH /api/companies/:param/skills/:param/files` | `skills.rs` |
| `PATCH /api/companies/:param/skills/:param/test-inputs/:param` | `skills.rs` |
| `PATCH /api/companies/:param/user-secret-definitions/:param` | `user_secret_definitions.rs` |
| `PATCH /api/instance/settings` | `instance_settings.rs` |
| `PATCH /api/instance/settings/experimental` | `instance_settings.rs` |
| `PATCH /api/instance/settings/general` | `instance_settings.rs` |
| `PATCH /api/issues/:param/documents/:param/annotations/:param` | `issues.rs` |
| `PATCH /api/projects/:param/workspaces/:param` | `projects.rs` |
| `PATCH /api/routines/:param/description/annotations/:param` | `routine_annotations.rs` |
| `POST /api/admin/users/:param/demote-instance-admin` | `access_control.rs` |
| `POST /api/admin/users/:param/promote-instance-admin` | `access_control.rs` |
| `POST /api/agents/:param/skills/sync` | `agents.rs` |
| `POST /api/approvals/:param/request-revision` | `approvals.rs` |
| `POST /api/auth/sign-in/email` | `auth.rs` |
| `POST /api/auth/sign-out` | `auth.rs` |
| `POST /api/auth/sign-up/email` | `auth.rs` |
| `POST /api/board-api-keys` | `access_control.rs` |
| `POST /api/board-claim/:param` | `access_control.rs` |
| `POST /api/cases/:param/automation/retry-plan` | `cases.rs` |
| `POST /api/cases/:param/documents/:param` | `cases.rs` |
| `POST /api/cases/:param/documents/:param/annotations` | `cases.rs` |
| `POST /api/cases/:param/documents/:param/annotations/:param/reply` | `cases.rs` |
| `POST /api/cases/:param/terminal` | `cases.rs` |
| `POST /api/cli-auth/challenges` | `access_control.rs` |
| `POST /api/cli-auth/challenges/:param/approve` | `access_control.rs` |
| `POST /api/cli-auth/challenges/:param/cancel` | `access_control.rs` |
| `POST /api/cloud-upstreams/:param/push-runs` | `cloud_upstreams.rs` |
| `POST /api/cloud-upstreams/:param/push-runs/:param/activation` | `cloud_upstreams.rs` |
| `POST /api/cloud-upstreams/:param/push-runs/:param/cancel` | `cloud_upstreams.rs` |
| `POST /api/cloud-upstreams/:param/push-runs/preview` | `cloud_upstreams.rs` |
| `POST /api/cloud-upstreams/connect/finish` | `cloud_upstreams.rs` |
| `POST /api/cloud-upstreams/connect/start` | `cloud_upstreams.rs` |
| `POST /api/companies/:param/adapters/:param/detect-model` | `adapters.rs` |
| `POST /api/companies/:param/adapters/:param/test-environment` | `adapters.rs` |
| `POST /api/companies/:param/budget-incidents/:param/resolve` | `costs.rs` |
| `POST /api/companies/:param/budgets/policies` | `costs.rs` |
| `POST /api/companies/:param/built-in-agents/:param/provision` | `built_in_agents.rs` |
| `POST /api/companies/:param/built-in-agents/:param/routines/:param/disable` | `built_in_agents.rs` |
| `POST /api/companies/:param/built-in-agents/:param/routines/:param/enable` | `built_in_agents.rs` |
| `POST /api/companies/:param/built-in-agents/:param/routines/:param/run` | `built_in_agents.rs` |
| `POST /api/companies/:param/decision-archive-proposals` | `decisions.rs` |
| `POST /api/companies/:param/decision-queues/:param/items` | `decisions.rs` |
| `POST /api/companies/:param/decision-training` | `decisions.rs` |
| `POST /api/companies/:param/decision-training/preview` | `decisions.rs` |
| `POST /api/companies/:param/environments/probe-config` | `environments.rs` |
| `POST /api/companies/:param/events/:param` | `sse.rs` |
| `POST /api/companies/:param/folders/ensure-my` | `folders.rs` |
| `POST /api/companies/:param/inbox-dismissals` | `companies.rs` |
| `POST /api/companies/:param/invites` | `access_control.rs` |
| `POST /api/companies/:param/issues/:param/watchdog` | `watchdogs.rs` |
| `POST /api/companies/:param/issues/:param/watchdog/evaluate` | `watchdogs.rs` |
| `POST /api/companies/:param/issues/batch-update` | `issues.rs` |
| `POST /api/companies/:param/join-requests/:param/approve` | `access_control.rs` |
| `POST /api/companies/:param/join-requests/:param/reject` | `access_control.rs` |
| `POST /api/companies/:param/me/user-secrets` | `user_secret_definitions.rs` |
| `POST /api/companies/:param/me/user-secrets/:param/rotate` | `user_secret_definitions.rs` |
| `POST /api/companies/:param/members/:param/archive` | `access_control.rs` |
| `POST /api/companies/:param/openclaw/invite-prompt` | `openclaw.rs` |
| `POST /api/companies/:param/secret-provider-configs/discovery/preview` | `secret_provider_configs.rs` |
| `POST /api/companies/:param/secrets/remote-import` | `secret_remote_import.rs` |
| `POST /api/companies/:param/secrets/remote-import/preview` | `secret_remote_import.rs` |
| `POST /api/companies/:param/skill-policy` | `skill_policy.rs` |
| `POST /api/companies/:param/skill-policy/simulate` | `skill_policy.rs` |
| `POST /api/companies/:param/skill-test-run-templates` | `skills.rs` |
| `POST /api/companies/:param/skills/:param/audit` | `skills.rs` |
| `POST /api/companies/:param/skills/:param/comments` | `skills.rs` |
| `POST /api/companies/:param/skills/:param/fork` | `skills.rs` |
| `POST /api/companies/:param/skills/:param/install-update` | `skills.rs` |
| `POST /api/companies/:param/skills/:param/reset` | `skills.rs` |
| `POST /api/companies/:param/skills/:param/test-inputs` | `skills.rs` |
| `POST /api/companies/:param/skills/import` | `skills.rs` |
| `POST /api/companies/:param/skills/install-catalog` | `skills.rs` |
| `POST /api/companies/:param/skills/scan-projects` | `skills.rs` |
| `POST /api/companies/:param/summary-slots/:param/:param/generate` | `summary_slots.rs` |
| `POST /api/companies/:param/teams/catalog/:param/install` | `teams_catalog.rs` |
| `POST /api/companies/:param/teams/catalog/:param/preview` | `teams_catalog.rs` |
| `POST /api/companies/:param/user-secret-definitions` | `user_secret_definitions.rs` |
| `POST /api/companies/:param/watchdogs/evaluate` | `watchdogs.rs` |
| `POST /api/environment-custom-image-setup-sessions/:param/cancel` | `environments.rs` |
| `POST /api/environment-custom-image-setup-sessions/:param/finish` | `environments.rs` |
| `POST /api/environment-custom-image-setup-sessions/:param/terminal-session-token` | `custom_image_setup.rs` |
| `POST /api/environments/:param/acquire` | `environment_diagnostics.rs` |
| `POST /api/environments/:param/custom-image-setup-sessions` | `environments.rs` |
| `POST /api/goals/:param/abandon` | `goals.rs` |
| `POST /api/goals/:param/complete` | `goals.rs` |
| `POST /api/instance/settings/experimental/issue-graph-liveness-auto-recovery/preview` | `instance_settings.rs` |
| `POST /api/instance/settings/experimental/issue-graph-liveness-auto-recovery/run` | `instance_settings.rs` |
| `POST /api/invites/:param/accept` | `access_control.rs` |
| `POST /api/issues/:param/attachments` | `attachments.rs` |
| `POST /api/issues/:param/documents/:param/annotations` | `issues.rs` |
| `POST /api/issues/:param/documents/:param/annotations/:param/reply` | `issues.rs` |
| `POST /api/issues/:param/documents/:param/revisions/:param/restore` | `issues.rs` |
| `POST /api/issues/:param/interactions/:param/accept` | `interactions.rs` |
| `POST /api/issues/:param/interactions/:param/answer` | `interactions.rs` |
| `POST /api/issues/:param/interactions/:param/cancel` | `interactions.rs` |
| `POST /api/issues/:param/interactions/:param/reject` | `interactions.rs` |
| `POST /api/issues/:param/tree-holds/:param/release` | `issue_tree_control.rs` |
| `POST /api/join-requests/:param/claim-api-key` | `auth.rs` |
| `POST /api/plugins/:param/jobs/:param/runs/:param/cancel` | `plugins.rs` |
| `POST /api/plugins/:param/jobs/:param/runs/:param/retry` | `plugins.rs` |
| `POST /api/routine-triggers/:param/rotate-secret` | `routines.rs` |
| `POST /api/routines/:param/description/annotations` | `routine_annotations.rs` |
| `POST /api/routines/:param/description/annotations/:param/comments` | `routine_annotations.rs` |
| `POST /api/routines/:param/pause` | `routines.rs` |
| `POST /api/routines/:param/resume` | `routines.rs` |
| `POST /api/routines/:param/trigger` | `routines.rs` |
| `POST /api/tool-gateway/mcp` | `tools.rs` |
| `PUT /api/admin/users/:param/company-access` | `auth.rs` |
| `PUT /api/comments/:param` | `issue_comments.rs` |
| `PUT /api/companies/:param/decision-triage/:param/:param` | `decisions.rs` |
| `PUT /api/companies/:param/resource-memberships/me/agents/:param` | `resource_memberships.rs` |
| `PUT /api/companies/:param/resource-memberships/me/projects/:param` | `resource_memberships.rs` |
| `PUT /api/companies/:param/sidebar-preferences/me` | `companies.rs` |
| `PUT /api/companies/:param/summary-slots/:param/:param` | `summary_slots.rs` |
| `PUT /api/companies/:param/watchdogs/:param/status` | `watchdogs.rs` |
