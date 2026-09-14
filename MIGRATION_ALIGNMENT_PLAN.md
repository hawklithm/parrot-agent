# Migration Alignment Plan (Parrot ↔ Paperclip)

自动生成：`scripts/plan_migration_alignment.py`。
规则：**Paperclip 的 company_id FK onDelete 怎么设计，Parrot 就怎么设计**。

- Parrot 含 company_id 的表: **136**
- ADD_CASCADE: **4**
- CHANGE_TO_RESTRICT: **26**
- MATCH_RESTRICT: **16**
- OK: **27**
- REVIEW: **63**

| Parrot table | migration | Parrot 当前 cascade | Paperclip 设计 | 动作 | 说明 |
|---|---|---|---|---|---|
| `activity_log` | `14_add_priority_and_ended_at.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `activity_logs` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `agent_memberships` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `agent_runtime_states` | `26_create_agent_runtime_states.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `agent_wakeup_requests` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `agents` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `annotation_threads` | `12_create_documents_and_versions.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `approvals` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `assets` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `attachments` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `board_api_keys` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `budget_incidents` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `budget_policies` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `builtin_managed_resources` | `48_add_builtin_managed_resources.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_attachments` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_documents` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_events` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_issue_links` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `case_labels` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `cases` | `00_init_schema_unified.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `claude_setup_token_sessions` | `27_add_claude_setup_token_sessions.sql` | yes | cascade | OK | Paperclip=cascade |
| `cloud_upstream_connections` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `cloud_upstream_runs` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `company_memberships` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `company_secret_bindings` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `company_secret_proposals` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `company_secret_provider_configs` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `company_secrets` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `company_skill_policies` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `company_skills` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `company_team_installs` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `company_user_sidebar_preferences` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `connection_grants` | `20_add_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `connection_token_issuances` | `67_create_connection_token_issuances.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_archive_notification_outbox` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_bundles` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `decision_effect_executions` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_proposals` | `13_add_missing_tables_and_columns.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_queue_items` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_queues` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `decision_retention` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_target_issues` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_training_examples` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `decision_triage` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decision_triage_events` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `decisions` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `document_annotation_comments` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `document_annotation_threads` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `documents` | `12_create_documents_and_versions.sql` | yes | cascade | OK | Paperclip=cascade |
| `environment_leases` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `environments` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `execution_workspaces` | `00_init_schema_unified.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `external_object_mentions` | `39_add_external_object_references.sql` | yes | cascade | OK | Paperclip=cascade |
| `external_objects` | `39_add_external_object_references.sql` | yes | cascade | OK | Paperclip=cascade |
| `feedback_traces` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `feedback_votes` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `finance_events` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `folders` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `goals` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `heartbeat_run_watchdog_decisions` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `heartbeat_runs` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `inbox_dismissals` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `invites` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_comments` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_create_idempotency_keys` | `37_add_issue_create_idempotency_keys.sql` | yes | cascade | OK | Paperclip=cascade |
| `issue_documents` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_inbox_archives` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_labels` | `00_init_schema_unified.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `issue_plan_decompositions` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_read_status` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `issue_recovery_actions` | `77_align_issue_recovery_actions.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_relations` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_thread_interactions` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `issue_tree_hold_members` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_tree_holds` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issue_watchdogs` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `issue_work_products` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `issues` | `00_init_schema_unified.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
| `join_requests` | `00_init_schema_unified.sql` | yes | restrict | CHANGE_TO_RESTRICT | Paperclip=restrict |
| `labels` | `00_init_schema_unified.sql` | no | cascade | ADD_CASCADE | Paperclip=cascade |
| `pipeline_cases` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `pipeline_logs` | `63_create_pipeline_runtime_tables.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `pipeline_runs` | `63_create_pipeline_runtime_tables.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `pipeline_triggers` | `63_create_pipeline_runtime_tables.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `pipelines` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `plan_decompositions` | `00_init_schema_unified.sql` | no | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `plugin_managed_resources` | `10_create_plugin_managed_resources.sql` | yes | cascade | OK | Paperclip=cascade |
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
| `tool_action_requests` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_applications` | `20_add_tool_access_core.sql` | yes | cascade | OK | Paperclip=cascade |
| `tool_call_events` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_catalog_entries` | `50_add_tool_catalog_entries.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_connection_grants` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_connection_installs` | `20_add_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_connections` | `20_add_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_gateway_sessions` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_invocations` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_mcp_gateway_tokens` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_mcp_gateways` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_oauth_states` | `20_add_tool_access_core.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_policies` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_profile_bindings` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_profiles` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_rate_limit_counters` | `79_align_tool_rate_limit_counters.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_runtime_slots` | `51_add_tool_runtime_slots.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `tool_stdio_command_templates` | `66_add_tool_stdio_command_templates.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `user_inbox_agent_policies` | `31_add_user_inbox_agent_policies.sql` | yes | cascade | OK | Paperclip=cascade |
| `user_preferences` | `00_init_schema_unified.sql` | yes | n/a | REVIEW | Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认 |
| `user_secret_declarations` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `user_secret_definitions` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `workspace_operations` | `00_init_schema_unified.sql` | yes | cascade | OK | Paperclip=cascade |
| `workspace_runtime_services` | `53_add_workspace_runtime_services.sql` | no | restrict | MATCH_RESTRICT | Paperclip=restrict |
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
| `plugin_jobs` | `00_init_schema_unified.sql` | `plugin_jobs` |
| `plugin_logs` | `00_init_schema_unified.sql` | `plugin_logs` |
| `project_workspaces` | `00_init_schema_unified.sql` | `project_workspaces` |

> 加 `company_id uuid NOT NULL REFERENCES companies(id) ON DELETE NO ACTION` 需先回填（按各表父链路推导 company_id），本迁移不自动生成，逐表人工设计后补 migration。
