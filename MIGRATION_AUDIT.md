# Migration Audit (P2.4, static / no DB)

自动生成：`scripts/audit_migrations.py`。正则启发式，输出供人工复核。

- migrations: **106**
- CREATE TABLE: **144**（含 IF NOT EXISTS: **84**）
- 无 company_id 的业务表（候选待核）: **28**
- 有 company_id 但无索引: **39**
- 有 company_id 但无 companies FK: **0**
- company_id FK 无 ON DELETE CASCADE: **31**
- money 列（numeric/decimal/money）: **0**
- JSONB 列: **54**

## 无 company_id 的表（业务表待核，系统表已豁免）

| 内容 |
|---|
| `agent_api_keys` |
| `agent_config_revisions` |
| `annotation_comments` |
| `approval_comments` |
| `companies` |
| `company_secret_versions` |
| `cost_events` |
| `document_revisions` |
| `environment_custom_image_setup_sessions` |
| `environment_custom_image_templates` |
| `folder_items` |
| `instance_user_roles` |
| `issue_approvals` |
| `pipeline_case_events` |
| `pipeline_stages` |
| `pipeline_transitions` |
| `plugin_data` |
| `plugin_job_runs` |
| `plugin_jobs` |
| `plugin_logs` |
| `plugins` |
| `project_workspaces` |
| `skill_catalogs` |
| `status_card_summary_revisions` |
| `status_card_updates` |
| `summary_slot_revisions` |
| `tool_profile_entries` |
| `user_sidebar_preferences` |

## company_id 无索引

| 内容 |
|---|
| `thread_interactions` (20260711000002_create_issues.sql) |
| `issue_documents` (20260711000002_create_issues.sql) |
| `annotation_threads` (20260711000002_create_issues.sql) |
| `issue_labels` (20260711000002_create_issues.sql) |
| `case_events` (20260711000004_create_cases.sql) |
| `case_issue_links` (20260711000004_create_cases.sql) |
| `case_documents` (20260711000004_create_cases.sql) |
| `case_attachments` (20260711000004_create_cases.sql) |
| `case_labels` (20260711000004_create_cases.sql) |
| `skill_versions` (20260712000017_create_skill_tables.sql) |
| `skill_test_inputs` (20260712000017_create_skill_tables.sql) |
| `skill_test_run_templates` (20260712000017_create_skill_tables.sql) |
| `skill_test_runs` (20260712000017_create_skill_tables.sql) |
| `skill_stars` (20260712000017_create_skill_tables.sql) |
| `skill_comments` (20260712000017_create_skill_tables.sql) |
| `skill_files` (20260712000017_create_skill_tables.sql) |
| `cloud_upstream_runs` (20260719000006_create_cloud_upstreams.sql) |
| `user_preferences` (20260719000009_create_user_preferences.sql) |
| `tool_connections` (20260722000001_create_tool_access_core.sql) |
| `tool_profiles` (20260722000001_create_tool_access_core.sql) |
| `tool_profile_bindings` (20260722000001_create_tool_access_core.sql) |
| `tool_policies` (20260722000001_create_tool_access_core.sql) |
| `tool_invocations` (20260728000001_create_tool_invocation_audit.sql) |
| `tool_action_requests` (20260728000001_create_tool_invocation_audit.sql) |
| `tool_call_events` (20260728000001_create_tool_invocation_audit.sql) |
| `tool_gateway_sessions` (20260728000001_create_tool_invocation_audit.sql) |
| `routine_documents` (20260728000005_create_routine_document_annotations.sql) |
| `decisions` (20260813000002_create_decisions.sql) |
| `decision_target_issues` (20260813000002_create_decisions.sql) |
| `decision_effect_executions` (20260813000002_create_decisions.sql) |
| `decision_queue_items` (20260813000003_create_decision_queues.sql) |
| `decision_triage` (20260813000003_create_decision_queues.sql) |
| `decision_triage_events` (20260813000003_create_decision_queues.sql) |
| `decision_retention` (20260813000003_create_decision_queues.sql) |
| `decision_archive_notification_outbox` (20260813000003_create_decision_queues.sql) |
| `company_user_sidebar_preferences` (20260813000004_create_sidebar_preferences.sql) |
| `company_skill_policies` (20260813000005_create_company_skill_policies.sql) |
| `company_team_installs` (20260813000006_create_company_team_installs.sql) |
| `company_secret_proposals` (20260813000006_create_secret_proposals.sql) |

## company_id 无 companies FK

（无）

## company_id FK 无 ON DELETE CASCADE

| 内容 |
|---|
| `issues` (20260711000002_create_issues.sql) |
| `issue_comments` (20260711000002_create_issues.sql) |
| `thread_interactions` (20260711000002_create_issues.sql) |
| `documents` (20260711000002_create_issues.sql) |
| `issue_documents` (20260711000002_create_issues.sql) |
| `annotation_threads` (20260711000002_create_issues.sql) |
| `issue_work_products` (20260711000002_create_issues.sql) |
| `labels` (20260711000002_create_issues.sql) |
| `issue_labels` (20260711000002_create_issues.sql) |
| `attachments` (20260711000002_create_issues.sql) |
| `issue_tree_holds` (20260711000003_create_issue_tree_control.sql) |
| `issue_tree_hold_members` (20260711000003_create_issue_tree_control.sql) |
| `cases` (20260711000004_create_cases.sql) |
| `case_events` (20260711000004_create_cases.sql) |
| `case_issue_links` (20260711000004_create_cases.sql) |
| `case_documents` (20260711000004_create_cases.sql) |
| `case_attachments` (20260711000004_create_cases.sql) |
| `case_labels` (20260711000004_create_cases.sql) |
| `company_secrets` (20260711000022_create_company_secrets.sql) |
| `issue_read_status` (20260712000001_create_issue_read_status_and_archive.sql) |
| `issue_inbox_archives` (20260712000001_create_issue_read_status_and_archive.sql) |
| `feedback_votes` (20260712000001_create_issue_read_status_and_archive.sql) |
| `feedback_traces` (20260712000001_create_issue_read_status_and_archive.sql) |
| `recovery_actions` (20260712000001_create_issue_read_status_and_archive.sql) |
| `plan_decompositions` (20260712000001_create_issue_read_status_and_archive.sql) |
| `execution_workspaces` (20260712000007_create_execution_workspaces.sql) |
| `assets` (20260712000009_create_assets.sql) |
| `company_secret_bindings` (20260712000012_create_company_secret_bindings.sql) |
| `secret_access_events` (20260712000016_create_secret_access_events.sql) |
| `document_annotation_threads` (20260728000005_create_routine_document_annotations.sql) |
| `document_annotation_comments` (20260728000005_create_routine_document_annotations.sql) |

## money 列（核对 Paperclip 金额语义）

（无）

## JSONB 列

| 内容 |
|---|
| `projects.env` (002_create_projects.sql) |
| `agent_config_revisions.snapshot` (20260711000001_create_agents.sql) |
| `issues.assignee_adapter_overrides` (20260711000002_create_issues.sql) |
| `issues.execution_policy` (20260711000002_create_issues.sql) |
| `issues.execution_state` (20260711000002_create_issues.sql) |
| `issues.execution_workspace_settings` (20260711000002_create_issues.sql) |
| `issues.source_trust` (20260711000002_create_issues.sql) |
| `issue_comments.metadata` (20260711000002_create_issues.sql) |
| `thread_interactions.metadata` (20260711000002_create_issues.sql) |
| `annotation_threads.position` (20260711000002_create_issues.sql) |
| `issue_work_products.artifact` (20260711000002_create_issues.sql) |
| `issue_tree_holds.metadata` (20260711000003_create_issue_tree_control.sql) |
| `routine_triggers.last_result` (20260711000007_create_routine_triggers.sql) |
| `routine_revisions.snapshot` (20260711000008_create_routine_revisions.sql) |
| `routine_runs.trigger_payload` (20260711000009_create_routine_runs.sql) |
| `approvals.payload` (20260711000011_create_approvals.sql) |
| `pipeline_cases.pending_suggestion` (20260711000021_create_pipeline_cases.sql) |
| `company_secrets.provider_metadata` (20260711000022_create_company_secrets.sql) |
| `feedback_traces.payload` (20260712000001_create_issue_read_status_and_archive.sql) |
| `recovery_actions.metadata` (20260712000001_create_issue_read_status_and_archive.sql) |
| `plan_decompositions.plan` (20260712000001_create_issue_read_status_and_archive.sql) |
| `heartbeat_runs.context_snapshot` (20260712000002_create_heartbeat_runs.sql) |
| `environments.metadata` (20260712000006_create_environments.sql) |
| `execution_workspaces.metadata` (20260712000007_create_execution_workspaces.sql) |
| `environment_leases.metadata` (20260712000008_create_environment_leases.sql) |
| `company_secret_provider_configs.health_details` (20260712000010_create_company_secret_provider_configs.sql) |
| `company_secret_versions.material` (20260712000011_create_company_secret_versions.sql) |
| `user_secret_definitions.provider_metadata` (20260712000014_create_user_secret_definitions.sql) |
| `user_secret_declarations.value_material` (20260712000015_create_user_secret_declarations.sql) |
| `skill_test_runs.result` (20260712000017_create_skill_tables.sql) |
| `plugin_data.value` (20260719000007_create_plugins.sql) |
| `tool_policies.conditions` (20260722000001_create_tool_access_core.sql) |
| `tool_policies.config` (20260722000001_create_tool_access_core.sql) |
| `tool_invocations.arguments_summary` (20260728000001_create_tool_invocation_audit.sql) |
| `tool_invocations.result_summary` (20260728000001_create_tool_invocation_audit.sql) |
| `tool_action_requests.canonical_arguments_summary` (20260728000001_create_tool_invocation_audit.sql) |
| `tool_call_events.arguments_summary` (20260728000001_create_tool_invocation_audit.sql) |
| `tool_call_events.request_summary` (20260728000001_create_tool_invocation_audit.sql) |
| `tool_call_events.result_summary` (20260728000001_create_tool_invocation_audit.sql) |
| `tool_call_events.redaction_plan` (20260728000001_create_tool_invocation_audit.sql) |
| `tool_call_events.rate_limit_state` (20260728000001_create_tool_invocation_audit.sql) |
| `tool_call_events.metadata` (20260728000001_create_tool_invocation_audit.sql) |
| `workspace_operations.metadata` (20260728000003_create_workspace_operations.sql) |
| `document_annotation_threads.anchor_selector` (20260728000005_create_routine_document_annotations.sql) |
| `environment_custom_image_setup_sessions.connection_summary` (20260728000006_create_custom_image_setup_sessions.sql) |
| `environment_custom_image_setup_sessions.metadata` (20260728000006_create_custom_image_setup_sessions.sql) |
| `environment_custom_image_templates.metadata` (20260802000002_create_environment_custom_image_templates.sql) |
| `decisions.options` (20260813000002_create_decisions.sql) |
| `decisions.inputs` (20260813000002_create_decisions.sql) |
| `decisions.input_values` (20260813000002_create_decisions.sql) |
| `decision_effect_executions.result` (20260813000002_create_decisions.sql) |
| `decision_training_examples.snapshot` (20260813000004_create_decision_training_examples.sql) |
| `company_secret_proposals.value_ciphertext` (20260813000006_create_secret_proposals.sql) |
| `smoke_run_steps.screenshot_artifact_ref` (20260813000009_create_smoke_lab.sql) |

## 结论与待办

- 运行期 migration 测试（decision/skill_policy/watchdog decision/plugin lifecycle 等）需要真实 Postgres，本环境无法执行；建议在 CI 中用空库+已有库各跑一次 `sqlx::migrate!`。
- 上表为静态审计起点，请逐条复核 `no company_id` / `no index` / `no cascade` 项。