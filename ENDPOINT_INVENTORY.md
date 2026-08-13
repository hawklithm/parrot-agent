# Paperclip / Parrot Endpoint Inventory

自动生成：`scripts/endpoint_inventory.py`（P2.1 脚本化检查）。
状态为**结构性匹配**（方法 + 规范化路径）；`partial`/`by-design` 需人工复核。

- Paperclip endpoints: **594**
- Parrot endpoints: **626**
- Implemented (match): **440**
- Missing in Parrot: **154**
- Parrot-only (extension): **186**

## 1. Implemented（Paperclip 在 Parrot 中已存在）

| Paperclip Endpoint | Parrot Source |
|---|---|
| `DELETE /api/adapters/:param` | `adapters.rs` |
| `DELETE /api/agents/:param` | `agents.rs` |
| `DELETE /api/agents/:param/instructions-bundle/file` | `agents.rs` |
| `DELETE /api/agents/:param/keys/:param` | `agents.rs` |
| `DELETE /api/attachments/:param` | `attachments.rs` |
| `DELETE /api/board-api-keys/:param` | `access_control.rs` |
| `DELETE /api/cases/:param/documents/:param` | `cases.rs` |
| `DELETE /api/cases/:param/issue-links/:param` | `cases.rs` |
| `DELETE /api/companies/:param/me/user-secrets/:param` | `user_secret_definitions.rs` |
| `DELETE /api/companies/:param/skill-policy` | `skill_policy.rs` |
| `DELETE /api/companies/:param/skill-test-run-templates/:param` | `skills.rs` |
| `DELETE /api/companies/:param/skills/:param` | `skills.rs` |
| `DELETE /api/companies/:param/skills/:param/comments/:param` | `skills.rs` |
| `DELETE /api/companies/:param/skills/:param/star` | `skills.rs` |
| `DELETE /api/companies/:param/skills/:param/test-inputs/:param` | `skills.rs` |
| `DELETE /api/companies/:param/skills/:param/test-runs/:param` | `skills.rs` |
| `DELETE /api/companies/:param/tools/policies/:param` | `tools.rs` |
| `DELETE /api/companies/:param/user-secret-definitions/:param` | `user_secret_definitions.rs` |
| `DELETE /api/decision-training/:param` | `decisions.rs` |
| `DELETE /api/environments/:param` | `environments.rs` |
| `DELETE /api/environments/:param/custom-image-template` | `environments.rs` |
| `DELETE /api/goals/:param` | `goals.rs` |
| `DELETE /api/issues/:param` | `issues.rs` |
| `DELETE /api/issues/:param/approvals/:param` | `issues.rs` |
| `DELETE /api/issues/:param/documents/:param` | `issues.rs` |
| `DELETE /api/issues/:param/inbox-archive` | `issues.rs` |
| `DELETE /api/issues/:param/read` | `issues.rs` |
| `DELETE /api/labels/:param` | `labels.rs` |
| `DELETE /api/pipelines/:param/stages/:param` | `pipelines.rs` |
| `DELETE /api/plugins/:param` | `plugins.rs` |
| `DELETE /api/projects/:param` | `projects.rs` |
| `DELETE /api/projects/:param/workspaces/:param` | `projects.rs` |
| `DELETE /api/routine-triggers/:param` | `routines.rs` |
| `DELETE /api/secret-provider-configs/:param` | `secret_provider_configs.rs` |
| `DELETE /api/secrets/:param` | `secrets.rs` |
| `DELETE /api/work-products/:param` | `work_products.rs` |
| `GET /api/adapters` | `adapters.rs` |
| `GET /api/adapters/:param` | `adapters.rs` |
| `GET /api/adapters/:param/config-schema` | `adapters.rs` |
| `GET /api/adapters/:param/ui-parser.js` | `adapters.rs` |
| `GET /api/admin/users` | `access_control.rs` |
| `GET /api/admin/users/:param/company-access` | `auth.rs` |
| `GET /api/agents/:param` | `agents.rs` |
| `GET /api/agents/:param/config-revisions` | `config_revisions.rs` |
| `GET /api/agents/:param/config-revisions/:param` | `config_revisions.rs` |
| `GET /api/agents/:param/configuration` | `agents.rs` |
| `GET /api/agents/:param/instructions-bundle` | `agents.rs` |
| `GET /api/agents/:param/instructions-bundle/file` | `agents.rs` |
| `GET /api/agents/:param/keys` | `agents.rs` |
| `GET /api/agents/:param/runtime-state` | `agents.rs` |
| `GET /api/agents/:param/skills` | `agents.rs` |
| `GET /api/agents/:param/task-sessions` | `agents.rs` |
| `GET /api/agents/me` | `agents.rs` |
| `GET /api/agents/me/inbox-lite` | `agents.rs` |
| `GET /api/agents/me/inbox/mine` | `agents.rs` |
| `GET /api/approvals/:param` | `approvals.rs` |
| `GET /api/approvals/:param/comments` | `approvals.rs` |
| `GET /api/approvals/:param/issues` | `approvals.rs` |
| `GET /api/assets/:param/content` | `assets.rs` |
| `GET /api/attachments/:param/content` | `attachments.rs` |
| `GET /api/auth/get-session` | `auth.rs` |
| `GET /api/auth/profile` | `auth.rs` |
| `GET /api/board-api-keys` | `access_control.rs` |
| `GET /api/cases/:param` | `cases.rs` |
| `GET /api/cases/:param/children` | `cases.rs` |
| `GET /api/cases/:param/children/tree` | `cases.rs` |
| `GET /api/cases/:param/context-pack` | `cases.rs` |
| `GET /api/cases/:param/documents/:param` | `cases.rs` |
| `GET /api/cases/:param/documents/:param/annotations` | `cases.rs` |
| `GET /api/cases/:param/documents/:param/annotations/:param` | `cases.rs` |
| `GET /api/cases/:param/documents/:param/revisions` | `cases.rs` |
| `GET /api/cases/:param/events` | `cases.rs` |
| `GET /api/cases/:param/issue-links` | `cases.rs` |
| `GET /api/cases/:param/outputs` | `cases.rs` |
| `GET /api/cases/:param/rollup` | `cases.rs` |
| `GET /api/cli-auth/me` | `access_control.rs` |
| `GET /api/companies/:param/activity` | `activity.rs` |
| `GET /api/companies/:param/adapters/:param/detect-model` | `adapters.rs` |
| `GET /api/companies/:param/adapters/:param/model-profiles` | `adapters.rs` |
| `GET /api/companies/:param/adapters/:param/models` | `adapters.rs` |
| `GET /api/companies/:param/agent-configurations` | `agents.rs` |
| `GET /api/companies/:param/agents` | `agents.rs` |
| `GET /api/companies/:param/approvals` | `approvals.rs` |
| `GET /api/companies/:param/attention` | `decisions.rs` |
| `GET /api/companies/:param/budgets/overview` | `costs.rs` |
| `GET /api/companies/:param/built-in-agents` | `built_in_agents.rs` |
| `GET /api/companies/:param/built-in-agents/:param/status` | `built_in_agents.rs` |
| `GET /api/companies/:param/case-events` | `pipelines.rs` |
| `GET /api/companies/:param/cases` | `cases.rs` |
| `GET /api/companies/:param/costs/by-agent` | `costs.rs` |
| `GET /api/companies/:param/costs/by-agent-model` | `costs.rs` |
| `GET /api/companies/:param/costs/by-biller` | `costs.rs` |
| `GET /api/companies/:param/costs/by-project` | `costs.rs` |
| `GET /api/companies/:param/costs/by-provider` | `costs.rs` |
| `GET /api/companies/:param/costs/finance-by-biller` | `costs.rs` |
| `GET /api/companies/:param/costs/finance-by-kind` | `costs.rs` |
| `GET /api/companies/:param/costs/finance-events` | `costs.rs` |
| `GET /api/companies/:param/costs/finance-summary` | `costs.rs` |
| `GET /api/companies/:param/costs/quota-windows` | `costs.rs` |
| `GET /api/companies/:param/costs/summary` | `costs.rs` |
| `GET /api/companies/:param/costs/window-spend` | `costs.rs` |
| `GET /api/companies/:param/dashboard` | `companies.rs` |
| `GET /api/companies/:param/decision-queue-seed-rules` | `decisions.rs` |
| `GET /api/companies/:param/decision-queues` | `decisions.rs` |
| `GET /api/companies/:param/decision-queues/:param/items` | `decisions.rs` |
| `GET /api/companies/:param/decision-training` | `decisions.rs` |
| `GET /api/companies/:param/decision-training/export.jsonl` | `decisions.rs` |
| `GET /api/companies/:param/decision-triage/:param/:param` | `decisions.rs` |
| `GET /api/companies/:param/decisions` | `decisions.rs` |
| `GET /api/companies/:param/decisions/stats` | `decisions.rs` |
| `GET /api/companies/:param/environments` | `environments.rs` |
| `GET /api/companies/:param/environments/capabilities` | `environments.rs` |
| `GET /api/companies/:param/execution-workspaces` | `execution_workspaces.rs` |
| `GET /api/companies/:param/folders` | `routines.rs` |
| `GET /api/companies/:param/goals` | `goals.rs` |
| `GET /api/companies/:param/heartbeat-runs` | `heartbeat_runs.rs` |
| `GET /api/companies/:param/inbox-dismissals` | `companies.rs` |
| `GET /api/companies/:param/issues` | `issues.rs` |
| `GET /api/companies/:param/issues/count` | `issues.rs` |
| `GET /api/companies/:param/join-requests` | `access_control.rs` |
| `GET /api/companies/:param/labels` | `labels.rs` |
| `GET /api/companies/:param/live-runs` | `heartbeat_runs.rs` |
| `GET /api/companies/:param/me/user-secrets` | `user_secret_definitions.rs` |
| `GET /api/companies/:param/members` | `access_control.rs` |
| `GET /api/companies/:param/org` | `org_chart.rs` |
| `GET /api/companies/:param/org.png` | `org_chart.rs` |
| `GET /api/companies/:param/pipelines` | `pipelines.rs` |
| `GET /api/companies/:param/pipelines-attention` | `pipelines.rs` |
| `GET /api/companies/:param/projects` | `projects.rs` |
| `GET /api/companies/:param/resource-memberships/me` | `resource_memberships.rs` |
| `GET /api/companies/:param/review-cases` | `pipelines.rs` |
| `GET /api/companies/:param/routines` | `routines.rs` |
| `GET /api/companies/:param/search` | `companies.rs` |
| `GET /api/companies/:param/secret-provider-configs` | `secret_provider_configs.rs` |
| `GET /api/companies/:param/secret-providers` | `secrets.rs` |
| `GET /api/companies/:param/secret-providers/health` | `secret_provider_configs.rs` |
| `GET /api/companies/:param/secrets` | `secrets.rs` |
| `GET /api/companies/:param/sidebar-badges` | `companies.rs` |
| `GET /api/companies/:param/sidebar-preferences/me` | `companies.rs` |
| `GET /api/companies/:param/skill-policy` | `skill_policy.rs` |
| `GET /api/companies/:param/skill-test-run-templates` | `skills.rs` |
| `GET /api/companies/:param/skills` | `skills.rs` |
| `GET /api/companies/:param/skills/:param` | `skills.rs` |
| `GET /api/companies/:param/skills/:param/comments` | `skills.rs` |
| `GET /api/companies/:param/skills/:param/files` | `skills.rs` |
| `GET /api/companies/:param/skills/:param/fork-precheck` | `skills.rs` |
| `GET /api/companies/:param/skills/:param/test-inputs` | `skills.rs` |
| `GET /api/companies/:param/skills/:param/test-runs` | `skills.rs` |
| `GET /api/companies/:param/skills/:param/test-runs/:param` | `skills.rs` |
| `GET /api/companies/:param/skills/:param/update-status` | `skills.rs` |
| `GET /api/companies/:param/skills/:param/versions` | `skills.rs` |
| `GET /api/companies/:param/skills/:param/versions/:param` | `skills.rs` |
| `GET /api/companies/:param/skills/categories` | `skills.rs` |
| `GET /api/companies/:param/teams/catalog/installed` | `teams_catalog.rs` |
| `GET /api/companies/:param/tools/connections` | `tools.rs` |
| `GET /api/companies/:param/tools/gateways` | `tools.rs` |
| `GET /api/companies/:param/tools/policies` | `tools.rs` |
| `GET /api/companies/:param/tools/profiles/effective/agents/:param` | `tools.rs` |
| `GET /api/companies/:param/tools/runs/:param/decisions` | `tools.rs` |
| `GET /api/companies/:param/user-directory` | `user_directory.rs` |
| `GET /api/companies/:param/user-secret-definitions` | `user_secret_definitions.rs` |
| `GET /api/companies/:param/user-secret-definitions/:param/coverage` | `user_secret_definitions.rs` |
| `GET /api/companies/:param/users/:param/profile` | `companies.rs` |
| `GET /api/companies/:param/workspace-overview` | `execution_workspaces.rs` |
| `GET /api/decision-training/:param` | `decisions.rs` |
| `GET /api/decisions/:param` | `decisions.rs` |
| `GET /api/environment-custom-image-setup-sessions/:param` | `custom_image_setup.rs` |
| `GET /api/environment-leases/:param` | `environments.rs` |
| `GET /api/environments/:param` | `environments.rs` |
| `GET /api/environments/:param/custom-image-template` | `environments.rs` |
| `GET /api/environments/:param/delete-blast-radius` | `environment_diagnostics.rs` |
| `GET /api/execution-workspaces/:param` | `execution_workspaces.rs` |
| `GET /api/execution-workspaces/:param/close-readiness` | `execution_workspaces.rs` |
| `GET /api/execution-workspaces/:param/workspace-operations` | `execution_workspaces.rs` |
| `GET /api/feedback-traces/:param` | `feedback_traces.rs` |
| `GET /api/feedback-traces/:param/bundle` | `feedback_traces.rs` |
| `GET /api/goals/:param` | `goals.rs` |
| `GET /api/heartbeat-runs/:param` | `heartbeat_runs.rs` |
| `GET /api/heartbeat-runs/:param/events` | `heartbeat_runs.rs` |
| `GET /api/heartbeat-runs/:param/issues` | `heartbeat_runs.rs` |
| `GET /api/heartbeat-runs/:param/log` | `heartbeat_runs.rs` |
| `GET /api/heartbeat-runs/:param/workspace-operations` | `heartbeat_runs.rs` |
| `GET /api/instance/scheduler-heartbeats` | `agents.rs` |
| `GET /api/instance/settings` | `instance_settings.rs` |
| `GET /api/instance/settings/experimental` | `instance_settings.rs` |
| `GET /api/instance/settings/general` | `instance_settings.rs` |
| `GET /api/invites/:param` | `access_control.rs` |
| `GET /api/invites/:param/logo` | `invite_resources.rs` |
| `GET /api/invites/:param/onboarding` | `invite_resources.rs` |
| `GET /api/invites/:param/onboarding.txt` | `invite_resources.rs` |
| `GET /api/invites/:param/skills/:param` | `invite_resources.rs` |
| `GET /api/invites/:param/skills/index` | `invite_resources.rs` |
| `GET /api/issues` | `issues.rs` |
| `GET /api/issues/:param` | `issues.rs` |
| `GET /api/issues/:param/accepted-plan-decompositions` | `issues.rs` |
| `GET /api/issues/:param/active-run` | `issues.rs` |
| `GET /api/issues/:param/activity` | `activity.rs` |
| `GET /api/issues/:param/approvals` | `issues.rs` |
| `GET /api/issues/:param/attachments` | `attachments.rs` |
| `GET /api/issues/:param/cases` | `issues.rs` |
| `GET /api/issues/:param/comments` | `issue_comments.rs` |
| `GET /api/issues/:param/comments/:param` | `issue_comments.rs` |
| `GET /api/issues/:param/cost-summary` | `costs.rs` |
| `GET /api/issues/:param/diagnostics/blockers` | `issue_diagnostics.rs` |
| `GET /api/issues/:param/diagnostics/subtree` | `issue_diagnostics.rs` |
| `GET /api/issues/:param/diagnostics/wakes` | `issue_diagnostics.rs` |
| `GET /api/issues/:param/documents` | `issues.rs` |
| `GET /api/issues/:param/documents/:param` | `issues.rs` |
| `GET /api/issues/:param/documents/:param/annotations` | `issues.rs` |
| `GET /api/issues/:param/documents/:param/annotations/:param` | `issues.rs` |
| `GET /api/issues/:param/documents/:param/revisions` | `issues.rs` |
| `GET /api/issues/:param/external-object-summary` | `issues.rs` |
| `GET /api/issues/:param/external-objects` | `issues.rs` |
| `GET /api/issues/:param/feedback-traces` | `issues.rs` |
| `GET /api/issues/:param/feedback-votes` | `issues.rs` |
| `GET /api/issues/:param/file-resources/content` | `issues.rs` |
| `GET /api/issues/:param/file-resources/list` | `issues.rs` |
| `GET /api/issues/:param/file-resources/resolve` | `issues.rs` |
| `GET /api/issues/:param/heartbeat-context` | `issues.rs` |
| `GET /api/issues/:param/interactions` | `interactions.rs` |
| `GET /api/issues/:param/live-runs` | `issues.rs` |
| `GET /api/issues/:param/recovery-actions` | `issues.rs` |
| `GET /api/issues/:param/runs` | `heartbeat_runs.rs` |
| `GET /api/issues/:param/tree-control/state` | `issue_tree_control.rs` |
| `GET /api/issues/:param/tree-holds` | `issue_tree_control.rs` |
| `GET /api/issues/:param/tree-holds/:param` | `issue_tree_control.rs` |
| `GET /api/issues/:param/work-products` | `work_products.rs` |
| `GET /api/llms/agent-configuration.txt` | `llms.rs` |
| `GET /api/llms/agent-configuration/:param.txt` | `llms.rs` |
| `GET /api/llms/agent-icons.txt` | `llms.rs` |
| `GET /api/mcp/gateways/:param` | `tools.rs` |
| `GET /api/openapi.json` | `llms.rs` |
| `GET /api/pipelines/:param` | `pipelines.rs` |
| `GET /api/pipelines/:param/cases` | `pipelines.rs` |
| `GET /api/pipelines/:param/documents/:param` | `pipelines.rs` |
| `GET /api/pipelines/:param/documents/:param/revisions` | `pipelines.rs` |
| `GET /api/pipelines/:param/health` | `pipelines.rs` |
| `GET /api/pipelines/:param/intake-form` | `pipelines.rs` |
| `GET /api/plugins` | `plugins.rs` |
| `GET /api/plugins/:param` | `plugins.rs` |
| `GET /api/plugins/:param/bridge/stream/:param` | `plugins.rs` |
| `GET /api/plugins/:param/companies/:param/local-folders` | `plugins.rs` |
| `GET /api/plugins/:param/companies/:param/local-folders/:param/status` | `plugins.rs` |
| `GET /api/plugins/:param/config` | `plugins.rs` |
| `GET /api/plugins/:param/dashboard` | `plugins.rs` |
| `GET /api/plugins/:param/health` | `plugins.rs` |
| `GET /api/plugins/:param/jobs` | `plugins.rs` |
| `GET /api/plugins/:param/jobs/:param/runs` | `plugins.rs` |
| `GET /api/plugins/:param/logs` | `plugins.rs` |
| `GET /api/plugins/examples` | `plugins.rs` |
| `GET /api/plugins/tools` | `plugins.rs` |
| `GET /api/plugins/ui-contributions` | `plugins.rs` |
| `GET /api/projects/:param` | `projects.rs` |
| `GET /api/projects/:param/external-object-summary` | `projects.rs` |
| `GET /api/projects/:param/workspaces` | `projects.rs` |
| `GET /api/routines/:param` | `routines.rs` |
| `GET /api/routines/:param/description/annotations` | `routine_annotations.rs` |
| `GET /api/routines/:param/revisions` | `routines.rs` |
| `GET /api/routines/:param/runs` | `routines.rs` |
| `GET /api/secret-provider-configs/:param` | `secret_provider_configs.rs` |
| `GET /api/secrets/:param/access-events` | `secrets.rs` |
| `GET /api/secrets/:param/usage` | `secrets.rs` |
| `GET /api/skills/:param` | `skills.rs` |
| `GET /api/skills/available` | `skills.rs` |
| `GET /api/skills/catalog` | `skills.rs` |
| `GET /api/skills/catalog/:param` | `skills.rs` |
| `GET /api/skills/index` | `skills.rs` |
| `GET /api/stats` | `llms.rs` |
| `GET /api/teams/catalog` | `teams_catalog.rs` |
| `GET /api/teams/catalog/:param` | `teams_catalog.rs` |
| `GET /api/teams/catalog/:param/files` | `teams_catalog.rs` |
| `GET /api/tool-gateway/gateways/:param/mcp` | `tools.rs` |
| `GET /api/tool-gateway/tools` | `tools.rs` |
| `GET /api/workspace-operations/:param/log` | `heartbeat_runs.rs` |
| `PATCH /api/adapters/:param` | `adapters.rs` |
| `PATCH /api/adapters/:param/override` | `adapters.rs` |
| `PATCH /api/agents/:param` | `agents.rs` |
| `PATCH /api/agents/:param/budgets` | `agents.rs` |
| `PATCH /api/agents/:param/instructions-bundle` | `agents.rs` |
| `PATCH /api/agents/:param/instructions-path` | `agents.rs` |
| `PATCH /api/agents/:param/permissions` | `agents.rs` |
| `PATCH /api/auth/profile` | `auth.rs` |
| `PATCH /api/cases/:param` | `cases.rs` |
| `PATCH /api/companies/:param/budgets` | `costs.rs` |
| `PATCH /api/companies/:param/decision-queues/:param` | `decisions.rs` |
| `PATCH /api/decision-training/:param` | `decisions.rs` |
| `PATCH /api/environments/:param` | `environments.rs` |
| `PATCH /api/execution-workspaces/:param` | `execution_workspaces.rs` |
| `PATCH /api/goals/:param` | `goals.rs` |
| `PATCH /api/issues/:param` | `issues.rs` |
| `PATCH /api/pipelines/:param/stages/:param` | `pipelines.rs` |
| `PATCH /api/pipelines/:param/stages/:param/automation-env` | `pipelines.rs` |
| `PATCH /api/projects/:param` | `projects.rs` |
| `PATCH /api/routine-triggers/:param` | `routines.rs` |
| `PATCH /api/routines/:param` | `routines.rs` |
| `PATCH /api/secret-provider-configs/:param` | `secret_provider_configs.rs` |
| `PATCH /api/secrets/:param` | `secrets.rs` |
| `PATCH /api/tool-gateway/gateways/:param` | `tools.rs` |
| `PATCH /api/work-products/:param` | `work_products.rs` |
| `POST /api/adapters/:param/reinstall` | `adapters.rs` |
| `POST /api/adapters/:param/reload` | `adapters.rs` |
| `POST /api/adapters/install` | `adapters.rs` |
| `POST /api/agents/:param/approve` | `agents.rs` |
| `POST /api/agents/:param/claude-login` | `agents.rs` |
| `POST /api/agents/:param/clear-error` | `agents.rs` |
| `POST /api/agents/:param/config-revisions/:param/rollback` | `agents.rs` |
| `POST /api/agents/:param/heartbeat/invoke` | `agents.rs` |
| `POST /api/agents/:param/keys` | `agents.rs` |
| `POST /api/agents/:param/pause` | `agents.rs` |
| `POST /api/agents/:param/resume` | `agents.rs` |
| `POST /api/agents/:param/runtime-state/reset-session` | `agents.rs` |
| `POST /api/agents/:param/terminate` | `agents.rs` |
| `POST /api/agents/:param/wakeup` | `agents.rs` |
| `POST /api/approvals/:param/approve` | `approvals.rs` |
| `POST /api/approvals/:param/comments` | `approvals.rs` |
| `POST /api/approvals/:param/reject` | `approvals.rs` |
| `POST /api/approvals/:param/resubmit` | `approvals.rs` |
| `POST /api/board/chat/stream` | `board_chat.rs` |
| `POST /api/bootstrap/claim` | `access_control.rs` |
| `POST /api/cases/:param/acknowledge-drift` | `cases.rs` |
| `POST /api/cases/:param/attachments` | `cases.rs` |
| `POST /api/cases/:param/automation/current-stage/rerun` | `cases.rs` |
| `POST /api/cases/:param/automation/retry` | `cases.rs` |
| `POST /api/cases/:param/automations/:param/retry` | `cases.rs` |
| `POST /api/cases/:param/breakdown` | `cases.rs` |
| `POST /api/cases/:param/documents/:param/lock` | `cases.rs` |
| `POST /api/cases/:param/documents/:param/revisions/:param/restore` | `cases.rs` |
| `POST /api/cases/:param/documents/:param/unlock` | `cases.rs` |
| `POST /api/cases/:param/issue-links` | `cases.rs` |
| `POST /api/cases/:param/links` | `cases.rs` |
| `POST /api/cases/:param/open-conversation` | `cases.rs` |
| `POST /api/cases/:param/resolve-suggestion` | `cases.rs` |
| `POST /api/cases/:param/review` | `cases.rs` |
| `POST /api/cases/:param/suggest-transition` | `cases.rs` |
| `POST /api/cli-auth/revoke-current` | `access_control.rs` |
| `POST /api/companies/:param/activity` | `activity.rs` |
| `POST /api/companies/:param/agent-hires` | `agents.rs` |
| `POST /api/companies/:param/approvals` | `approvals.rs` |
| `POST /api/companies/:param/assets/images` | `assets.rs` |
| `POST /api/companies/:param/built-in-agents/:param/reconcile` | `built_in_agents.rs` |
| `POST /api/companies/:param/built-in-agents/:param/reset` | `built_in_agents.rs` |
| `POST /api/companies/:param/cases` | `cases.rs` |
| `POST /api/companies/:param/cost-events` | `costs.rs` |
| `POST /api/companies/:param/decision-bundles` | `decisions.rs` |
| `POST /api/companies/:param/decision-queues` | `decisions.rs` |
| `POST /api/companies/:param/decision-retention/:param/:param/archive` | `decisions.rs` |
| `POST /api/companies/:param/decision-retention/:param/:param/revive` | `decisions.rs` |
| `POST /api/companies/:param/decisions` | `decisions.rs` |
| `POST /api/companies/:param/environments` | `environments.rs` |
| `POST /api/companies/:param/finance-events` | `costs.rs` |
| `POST /api/companies/:param/goals` | `goals.rs` |
| `POST /api/companies/:param/issues` | `issues.rs` |
| `POST /api/companies/:param/issues/:param/attachments` | `attachments.rs` |
| `POST /api/companies/:param/issues/external-object-summaries` | `companies.rs` |
| `POST /api/companies/:param/labels` | `labels.rs` |
| `POST /api/companies/:param/logo` | `assets.rs` |
| `POST /api/companies/:param/pipelines` | `pipelines.rs` |
| `POST /api/companies/:param/projects` | `projects.rs` |
| `POST /api/companies/:param/review-cases/bulk` | `pipelines.rs` |
| `POST /api/companies/:param/routines` | `routines.rs` |
| `POST /api/companies/:param/secret-provider-configs` | `secret_provider_configs.rs` |
| `POST /api/companies/:param/secrets` | `secrets.rs` |
| `POST /api/companies/:param/skills/:param/star` | `skills.rs` |
| `POST /api/companies/:param/skills/:param/test-runs/:param/cancel` | `skills.rs` |
| `POST /api/companies/:param/tools/gateways` | `tools.rs` |
| `POST /api/companies/:param/tools/policies` | `tools.rs` |
| `POST /api/decisions/:param/cancel` | `decisions.rs` |
| `POST /api/decisions/:param/decide` | `decisions.rs` |
| `POST /api/decisions/:param/dismiss` | `decisions.rs` |
| `POST /api/environments/:param/custom-image-template/rollback` | `environments.rs` |
| `POST /api/environments/:param/probe` | `environments.rs` |
| `POST /api/execution-workspaces/:param/reconcile-branch` | `execution_workspaces.rs` |
| `POST /api/execution-workspaces/:param/runtime-commands/:param` | `execution_workspaces.rs` |
| `POST /api/execution-workspaces/:param/runtime-services/:param` | `execution_workspaces.rs` |
| `POST /api/heartbeat-runs/:param/cancel` | `heartbeat_runs.rs` |
| `POST /api/heartbeat-runs/:param/watchdog-decisions` | `heartbeat_runs.rs` |
| `POST /api/instance/database-backups` | `instance_settings.rs` |
| `POST /api/issues/:param/accepted-plan-decompositions` | `issues.rs` |
| `POST /api/issues/:param/admin/force-release` | `issues.rs` |
| `POST /api/issues/:param/approvals` | `issues.rs` |
| `POST /api/issues/:param/checkout` | `issues.rs` |
| `POST /api/issues/:param/children` | `issues.rs` |
| `POST /api/issues/:param/comments` | `issue_comments.rs` |
| `POST /api/issues/:param/documents/:param/lock` | `issues.rs` |
| `POST /api/issues/:param/documents/:param/unlock` | `issues.rs` |
| `POST /api/issues/:param/external-objects/refresh` | `issues.rs` |
| `POST /api/issues/:param/feedback-votes` | `issues.rs` |
| `POST /api/issues/:param/inbox-archive` | `issues.rs` |
| `POST /api/issues/:param/interactions` | `interactions.rs` |
| `POST /api/issues/:param/low-trust/promotions` | `low_trust.rs` |
| `POST /api/issues/:param/monitor/check-now` | `issues.rs` |
| `POST /api/issues/:param/read` | `issues.rs` |
| `POST /api/issues/:param/recovery-actions/resolve` | `issues.rs` |
| `POST /api/issues/:param/release` | `issues.rs` |
| `POST /api/issues/:param/scheduled-retry/retry-now` | `issues.rs` |
| `POST /api/issues/:param/tree-control/preview` | `issue_tree_control.rs` |
| `POST /api/issues/:param/tree-holds` | `issue_tree_control.rs` |
| `POST /api/issues/:param/work-products` | `work_products.rs` |
| `POST /api/mcp/gateways/:param` | `tools.rs` |
| `POST /api/pipelines/:param/cases` | `pipelines.rs` |
| `POST /api/pipelines/:param/cases/batch` | `pipelines.rs` |
| `POST /api/pipelines/:param/documents/:param/revisions/:param/restore` | `pipelines.rs` |
| `POST /api/pipelines/:param/stages` | `pipelines.rs` |
| `POST /api/plugins/:param/actions/:param` | `plugins.rs` |
| `POST /api/plugins/:param/bridge/action` | `plugins.rs` |
| `POST /api/plugins/:param/bridge/data` | `plugins.rs` |
| `POST /api/plugins/:param/companies/:param/local-folders/:param/validate` | `plugins.rs` |
| `POST /api/plugins/:param/config` | `plugins.rs` |
| `POST /api/plugins/:param/config/test` | `plugins.rs` |
| `POST /api/plugins/:param/data/:param` | `plugins.rs` |
| `POST /api/plugins/:param/disable` | `plugins.rs` |
| `POST /api/plugins/:param/enable` | `plugins.rs` |
| `POST /api/plugins/:param/jobs/:param/trigger` | `plugins.rs` |
| `POST /api/plugins/:param/upgrade` | `plugins.rs` |
| `POST /api/plugins/:param/webhooks/:param` | `plugins.rs` |
| `POST /api/plugins/install` | `plugins.rs` |
| `POST /api/plugins/tools/execute` | `plugins.rs` |
| `POST /api/projects/:param/workspaces` | `projects.rs` |
| `POST /api/routine-triggers/public/:param/fire` | `routines.rs` |
| `POST /api/routines/:param/revisions/:param/restore` | `routines.rs` |
| `POST /api/routines/:param/run` | `routines.rs` |
| `POST /api/routines/:param/triggers` | `routines.rs` |
| `POST /api/secret-provider-configs/:param/default` | `secret_provider_configs.rs` |
| `POST /api/secret-provider-configs/:param/health` | `secret_provider_configs.rs` |
| `POST /api/secrets/:param/rotate` | `secrets.rs` |
| `POST /api/tool-gateway/action-requests/:param/approve` | `tools.rs` |
| `POST /api/tool-gateway/action-requests/:param/decline` | `tools.rs` |
| `POST /api/tool-gateway/gateway-tokens/:param/revoke` | `tools.rs` |
| `POST /api/tool-gateway/gateways/:param/mcp` | `tools.rs` |
| `POST /api/tool-gateway/gateways/:param/tokens` | `tools.rs` |
| `POST /api/tool-gateway/sessions` | `tools.rs` |
| `POST /api/tool-gateway/sessions/:param/revoke` | `tools.rs` |
| `POST /api/tool-gateway/tools/call` | `tools.rs` |
| `PUT /api/agents/:param/instructions-bundle/file` | `agents.rs` |
| `PUT /api/cases/:param/blockers` | `cases.rs` |
| `PUT /api/cases/:param/documents/:param` | `cases.rs` |
| `PUT /api/issues/:param/documents/:param` | `issues.rs` |
| `PUT /api/pipelines/:param/documents/:param` | `pipelines.rs` |
| `PUT /api/pipelines/:param/transitions` | `pipelines.rs` |
| `PUT /api/plugins/:param/companies/:param/local-folders/:param` | `plugins.rs` |

## 2. Missing in Parrot（需人工判定 partial / by-design）

| Paperclip Endpoint | Declared | Source |
|---|---|---|
| `DELETE /api/:param` | `/:companyId` | `routes/companies.ts:747` |
| `DELETE /api/agents/me/secret-proposals/:param` | `/agents/me/secret-proposals/:id` | `routes/secrets.ts:282` |
| `DELETE /api/companies/:param/folders/:param` | `/companies/:companyId/folders/:folderId` | `routes/folders.ts:149` |
| `DELETE /api/companies/:param/inbox-dismissals/:param` | `/companies/:companyId/inbox-dismissals/:itemKey` | `routes/inbox-dismissals.ts:98` |
| `DELETE /api/issues/:param/comments/:param` | `/issues/:id/comments/:commentId` | `routes/issues.ts:10741` |
| `DELETE /api/issues/:param/watchdog` | `/issues/:id/watchdog` | `routes/issues.ts:6113` |
| `DELETE /api/status-cards/:param` | `/status-cards/:id` | `routes/status-cards.ts:199` |
| `DELETE /api/tool-applications/:param` | `/tool-applications/:applicationId` | `routes/tool-access.ts:511` |
| `DELETE /api/tool-connections/:param` | `/tool-connections/:connectionId` | `routes/tool-access.ts:783` |
| `DELETE /api/tool-connections/:param/grants/:param` | `/tool-connections/:connectionId/grants/:grantId` | `routes/tool-access.ts:601` |
| `DELETE /api/tool-profile-entries/:param` | `/tool-profile-entries/:entryId` | `routes/tool-access.ts:1027` |
| `DELETE /api/tool-profiles/:param` | `/tool-profiles/:profileId` | `routes/tool-access.ts:954` |
| `GET /api` | `/` | `routes/companies.ts:264` |
| `GET /api/:param` | `/:companyId` | `routes/companies.ts:359` |
| `GET /api/:param/artifacts` | `/:companyId/artifacts` | `routes/companies.ts:296` |
| `GET /api/:param/export/fidelity` | `/:companyId/export/fidelity` | `routes/companies.ts:410` |
| `GET /api/:param/feedback-traces` | `/:companyId/feedback-traces` | `routes/companies.ts:374` |
| `GET /api/:param/timeline` | `/:companyId/timeline` | `routes/companies.ts:305` |
| `GET /api/_plugins/:param/ui/*filePath` | `/_plugins/:pluginId/ui/*filePath` | `routes/plugin-ui-static.ts:230` |
| `GET /api/agents/me/secret-proposals` | `/agents/me/secret-proposals` | `routes/secrets.ts:267` |
| `GET /api/agents/me/secrets` | `/agents/me/secrets` | `routes/secrets.ts:336` |
| `GET /api/board-claim/:param` | `/board-claim/:token` | `routes/access.ts:2655` |
| `GET /api/cases/:param/automation/retry-plan` | `/cases/:caseId/automation/retry-plan` | `routes/pipelines.ts:2217` |
| `GET /api/cli-auth/challenges/:param` | `/cli-auth/challenges/:id` | `routes/access.ts:2751` |
| `GET /api/companies/:param/audit/agent-actions` | `/companies/:companyId/audit/agent-actions` | `routes/activity.ts:238` |
| `GET /api/companies/:param/audit/agent-actions.csv` | `/companies/:companyId/audit/agent-actions.csv` | `routes/activity.ts:274` |
| `GET /api/companies/:param/invites` | `/companies/:companyId/invites` | `routes/access.ts:4131` |
| `GET /api/companies/:param/org.svg` | `/companies/:companyId/org.svg` | `routes/agents.ts:2234` |
| `GET /api/companies/:param/recovery-observability` | `/companies/:companyId/recovery-observability` | `routes/dashboard.ts:34` |
| `GET /api/companies/:param/search/extract` | `/companies/:companyId/search/extract` | `routes/issues.ts:5195` |
| `GET /api/companies/:param/secret-proposals` | `/companies/:companyId/secret-proposals` | `routes/secrets.ts:288` |
| `GET /api/companies/:param/smoke-lab/oauth/authorize` | `/companies/:companyId/smoke-lab/oauth/authorize` | `routes/smoke-lab.ts:58` |
| `GET /api/companies/:param/smoke-lab/oauth/userinfo` | `/companies/:companyId/smoke-lab/oauth/userinfo` | `routes/smoke-lab.ts:99` |
| `GET /api/companies/:param/smoke-lab/runs` | `/companies/:companyId/smoke-lab/runs` | `routes/smoke-lab.ts:191` |
| `GET /api/companies/:param/smoke-lab/runs/:param` | `/companies/:companyId/smoke-lab/runs/:runId` | `routes/smoke-lab.ts:219` |
| `GET /api/companies/:param/smoke-lab/services` | `/companies/:companyId/smoke-lab/services` | `routes/smoke-lab.ts:115` |
| `GET /api/companies/:param/status-cards` | `/companies/:companyId/status-cards` | `routes/status-cards.ts:142` |
| `GET /api/companies/:param/summary-slots/:param/:param` | `/companies/:companyId/summary-slots/:scopeKind/:slotKey` | `routes/summary-slots.ts:76` |
| `GET /api/companies/:param/summary-slots/:param/:param/revisions` | `/companies/:companyId/summary-slots/:scopeKind/:slotKey/revisions` | `routes/summary-slots.ts:89` |
| `GET /api/companies/:param/tools/action-requests` | `/companies/:companyId/tools/action-requests` | `routes/tool-access.ts:402` |
| `GET /api/companies/:param/tools/applications` | `/companies/:companyId/tools/applications` | `routes/tool-access.ts:464` |
| `GET /api/companies/:param/tools/apps/attention` | `/companies/:companyId/tools/apps/attention` | `routes/tool-access.ts:395` |
| `GET /api/companies/:param/tools/examples` | `/companies/:companyId/tools/examples` | `routes/tool-access.ts:388` |
| `GET /api/companies/:param/tools/gallery` | `/companies/:companyId/tools/gallery` | `routes/tool-access.ts:235` |
| `GET /api/companies/:param/tools/profiles` | `/companies/:companyId/tools/profiles` | `routes/tool-access.ts:867` |
| `GET /api/companies/:param/tools/runtime-health` | `/companies/:companyId/tools/runtime-health` | `routes/tool-access.ts:1107` |
| `GET /api/companies/:param/tools/runtime-slots` | `/companies/:companyId/tools/runtime-slots` | `routes/tool-access.ts:1089` |
| `GET /api/companies/:param/tools/stdio-templates` | `/companies/:companyId/tools/stdio-templates` | `routes/tool-access.ts:1299` |
| `GET /api/companies/:param/tools/trust-rules` | `/companies/:companyId/tools/trust-rules` | `routes/tool-access.ts:1121` |
| `GET /api/companies/:param/users/:param/inbox-agent-policy` | `/companies/:companyId/users/:userId/inbox-agent-policy` | `routes/inbox-agent-policy.ts:84` |
| `GET /api/companies/:param/users/me/inbox-agent-policy` | `/companies/:companyId/users/me/inbox-agent-policy` | `routes/inbox-agent-policy.ts:68` |
| `GET /api/environments/:param/leases` | `/environments/:id/leases` | `routes/environments.ts:951` |
| `GET /api/environments/:param/secret-refs` | `/environments/:id/secret-refs` | `routes/environments.ts:935` |
| `GET /api/import/jobs/:param` | `/import/jobs/:jobId` | `routes/companies.ts:425` |
| `GET /api/invites/:param/test-resolution` | `/invites/:token/test-resolution` | `routes/access.ts:3559` |
| `GET /api/issues/:param/watchdog` | `/issues/:id/watchdog` | `routes/issues.ts:6064` |
| `GET /api/routines/:param/description/annotations/:param` | `/routines/:id/description/annotations/:threadId` | `routes/routines.ts:224` |
| `GET /api/sidebar-preferences/me` | `/sidebar-preferences/me` | `routes/sidebar-preferences.ts:21` |
| `GET /api/skills/catalog/:param/files` | `/skills/catalog/:catalogId/files` | `routes/company-skills.ts:295` |
| `GET /api/stacks` | `/stacks` | `routes/cloud.ts:30` |
| `GET /api/status-cards/:param` | `/status-cards/:id` | `routes/status-cards.ts:170` |
| `GET /api/status-cards/:param/dry-run` | `/status-cards/:id/dry-run` | `routes/status-cards.ts:251` |
| `GET /api/status-cards/:param/summary-revisions` | `/status-cards/:id/summary-revisions` | `routes/status-cards.ts:216` |
| `GET /api/status-cards/:param/updates` | `/status-cards/:id/updates` | `routes/status-cards.ts:209` |
| `GET /api/tool-connections/:param` | `/tool-connections/:connectionId` | `routes/tool-access.ts:559` |
| `GET /api/tool-connections/:param/activity` | `/tool-connections/:connectionId/activity` | `routes/tool-access.ts:857` |
| `GET /api/tool-connections/:param/catalog` | `/tool-connections/:connectionId/catalog` | `routes/tool-access.ts:849` |
| `GET /api/tool-connections/:param/grants` | `/tool-connections/:connectionId/grants` | `routes/tool-access.ts:567` |
| `GET /api/tool-connections/:param/installs` | `/tool-connections/:connectionId/installs` | `routes/tool-access.ts:628` |
| `GET /api/tool-connections/:param/test-agents` | `/tool-connections/:connectionId/test-agents` | `routes/tool-access.ts:659` |
| `GET /api/tool-connections/:param/test-calls/:param` | `/tool-connections/:connectionId/test-calls/:actionRequestId` | `routes/tool-access.ts:720` |
| `GET /api/tool-connections/:param/usage` | `/tool-connections/:connectionId/usage` | `routes/tool-access.ts:618` |
| `GET /api/tool-gateway/audit` | `/tool-gateway/audit` | `routes/tool-gateway.ts:636` |
| `GET /api/tool-gateway/runtime-slots` | `/tool-gateway/runtime-slots` | `routes/tool-gateway.ts:567` |
| `GET /api/tool-profiles/:param/new-tools` | `/tool-profiles/:profileId/new-tools` | `routes/tool-access.ts:874` |
| `GET /api/tools/oauth/callback` | `/tools/oauth/callback` | `routes/tool-access.ts:320` |
| `PATCH /api/:param` | `/:companyId` | `routes/companies.ts:633` |
| `PATCH /api/:param/branding` | `/:companyId/branding` | `routes/companies.ts:710` |
| `PATCH /api/companies/:param/folders/:param` | `/companies/:companyId/folders/:folderId` | `routes/folders.ts:79` |
| `PATCH /api/companies/:param/smoke-lab/runs/:param` | `/companies/:companyId/smoke-lab/runs/:runId` | `routes/smoke-lab.ts:226` |
| `PATCH /api/companies/:param/tools/policies/:param` | `/companies/:companyId/tools/policies/:policyId` | `routes/tool-access.ts:1206` |
| `PATCH /api/pipelines/:param` | `/pipelines/:pipelineId` | `routes/pipelines.ts:1171` |
| `PATCH /api/status-cards/:param` | `/status-cards/:id` | `routes/status-cards.ts:177` |
| `PATCH /api/tool-applications/:param` | `/tool-applications/:applicationId` | `routes/tool-access.ts:491` |
| `PATCH /api/tool-connections/:param` | `/tool-connections/:connectionId` | `routes/tool-access.ts:740` |
| `PATCH /api/tool-profile-entries/:param` | `/tool-profile-entries/:entryId` | `routes/tool-access.ts:1011` |
| `PATCH /api/tool-profiles/:param` | `/tool-profiles/:profileId` | `routes/tool-access.ts:909` |
| `POST /api` | `/` | `routes/companies.ts:584` |
| `POST /api/:param/archive` | `/:companyId/archive` | `routes/companies.ts:735` |
| `POST /api/:param/export` | `/:companyId/export` | `routes/companies.ts:402` |
| `POST /api/:param/exports` | `/:companyId/exports` | `routes/companies.ts:522` |
| `POST /api/:param/exports/preview` | `/:companyId/exports/preview` | `routes/companies.ts:514` |
| `POST /api/:param/imports/apply` | `/:companyId/imports/apply` | `routes/companies.ts:547` |
| `POST /api/:param/imports/preview` | `/:companyId/imports/preview` | `routes/companies.ts:530` |
| `POST /api/agents/me/connections/:param/start-authorization` | `/agents/me/connections/:connectionId/start-authorization` | `routes/tool-access.ts:178` |
| `POST /api/agents/me/connections/:param/token` | `/agents/me/connections/:connectionId/token` | `routes/tool-access.ts:196` |
| `POST /api/agents/me/secret-proposals` | `/agents/me/secret-proposals` | `routes/secrets.ts:243` |
| `POST /api/agents/me/secrets/:param/value` | `/agents/me/secrets/:key/value` | `routes/secrets.ts:355` |
| `POST /api/board-claim/:param/claim` | `/board-claim/:token/claim` | `routes/access.ts:2666` |
| `POST /api/cases/:param/claim` | `/cases/:caseId/claim` | `routes/pipelines.ts:1927` |
| `POST /api/cases/:param/release` | `/cases/:caseId/release` | `routes/pipelines.ts:1936` |
| `POST /api/cases/:param/transition` | `/cases/:caseId/transition` | `routes/pipelines.ts:1944` |
| `POST /api/companies/:param/agents` | `/companies/:companyId/agents` | `routes/agents.ts:2702` |
| `POST /api/companies/:param/folders` | `/companies/:companyId/folders` | `routes/folders.ts:32` |
| `POST /api/companies/:param/folders/:param/move` | `/companies/:companyId/folders/:folderId/move` | `routes/folders.ts:124` |
| `POST /api/companies/:param/folders/items/move` | `/companies/:companyId/folders/items/move` | `routes/folders.ts:104` |
| `POST /api/companies/:param/secret-proposals/:param/approve` | `/companies/:companyId/secret-proposals/:id/approve` | `routes/secrets.ts:303` |
| `POST /api/companies/:param/secret-proposals/:param/reject` | `/companies/:companyId/secret-proposals/:id/reject` | `routes/secrets.ts:319` |
| `POST /api/companies/:param/smoke-lab/install-fixtures` | `/companies/:companyId/smoke-lab/install-fixtures` | `routes/smoke-lab.ts:164` |
| `POST /api/companies/:param/smoke-lab/oauth/authorize` | `/companies/:companyId/smoke-lab/oauth/authorize` | `routes/smoke-lab.ts:72` |
| `POST /api/companies/:param/smoke-lab/oauth/revoke` | `/companies/:companyId/smoke-lab/oauth/revoke` | `routes/smoke-lab.ts:107` |
| `POST /api/companies/:param/smoke-lab/oauth/token` | `/companies/:companyId/smoke-lab/oauth/token` | `routes/smoke-lab.ts:87` |
| `POST /api/companies/:param/smoke-lab/reset` | `/companies/:companyId/smoke-lab/reset` | `routes/smoke-lab.ts:268` |
| `POST /api/companies/:param/smoke-lab/runs` | `/companies/:companyId/smoke-lab/runs` | `routes/smoke-lab.ts:198` |
| `POST /api/companies/:param/smoke-lab/runs/:param/steps` | `/companies/:companyId/smoke-lab/runs/:runId/steps` | `routes/smoke-lab.ts:247` |
| `POST /api/companies/:param/smoke-lab/services/start` | `/companies/:companyId/smoke-lab/services/start` | `routes/smoke-lab.ts:122` |
| `POST /api/companies/:param/smoke-lab/services/stop` | `/companies/:companyId/smoke-lab/services/stop` | `routes/smoke-lab.ts:143` |
| `POST /api/companies/:param/status-cards` | `/companies/:companyId/status-cards` | `routes/status-cards.ts:150` |
| `POST /api/companies/:param/tools/applications` | `/companies/:companyId/tools/applications` | `routes/tool-access.ts:471` |
| `POST /api/companies/:param/tools/apps/:param/finish` | `/companies/:companyId/tools/apps/:connectionId/finish` | `routes/tool-access.ts:364` |
| `POST /api/companies/:param/tools/apps/connect` | `/companies/:companyId/tools/apps/connect` | `routes/tool-access.ts:255` |
| `POST /api/companies/:param/tools/connections` | `/companies/:companyId/tools/connections` | `routes/tool-access.ts:534` |
| `POST /api/companies/:param/tools/examples/:param/install` | `/companies/:companyId/tools/examples/:id/install` | `routes/tool-access.ts:413` |
| `POST /api/companies/:param/tools/examples/:param/smoke` | `/companies/:companyId/tools/examples/:id/smoke` | `routes/tool-access.ts:435` |
| `POST /api/companies/:param/tools/mcp/import-json` | `/companies/:companyId/tools/mcp/import-json` | `routes/tool-access.ts:1347` |
| `POST /api/companies/:param/tools/policies/:param/duplicate` | `/companies/:companyId/tools/policies/:policyId/duplicate` | `routes/tool-access.ts:1176` |
| `POST /api/companies/:param/tools/policies/reorder` | `/companies/:companyId/tools/policies/reorder` | `routes/tool-access.ts:1137` |
| `POST /api/companies/:param/tools/policy/test` | `/companies/:companyId/tools/policy/test` | `routes/tool-access.ts:1364` |
| `POST /api/companies/:param/tools/profiles` | `/companies/:companyId/tools/profiles` | `routes/tool-access.ts:882` |
| `POST /api/companies/:param/tools/runtime-slots/:param/restart` | `/companies/:companyId/tools/runtime-slots/:id/restart` | `routes/tool-access.ts:1101` |
| `POST /api/companies/:param/tools/runtime-slots/:param/stop` | `/companies/:companyId/tools/runtime-slots/:id/stop` | `routes/tool-access.ts:1095` |
| `POST /api/companies/:param/tools/stdio-templates` | `/companies/:companyId/tools/stdio-templates` | `routes/tool-access.ts:1305` |
| `POST /api/companies/:param/tools/trust-rules/:param/revoke` | `/companies/:companyId/tools/trust-rules/:policyId/revoke` | `routes/tool-access.ts:1278` |
| `POST /api/dev-server/restart` | `/dev-server/restart` | `routes/health.ts:95` |
| `POST /api/import/preview` | `/import/preview` | `routes/companies.ts:417` |
| `POST /api/invites/:param/revoke` | `/invites/:inviteId/revoke` | `routes/access.ts:4090` |
| `POST /api/projects/:param/workspaces/:param/runtime-commands/:param` | `/projects/:id/workspaces/:workspaceId/runtime-commands/:action` | `routes/projects.ts:635` |
| `POST /api/projects/:param/workspaces/:param/runtime-services/:param` | `/projects/:id/workspaces/:workspaceId/runtime-services/:action` | `routes/projects.ts:634` |
| `POST /api/status-cards/:param/recompile` | `/status-cards/:id/recompile` | `routes/status-cards.ts:223` |
| `POST /api/status-cards/:param/refresh` | `/status-cards/:id/refresh` | `routes/status-cards.ts:236` |
| `POST /api/tool-connections/:param/catalog/refresh` | `/tool-connections/:connectionId/catalog/refresh` | `routes/tool-access.ts:843` |
| `POST /api/tool-connections/:param/grants/installations` | `/tool-connections/:connectionId/grants/installations` | `routes/tool-access.ts:575` |
| `POST /api/tool-connections/:param/health-check` | `/tool-connections/:connectionId/health-check` | `routes/tool-access.ts:812` |
| `POST /api/tool-connections/:param/test-calls` | `/tool-connections/:connectionId/test-calls` | `routes/tool-access.ts:696` |
| `POST /api/tool-gateway/runtime-slots/:param/restart` | `/tool-gateway/runtime-slots/:slotId/restart` | `routes/tool-gateway.ts:609` |
| `POST /api/tool-gateway/runtime-slots/:param/stop` | `/tool-gateway/runtime-slots/:slotId/stop` | `routes/tool-gateway.ts:582` |
| `POST /api/tool-profiles/:param/duplicate` | `/tool-profiles/:profileId/duplicate` | `routes/tool-access.ts:929` |
| `POST /api/tool-profiles/:param/entries` | `/tool-profiles/:profileId/entries` | `routes/tool-access.ts:995` |
| `POST /api/tool-profiles/:param/new-tools/review` | `/tool-profiles/:profileId/new-tools/review` | `routes/tool-access.ts:975` |
| `POST /api/tools/oauth/:param/start` | `/tools/oauth/:connectionId/start` | `routes/tool-access.ts:310` |
| `PUT /api/issues/:param/watchdog` | `/issues/:id/watchdog` | `routes/issues.ts:6072` |
| `PUT /api/sidebar-preferences/me` | `/sidebar-preferences/me` | `routes/sidebar-preferences.ts:27` |
| `PUT /api/status-cards/:param/query` | `/status-cards/:id/query` | `routes/status-cards.ts:271` |
| `PUT /api/status-cards/:param/summary` | `/status-cards/:id/summary` | `routes/status-cards.ts:290` |

## 3. Parrot-only（Parrot 扩展端点）

| Parrot Endpoint | Source |
|---|---|
| `DELETE /api/comments/:param` | `issue_comments.rs` |
| `DELETE /api/companies/:param` | `companies.rs` |
| `DELETE /api/companies/:param/decision-queues/:param/items/:param/:param` | `decisions.rs` |
| `DELETE /api/companies/:param/skills/:param/files` | `skills.rs` |
| `DELETE /api/mcp/gateways/:param` | `tools.rs` |
| `DELETE /api/routines/:param` | `routines.rs` |
| `DELETE /api/tool-gateway/gateways/:param/mcp` | `tools.rs` |
| `DELETE /api/tool-gateway/mcp` | `tools.rs` |
| `GET /api/agents/:param/config-revisions/:param/diff` | `config_revisions.rs` |
| `GET /api/api/admin/users` | `user_directory.rs` |
| `GET /api/api/board-claim/:param` | `access_control.rs` |
| `GET /api/api/cli-auth/challenges/:param` | `access_control.rs` |
| `GET /api/cases/:param/detail` | `cases.rs` |
| `GET /api/cloud-upstreams` | `cloud_upstreams.rs` |
| `GET /api/cloud-upstreams/:param/push-runs/:param` | `cloud_upstreams.rs` |
| `GET /api/comments/:param` | `issue_comments.rs` |
| `GET /api/companies` | `companies.rs` |
| `GET /api/companies/:param` | `companies.rs` |
| `GET /api/companies/:param/adapters` | `adapters.rs` |
| `GET /api/companies/:param/adapters/:param` | `adapters.rs` |
| `GET /api/companies/:param/artifacts` | `companies.rs` |
| `GET /api/companies/:param/budgets/invocation-block/:param` | `costs.rs` |
| `GET /api/companies/:param/budgets/policies` | `costs.rs` |
| `GET /api/companies/:param/events/:param` | `sse.rs` |
| `GET /api/companies/:param/events/:param/stats` | `sse.rs` |
| `GET /api/companies/:param/events/ws` | `websocket.rs` |
| `GET /api/companies/:param/feedback-traces` | `companies.rs` |
| `GET /api/companies/:param/issues/:param/watchdog` | `watchdogs.rs` |
| `GET /api/companies/:param/issues/search` | `issues.rs` |
| `GET /api/companies/:param/me/user-secrets/:param` | `user_secret_definitions.rs` |
| `GET /api/companies/:param/me/user-secrets/:param/bindings` | `user_secret_definitions.rs` |
| `GET /api/companies/:param/org-chart.svg` | `org_chart.rs` |
| `GET /api/companies/:param/teams-catalog` | `companies.rs` |
| `GET /api/companies/:param/timeline` | `companies.rs` |
| `GET /api/companies/:param/user-secret-definitions/:param` | `user_secret_definitions.rs` |
| `GET /api/companies/stats` | `companies.rs` |
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
| `GET /api/plugins/:param/ui/*file_path` | `plugins.rs` |
| `GET /api/routines/:param/triggers` | `routines.rs` |
| `GET /api/runs/:param` | `routines.rs` |
| `GET /api/secrets/:param` | `secrets.rs` |
| `GET /api/secrets/:param/bindings` | `user_secret_definitions.rs` |
| `GET /api/skills/catalog/files` | `skills.rs` |
| `GET /api/tool-gateway/mcp` | `tools.rs` |
| `PATCH /api/api/companies/:param/members/:param` | `access_control.rs` |
| `PATCH /api/api/companies/:param/members/:param/role-and-grants` | `access_control.rs` |
| `PATCH /api/cases/:param/advance` | `cases.rs` |
| `PATCH /api/cases/:param/documents/:param/annotations/:param` | `cases.rs` |
| `PATCH /api/companies/:param` | `companies.rs` |
| `PATCH /api/companies/:param/branding` | `companies.rs` |
| `PATCH /api/companies/:param/decision-retention/:param/:param` | `decisions.rs` |
| `PATCH /api/companies/:param/me/user-secrets/:param` | `user_secret_definitions.rs` |
| `PATCH /api/companies/:param/members/:param/permissions` | `companies.rs` |
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
| `POST /api/admin/users/:param/demote-instance-admin` | `auth.rs` |
| `POST /api/admin/users/:param/promote-instance-admin` | `auth.rs` |
| `POST /api/agents/:param/skills/sync` | `agents.rs` |
| `POST /api/api/admin/users/:param/demote-instance-admin` | `access_control.rs` |
| `POST /api/api/admin/users/:param/promote-instance-admin` | `access_control.rs` |
| `POST /api/api/board-claim/:param` | `access_control.rs` |
| `POST /api/api/cli-auth/challenges/:param/approve` | `access_control.rs` |
| `POST /api/api/cli-auth/challenges/:param/cancel` | `access_control.rs` |
| `POST /api/api/companies/:param/join-requests/:param/approve` | `access_control.rs` |
| `POST /api/api/companies/:param/join-requests/:param/reject` | `access_control.rs` |
| `POST /api/api/companies/:param/members/:param/archive` | `access_control.rs` |
| `POST /api/approvals/:param/request-revision` | `approvals.rs` |
| `POST /api/auth/sign-in/email` | `auth.rs` |
| `POST /api/auth/sign-out` | `auth.rs` |
| `POST /api/auth/sign-up/email` | `auth.rs` |
| `POST /api/board-api-keys` | `access_control.rs` |
| `POST /api/cases/:param/automation/retry-plan` | `cases.rs` |
| `POST /api/cases/:param/documents/:param` | `cases.rs` |
| `POST /api/cases/:param/documents/:param/annotations` | `cases.rs` |
| `POST /api/cases/:param/documents/:param/annotations/:param/reply` | `cases.rs` |
| `POST /api/cases/:param/terminal` | `cases.rs` |
| `POST /api/cli-auth/challenges` | `access_control.rs` |
| `POST /api/cloud-upstreams/:param/push-runs` | `cloud_upstreams.rs` |
| `POST /api/cloud-upstreams/:param/push-runs/:param/activation` | `cloud_upstreams.rs` |
| `POST /api/cloud-upstreams/:param/push-runs/:param/cancel` | `cloud_upstreams.rs` |
| `POST /api/cloud-upstreams/:param/push-runs/preview` | `cloud_upstreams.rs` |
| `POST /api/cloud-upstreams/connect/finish` | `cloud_upstreams.rs` |
| `POST /api/cloud-upstreams/connect/start` | `cloud_upstreams.rs` |
| `POST /api/companies` | `companies.rs` |
| `POST /api/companies/:param/adapters/:param/detect-model` | `adapters.rs` |
| `POST /api/companies/:param/adapters/:param/test-environment` | `adapters.rs` |
| `POST /api/companies/:param/archive` | `companies.rs` |
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
| `POST /api/companies/:param/export` | `companies.rs` |
| `POST /api/companies/:param/exports` | `companies.rs` |
| `POST /api/companies/:param/exports/preview` | `companies.rs` |
| `POST /api/companies/:param/imports/apply` | `companies.rs` |
| `POST /api/companies/:param/imports/preview` | `companies.rs` |
| `POST /api/companies/:param/inbox-dismissals` | `companies.rs` |
| `POST /api/companies/:param/invites` | `access_control.rs` |
| `POST /api/companies/:param/issues/:param/watchdog` | `watchdogs.rs` |
| `POST /api/companies/:param/issues/:param/watchdog/evaluate` | `watchdogs.rs` |
| `POST /api/companies/:param/issues/batch-update` | `issues.rs` |
| `POST /api/companies/:param/me/user-secrets` | `user_secret_definitions.rs` |
| `POST /api/companies/:param/me/user-secrets/:param/rotate` | `user_secret_definitions.rs` |
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
| `PUT /api/companies/:param/watchdogs/:param/status` | `watchdogs.rs` |
