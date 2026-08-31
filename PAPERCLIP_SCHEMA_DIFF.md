# Paperclip ↔ Parrot Schema Diff

自动生成：`scripts/diff_schema.py`
Paperclip schema dir: `/mnt/d/workspace/paperclip/packages/db/src/schema`
Parrot migrations dir: `/mnt/d/workspace/parrot/parrot-agent/migrations`

## 统计

| 来源 | 表数量 |
|---|---|
| Paperclip 声明 | 173 |
| Parrot 创建 | 178 |
| 共有 | 132 |
| Paperclip 独有（缺失） | 41 |
| Parrot 独有（扩展） | 46 |

## Paperclip 独有表（Parrot 缺失）

| 表名 | Paperclip 文件 | 备注 |
|---|---|---|
| `account` | `auth.ts` |  |
| `adapter_auth_sessions` | `adapter_auth_sessions.ts` |  |
| `agent_runtime_state` | `agent_runtime_state.ts` |  |
| `agent_task_sessions` | `agent_task_sessions.ts` |  |
| `built_in_managed_resources` | `built_in_managed_resources.ts` |  |
| `company_logos` | `company_logos.ts` |  |
| `company_onboarding_seeds` | `company_onboarding_seeds.ts` |  |
| `company_skill_comments` | `company_skills.ts` |  |
| `company_skill_stars` | `company_skills.ts` |  |
| `company_skill_test_inputs` | `company_skills.ts` |  |
| `company_skill_test_run_templates` | `company_skills.ts` |  |
| `company_skill_test_runs` | `company_skills.ts` |  |
| `company_skill_versions` | `company_skills.ts` |  |
| `company_transfer_runs` | `company_transfer_runs.ts` |  |
| `document_annotation_anchor_snapshots` | `document_annotation_anchor_snapshots.ts` |  |
| `document_memberships` | `document_memberships.ts` |  |
| `execution_workspace_runtime_leases` | `execution_workspace_runtime_leases.ts` |  |
| `feedback_exports` | `feedback_exports.ts` |  |
| `heartbeat_run_events` | `heartbeat_run_events.ts` |  |
| `issue_attachments` | `issue_attachments.ts` |  |
| `issue_execution_decisions` | `issue_execution_decisions.ts` |  |
| `issue_read_states` | `issue_read_states.ts` |  |
| `issue_reference_mentions` | `issue_reference_mentions.ts` |  |
| `pipeline_automation_executions` | `pipeline_cases.ts` |  |
| `pipeline_case_blockers` | `pipeline_cases.ts` |  |
| `pipeline_case_documents` | `pipeline_cases.ts` |  |
| `pipeline_case_issue_links` | `pipeline_cases.ts` |  |
| `pipeline_documents` | `pipeline_cases.ts` |  |
| `plugin_company_settings` | `plugin_company_settings.ts` |  |
| `plugin_config` | `plugin_config.ts` |  |
| `plugin_database_namespaces` | `plugin_database.ts` |  |
| `plugin_entities` | `plugin_entities.ts` |  |
| `plugin_migrations` | `plugin_database.ts` |  |
| `plugin_state` | `plugin_state.ts` |  |
| `plugin_webhook_deliveries` | `plugin_webhooks.ts` |  |
| `session` | `auth.ts` |  |
| `tool_access_audit_events` | `tool_access.ts` |  |
| `tool_gateway_rate_limit_counters` | `tool_access.ts` |  |
| `tool_runtime_metric_counters` | `tool_access.ts` |  |
| `user` | `auth.ts` |  |
| `verification` | `auth.ts` |  |

## Parrot 独有表（Parrot 扩展，Paperclip 无对应）

| 表名 | 首次迁移 |
|---|---|
| `activity_logs` | `00_init_schema_unified.sql` |
| `agent_runtime_states` | `26_create_agent_runtime_states.sql` |
| `agent_start_locks` | `59_create_agent_start_locks.sql` |
| `annotation_comments` | `00_init_schema_unified.sql` |
| `annotation_threads` | `00_init_schema_unified.sql` |
| `attachments` | `00_init_schema_unified.sql` |
| `auth_sessions` | `00_init_schema_unified.sql` |
| `auth_users` | `00_init_schema_unified.sql` |
| `builtin_managed_resources` | `48_add_builtin_managed_resources.sql` |
| `cloud_upstream_connections` | `00_init_schema_unified.sql` |
| `cloud_upstream_runs` | `00_init_schema_unified.sql` |
| `company_team_installs` | `00_init_schema_unified.sql` |
| `decision_proposals` | `13_add_missing_tables_and_columns.sql` |
| `document_versions` | `12_create_documents_and_versions.sql` |
| `feedback_traces` | `00_init_schema_unified.sql` |
| `folder_items` | `00_init_schema_unified.sql` |
| `instruction_templates` | `00_init_schema_unified.sql` |
| `issue_read_status` | `00_init_schema_unified.sql` |
| `pipeline_case_outputs` | `63_create_pipeline_runtime_tables.sql` |
| `pipeline_logs` | `63_create_pipeline_runtime_tables.sql` |
| `pipeline_runs` | `63_create_pipeline_runtime_tables.sql` |
| `pipeline_triggers` | `63_create_pipeline_runtime_tables.sql` |
| `plan_decompositions` | `00_init_schema_unified.sql` |
| `plugin_data` | `00_init_schema_unified.sql` |
| `recovery_actions` | `00_init_schema_unified.sql` |
| `run_continuations` | `60_create_run_continuations.sql` |
| `runs` | `13_add_missing_tables_and_columns.sql` |
| `scheduler_job_executions` | `42_scheduler_job_executions.sql` |
| `scheduler_job_leases` | `41_scheduler_job_leases.sql` |
| `skill_catalogs` | `00_init_schema_unified.sql` |
| `skill_comments` | `00_init_schema_unified.sql` |
| `skill_files` | `00_init_schema_unified.sql` |
| `skill_stars` | `00_init_schema_unified.sql` |
| `skill_test_inputs` | `00_init_schema_unified.sql` |
| `skill_test_run_templates` | `00_init_schema_unified.sql` |
| `skill_test_runs` | `00_init_schema_unified.sql` |
| `skill_versions` | `00_init_schema_unified.sql` |
| `status_card_summary_revisions` | `00_init_schema_unified.sql` |
| `status_card_update_runs` | `00_init_schema_unified.sql` |
| `summary_slot_revisions` | `00_init_schema_unified.sql` |
| `thread_interactions` | `00_init_schema_unified.sql` |
| `tool_connection_grants` | `00_init_schema_unified.sql` |
| `user_preferences` | `00_init_schema_unified.sql` |
| `workspace_cleanup_tasks` | `28_add_workspace_cleanup_tasks.sql` |
| `workspace_state_snapshots` | `13_add_missing_tables_and_columns.sql` |
| `workspaces` | `13_add_missing_tables_and_columns.sql` |

## 共有表

共 132 张表在两者中都有定义。

## 说明

- 本对比只检查表级别存在性。列级、索引级和约束级差异需要逐表详细分析。
- Parrot 的 `00_init_schema_unified.sql` 是统一基线，包含大部分核心表。
- Paperclip 使用 Drizzle ORM schema 定义；Parrot 使用纯 SQL 迁移。
- Parrot 独有表可能是 Paperclip 上线后的新功能，或是 Parrot 自定义扩展。
- Paperclip 独有且 Parrot 缺失的表需要手动判断是否属于当前迁移范围。
