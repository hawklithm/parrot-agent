# Migration Alignment Plan (Parrot ↔ Paperclip)

自动生成：`scripts/plan_migration_alignment.py`。
规则：**Paperclip 的 company_id FK onDelete 怎么设计，Parrot 就怎么设计**。

- Parrot 含 company_id 的表: **142**
- ADD_CASCADE: **3**
- CHANGE_TO_RESTRICT: **27**
- MATCH_RESTRICT: **14**
- OK: **29**
- REVIEW: **69**

| Parrot table | migration | Parrot 当前 cascade | Paperclip 设计 | 动作 | 说明 |
|---|---|---|---|---|---|
| `activity_log` | `14_add_priority_and_ended_at.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `activity_logs` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `agent_memberships` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `agent_runtime_states` | `20_create_agent_runtime_states.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `agent_wakeup_requests` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `agents` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `annotation_threads` | `12_create_documents_and_versions.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `approvals` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `assets` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `attachments` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `board_api_keys` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `budget_incidents` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `budget_policies` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `case_attachments` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_documents` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_events` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_issue_links` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_labels` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `cases` | `00_init_schema_unified.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `cloud_upstream_connections` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `cloud_upstream_runs` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `company_memberships` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `company_secret_bindings` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `company_secret_proposals` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `company_secret_provider_configs` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `company_secrets` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `company_skill_comments` | `22_add_company_skills_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `company_skill_policies` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `company_skill_stars` | `22_add_company_skills_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `company_skill_test_inputs` | `22_add_company_skills_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `company_skill_test_run_templates` | `22_add_company_skills_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `company_skill_test_runs` | `22_add_company_skills_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `company_skill_versions` | `22_add_company_skills_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `company_skills` | `22_add_company_skills_system.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `company_team_installs` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `company_user_sidebar_preferences` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `connection_grants` | `20_add_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `connection_token_issuances` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_archive_notification_outbox` | `24_add_decision_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_bundles` | `24_add_decision_system.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `decision_proposals` | `13_add_missing_tables_and_columns.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_queue_items` | `24_add_decision_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_queues` | `24_add_decision_system.sql` | yes | cascade | OK | Paperclip=cascade |
| `decision_retention` | `24_add_decision_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_target_issues` | `24_add_decision_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_training_examples` | `24_add_decision_system.sql` | yes | cascade | OK | Paperclip=cascade |
| `decision_triage` | `24_add_decision_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_triage_events` | `24_add_decision_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decisions` | `24_add_decision_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `document_annotation_comments` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `document_annotation_threads` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `documents` | `12_create_documents_and_versions.sql` | yes | cascade | OK | Paperclip=cascade |
| `environment_leases` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `environments` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `execution_workspace_runtime_leases` | `25_add_issue_execution_system.sql` | yes | cascade | OK | Paperclip=cascade |
| `execution_workspaces` | `25_add_issue_execution_system.sql` | yes | cascade | OK | Paperclip=cascade |
| `feedback_traces` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `feedback_votes` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `finance_events` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `folders` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `goals` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `heartbeat_run_watchdog_decisions` | `24_add_decision_system.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `heartbeat_runs` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `inbox_dismissals` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `invites` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_comments` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_documents` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_execution_decisions` | `24_add_decision_system.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_inbox_archives` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_labels` | `00_init_schema_unified.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `issue_plan_decompositions` | `25_add_issue_execution_system.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_read_status` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `issue_relations` | `20260818000001_create_issue_relations.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_thread_interactions` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_tree_hold_members` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_tree_holds` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_watchdogs` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `issue_work_products` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issues` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `join_requests` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `labels` | `00_init_schema_unified.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `pipeline_cases` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `pipelines` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `plan_decompositions` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `plugin_company_settings` | `23_add_plugin_system.sql` | yes | cascade | OK | Paperclip=cascade |
| `plugin_config` | `23_add_plugin_system.sql` | yes | cascade | OK | Paperclip=cascade |
| `plugin_entities` | `23_add_plugin_system.sql` | yes | cascade | OK | Paperclip=cascade |
| `plugin_job_runs` | `23_add_plugin_system.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `plugin_logs` | `23_add_plugin_system.sql` | yes | cascade | OK | Paperclip=cascade |
| `plugin_managed_resources` | `23_add_plugin_system.sql` | yes | cascade | OK | Paperclip=cascade |
| `plugin_webhook_deliveries` | `23_add_plugin_system.sql` | yes | cascade | OK | Paperclip=cascade |
| `principal_permission_grants` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `project_goals` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `project_memberships` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `projects` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `recovery_actions` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `routine_documents` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `routine_revisions` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `routine_runs` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `routine_triggers` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `routines` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `runs` | `13_add_missing_tables_and_columns.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `secret_access_events` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `skill_comments` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_files` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_stars` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_test_inputs` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_test_run_templates` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_test_runs` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `skill_versions` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `smoke_run_steps` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `smoke_runs` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `status_cards` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `summary_slots` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `thread_interactions` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_access_audit_events` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_action_requests` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_applications` | `20_add_tool_access_core.sql` | yes | cascade | OK | Paperclip=cascade |
| `tool_call_events` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_catalog_entries` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_connection_grants` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_connection_installs` | `20_add_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_connections` | `20_add_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_gateway_rate_limit_counters` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_gateway_sessions` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_invocations` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_mcp_gateway_tokens` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_mcp_gateways` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_oauth_states` | `20_add_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_policies` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_profile_bindings` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_profile_entries` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_profiles` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_rate_limit_counters` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_runtime_metric_counters` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_runtime_slots` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_stdio_command_templates` | `21_add_tool_access_remaining.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `user_preferences` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `user_secret_declarations` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `user_secret_definitions` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `workspace_operations` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `workspaces` | `13_add_missing_tables_and_columns.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |

## 动作说明

- **ADD_CASCADE**：Paperclip 为 cascade 而 Parrot 缺 → 需新增 migration（DROP+ADD constraint）。
- **MATCH_RESTRICT**：两边都是 restrict/无 cascade → 无需改动。
- **CHANGE_TO_RESTRICT**：Parrot 是 cascade 而 Paperclip 是 restrict → 需移除 cascade（按 Paperclip 对齐）。
- **REVIEW**：Parrot-only 或 Paperclip 无直接 company_id FK → 人工确认。

## 5. #4：Parrot 缺 company_id 而 Paperclip 对应表有 company_id（需补列+回填）

| Parrot table | migration | Paperclip 对应 |
|---|---|---|
| `agent_api_keys` | `00_init_schema_unified.sql` | `agent_api_keys` |
| `agent_config_revisions` | `00_init_schema_unified.sql` | `agent_config_revisions` |
| `approval_comments` | `00_init_schema_unified.sql` | `approval_comments` |
| `cost_events` | `00_init_schema_unified.sql` | `cost_events` |
| `document_revisions` | `00_init_schema_unified.sql` | `document_revisions` |
| `issue_approvals` | `00_init_schema_unified.sql` | `issue_approvals` |
| `pipeline_case_events` | `00_init_schema_unified.sql` | `pipeline_case_events` |
| `plugin_jobs` | `23_add_plugin_system.sql` | `plugin_jobs` |
| `project_workspaces` | `00_init_schema_unified.sql` | `project_workspaces` |

> 加 `company_id uuid NOT NULL REFERENCES companies(id) ON DELETE NO ACTION` 需先回填（按各表父链路推导 company_id），本迁移不自动生成，逐表人工设计后补 migration。
