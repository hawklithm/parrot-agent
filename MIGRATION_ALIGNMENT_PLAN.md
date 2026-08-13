# Migration Alignment Plan (Parrot ↔ Paperclip)

自动生成：`scripts/plan_migration_alignment.py`。
规则：**Paperclip 的 company_id FK onDelete 怎么设计，Parrot 就怎么设计**。

- Parrot 含 company_id 的表: **103**
- ADD_CASCADE: **5**
- CHANGE_TO_RESTRICT: **22**
- MATCH_RESTRICT: **14**
- OK: **16**
- REVIEW: **46**

| Parrot table | migration | Parrot 当前 cascade | Paperclip 设计 | 动作 | 说明 |
|---|---|---|---|---|---|
| `activity_logs` | `003_create_activity_logs.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `agent_memberships` | `20260808000004_create_agent_memberships.sql` | yes | cascade | OK | Paperclip=cascade |
| `agent_wakeup_requests` | `20260712000004_create_agent_wakeup_requests.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `agents` | `20260711000001_create_agents.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `annotation_threads` | `20260711000002_create_issues.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `approvals` | `20260711000011_create_approvals.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `assets` | `20260712000009_create_assets.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `attachments` | `20260711000002_create_issues.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `board_api_keys` | `20260711000014_create_board_api_keys.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `budget_incidents` | `20260719000002_create_budget_and_finance_tables.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `budget_policies` | `20260719000002_create_budget_and_finance_tables.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `case_attachments` | `20260711000004_create_cases.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_documents` | `20260711000004_create_cases.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_events` | `20260711000004_create_cases.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_issue_links` | `20260711000004_create_cases.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_labels` | `20260711000004_create_cases.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `cases` | `20260711000004_create_cases.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `cloud_upstream_connections` | `20260719000006_create_cloud_upstreams.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `cloud_upstream_runs` | `20260719000006_create_cloud_upstreams.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `company_memberships` | `001_create_companies.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `company_secret_bindings` | `20260712000012_create_company_secret_bindings.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `company_secret_provider_configs` | `20260712000010_create_company_secret_provider_configs.sql` | yes | cascade | OK | Paperclip=cascade |
| `company_secrets` | `20260711000022_create_company_secrets.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `company_skill_policies` | `20260813000005_create_company_skill_policies.sql` | yes | cascade | OK | Paperclip=cascade |
| `company_skills` | `20260712000017_create_skill_tables.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `company_team_installs` | `20260813000006_create_company_team_installs.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_archive_notification_outbox` | `20260813000003_create_decision_queues.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_bundles` | `20260813000002_create_decisions.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `decision_effect_executions` | `20260813000002_create_decisions.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_queue_items` | `20260813000003_create_decision_queues.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_queues` | `20260813000003_create_decision_queues.sql` | yes | cascade | OK | Paperclip=cascade |
| `decision_retention` | `20260813000003_create_decision_queues.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_target_issues` | `20260813000002_create_decisions.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_training_examples` | `20260813000004_create_decision_training_examples.sql` | yes | cascade | OK | Paperclip=cascade |
| `decision_triage` | `20260813000003_create_decision_queues.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_triage_events` | `20260813000003_create_decision_queues.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decisions` | `20260813000002_create_decisions.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `document_annotation_comments` | `20260728000005_create_routine_document_annotations.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `document_annotation_threads` | `20260728000005_create_routine_document_annotations.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `documents` | `20260711000002_create_issues.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `environment_leases` | `20260712000008_create_environment_leases.sql` | yes | cascade | OK | Paperclip=cascade |
| `environments` | `20260712000006_create_environments.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `execution_workspaces` | `20260712000007_create_execution_workspaces.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `feedback_traces` | `20260712000001_create_issue_read_status_and_archive.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `feedback_votes` | `20260712000001_create_issue_read_status_and_archive.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `finance_events` | `20260719000002_create_budget_and_finance_tables.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `folders` | `20260722000003_create_folders.sql` | yes | cascade | OK | Paperclip=cascade |
| `goals` | `20260711000010_create_goals.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `heartbeat_run_watchdog_decisions` | `20260813000001_create_heartbeat_run_watchdog_decisions.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `heartbeat_runs` | `20260712000002_create_heartbeat_runs.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `invites` | `20260711000016_create_invites_join_requests.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_comments` | `20260711000002_create_issues.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_documents` | `20260711000002_create_issues.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_inbox_archives` | `20260712000001_create_issue_read_status_and_archive.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_labels` | `20260711000002_create_issues.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `issue_plan_decompositions` | `20260808000006_create_issue_plan_decompositions.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_read_status` | `20260712000001_create_issue_read_status_and_archive.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `issue_relations` | `20260805000004_create_issue_relations.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_thread_interactions` | `20260712000005_create_issue_thread_interactions.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_tree_hold_members` | `20260711000003_create_issue_tree_control.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_tree_holds` | `20260711000003_create_issue_tree_control.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_watchdogs` | `20260712000003_create_issue_watchdogs.sql` | yes | cascade | OK | Paperclip=cascade |
| `issue_work_products` | `20260711000002_create_issues.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issues` | `20260711000002_create_issues.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `join_requests` | `20260711000016_create_invites_join_requests.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `labels` | `20260711000002_create_issues.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `pipeline_cases` | `20260711000021_create_pipeline_cases.sql` | yes | cascade | OK | Paperclip=cascade |
| `pipelines` | `20260711000018_create_pipelines.sql` | yes | cascade | OK | Paperclip=cascade |
| `plan_decompositions` | `20260712000001_create_issue_read_status_and_archive.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `plugin_managed_resources` | `20260808000002_create_plugin_managed_resources.sql` | yes | cascade | OK | Paperclip=cascade |
| `principal_permission_grants` | `20260711000015_create_permission_grants.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `project_goals` | `20260808000001_create_project_goals.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `project_memberships` | `20260808000003_create_project_memberships.sql` | yes | cascade | OK | Paperclip=cascade |
| `projects` | `002_create_projects.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `recovery_actions` | `20260712000001_create_issue_read_status_and_archive.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `routine_documents` | `20260728000005_create_routine_document_annotations.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `routine_revisions` | `20260711000008_create_routine_revisions.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `routine_runs` | `20260711000009_create_routine_runs.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `routine_triggers` | `20260711000007_create_routine_triggers.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `routines` | `20260711000006_create_routines.sql` | yes | cascade | OK | Paperclip=cascade |
| `secret_access_events` | `20260712000016_create_secret_access_events.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `skill_comments` | `20260712000017_create_skill_tables.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_files` | `20260712000017_create_skill_tables.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_stars` | `20260712000017_create_skill_tables.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_test_inputs` | `20260712000017_create_skill_tables.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_test_run_templates` | `20260712000017_create_skill_tables.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_test_runs` | `20260712000017_create_skill_tables.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_versions` | `20260712000017_create_skill_tables.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `thread_interactions` | `20260711000002_create_issues.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_action_requests` | `20260728000001_create_tool_invocation_audit.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_call_events` | `20260728000001_create_tool_invocation_audit.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_connections` | `20260722000001_create_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_gateway_sessions` | `20260728000001_create_tool_invocation_audit.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_invocations` | `20260728000001_create_tool_invocation_audit.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_mcp_gateway_tokens` | `20260728000002_create_named_mcp_gateways.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_mcp_gateways` | `20260728000002_create_named_mcp_gateways.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_policies` | `20260722000001_create_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_profile_bindings` | `20260722000001_create_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_profiles` | `20260722000001_create_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `user_preferences` | `20260719000009_create_user_preferences.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `user_secret_declarations` | `20260712000015_create_user_secret_declarations.sql` | yes | cascade | OK | Paperclip=cascade |
| `user_secret_definitions` | `20260712000014_create_user_secret_definitions.sql` | yes | cascade | OK | Paperclip=cascade |
| `workspace_operations` | `20260728000003_create_workspace_operations.sql` | yes | cascade | OK | Paperclip=cascade |

## 动作说明

- **ADD_CASCADE**：Paperclip 为 cascade 而 Parrot 缺 → 需新增 migration（DROP+ADD constraint）。
- **MATCH_RESTRICT**：两边都是 restrict/无 cascade → 无需改动。
- **CHANGE_TO_RESTRICT**：Parrot 是 cascade 而 Paperclip 是 restrict → 需移除 cascade（按 Paperclip 对齐）。
- **REVIEW**：Parrot-only 或 Paperclip 无直接 company_id FK → 人工确认。

## 5. #4：Parrot 缺 company_id 而 Paperclip 对应表有 company_id（需补列+回填）

| Parrot table | migration | Paperclip 对应 |
|---|---|---|
| `agent_api_keys` | `20260711000005_create_agent_api_keys.sql` | `agent_api_keys` |
| `agent_config_revisions` | `20260711000001_create_agents.sql` | `agent_config_revisions` |
| `approval_comments` | `20260802000001_create_approval_comments.sql` | `approval_comments` |
| `cost_events` | `20260711000001_create_agents.sql` | `cost_events` |
| `document_revisions` | `20260711000004_create_cases.sql` | `document_revisions` |
| `issue_approvals` | `20260711000011_create_approvals.sql` | `issue_approvals` |
| `pipeline_case_events` | `20260711000023_create_pipeline_case_events.sql` | `pipeline_case_events` |
| `plugin_jobs` | `20260719000007_create_plugins.sql` | `plugin_jobs` |
| `plugin_logs` | `20260719000007_create_plugins.sql` | `plugin_logs` |
| `project_workspaces` | `002_create_projects.sql` | `project_workspaces` |

> 加 `company_id uuid NOT NULL REFERENCES companies(id) ON DELETE NO ACTION` 需先回填（按各表父链路推导 company_id），本迁移不自动生成，逐表人工设计后补 migration。
