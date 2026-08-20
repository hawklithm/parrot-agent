# Paperclip Schema Baseline (drizzle)

自动提取：`scripts/extract_paperclip_schema.py`。作为 Parrot migration 对齐的基准。

- schema files: **116**；FK references: **357**；index defs: **592**

## 1. company_id → companies FK 的 onDelete 策略

| table | onDelete |
|---|---|
| `activity_log` | `restrict` |
| `adapter_auth_sessions` | `cascade` |
| `agent_api_keys` | `restrict` |
| `agent_config_revisions` | `restrict` |
| `agent_memberships` | `cascade` |
| `agent_runtime_state` | `restrict` |
| `agent_task_sessions` | `restrict` |
| `agent_wakeup_requests` | `restrict` |
| `agents` | `restrict` |
| `approval_comments` | `restrict` |
| `approvals` | `restrict` |
| `assets` | `restrict` |
| `budget_incidents` | `restrict` |
| `budget_policies` | `restrict` |
| `built_in_managed_resources` | `cascade` |
| `cases` | `cascade` |
| `cases` | `cascade` |
| `cases` | `cascade` |
| `cases` | `cascade` |
| `cases` | `cascade` |
| `cases` | `cascade` |
| `claude_setup_token_sessions` | `cascade` |
| `company_logos` | `cascade` |
| `company_memberships` | `restrict` |
| `company_onboarding_seeds` | `cascade` |
| `company_secret_bindings` | `restrict` |
| `company_secret_proposals` | `restrict` |
| `company_secret_provider_configs` | `cascade` |
| `company_secrets` | `restrict` |
| `company_skill_policies` | `cascade` |
| `company_skills` | `cascade` |
| `company_skills` | `cascade` |
| `company_skills` | `cascade` |
| `company_skills` | `cascade` |
| `company_skills` | `cascade` |
| `company_skills` | `cascade` |
| `company_skills` | `restrict` |
| `company_user_sidebar_preferences` | `cascade` |
| `cost_events` | `restrict` |
| `decision_bundles` | `restrict` |
| `decision_bundles` | `restrict` |
| `decision_bundles` | `restrict` |
| `decision_queues` | `cascade` |
| `decision_queues` | `cascade` |
| `decision_queues` | `cascade` |
| `decision_queues` | `cascade` |
| `decision_queues` | `cascade` |
| `decision_queues` | `cascade` |
| `decision_training_examples` | `cascade` |
| `document_annotation_anchor_snapshots` | `restrict` |
| `document_annotation_comments` | `restrict` |
| `document_annotation_threads` | `restrict` |
| `document_memberships` | `cascade` |
| `document_revisions` | `cascade` |
| `documents` | `cascade` |
| `environment_leases` | `cascade` |
| `execution_workspace_runtime_leases` | `cascade` |
| `execution_workspaces` | `cascade` |
| `external_object_mentions` | `cascade` |
| `external_objects` | `cascade` |
| `feedback_exports` | `restrict` |
| `feedback_votes` | `restrict` |
| `finance_events` | `restrict` |
| `folders` | `cascade` |
| `goals` | `restrict` |
| `heartbeat_run_events` | `restrict` |
| `heartbeat_run_watchdog_decisions` | `restrict` |
| `heartbeat_runs` | `restrict` |
| `inbox_dismissals` | `restrict` |
| `invites` | `restrict` |
| `issue_approvals` | `restrict` |
| `issue_attachments` | `restrict` |
| `issue_comments` | `restrict` |
| `issue_create_idempotency_keys` | `cascade` |
| `issue_documents` | `restrict` |
| `issue_execution_decisions` | `restrict` |
| `issue_inbox_archives` | `restrict` |
| `issue_labels` | `cascade` |
| `issue_plan_decompositions` | `restrict` |
| `issue_read_states` | `restrict` |
| `issue_recovery_actions` | `restrict` |
| `issue_reference_mentions` | `restrict` |
| `issue_relations` | `restrict` |
| `issue_thread_interactions` | `restrict` |
| `issue_tree_hold_members` | `restrict` |
| `issue_tree_holds` | `restrict` |
| `issue_watchdogs` | `cascade` |
| `issue_work_products` | `restrict` |
| `issues` | `restrict` |
| `join_requests` | `restrict` |
| `labels` | `cascade` |
| `pipeline_case_events` | `cascade` |
| `pipeline_cases` | `cascade` |
| `pipeline_cases` | `cascade` |
| `pipeline_cases` | `cascade` |
| `pipeline_cases` | `cascade` |
| `pipeline_cases` | `cascade` |
| `pipeline_cases` | `cascade` |
| `pipelines` | `cascade` |
| `plugin_company_settings` | `cascade` |
| `plugin_config` | `cascade` |
| `plugin_entities` | `cascade` |
| `plugin_jobs` | `cascade` |
| `plugin_logs` | `cascade` |
| `plugin_managed_resources` | `cascade` |
| `plugin_webhook_deliveries` | `cascade` |
| `principal_permission_grants` | `restrict` |
| `project_goals` | `restrict` |
| `project_memberships` | `cascade` |
| `project_workspaces` | `restrict` |
| `projects` | `restrict` |
| `routine_documents` | `restrict` |
| `routines` | `cascade` |
| `routines` | `cascade` |
| `routines` | `cascade` |
| `routines` | `cascade` |
| `secret_access_events` | `restrict` |
| `smoke_runs` | `cascade` |
| `smoke_runs` | `cascade` |
| `status_cards` | `cascade` |
| `summary_slots` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `tool_applications` | `cascade` |
| `user_inbox_agent_policies` | `cascade` |
| `user_secret_declarations` | `cascade` |
| `user_secret_definitions` | `cascade` |
| `workspace_operations` | `cascade` |
| `workspace_runtime_services` | `restrict` |

## 2. 无 company_id 列（或未直连 companies FK）的表

| table |
|---|
| `board_api_keys` |
| `company_secret_versions` |
| `environment_custom_image_setup_sessions` |
| `environment_custom_image_templates` |
| `plugin_database_namespaces` |
| `plugin_state` |
| `user` |

## 3. company_id 相关索引

| table | index | columns |
|---|---|---|
| `activity_log` | `activity_log_company_created_idx` | `companyId, createdAt` |
| `activity_log` | `activity_log_company_agent_created_idx` | `companyId, agentId, createdAt, ` |
| `activity_log` | `activity_log_company_responsible_user_created_idx` | `companyId, responsibleUserId, createdAt, ` |
| `adapter_auth_sessions` | `adapter_auth_sessions_company_status_idx` | `companyId, status, ` |
| `adapter_auth_sessions` | `adapter_auth_sessions_company_adapter_active_uq` | `companyId, adapterType` |
| `agent_api_keys` | `agent_api_keys_company_agent_idx` | `companyId, agentId` |
| `agent_config_revisions` | `agent_config_revisions_company_agent_created_idx` | `companyId, agentId, createdAt, ` |
| `agent_memberships` | `agent_memberships_company_user_idx` | `companyId, userId` |
| `agent_memberships` | `agent_memberships_company_user_starred_idx` | `companyId, userId, starredAt, ` |
| `agent_memberships` | `agent_memberships_company_user_agent_uq` | `companyId, userId, agentId, ` |
| `agent_runtime_state` | `agent_runtime_state_company_agent_idx` | `companyId, agentId` |
| `agent_runtime_state` | `agent_runtime_state_company_updated_idx` | `companyId, updatedAt` |
| `agent_task_sessions` | `agent_task_sessions_company_agent_adapter_task_uniq` | `companyId, agentId, adapterType, taskKey, ` |
| `agent_task_sessions` | `agent_task_sessions_company_agent_updated_idx` | `companyId, agentId, updatedAt, ` |
| `agent_task_sessions` | `agent_task_sessions_company_task_updated_idx` | `companyId, taskKey, updatedAt, ` |
| `agent_wakeup_requests` | `agent_wakeup_requests_company_agent_status_idx` | `companyId, agentId, status, ` |
| `agent_wakeup_requests` | `agent_wakeup_requests_company_requested_idx` | `companyId, requestedAt, ` |
| `agent_wakeup_requests` | `agent_wakeup_requests_review_path_recovery_idempotency_uq` | `companyId, idempotencyKey` |
| `agent_wakeup_requests` | `agent_wakeup_requests_company_payload_issue_idx` | `companyId, payload} ->> 'issueId'` |
| `agents` | `agents_company_status_idx` | `companyId, status` |
| `agents` | `agents_company_reports_to_idx` | `companyId, reportsTo` |
| `agents` | `agents_company_default_environment_idx` | `companyId, defaultEnvironmentId` |
| `approval_comments` | `approval_comments_company_idx` | `companyId` |
| `approvals` | `approvals_company_status_type_idx` | `companyId, status, type, ` |
| `assets` | `assets_company_created_idx` | `companyId, createdAt` |
| `assets` | `assets_company_provider_idx` | `companyId, provider` |
| `assets` | `assets_company_object_key_uq` | `companyId, objectKey` |
| `budget_incidents` | `budget_incidents_company_status_idx` | `companyId, status` |
| `budget_incidents` | `budget_incidents_company_scope_idx` | `companyId, scopeType, scopeId, status, ` |
| `budget_policies` | `budget_policies_company_scope_active_idx` | `companyId, scopeType, scopeId, isActive, ` |
| `budget_policies` | `budget_policies_company_window_idx` | `companyId, windowKind, metric, ` |
| `budget_policies` | `budget_policies_company_scope_metric_unique_idx` | `companyId, scopeType, scopeId, metric, windowKind, ` |
| `built_in_managed_resources` | `built_in_managed_resources_company_idx` | `companyId` |
| `built_in_managed_resources` | `built_in_managed_resources_company_bundle_resource_uq` | `companyId, bundleKey, resourceKind, resourceKey, ` |
| `cases` | `cases_company_case_number_uq` | `companyId, caseNumber` |
| `cases` | `cases_company_type_key_uq` | `companyId, caseType, key` |
| `cases` | `cases_company_status_idx` | `companyId, status` |
| `cases` | `cases_company_type_idx` | `companyId, caseType` |
| `cases` | `cases_company_project_idx` | `companyId, projectId` |
| `cases` | `case_issue_links_company_case_idx` | `companyId, caseId` |
| `cases` | `case_events_company_case_idx` | `companyId, caseId` |
| `cases` | `case_documents_company_case_key_uq` | `companyId, caseId, key` |
| `cases` | `case_documents_company_case_updated_idx` | `companyId, caseId, updatedAt` |
| `cases` | `case_labels_company_case_idx` | `companyId, caseId` |
| `cases` | `case_attachments_company_case_idx` | `companyId, caseId` |
| `claude_setup_token_sessions` | `claude_setup_token_sessions_active_uq` | `companyId, ownerUserId, adapterType, environmentId` |
| `cli_auth_challenges` | `cli_auth_challenges_requested_company_idx` | `requestedCompanyId` |
| `company_logos` | `company_logos_company_uq` | `companyId` |
| `company_logos` | `company_logos_asset_uq` | `assetId` |
| `company_memberships` | `company_memberships_company_principal_unique_idx` | `companyId, principalType, principalId, ` |
| `company_memberships` | `company_memberships_principal_status_idx` | `principalType, principalId, status, ` |
| `company_memberships` | `company_memberships_company_status_idx` | `companyId, status` |
| `company_onboarding_seeds` | `company_onboarding_seeds_company_uq` | `companyId` |
| `company_secret_bindings` | `company_secret_bindings_company_idx` | `companyId` |
| `company_secret_bindings` | `company_secret_bindings_secret_idx` | `secretId` |
| `company_secret_bindings` | `company_secret_bindings_target_idx` | `companyId, targetType, targetId` |
| `company_secret_bindings` | `company_secret_bindings_target_path_uq` | `companyId, targetType, targetId, configPath, ` |
| `company_secret_proposals` | `company_secret_proposals_company_status_idx` | `companyId, status` |
| `company_secret_proposals` | `company_secret_proposals_proposer_status_idx` | `proposedByAgentId, status` |
| `company_secret_proposals` | `company_secret_proposals_expiry_idx` | `status, expiresAt` |
| `company_secret_proposals` | `company_secret_proposals_secret_proposal_idx` | `secretProposalId` |
| `company_secret_provider_configs` | `company_secret_provider_configs_company_idx` | `companyId` |
| `company_secret_provider_configs` | `company_secret_provider_configs_company_provider_idx` | `companyId, provider` |
| `company_secret_provider_configs` | `company_secret_provider_configs_default_uq` | `companyId, provider` |
| `company_secret_versions` | `company_secret_versions_secret_idx` | `secretId, createdAt` |
| `company_secret_versions` | `company_secret_versions_value_sha256_idx` | `valueSha256` |
| `company_secret_versions` | `company_secret_versions_fingerprint_idx` | `fingerprintSha256` |
| `company_secret_versions` | `company_secret_versions_secret_version_uq` | `secretId, version` |
| `company_secrets` | `company_secrets_company_idx` | `companyId` |
| `company_secrets` | `company_secrets_company_scope_idx` | `companyId, scope` |
| `company_secrets` | `company_secrets_company_owner_idx` | `companyId, ownerUserId` |
| `company_secrets` | `company_secrets_user_definition_owner_idx` | `companyId, userSecretDefinitionId, ownerUserId, ` |
| `company_secrets` | `company_secrets_company_provider_idx` | `companyId, provider` |
| `company_secrets` | `company_secrets_provider_config_idx` | `providerConfigId` |
| `company_secrets` | `company_secrets_company_name_uq` | `companyId, name` |
| `company_secrets` | `company_secrets_company_key_uq` | `companyId, key` |
| `company_secrets` | `company_secrets_user_definition_owner_uq` | `companyId, userSecretDefinitionId, ownerUserId` |
| `company_skills` | `company_skills_company_key_idx` | `companyId, key` |
| `company_skills` | `company_skills_company_name_idx` | `companyId, name` |
| `company_skills` | `company_skills_company_folder_idx` | `companyId, folderId` |
| `company_skills` | `company_skills_company_sharing_scope_idx` | `companyId, sharingScope` |
| `company_skills` | `company_skills_company_current_version_idx` | `companyId, currentVersionId` |
| `company_skills` | `company_skills_company_forked_from_idx` | `companyId, forkedFromSkillId` |
| `company_skills` | `company_skill_versions_skill_revision_idx` | `companySkillId, revisionNumber, ` |
| `company_skills` | `company_skill_versions_skill_release_idx` | `companySkillId, releaseId` |
| `company_skills` | `company_skill_versions_company_skill_created_idx` | `companyId, companySkillId, createdAt, ` |
| `company_skills` | `company_skill_stars_skill_agent_idx` | `companySkillId, agentId` |
| `company_skills` | `company_skill_stars_skill_user_idx` | `companySkillId, userId` |
| `company_skills` | `company_skill_stars_company_skill_created_idx` | `companyId, companySkillId, createdAt, ` |
| `company_skills` | `company_skill_comments_company_skill_created_idx` | `companyId, companySkillId, createdAt, ` |
| `company_skills` | `company_skill_comments_parent_idx` | `parentCommentId` |
| `company_skills` | `company_skill_test_inputs_company_skill_name_idx` | `companyId, skillId, name, ` |
| `company_skills` | `company_skill_test_inputs_company_skill_active_idx` | `companyId, skillId, deletedAt, ` |
| `company_skills` | `company_skill_test_run_templates_company_active_idx` | `companyId, deletedAt, name, ` |
| `company_skills` | `company_skill_test_runs_company_skill_created_idx` | `companyId, skillId, createdAt, ` |
| `company_skills` | `company_skill_test_runs_company_issue_idx` | `companyId, issueId` |
| `company_skills` | `company_skill_test_runs_company_input_created_idx` | `companyId, inputId, createdAt, ` |
| `company_skills` | `company_skill_test_runs_company_status_idx` | `companyId, status` |
| `company_skills` | `company_skill_test_runs_company_harness_expires_idx` | `companyId, harnessIssueExpiresAt, ` |
| `company_transfer_runs` | `company_transfer_runs_company_idx` | `companyId` |
| `company_transfer_runs` | `company_transfer_runs_idempotency_direction_idx` | `idempotencyKey, direction, ` |
| `company_transfer_runs` | `company_transfer_runs_actor_status_idx` | `actorKey, status` |
| `company_user_sidebar_preferences` | `company_user_sidebar_preferences_company_idx` | `companyId` |
| `company_user_sidebar_preferences` | `company_user_sidebar_preferences_user_idx` | `userId` |
| `company_user_sidebar_preferences` | `company_user_sidebar_preferences_company_user_uq` | `companyId, userId, ` |
| `cost_events` | `cost_events_company_occurred_idx` | `companyId, occurredAt` |
| `cost_events` | `cost_events_company_agent_occurred_idx` | `companyId, agentId, occurredAt, ` |
| `cost_events` | `cost_events_company_provider_occurred_idx` | `companyId, provider, occurredAt, ` |
| `cost_events` | `cost_events_company_biller_occurred_idx` | `companyId, biller, occurredAt, ` |
| `cost_events` | `cost_events_company_heartbeat_run_idx` | `companyId, heartbeatRunId, ` |
| `decision_queues` | `decision_queues_company_key_uq` | `companyId, key` |
| `decision_queues` | `decision_queues_company_updated_idx` | `companyId, updatedAt` |
| `decision_queues` | `decision_queue_items_company_source_idx` | `companyId, sourceKind, sourceId, ` |
| `decision_queues` | `decision_triage_company_source_uq` | `companyId, sourceKind, sourceId, ` |
| `decision_queues` | `decision_triage_company_decide_by_idx` | `companyId, decideBy` |
| `decision_queues` | `decision_triage_events_company_source_created_idx` | `companyId, sourceKind, sourceId, createdAt, ` |
| `decision_queues` | `decision_retention_company_source_uq` | `companyId, sourceKind, sourceId, ` |
| `decision_queues` | `decision_retention_company_archived_idx` | `companyId, archivedAt` |
| `decision_queues` | `decision_archive_notification_outbox_uq` | `companyId, sourceKind, sourceId, archiveVersion, originAgentId, ` |
| `decision_training_examples` | `decision_training_examples_company_created_at_idx` | `companyId, createdAt, ` |
| `decision_bundles` | `decision_bundles_company_created_at_idx` | `companyId, createdAt` |
| `decision_bundles` | `decisions_company_status_expires_at_idx` | `companyId, status, expiresAt, ` |
| `decision_bundles` | `decisions_company_idempotency_uq` | `companyId, idempotencyKey` |
| `document_annotation_anchor_snapshots` | `document_annotation_anchor_snapshots_company_thread_created_at_idx` | `companyId, threadId, createdAt, ` |
| `document_annotation_anchor_snapshots` | `document_annotation_anchor_snapshots_company_document_revision_idx` | `companyId, documentId, toRevisionNumber, ` |
| `document_annotation_comments` | `document_annotation_comments_company_thread_created_at_idx` | `companyId, threadId, createdAt, ` |
| `document_annotation_comments` | `document_annotation_comments_company_issue_created_at_idx` | `companyId, issueId, createdAt, ` |
| `document_annotation_comments` | `document_annotation_comments_company_routine_created_at_idx` | `companyId, routineId, createdAt, ` |
| `document_annotation_comments` | `document_annotation_comments_company_case_created_at_idx` | `companyId, caseId, createdAt, ` |
| `document_annotation_comments` | `document_annotation_comments_company_document_created_at_idx` | `companyId, documentId, createdAt, ` |
| `document_annotation_threads` | `document_annotation_threads_company_document_status_idx` | `companyId, documentId, status, ` |
| `document_annotation_threads` | `document_annotation_threads_company_issue_status_idx` | `companyId, issueId, status, ` |
| `document_annotation_threads` | `document_annotation_threads_company_routine_status_idx` | `companyId, routineId, status, ` |
| `document_annotation_threads` | `document_annotation_threads_company_case_status_idx` | `companyId, caseId, status, ` |
| `document_annotation_threads` | `document_annotation_threads_company_current_revision_open_idx` | `companyId, documentId, currentRevisionId, status, ` |
| `document_annotation_threads` | `document_annotation_threads_company_anchor_state_idx` | `companyId, anchorState, ` |
| `document_memberships` | `document_memberships_company_user_starred_idx` | `companyId, userId, starredAt, ` |
| `document_memberships` | `document_memberships_company_user_document_uq` | `companyId, userId, documentId, ` |
| `document_revisions` | `document_revisions_company_document_created_idx` | `companyId, documentId, createdAt, ` |
| `documents` | `documents_company_updated_idx` | `companyId, updatedAt` |
| `documents` | `documents_company_created_idx` | `companyId, createdAt` |
| `environment_leases` | `environment_leases_company_environment_status_idx` | `companyId, environmentId, status, ` |
| `environment_leases` | `environment_leases_company_execution_workspace_idx` | `companyId, executionWorkspaceId, ` |
| `environment_leases` | `environment_leases_company_issue_idx` | `companyId, issueId` |
| `environment_leases` | `environment_leases_company_last_used_idx` | `companyId, lastUsedAt` |
| `execution_workspace_runtime_leases` | `execution_workspace_runtime_leases_company_workspace_idx` | `companyId, executionWorkspaceId, ` |
| `execution_workspace_runtime_leases` | `execution_workspace_runtime_leases_company_owner_idx` | `companyId, ownerKey, ` |
| `execution_workspaces` | `execution_workspaces_company_project_status_idx` | `companyId, projectId, status, ` |
| `execution_workspaces` | `execution_workspaces_company_project_workspace_status_idx` | `companyId, projectWorkspaceId, status, ` |
| `execution_workspaces` | `execution_workspaces_company_source_issue_idx` | `companyId, sourceIssueId, ` |
| `execution_workspaces` | `execution_workspaces_company_last_used_idx` | `companyId, lastUsedAt, ` |
| `execution_workspaces` | `execution_workspaces_company_branch_idx` | `companyId, branchName, ` |
| `external_object_mentions` | `external_object_mentions_company_source_issue_idx` | `companyId, sourceIssueId, ` |
| `external_object_mentions` | `external_object_mentions_company_object_idx` | `companyId, objectId` |
| `external_object_mentions` | `external_object_mentions_company_provider_idx` | `companyId, providerKey, objectType, ` |
| `external_object_mentions` | `external_object_mentions_company_source_record_uq` | `companyId, sourceIssueId, sourceKind, sourceRecordId, documentKey, propertyKey, canonicalIdentityHash, ` |
| `external_object_mentions` | `external_object_mentions_company_source_null_record_uq` | `companyId, sourceIssueId, sourceKind, documentKey, propertyKey, canonicalIdentityHash, ` |
| `external_objects` | `external_objects_company_provider_object_idx` | `companyId, providerKey, objectType, ` |
| `external_objects` | `external_objects_company_provider_status_idx` | `companyId, providerKey, statusCategory, ` |
| `external_objects` | `external_objects_company_refresh_idx` | `companyId, nextRefreshAt` |
| `external_objects` | `external_objects_company_external_id_uq` | `companyId, providerKey, objectType, externalId, ` |
| `external_objects` | `external_objects_company_identity_uq` | `companyId, providerKey, objectType, canonicalIdentityHash, ` |
| `feedback_exports` | `feedback_exports_company_created_idx` | `companyId, createdAt` |
| `feedback_exports` | `feedback_exports_company_status_idx` | `companyId, status, createdAt` |
| `feedback_exports` | `feedback_exports_company_issue_idx` | `companyId, issueId, createdAt` |
| `feedback_exports` | `feedback_exports_company_project_idx` | `companyId, projectId, createdAt` |
| `feedback_exports` | `feedback_exports_company_author_idx` | `companyId, authorUserId, createdAt` |
| `feedback_votes` | `feedback_votes_company_issue_idx` | `companyId, issueId` |
| `feedback_votes` | `feedback_votes_company_target_author_idx` | `companyId, targetType, targetId, authorUserId, ` |
| `finance_events` | `finance_events_company_occurred_idx` | `companyId, occurredAt` |
| `finance_events` | `finance_events_company_biller_occurred_idx` | `companyId, biller, occurredAt, ` |
| `finance_events` | `finance_events_company_kind_occurred_idx` | `companyId, eventKind, occurredAt, ` |
| `finance_events` | `finance_events_company_direction_occurred_idx` | `companyId, direction, occurredAt, ` |
| `finance_events` | `finance_events_company_heartbeat_run_idx` | `companyId, heartbeatRunId, ` |
| `finance_events` | `finance_events_company_cost_event_idx` | `companyId, costEventId, ` |
| `folders` | `folders_company_kind_position_idx` | `companyId, kind, position, name, ` |
| `folders` | `folders_company_kind_root_slug_uq` | `companyId, kind, slug` |
| `folders` | `folders_company_kind_parent_slug_uq` | `companyId, kind, parentId, slug` |
| `folders` | `folders_company_kind_system_key_uq` | `companyId, kind, systemKey` |
| `folders` | `folders_company_kind_parent_position_idx` | `companyId, kind, parentId, position, name, ` |
| `goals` | `goals_company_idx` | `companyId` |
| `heartbeat_run_events` | `heartbeat_run_events_company_run_idx` | `companyId, runId` |
| `heartbeat_run_events` | `heartbeat_run_events_company_created_idx` | `companyId, createdAt` |
| `heartbeat_run_watchdog_decisions` | `heartbeat_run_watchdog_decisions_company_run_created_idx` | `companyId, runId, createdAt, ` |
| `heartbeat_run_watchdog_decisions` | `heartbeat_run_watchdog_decisions_company_run_snooze_idx` | `companyId, runId, snoozedUntil, ` |
| `heartbeat_runs` | `heartbeat_runs_company_agent_started_idx` | `companyId, agentId, startedAt, ` |
| `heartbeat_runs` | `heartbeat_runs_company_responsible_user_idx` | `companyId, responsibleUserId, createdAt, ` |
| `heartbeat_runs` | `heartbeat_runs_company_liveness_idx` | `companyId, livenessState, createdAt, ` |
| `heartbeat_runs` | `heartbeat_runs_company_status_last_output_idx` | `companyId, status, lastOutputAt, ` |
| `heartbeat_runs` | `heartbeat_runs_company_status_process_started_idx` | `companyId, status, processStartedAt, ` |
| `heartbeat_runs` | `heartbeat_runs_company_created_at_desc_idx` | `companyId, desc(` |
| `heartbeat_runs` | `heartbeat_runs_company_ctx_issue_created_idx` | `companyId, contextSnapshot} ->> 'issueId'` |
| `heartbeat_runs` | `heartbeat_runs_company_ctx_task_created_idx` | `companyId, contextSnapshot} ->> 'taskId'` |
| `heartbeat_runs` | `heartbeat_runs_company_ctx_taskkey_created_idx` | `companyId, contextSnapshot} ->> 'taskKey'` |
| `inbox_dismissals` | `inbox_dismissals_company_user_idx` | `companyId, userId` |
| `inbox_dismissals` | `inbox_dismissals_company_item_idx` | `companyId, itemKey` |
| `inbox_dismissals` | `inbox_dismissals_company_user_item_idx` | `companyId, userId, itemKey, ` |
| `invites` | `invites_company_invite_state_idx` | `companyId, inviteType, revokedAt, expiresAt, ` |
| `issue_approvals` | `issue_approvals_company_idx` | `companyId` |
| `issue_attachments` | `issue_attachments_company_issue_idx` | `companyId, issueId` |
| `issue_comments` | `issue_comments_company_idx` | `companyId` |
| `issue_comments` | `issue_comments_company_issue_created_at_idx` | `companyId, issueId, createdAt, ` |
| `issue_comments` | `issue_comments_company_author_issue_created_at_idx` | `companyId, authorUserId, issueId, createdAt, ` |
| `issue_create_idempotency_keys` | `issue_create_idempotency_keys_company_key_uq` | `companyId, idempotencyKey, ` |
| `issue_create_idempotency_keys` | `issue_create_idempotency_keys_company_created_at_idx` | `companyId, createdAt, ` |
| `issue_documents` | `issue_documents_company_issue_key_uq` | `companyId, issueId, key, ` |
| `issue_documents` | `issue_documents_company_issue_updated_idx` | `companyId, issueId, updatedAt, ` |
| `issue_execution_decisions` | `issue_execution_decisions_company_issue_idx` | `companyId, issueId` |
| `issue_inbox_archives` | `issue_inbox_archives_company_issue_idx` | `companyId, issueId` |
| `issue_inbox_archives` | `issue_inbox_archives_company_user_idx` | `companyId, userId` |
| `issue_inbox_archives` | `issue_inbox_archives_company_issue_user_idx` | `companyId, issueId, userId, ` |
| `issue_labels` | `issue_labels_company_idx` | `companyId` |
| `issue_plan_decompositions` | `issue_plan_decompositions_company_source_status_idx` | `companyId, sourceIssueId, status, ` |
| `issue_plan_decompositions` | `issue_plan_decompositions_active_owner_idx` | `companyId, ownerAgentId` |
| `issue_plan_decompositions` | `issue_plan_decompositions_source_revision_uq` | `companyId, sourceIssueId, acceptedPlanRevisionId, ` |
| `issue_read_states` | `issue_read_states_company_issue_idx` | `companyId, issueId` |
| `issue_read_states` | `issue_read_states_company_user_idx` | `companyId, userId` |
| `issue_read_states` | `issue_read_states_company_issue_user_idx` | `companyId, issueId, userId, ` |
| `issue_recovery_actions` | `issue_recovery_actions_company_source_status_idx` | `companyId, sourceIssueId, status, ` |
| `issue_recovery_actions` | `issue_recovery_actions_company_owner_status_idx` | `companyId, ownerAgentId, status, ` |
| `issue_recovery_actions` | `issue_recovery_actions_company_recovery_issue_idx` | `companyId, recoveryIssueId, ` |
| `issue_recovery_actions` | `issue_recovery_actions_active_source_uq` | `companyId, sourceIssueId` |
| `issue_recovery_actions` | `issue_recovery_actions_active_fingerprint_uq` | `companyId, sourceIssueId, cause, fingerprint` |
| `issue_reference_mentions` | `issue_reference_mentions_company_source_issue_idx` | `companyId, sourceIssueId, ` |
| `issue_reference_mentions` | `issue_reference_mentions_company_target_issue_idx` | `companyId, targetIssueId, ` |
| `issue_reference_mentions` | `issue_reference_mentions_company_issue_pair_idx` | `companyId, sourceIssueId, targetIssueId, ` |
| `issue_reference_mentions` | `issue_reference_mentions_company_source_mention_record_uq` | `companyId, sourceIssueId, targetIssueId, sourceKind, sourceRecordId, ` |
| `issue_reference_mentions` | `issue_reference_mentions_company_source_mention_null_record_uq` | `companyId, sourceIssueId, targetIssueId, sourceKind, ` |
| `issue_relations` | `issue_relations_company_issue_idx` | `companyId, issueId` |
| `issue_relations` | `issue_relations_company_related_issue_idx` | `companyId, relatedIssueId` |
| `issue_relations` | `issue_relations_company_type_idx` | `companyId, type` |
| `issue_relations` | `issue_relations_company_edge_uq` | `companyId, issueId, relatedIssueId, type, ` |
| `issue_thread_interactions` | `issue_thread_interactions_company_issue_created_at_idx` | `companyId, issueId, createdAt, ` |
| `issue_thread_interactions` | `issue_thread_interactions_company_issue_status_idx` | `companyId, issueId, status, ` |
| `issue_thread_interactions` | `issue_thread_interactions_company_issue_idempotency_uq` | `companyId, issueId, idempotencyKey` |
| `issue_tree_hold_members` | `issue_tree_hold_members_company_issue_idx` | `companyId, issueId` |
| `issue_tree_holds` | `issue_tree_holds_company_root_status_idx` | `companyId, rootIssueId, status, ` |
| `issue_tree_holds` | `issue_tree_holds_company_status_mode_idx` | `companyId, status, mode` |
| `issue_watchdogs` | `issue_watchdogs_company_issue_uq` | `companyId, issueId` |
| `issue_watchdogs` | `issue_watchdogs_company_status_idx` | `companyId, status` |
| `issue_watchdogs` | `issue_watchdogs_company_agent_idx` | `companyId, watchdogAgentId` |
| `issue_watchdogs` | `issue_watchdogs_company_watchdog_issue_uq` | `companyId, watchdogIssueId` |
| `issue_work_products` | `issue_work_products_company_issue_type_idx` | `companyId, issueId, type, ` |
| `issue_work_products` | `issue_work_products_company_execution_workspace_type_idx` | `companyId, executionWorkspaceId, type, ` |
| `issue_work_products` | `issue_work_products_company_provider_external_id_idx` | `companyId, provider, externalId, ` |
| `issue_work_products` | `issue_work_products_company_updated_idx` | `companyId, updatedAt, ` |
| `issues` | `issues_company_status_idx` | `companyId, status` |
| `issues` | `issues_company_harness_kind_idx` | `companyId, harnessKind` |
| `issues` | `issues_company_assignee_status_idx` | `companyId, assigneeAgentId, status, ` |
| `issues` | `issues_company_assignee_user_status_idx` | `companyId, assigneeUserId, status, ` |
| `issues` | `issues_company_responsible_user_idx` | `companyId, responsibleUserId` |
| `issues` | `issues_company_parent_idx` | `companyId, parentId` |
| `issues` | `issues_company_project_idx` | `companyId, projectId` |
| `issues` | `issues_company_origin_idx` | `companyId, originKind, originId` |
| `issues` | `issues_company_project_workspace_idx` | `companyId, projectWorkspaceId` |
| `issues` | `issues_company_execution_workspace_idx` | `companyId, executionWorkspaceId` |
| `issues` | `issues_company_monitor_due_idx` | `companyId, monitorNextCheckAt` |
| `issues` | `issues_company_updated_idx` | `companyId, updatedAt` |
| `issues` | `issues_company_created_idx` | `companyId, createdAt` |
| `issues` | `issues_open_normalized_title_created_idx` | `companyId, parentId, title}` |
| `issues` | `issues_company_priority_idx` | `companyId, priority` |
| `issues` | `issues_open_routine_execution_uq` | `companyId, originKind, originId, originFingerprint` |
| `issues` | `issues_active_liveness_recovery_incident_uq` | `companyId, originKind, originId` |
| `issues` | `issues_active_liveness_recovery_leaf_uq` | `companyId, originKind, originFingerprint` |
| `issues` | `issues_active_stale_run_evaluation_uq` | `companyId, originKind, originId` |
| `issues` | `issues_active_task_watchdog_uq` | `companyId, originKind, originId` |
| `issues` | `issues_active_productivity_review_uq` | `companyId, originKind, originId` |
| `issues` | `issues_active_stranded_issue_recovery_uq` | `companyId, originKind, originId` |
| `issues` | `issues_onboarding_first_task_uq` | `companyId` |
| `join_requests` | `join_requests_company_status_type_created_idx` | `companyId, status, requestType, createdAt, ` |
| `join_requests` | `join_requests_pending_human_user_uq` | `companyId, requestingUserId` |
| `join_requests` | `join_requests_pending_human_email_uq` | `companyId, requestEmailSnapshot}` |
| `labels` | `labels_company_idx` | `companyId` |
| `labels` | `labels_company_name_idx` | `companyId, name` |
| `pipeline_case_events` | `pipeline_case_events_company_case_idx` | `companyId, caseId` |
| `pipeline_cases` | `pipeline_cases_company_idx` | `companyId` |
| `pipeline_cases` | `pipeline_cases_retired_idx` | `companyId, retiredAt` |
| `pipeline_cases` | `pipeline_case_issue_links_company_case_idx` | `companyId, caseId` |
| `pipeline_cases` | `pipeline_case_blockers_company_case_idx` | `companyId, caseId` |
| `pipeline_cases` | `pipeline_documents_company_pipeline_key_uq` | `companyId, pipelineId, key, ` |
| `pipeline_cases` | `pipeline_documents_company_pipeline_updated_idx` | `companyId, pipelineId, updatedAt, ` |
| `pipeline_cases` | `pipeline_case_documents_company_case_key_uq` | `companyId, caseId, key, ` |
| `pipeline_cases` | `pipeline_case_documents_company_case_updated_idx` | `companyId, caseId, updatedAt, ` |
| `pipeline_cases` | `pipeline_automation_executions_company_case_idx` | `companyId, caseId` |
| `pipelines` | `pipelines_company_key_uq` | `companyId, key` |
| `pipelines` | `pipelines_company_idx` | `companyId` |
| `pipelines` | `pipelines_company_project_idx` | `companyId, projectId` |
| `plugin_company_settings` | `plugin_company_settings_company_idx` | `companyId` |
| `plugin_company_settings` | `plugin_company_settings_plugin_idx` | `pluginId` |
| `plugin_company_settings` | `plugin_company_settings_company_plugin_uq` | `companyId, pluginId, ` |
| `plugin_config` | `plugin_config_plugin_company_idx` | `pluginId, companyId, ` |
| `plugin_entities` | `plugin_entities_company_idx` | `companyId` |
| `plugin_jobs` | `plugin_job_runs_company_idx` | `companyId` |
| `plugin_logs` | `plugin_logs_company_idx` | `companyId` |
| `plugin_managed_resources` | `plugin_managed_resources_company_idx` | `companyId` |
| `plugin_managed_resources` | `plugin_managed_resources_company_plugin_resource_uq` | `companyId, pluginId, resourceKind, resourceKey, ` |
| `plugin_webhook_deliveries` | `plugin_webhook_deliveries_company_idx` | `companyId` |
| `principal_permission_grants` | `principal_permission_grants_unique_idx` | `companyId, principalType, principalId, permissionKey, ` |
| `principal_permission_grants` | `principal_permission_grants_company_permission_idx` | `companyId, permissionKey, ` |
| `project_goals` | `project_goals_company_idx` | `companyId` |
| `project_memberships` | `project_memberships_company_user_idx` | `companyId, userId` |
| `project_memberships` | `project_memberships_company_user_starred_idx` | `companyId, userId, starredAt, ` |
| `project_memberships` | `project_memberships_company_user_project_uq` | `companyId, userId, projectId, ` |
| `project_workspaces` | `project_workspaces_company_project_idx` | `companyId, projectId` |
| `project_workspaces` | `project_workspaces_company_shared_key_idx` | `companyId, sharedWorkspaceKey` |
| `projects` | `projects_company_idx` | `companyId` |
| `routine_documents` | `routine_documents_company_routine_key_uq` | `companyId, routineId, key, ` |
| `routine_documents` | `routine_documents_company_routine_updated_idx` | `companyId, routineId, updatedAt, ` |
| `routines` | `routines_company_status_idx` | `companyId, status` |
| `routines` | `routines_company_assignee_idx` | `companyId, assigneeAgentId` |
| `routines` | `routines_company_project_idx` | `companyId, projectId` |
| `routines` | `routines_company_folder_idx` | `companyId, folderId` |
| `routines` | `routines_company_responsible_user_idx` | `companyId, responsibleUserId` |
| `routines` | `routines_company_origin_idx` | `companyId, originKind, originId` |
| `routines` | `routine_revisions_company_routine_created_idx` | `companyId, routineId, createdAt, ` |
| `routines` | `routine_revisions_company_responsible_user_idx` | `companyId, responsibleUserId, createdAt, ` |
| `routines` | `routine_triggers_company_routine_idx` | `companyId, routineId` |
| `routines` | `routine_triggers_company_kind_idx` | `companyId, kind` |
| `routines` | `routine_runs_company_routine_idx` | `companyId, routineId, createdAt` |
| `routines` | `routine_runs_company_responsible_user_idx` | `companyId, responsibleUserId, createdAt, ` |
| `secret_access_events` | `secret_access_events_company_created_idx` | `companyId, createdAt` |
| `secret_access_events` | `secret_access_events_company_credential_owner_idx` | `companyId, credentialOwnerUserId, createdAt, ` |
| `secret_access_events` | `secret_access_events_consumer_idx` | `companyId, consumerType, consumerId` |
| `smoke_runs` | `smoke_runs_company_started_idx` | `companyId, startedAt` |
| `smoke_runs` | `smoke_runs_company_status_idx` | `companyId, status` |
| `smoke_runs` | `smoke_run_steps_company_run_idx` | `companyId, runId` |
| `smoke_runs` | `smoke_run_steps_company_path_idx` | `companyId, path` |
| `status_cards` | `status_cards_company_archived_idx` | `companyId, archivedAt` |
| `status_cards` | `status_cards_company_next_eval_idx` | `companyId, nextEvalAt` |
| `summary_slots` | `summary_slots_company_scope_idx` | `companyId, scopeKind, scopeId` |
| `summary_slots` | `summary_slots_company_generating_issue_idx` | `companyId, generatingIssueId, ` |
| `summary_slots` | `summary_slots_company_updated_idx` | `companyId, updatedAt` |
| `tool_applications` | `tool_applications_company_idx` | `companyId` |
| `tool_applications` | `tool_applications_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_applications_company_name_uq` | `companyId, name` |
| `tool_applications` | `tool_applications_company_key_uq` | `companyId, applicationKey` |
| `tool_applications` | `tool_connections_company_idx` | `companyId` |
| `tool_applications` | `tool_connections_company_enabled_idx` | `companyId, enabled` |
| `tool_applications` | `tool_connections_company_uid_uq` | `companyId, uid` |
| `tool_applications` | `connection_grants_company_connection_idx` | `companyId, connectionId` |
| `tool_applications` | `connection_grants_subject_user_idx` | `companyId, subjectUserId` |
| `tool_applications` | `tool_connection_installs_company_target_idx` | `companyId, targetType, targetId` |
| `tool_applications` | `tool_connection_installs_connection_idx` | `companyId, connectionId` |
| `tool_applications` | `tool_connection_installs_target_uq` | `companyId, connectionId, targetType, targetId, ` |
| `tool_applications` | `tool_oauth_states_company_idx` | `companyId` |
| `tool_applications` | `tool_catalog_entries_company_idx` | `companyId` |
| `tool_applications` | `tool_catalog_entries_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_profiles_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_profiles_company_key_uq` | `companyId, profileKey` |
| `tool_applications` | `tool_profiles_company_name_uq` | `companyId, name` |
| `tool_applications` | `tool_profile_entries_company_profile_idx` | `companyId, profileId` |
| `tool_applications` | `tool_profile_entries_application_idx` | `companyId, applicationId` |
| `tool_applications` | `tool_profile_entries_connection_idx` | `companyId, connectionId` |
| `tool_applications` | `tool_profile_entries_catalog_entry_idx` | `companyId, catalogEntryId` |
| `tool_applications` | `tool_profile_bindings_company_target_idx` | `companyId, targetType, targetId` |
| `tool_applications` | `tool_profile_bindings_target_profile_uq` | `companyId, targetType, targetId, profileId, ` |
| `tool_applications` | `tool_mcp_gateways_company_idx` | `companyId` |
| `tool_applications` | `tool_mcp_gateways_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_mcp_gateways_profile_idx` | `companyId, profileId` |
| `tool_applications` | `tool_mcp_gateways_company_slug_uq` | `companyId, slug` |
| `tool_applications` | `tool_mcp_gateways_company_name_uq` | `companyId, name` |
| `tool_applications` | `tool_mcp_gateway_tokens_gateway_idx` | `companyId, gatewayId` |
| `tool_applications` | `tool_mcp_gateway_tokens_subject_idx` | `companyId, subjectType, subjectId` |
| `tool_applications` | `tool_mcp_gateway_tokens_company_expires_idx` | `companyId, expiresAt` |
| `tool_applications` | `tool_policies_company_enabled_idx` | `companyId, enabled` |
| `tool_applications` | `tool_policies_company_type_idx` | `companyId, policyType` |
| `tool_applications` | `tool_policies_company_name_uq` | `companyId, name` |
| `tool_applications` | `tool_runtime_slots_company_idx` | `companyId` |
| `tool_applications` | `tool_runtime_slots_execution_workspace_idx` | `companyId, executionWorkspaceId` |
| `tool_applications` | `tool_runtime_slots_slot_key_uq` | `companyId, slotKey` |
| `tool_applications` | `tool_stdio_command_templates_company_idx` | `companyId` |
| `tool_applications` | `tool_stdio_command_templates_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_stdio_command_templates_company_key_uq` | `companyId, templateKey` |
| `tool_applications` | `tool_gateway_sessions_company_agent_idx` | `companyId, agentId` |
| `tool_applications` | `tool_gateway_sessions_company_expires_idx` | `companyId, expiresAt` |
| `tool_applications` | `tool_gateway_sessions_run_idx` | `companyId, runId` |
| `tool_applications` | `tool_gateway_sessions_issue_idx` | `companyId, issueId` |
| `tool_applications` | `tool_gateway_sessions_gateway_idx` | `companyId, gatewayId` |
| `tool_applications` | `tool_gateway_rate_limit_counters_company_idx` | `companyId` |
| `tool_applications` | `tool_gateway_rate_limit_counters_window_uq` | `companyId, counterKey, windowStartAt, ` |
| `tool_applications` | `tool_invocations_company_created_idx` | `companyId, createdAt` |
| `tool_applications` | `tool_invocations_run_idx` | `companyId, runId` |
| `tool_applications` | `tool_invocations_issue_idx` | `companyId, issueId` |
| `tool_applications` | `tool_invocations_gateway_idx` | `companyId, gatewayId` |
| `tool_applications` | `tool_invocations_company_idempotency_uq` | `companyId, idempotencyKey` |
| `tool_applications` | `tool_action_requests_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_action_requests_issue_idx` | `companyId, issueId` |
| `tool_applications` | `tool_call_events_company_created_idx` | `companyId, createdAt` |
| `tool_applications` | `tool_call_events_run_idx` | `companyId, runId` |
| `tool_applications` | `tool_call_events_issue_idx` | `companyId, issueId` |
| `tool_applications` | `tool_call_events_gateway_idx` | `companyId, gatewayId` |
| `tool_applications` | `connection_token_issuances_company_created_idx` | `companyId, createdAt` |
| `tool_applications` | `connection_token_issuances_connection_created_idx` | `companyId, connectionId, createdAt` |
| `tool_applications` | `connection_token_issuances_agent_connection_idx` | `companyId, agentId, connectionId, createdAt` |
| `tool_applications` | `connection_token_issuances_run_idx` | `companyId, runId` |
| `tool_applications` | `tool_rate_limit_counters_company_idx` | `companyId` |
| `tool_applications` | `tool_rate_limit_counters_window_uq` | `companyId, policyId, counterKey, windowKind, windowStartAt, ` |
| `tool_applications` | `tool_runtime_metric_counters_company_metric_idx` | `companyId, metric, bucketStartAt` |
| `tool_applications` | `tool_runtime_metric_counters_bucket_uq` | `companyId, metric, bucketStartAt` |
| `tool_applications` | `tool_access_audit_company_created_idx` | `companyId, createdAt` |
| `tool_applications` | `tool_access_audit_gateway_idx` | `companyId, gatewayId` |
| `user_inbox_agent_policies` | `user_inbox_agent_policies_company_user_uq` | `companyId, userId, ` |
| `user_secret_declarations` | `user_secret_declarations_company_idx` | `companyId` |
| `user_secret_declarations` | `user_secret_declarations_target_idx` | `companyId, targetType, targetId` |
| `user_secret_declarations` | `user_secret_declarations_company_required_idx` | `companyId, required` |
| `user_secret_declarations` | `user_secret_declarations_target_path_uq` | `companyId, targetType, targetId, configPath, ` |
| `user_secret_declarations` | `user_secret_declarations_required_override_idx` | `companyId, allowMissingOverride` |
| `user_secret_definitions` | `user_secret_definitions_company_status_idx` | `companyId, status` |
| `user_secret_definitions` | `user_secret_definitions_company_provider_idx` | `companyId, provider` |
| `user_secret_definitions` | `user_secret_definitions_company_key_uq` | `companyId, key` |
| `workspace_operations` | `workspace_operations_company_run_started_idx` | `companyId, heartbeatRunId, startedAt, ` |
| `workspace_operations` | `workspace_operations_company_workspace_started_idx` | `companyId, executionWorkspaceId, startedAt, ` |
| `workspace_operations` | `workspace_operations_company_workspace_issue_started_idx` | `companyId, executionWorkspaceId, issueId, startedAt, ` |
| `workspace_runtime_services` | `workspace_runtime_services_company_workspace_status_idx` | `companyId, projectWorkspaceId, status, ` |
| `workspace_runtime_services` | `workspace_runtime_services_company_execution_workspace_status_idx` | `companyId, executionWorkspaceId, status, ` |
| `workspace_runtime_services` | `workspace_runtime_services_company_project_status_idx` | `companyId, projectId, status, ` |
| `workspace_runtime_services` | `workspace_runtime_services_company_updated_idx` | `companyId, updatedAt, ` |

## 4. 全部索引（参考）

| table | index | columns |
|---|---|---|
| `activity_log` | `activity_log_company_created_idx` | `companyId, createdAt` |
| `activity_log` | `activity_log_company_agent_created_idx` | `companyId, agentId, createdAt, ` |
| `activity_log` | `activity_log_company_responsible_user_created_idx` | `companyId, responsibleUserId, createdAt, ` |
| `activity_log` | `activity_log_run_id_idx` | `runId` |
| `activity_log` | `activity_log_entity_type_id_idx` | `entityType, entityId` |
| `adapter_auth_sessions` | `adapter_auth_sessions_company_status_idx` | `companyId, status, ` |
| `adapter_auth_sessions` | `adapter_auth_sessions_company_adapter_active_uq` | `companyId, adapterType` |
| `adapter_auth_sessions` | `adapter_auth_sessions_environment_idx` | `environmentId` |
| `adapter_auth_sessions` | `adapter_auth_sessions_expires_idx` | `expiresAt` |
| `adapter_auth_sessions` | `adapter_auth_sessions_provider_lease_idx` | `providerLeaseId` |
| `agent_api_keys` | `agent_api_keys_key_hash_idx` | `keyHash` |
| `agent_api_keys` | `agent_api_keys_company_agent_idx` | `companyId, agentId` |
| `agent_config_revisions` | `agent_config_revisions_company_agent_created_idx` | `companyId, agentId, createdAt, ` |
| `agent_config_revisions` | `agent_config_revisions_agent_created_idx` | `agentId, createdAt` |
| `agent_memberships` | `agent_memberships_company_user_idx` | `companyId, userId` |
| `agent_memberships` | `agent_memberships_company_user_starred_idx` | `companyId, userId, starredAt, ` |
| `agent_memberships` | `agent_memberships_agent_idx` | `agentId` |
| `agent_memberships` | `agent_memberships_company_user_agent_uq` | `companyId, userId, agentId, ` |
| `agent_runtime_state` | `agent_runtime_state_company_agent_idx` | `companyId, agentId` |
| `agent_runtime_state` | `agent_runtime_state_company_updated_idx` | `companyId, updatedAt` |
| `agent_task_sessions` | `agent_task_sessions_company_agent_adapter_task_uniq` | `companyId, agentId, adapterType, taskKey, ` |
| `agent_task_sessions` | `agent_task_sessions_company_agent_updated_idx` | `companyId, agentId, updatedAt, ` |
| `agent_task_sessions` | `agent_task_sessions_company_task_updated_idx` | `companyId, taskKey, updatedAt, ` |
| `agent_wakeup_requests` | `agent_wakeup_requests_company_agent_status_idx` | `companyId, agentId, status, ` |
| `agent_wakeup_requests` | `agent_wakeup_requests_company_requested_idx` | `companyId, requestedAt, ` |
| `agent_wakeup_requests` | `agent_wakeup_requests_agent_requested_idx` | `agentId, requestedAt` |
| `agent_wakeup_requests` | `agent_wakeup_requests_review_path_recovery_idempotency_uq` | `companyId, idempotencyKey` |
| `agent_wakeup_requests` | `agent_wakeup_requests_company_payload_issue_idx` | `companyId, payload} ->> 'issueId'` |
| `agents` | `agents_company_status_idx` | `companyId, status` |
| `agents` | `agents_company_reports_to_idx` | `companyId, reportsTo` |
| `agents` | `agents_company_default_environment_idx` | `companyId, defaultEnvironmentId` |
| `approval_comments` | `approval_comments_company_idx` | `companyId` |
| `approval_comments` | `approval_comments_approval_idx` | `approvalId` |
| `approval_comments` | `approval_comments_approval_created_idx` | `approvalId, createdAt, ` |
| `approvals` | `approvals_company_status_type_idx` | `companyId, status, type, ` |
| `assets` | `assets_company_created_idx` | `companyId, createdAt` |
| `assets` | `assets_company_provider_idx` | `companyId, provider` |
| `assets` | `assets_company_object_key_uq` | `companyId, objectKey` |
| `board_api_keys` | `board_api_keys_key_hash_idx` | `keyHash` |
| `board_api_keys` | `board_api_keys_user_idx` | `userId` |
| `budget_incidents` | `budget_incidents_company_status_idx` | `companyId, status` |
| `budget_incidents` | `budget_incidents_company_scope_idx` | `companyId, scopeType, scopeId, status, ` |
| `budget_incidents` | `budget_incidents_policy_window_threshold_idx` | `policyId, windowStart, thresholdType, ` |
| `budget_policies` | `budget_policies_company_scope_active_idx` | `companyId, scopeType, scopeId, isActive, ` |
| `budget_policies` | `budget_policies_company_window_idx` | `companyId, windowKind, metric, ` |
| `budget_policies` | `budget_policies_company_scope_metric_unique_idx` | `companyId, scopeType, scopeId, metric, windowKind, ` |
| `built_in_managed_resources` | `built_in_managed_resources_company_idx` | `companyId` |
| `built_in_managed_resources` | `built_in_managed_resources_resource_idx` | `resourceKind, resourceId` |
| `built_in_managed_resources` | `built_in_managed_resources_company_bundle_resource_uq` | `companyId, bundleKey, resourceKind, resourceKey, ` |
| `cases` | `cases_company_case_number_uq` | `companyId, caseNumber` |
| `cases` | `cases_identifier_uq` | `identifier` |
| `cases` | `cases_company_type_key_uq` | `companyId, caseType, key` |
| `cases` | `cases_company_status_idx` | `companyId, status` |
| `cases` | `cases_company_type_idx` | `companyId, caseType` |
| `cases` | `cases_company_project_idx` | `companyId, projectId` |
| `cases` | `cases_parent_idx` | `parentCaseId` |
| `cases` | `case_issue_links_case_issue_uq` | `caseId, issueId` |
| `cases` | `case_issue_links_company_case_idx` | `companyId, caseId` |
| `cases` | `case_issue_links_issue_idx` | `issueId` |
| `cases` | `case_events_case_created_idx` | `caseId, createdAt` |
| `cases` | `case_events_company_case_idx` | `companyId, caseId` |
| `cases` | `case_documents_company_case_key_uq` | `companyId, caseId, key` |
| `cases` | `case_documents_document_uq` | `documentId` |
| `cases` | `case_documents_company_case_updated_idx` | `companyId, caseId, updatedAt` |
| `cases` | `case_labels_case_label_uq` | `caseId, labelId` |
| `cases` | `case_labels_company_case_idx` | `companyId, caseId` |
| `cases` | `case_labels_label_idx` | `labelId` |
| `cases` | `case_attachments_company_case_idx` | `companyId, caseId` |
| `cases` | `case_attachments_asset_uq` | `assetId` |
| `claude_setup_token_sessions` | `claude_setup_token_sessions_active_uq` | `companyId, ownerUserId, adapterType, environmentId` |
| `claude_setup_token_sessions` | `claude_setup_token_sessions_session_id_uq` | `sessionId` |
| `claude_setup_token_sessions` | `claude_setup_token_sessions_deadline_idx` | `deadlineAt` |
| `cli_auth_challenges` | `cli_auth_challenges_secret_hash_idx` | `secretHash` |
| `cli_auth_challenges` | `cli_auth_challenges_approved_by_idx` | `approvedByUserId` |
| `cli_auth_challenges` | `cli_auth_challenges_requested_company_idx` | `requestedCompanyId` |
| `companies` | `companies_issue_prefix_idx` | `issuePrefix` |
| `company_logos` | `company_logos_company_uq` | `companyId` |
| `company_logos` | `company_logos_asset_uq` | `assetId` |
| `company_memberships` | `company_memberships_company_principal_unique_idx` | `companyId, principalType, principalId, ` |
| `company_memberships` | `company_memberships_principal_status_idx` | `principalType, principalId, status, ` |
| `company_memberships` | `company_memberships_company_status_idx` | `companyId, status` |
| `company_onboarding_seeds` | `company_onboarding_seeds_company_uq` | `companyId` |
| `company_secret_bindings` | `company_secret_bindings_company_idx` | `companyId` |
| `company_secret_bindings` | `company_secret_bindings_secret_idx` | `secretId` |
| `company_secret_bindings` | `company_secret_bindings_target_idx` | `companyId, targetType, targetId` |
| `company_secret_bindings` | `company_secret_bindings_target_path_uq` | `companyId, targetType, targetId, configPath, ` |
| `company_secret_proposals` | `company_secret_proposals_company_status_idx` | `companyId, status` |
| `company_secret_proposals` | `company_secret_proposals_proposer_status_idx` | `proposedByAgentId, status` |
| `company_secret_proposals` | `company_secret_proposals_expiry_idx` | `status, expiresAt` |
| `company_secret_proposals` | `company_secret_proposals_secret_proposal_idx` | `secretProposalId` |
| `company_secret_provider_configs` | `company_secret_provider_configs_company_idx` | `companyId` |
| `company_secret_provider_configs` | `company_secret_provider_configs_company_provider_idx` | `companyId, provider` |
| `company_secret_provider_configs` | `company_secret_provider_configs_default_uq` | `companyId, provider` |
| `company_secret_versions` | `company_secret_versions_secret_idx` | `secretId, createdAt` |
| `company_secret_versions` | `company_secret_versions_value_sha256_idx` | `valueSha256` |
| `company_secret_versions` | `company_secret_versions_fingerprint_idx` | `fingerprintSha256` |
| `company_secret_versions` | `company_secret_versions_secret_version_uq` | `secretId, version` |
| `company_secrets` | `company_secrets_company_idx` | `companyId` |
| `company_secrets` | `company_secrets_company_scope_idx` | `companyId, scope` |
| `company_secrets` | `company_secrets_company_owner_idx` | `companyId, ownerUserId` |
| `company_secrets` | `company_secrets_user_definition_owner_idx` | `companyId, userSecretDefinitionId, ownerUserId, ` |
| `company_secrets` | `company_secrets_company_provider_idx` | `companyId, provider` |
| `company_secrets` | `company_secrets_provider_config_idx` | `providerConfigId` |
| `company_secrets` | `company_secrets_company_name_uq` | `companyId, name` |
| `company_secrets` | `company_secrets_company_key_uq` | `companyId, key` |
| `company_secrets` | `company_secrets_user_definition_owner_uq` | `companyId, userSecretDefinitionId, ownerUserId` |
| `company_skills` | `company_skills_company_key_idx` | `companyId, key` |
| `company_skills` | `company_skills_company_name_idx` | `companyId, name` |
| `company_skills` | `company_skills_company_folder_idx` | `companyId, folderId` |
| `company_skills` | `company_skills_company_sharing_scope_idx` | `companyId, sharingScope` |
| `company_skills` | `company_skills_company_current_version_idx` | `companyId, currentVersionId` |
| `company_skills` | `company_skills_company_forked_from_idx` | `companyId, forkedFromSkillId` |
| `company_skills` | `company_skill_versions_skill_revision_idx` | `companySkillId, revisionNumber, ` |
| `company_skills` | `company_skill_versions_skill_release_idx` | `companySkillId, releaseId` |
| `company_skills` | `company_skill_versions_company_skill_created_idx` | `companyId, companySkillId, createdAt, ` |
| `company_skills` | `company_skill_stars_skill_agent_idx` | `companySkillId, agentId` |
| `company_skills` | `company_skill_stars_skill_user_idx` | `companySkillId, userId` |
| `company_skills` | `company_skill_stars_company_skill_created_idx` | `companyId, companySkillId, createdAt, ` |
| `company_skills` | `company_skill_comments_company_skill_created_idx` | `companyId, companySkillId, createdAt, ` |
| `company_skills` | `company_skill_comments_parent_idx` | `parentCommentId` |
| `company_skills` | `company_skill_test_inputs_company_skill_name_idx` | `companyId, skillId, name, ` |
| `company_skills` | `company_skill_test_inputs_company_skill_active_idx` | `companyId, skillId, deletedAt, ` |
| `company_skills` | `company_skill_test_run_templates_company_active_idx` | `companyId, deletedAt, name, ` |
| `company_skills` | `company_skill_test_runs_company_skill_created_idx` | `companyId, skillId, createdAt, ` |
| `company_skills` | `company_skill_test_runs_company_issue_idx` | `companyId, issueId` |
| `company_skills` | `company_skill_test_runs_company_input_created_idx` | `companyId, inputId, createdAt, ` |
| `company_skills` | `company_skill_test_runs_company_status_idx` | `companyId, status` |
| `company_skills` | `company_skill_test_runs_company_harness_expires_idx` | `companyId, harnessIssueExpiresAt, ` |
| `company_transfer_runs` | `company_transfer_runs_company_idx` | `companyId` |
| `company_transfer_runs` | `company_transfer_runs_idempotency_direction_idx` | `idempotencyKey, direction, ` |
| `company_transfer_runs` | `company_transfer_runs_actor_status_idx` | `actorKey, status` |
| `company_user_sidebar_preferences` | `company_user_sidebar_preferences_company_idx` | `companyId` |
| `company_user_sidebar_preferences` | `company_user_sidebar_preferences_user_idx` | `userId` |
| `company_user_sidebar_preferences` | `company_user_sidebar_preferences_company_user_uq` | `companyId, userId, ` |
| `cost_events` | `cost_events_company_occurred_idx` | `companyId, occurredAt` |
| `cost_events` | `cost_events_company_agent_occurred_idx` | `companyId, agentId, occurredAt, ` |
| `cost_events` | `cost_events_company_provider_occurred_idx` | `companyId, provider, occurredAt, ` |
| `cost_events` | `cost_events_company_biller_occurred_idx` | `companyId, biller, occurredAt, ` |
| `cost_events` | `cost_events_company_heartbeat_run_idx` | `companyId, heartbeatRunId, ` |
| `decision_queues` | `decision_queues_company_key_uq` | `companyId, key` |
| `decision_queues` | `decision_queues_company_updated_idx` | `companyId, updatedAt` |
| `decision_queues` | `decision_queue_items_queue_source_uq` | `queueId, sourceKind, sourceId, ` |
| `decision_queues` | `decision_queue_items_company_source_idx` | `companyId, sourceKind, sourceId, ` |
| `decision_queues` | `decision_triage_company_source_uq` | `companyId, sourceKind, sourceId, ` |
| `decision_queues` | `decision_triage_company_decide_by_idx` | `companyId, decideBy` |
| `decision_queues` | `decision_triage_events_company_source_created_idx` | `companyId, sourceKind, sourceId, createdAt, ` |
| `decision_queues` | `decision_triage_events_queue_created_idx` | `queueId, createdAt` |
| `decision_queues` | `decision_retention_company_source_uq` | `companyId, sourceKind, sourceId, ` |
| `decision_queues` | `decision_retention_company_archived_idx` | `companyId, archivedAt` |
| `decision_queues` | `decision_archive_notification_outbox_uq` | `companyId, sourceKind, sourceId, archiveVersion, originAgentId, ` |
| `decision_queues` | `decision_archive_notification_outbox_pending_idx` | `status, createdAt` |
| `decision_training_examples` | `decision_training_examples_company_created_at_idx` | `companyId, createdAt, ` |
| `decision_training_examples` | `decision_training_examples_issue_idx` | `issueId` |
| `decision_training_examples` | `decision_training_examples_source_author_uq` | `sourceKind, sourceId, createdByUserId, ` |
| `decision_bundles` | `decision_bundles_company_created_at_idx` | `companyId, createdAt` |
| `decision_bundles` | `decisions_company_status_expires_at_idx` | `companyId, status, expiresAt, ` |
| `decision_bundles` | `decisions_bundle_idx` | `bundleId` |
| `decision_bundles` | `decisions_origin_issue_idx` | `originIssueId` |
| `decision_bundles` | `decisions_company_idempotency_uq` | `companyId, idempotencyKey` |
| `decision_bundles` | `decision_target_issues_decision_idx` | `decisionId` |
| `decision_bundles` | `decision_target_issues_issue_idx` | `issueId` |
| `decision_bundles` | `decision_effect_executions_decision_effect_uq` | `decisionId, effectIndex, ` |
| `decision_bundles` | `decision_effect_executions_target_issue_idx` | `targetIssueId` |
| `document_annotation_anchor_snapshots` | `document_annotation_anchor_snapshots_company_thread_created_at_idx` | `companyId, threadId, createdAt, ` |
| `document_annotation_anchor_snapshots` | `document_annotation_anchor_snapshots_company_document_revision_idx` | `companyId, documentId, toRevisionNumber, ` |
| `document_annotation_comments` | `document_annotation_comments_company_thread_created_at_idx` | `companyId, threadId, createdAt, ` |
| `document_annotation_comments` | `document_annotation_comments_company_issue_created_at_idx` | `companyId, issueId, createdAt, ` |
| `document_annotation_comments` | `document_annotation_comments_company_routine_created_at_idx` | `companyId, routineId, createdAt, ` |
| `document_annotation_comments` | `document_annotation_comments_company_case_created_at_idx` | `companyId, caseId, createdAt, ` |
| `document_annotation_comments` | `document_annotation_comments_company_document_created_at_idx` | `companyId, documentId, createdAt, ` |
| `document_annotation_comments` | `document_annotation_comments_issue_comment_idx` | `issueCommentId` |
| `document_annotation_threads` | `document_annotation_threads_company_document_status_idx` | `companyId, documentId, status, ` |
| `document_annotation_threads` | `document_annotation_threads_company_issue_status_idx` | `companyId, issueId, status, ` |
| `document_annotation_threads` | `document_annotation_threads_company_routine_status_idx` | `companyId, routineId, status, ` |
| `document_annotation_threads` | `document_annotation_threads_company_case_status_idx` | `companyId, caseId, status, ` |
| `document_annotation_threads` | `document_annotation_threads_company_current_revision_open_idx` | `companyId, documentId, currentRevisionId, status, ` |
| `document_annotation_threads` | `document_annotation_threads_company_anchor_state_idx` | `companyId, anchorState, ` |
| `document_memberships` | `document_memberships_company_user_starred_idx` | `companyId, userId, starredAt, ` |
| `document_memberships` | `document_memberships_company_user_document_uq` | `companyId, userId, documentId, ` |
| `document_revisions` | `document_revisions_document_revision_uq` | `documentId, revisionNumber, ` |
| `document_revisions` | `document_revisions_company_document_created_idx` | `companyId, documentId, createdAt, ` |
| `documents` | `documents_company_updated_idx` | `companyId, updatedAt` |
| `documents` | `documents_company_created_idx` | `companyId, createdAt` |
| `environment_custom_image_setup_sessions` | `environment_custom_image_setup_sessions_environment_status_idx` | `environmentId, status, ` |
| `environment_custom_image_setup_sessions` | `environment_custom_image_setup_sessions_environment_active_uq` | `environmentId` |
| `environment_custom_image_setup_sessions` | `environment_custom_image_setup_sessions_template_idx` | `templateId` |
| `environment_custom_image_setup_sessions` | `environment_custom_image_setup_sessions_promoted_template_idx` | `promotedTemplateId` |
| `environment_custom_image_setup_sessions` | `environment_custom_image_setup_sessions_expires_idx` | `expiresAt` |
| `environment_custom_image_setup_sessions` | `environment_custom_image_setup_sessions_provider_lease_idx` | `provider, providerLeaseId, ` |
| `environment_custom_image_templates` | `environment_custom_image_templates_environment_status_idx` | `environmentId, status, ` |
| `environment_custom_image_templates` | `environment_custom_image_templates_environment_provider_status_idx` | `environmentId, provider, status, ` |
| `environment_custom_image_templates` | `environment_custom_image_templates_environment_active_uq` | `environmentId` |
| `environment_custom_image_templates` | `environment_custom_image_templates_superseded_by_idx` | `supersededByTemplateId` |
| `environment_custom_image_templates` | `environment_custom_image_templates_last_used_idx` | `lastUsedAt` |
| `environment_leases` | `environment_leases_company_environment_status_idx` | `companyId, environmentId, status, ` |
| `environment_leases` | `environment_leases_company_execution_workspace_idx` | `companyId, executionWorkspaceId, ` |
| `environment_leases` | `environment_leases_company_issue_idx` | `companyId, issueId` |
| `environment_leases` | `environment_leases_heartbeat_run_idx` | `heartbeatRunId` |
| `environment_leases` | `environment_leases_company_last_used_idx` | `companyId, lastUsedAt` |
| `environment_leases` | `environment_leases_provider_lease_idx` | `providerLeaseId` |
| `environments` | `environments_status_idx` | `status` |
| `environments` | `environments_local_driver_idx` | `driver` |
| `environments` | `environments_managed_sandbox_idx` | `driver` |
| `environments` | `environments_name_idx` | `name` |
| `execution_workspace_runtime_leases` | `execution_workspace_runtime_leases_company_workspace_idx` | `companyId, executionWorkspaceId, ` |
| `execution_workspace_runtime_leases` | `execution_workspace_runtime_leases_company_owner_idx` | `companyId, ownerKey, ` |
| `execution_workspace_runtime_leases` | `execution_workspace_runtime_leases_expires_at_idx` | `expiresAt` |
| `execution_workspaces` | `execution_workspaces_company_project_status_idx` | `companyId, projectId, status, ` |
| `execution_workspaces` | `execution_workspaces_company_project_workspace_status_idx` | `companyId, projectWorkspaceId, status, ` |
| `execution_workspaces` | `execution_workspaces_company_source_issue_idx` | `companyId, sourceIssueId, ` |
| `execution_workspaces` | `execution_workspaces_company_last_used_idx` | `companyId, lastUsedAt, ` |
| `execution_workspaces` | `execution_workspaces_company_branch_idx` | `companyId, branchName, ` |
| `external_object_mentions` | `external_object_mentions_company_source_issue_idx` | `companyId, sourceIssueId, ` |
| `external_object_mentions` | `external_object_mentions_company_object_idx` | `companyId, objectId` |
| `external_object_mentions` | `external_object_mentions_company_provider_idx` | `companyId, providerKey, objectType, ` |
| `external_object_mentions` | `external_object_mentions_company_source_record_uq` | `companyId, sourceIssueId, sourceKind, sourceRecordId, documentKey, propertyKey, canonicalIdentityHash, ` |
| `external_object_mentions` | `external_object_mentions_company_source_null_record_uq` | `companyId, sourceIssueId, sourceKind, documentKey, propertyKey, canonicalIdentityHash, ` |
| `external_objects` | `external_objects_company_provider_object_idx` | `companyId, providerKey, objectType, ` |
| `external_objects` | `external_objects_company_provider_status_idx` | `companyId, providerKey, statusCategory, ` |
| `external_objects` | `external_objects_company_refresh_idx` | `companyId, nextRefreshAt` |
| `external_objects` | `external_objects_company_external_id_uq` | `companyId, providerKey, objectType, externalId, ` |
| `external_objects` | `external_objects_company_identity_uq` | `companyId, providerKey, objectType, canonicalIdentityHash, ` |
| `feedback_exports` | `feedback_exports_feedback_vote_idx` | `feedbackVoteId` |
| `feedback_exports` | `feedback_exports_company_created_idx` | `companyId, createdAt` |
| `feedback_exports` | `feedback_exports_company_status_idx` | `companyId, status, createdAt` |
| `feedback_exports` | `feedback_exports_company_issue_idx` | `companyId, issueId, createdAt` |
| `feedback_exports` | `feedback_exports_company_project_idx` | `companyId, projectId, createdAt` |
| `feedback_exports` | `feedback_exports_company_author_idx` | `companyId, authorUserId, createdAt` |
| `feedback_votes` | `feedback_votes_company_issue_idx` | `companyId, issueId` |
| `feedback_votes` | `feedback_votes_issue_target_idx` | `issueId, targetType, targetId` |
| `feedback_votes` | `feedback_votes_author_idx` | `authorUserId, createdAt` |
| `feedback_votes` | `feedback_votes_company_target_author_idx` | `companyId, targetType, targetId, authorUserId, ` |
| `finance_events` | `finance_events_company_occurred_idx` | `companyId, occurredAt` |
| `finance_events` | `finance_events_company_biller_occurred_idx` | `companyId, biller, occurredAt, ` |
| `finance_events` | `finance_events_company_kind_occurred_idx` | `companyId, eventKind, occurredAt, ` |
| `finance_events` | `finance_events_company_direction_occurred_idx` | `companyId, direction, occurredAt, ` |
| `finance_events` | `finance_events_company_heartbeat_run_idx` | `companyId, heartbeatRunId, ` |
| `finance_events` | `finance_events_company_cost_event_idx` | `companyId, costEventId, ` |
| `folders` | `folders_company_kind_position_idx` | `companyId, kind, position, name, ` |
| `folders` | `folders_company_kind_root_slug_uq` | `companyId, kind, slug` |
| `folders` | `folders_company_kind_parent_slug_uq` | `companyId, kind, parentId, slug` |
| `folders` | `folders_company_kind_system_key_uq` | `companyId, kind, systemKey` |
| `folders` | `folders_company_kind_parent_position_idx` | `companyId, kind, parentId, position, name, ` |
| `goals` | `goals_company_idx` | `companyId` |
| `heartbeat_run_events` | `heartbeat_run_events_run_seq_idx` | `runId, seq` |
| `heartbeat_run_events` | `heartbeat_run_events_company_run_idx` | `companyId, runId` |
| `heartbeat_run_events` | `heartbeat_run_events_company_created_idx` | `companyId, createdAt` |
| `heartbeat_run_watchdog_decisions` | `heartbeat_run_watchdog_decisions_company_run_created_idx` | `companyId, runId, createdAt, ` |
| `heartbeat_run_watchdog_decisions` | `heartbeat_run_watchdog_decisions_company_run_snooze_idx` | `companyId, runId, snoozedUntil, ` |
| `heartbeat_runs` | `heartbeat_runs_company_agent_started_idx` | `companyId, agentId, startedAt, ` |
| `heartbeat_runs` | `heartbeat_runs_company_responsible_user_idx` | `companyId, responsibleUserId, createdAt, ` |
| `heartbeat_runs` | `heartbeat_runs_company_liveness_idx` | `companyId, livenessState, createdAt, ` |
| `heartbeat_runs` | `heartbeat_runs_company_status_last_output_idx` | `companyId, status, lastOutputAt, ` |
| `heartbeat_runs` | `heartbeat_runs_company_status_process_started_idx` | `companyId, status, processStartedAt, ` |
| `heartbeat_runs` | `heartbeat_runs_company_created_at_desc_idx` | `companyId, desc(` |
| `heartbeat_runs` | `heartbeat_runs_company_ctx_issue_created_idx` | `companyId, contextSnapshot} ->> 'issueId'` |
| `heartbeat_runs` | `heartbeat_runs_company_ctx_task_created_idx` | `companyId, contextSnapshot} ->> 'taskId'` |
| `heartbeat_runs` | `heartbeat_runs_company_ctx_taskkey_created_idx` | `companyId, contextSnapshot} ->> 'taskKey'` |
| `inbox_dismissals` | `inbox_dismissals_company_user_idx` | `companyId, userId` |
| `inbox_dismissals` | `inbox_dismissals_company_item_idx` | `companyId, itemKey` |
| `inbox_dismissals` | `inbox_dismissals_company_user_item_idx` | `companyId, userId, itemKey, ` |
| `instance_settings` | `instance_settings_singleton_key_idx` | `singletonKey` |
| `instance_user_roles` | `instance_user_roles_user_role_unique_idx` | `userId, role` |
| `instance_user_roles` | `instance_user_roles_role_idx` | `role` |
| `invites` | `invites_token_hash_unique_idx` | `tokenHash` |
| `invites` | `invites_company_invite_state_idx` | `companyId, inviteType, revokedAt, expiresAt, ` |
| `issue_approvals` | `issue_approvals_issue_idx` | `issueId` |
| `issue_approvals` | `issue_approvals_approval_idx` | `approvalId` |
| `issue_approvals` | `issue_approvals_company_idx` | `companyId` |
| `issue_attachments` | `issue_attachments_company_issue_idx` | `companyId, issueId` |
| `issue_attachments` | `issue_attachments_issue_comment_idx` | `issueCommentId` |
| `issue_attachments` | `issue_attachments_asset_uq` | `assetId` |
| `issue_comments` | `issue_comments_issue_idx` | `issueId` |
| `issue_comments` | `issue_comments_company_idx` | `companyId` |
| `issue_comments` | `issue_comments_company_issue_created_at_idx` | `companyId, issueId, createdAt, ` |
| `issue_comments` | `issue_comments_company_author_issue_created_at_idx` | `companyId, authorUserId, issueId, createdAt, ` |
| `issue_create_idempotency_keys` | `issue_create_idempotency_keys_company_key_uq` | `companyId, idempotencyKey, ` |
| `issue_create_idempotency_keys` | `issue_create_idempotency_keys_issue_idx` | `issueId` |
| `issue_create_idempotency_keys` | `issue_create_idempotency_keys_company_created_at_idx` | `companyId, createdAt, ` |
| `issue_documents` | `issue_documents_company_issue_key_uq` | `companyId, issueId, key, ` |
| `issue_documents` | `issue_documents_document_uq` | `documentId` |
| `issue_documents` | `issue_documents_company_issue_updated_idx` | `companyId, issueId, updatedAt, ` |
| `issue_execution_decisions` | `issue_execution_decisions_company_issue_idx` | `companyId, issueId` |
| `issue_execution_decisions` | `issue_execution_decisions_stage_idx` | `issueId, stageId, createdAt` |
| `issue_inbox_archives` | `issue_inbox_archives_company_issue_idx` | `companyId, issueId` |
| `issue_inbox_archives` | `issue_inbox_archives_company_user_idx` | `companyId, userId` |
| `issue_inbox_archives` | `issue_inbox_archives_company_issue_user_idx` | `companyId, issueId, userId, ` |
| `issue_labels` | `issue_labels_issue_idx` | `issueId` |
| `issue_labels` | `issue_labels_label_idx` | `labelId` |
| `issue_labels` | `issue_labels_company_idx` | `companyId` |
| `issue_plan_decompositions` | `issue_plan_decompositions_company_source_status_idx` | `companyId, sourceIssueId, status, ` |
| `issue_plan_decompositions` | `issue_plan_decompositions_active_owner_idx` | `companyId, ownerAgentId` |
| `issue_plan_decompositions` | `issue_plan_decompositions_source_revision_uq` | `companyId, sourceIssueId, acceptedPlanRevisionId, ` |
| `issue_read_states` | `issue_read_states_company_issue_idx` | `companyId, issueId` |
| `issue_read_states` | `issue_read_states_company_user_idx` | `companyId, userId` |
| `issue_read_states` | `issue_read_states_company_issue_user_idx` | `companyId, issueId, userId, ` |
| `issue_recovery_actions` | `issue_recovery_actions_company_source_status_idx` | `companyId, sourceIssueId, status, ` |
| `issue_recovery_actions` | `issue_recovery_actions_company_owner_status_idx` | `companyId, ownerAgentId, status, ` |
| `issue_recovery_actions` | `issue_recovery_actions_company_recovery_issue_idx` | `companyId, recoveryIssueId, ` |
| `issue_recovery_actions` | `issue_recovery_actions_active_source_uq` | `companyId, sourceIssueId` |
| `issue_recovery_actions` | `issue_recovery_actions_active_fingerprint_uq` | `companyId, sourceIssueId, cause, fingerprint` |
| `issue_reference_mentions` | `issue_reference_mentions_company_source_issue_idx` | `companyId, sourceIssueId, ` |
| `issue_reference_mentions` | `issue_reference_mentions_company_target_issue_idx` | `companyId, targetIssueId, ` |
| `issue_reference_mentions` | `issue_reference_mentions_company_issue_pair_idx` | `companyId, sourceIssueId, targetIssueId, ` |
| `issue_reference_mentions` | `issue_reference_mentions_company_source_mention_record_uq` | `companyId, sourceIssueId, targetIssueId, sourceKind, sourceRecordId, ` |
| `issue_reference_mentions` | `issue_reference_mentions_company_source_mention_null_record_uq` | `companyId, sourceIssueId, targetIssueId, sourceKind, ` |
| `issue_relations` | `issue_relations_company_issue_idx` | `companyId, issueId` |
| `issue_relations` | `issue_relations_company_related_issue_idx` | `companyId, relatedIssueId` |
| `issue_relations` | `issue_relations_company_type_idx` | `companyId, type` |
| `issue_relations` | `issue_relations_company_edge_uq` | `companyId, issueId, relatedIssueId, type, ` |
| `issue_thread_interactions` | `issue_thread_interactions_issue_idx` | `issueId` |
| `issue_thread_interactions` | `issue_thread_interactions_company_issue_created_at_idx` | `companyId, issueId, createdAt, ` |
| `issue_thread_interactions` | `issue_thread_interactions_company_issue_status_idx` | `companyId, issueId, status, ` |
| `issue_thread_interactions` | `issue_thread_interactions_company_issue_idempotency_uq` | `companyId, issueId, idempotencyKey` |
| `issue_thread_interactions` | `issue_thread_interactions_source_comment_idx` | `sourceCommentId` |
| `issue_thread_interactions` | `issue_thread_interactions_addressee_agent_idx` | `addresseeAgentId` |
| `issue_tree_hold_members` | `issue_tree_hold_members_hold_issue_uq` | `holdId, issueId` |
| `issue_tree_hold_members` | `issue_tree_hold_members_company_issue_idx` | `companyId, issueId` |
| `issue_tree_hold_members` | `issue_tree_hold_members_hold_depth_idx` | `holdId, depth` |
| `issue_tree_holds` | `issue_tree_holds_company_root_status_idx` | `companyId, rootIssueId, status, ` |
| `issue_tree_holds` | `issue_tree_holds_company_status_mode_idx` | `companyId, status, mode` |
| `issue_watchdogs` | `issue_watchdogs_company_issue_uq` | `companyId, issueId` |
| `issue_watchdogs` | `issue_watchdogs_company_status_idx` | `companyId, status` |
| `issue_watchdogs` | `issue_watchdogs_company_agent_idx` | `companyId, watchdogAgentId` |
| `issue_watchdogs` | `issue_watchdogs_company_watchdog_issue_uq` | `companyId, watchdogIssueId` |
| `issue_work_products` | `issue_work_products_company_issue_type_idx` | `companyId, issueId, type, ` |
| `issue_work_products` | `issue_work_products_company_execution_workspace_type_idx` | `companyId, executionWorkspaceId, type, ` |
| `issue_work_products` | `issue_work_products_company_provider_external_id_idx` | `companyId, provider, externalId, ` |
| `issue_work_products` | `issue_work_products_company_updated_idx` | `companyId, updatedAt, ` |
| `issues` | `issues_company_status_idx` | `companyId, status` |
| `issues` | `issues_company_harness_kind_idx` | `companyId, harnessKind` |
| `issues` | `issues_company_assignee_status_idx` | `companyId, assigneeAgentId, status, ` |
| `issues` | `issues_company_assignee_user_status_idx` | `companyId, assigneeUserId, status, ` |
| `issues` | `issues_company_responsible_user_idx` | `companyId, responsibleUserId` |
| `issues` | `issues_company_parent_idx` | `companyId, parentId` |
| `issues` | `issues_company_project_idx` | `companyId, projectId` |
| `issues` | `issues_company_origin_idx` | `companyId, originKind, originId` |
| `issues` | `issues_company_project_workspace_idx` | `companyId, projectWorkspaceId` |
| `issues` | `issues_company_execution_workspace_idx` | `companyId, executionWorkspaceId` |
| `issues` | `issues_company_monitor_due_idx` | `companyId, monitorNextCheckAt` |
| `issues` | `issues_company_updated_idx` | `companyId, updatedAt` |
| `issues` | `issues_company_created_idx` | `companyId, createdAt` |
| `issues` | `issues_open_normalized_title_created_idx` | `companyId, parentId, title}` |
| `issues` | `issues_company_priority_idx` | `companyId, priority` |
| `issues` | `issues_identifier_idx` | `identifier` |
| `issues` | `issues_open_routine_execution_uq` | `companyId, originKind, originId, originFingerprint` |
| `issues` | `issues_active_liveness_recovery_incident_uq` | `companyId, originKind, originId` |
| `issues` | `issues_active_liveness_recovery_leaf_uq` | `companyId, originKind, originFingerprint` |
| `issues` | `issues_active_stale_run_evaluation_uq` | `companyId, originKind, originId` |
| `issues` | `issues_active_task_watchdog_uq` | `companyId, originKind, originId` |
| `issues` | `issues_active_productivity_review_uq` | `companyId, originKind, originId` |
| `issues` | `issues_active_stranded_issue_recovery_uq` | `companyId, originKind, originId` |
| `issues` | `issues_onboarding_first_task_uq` | `companyId` |
| `join_requests` | `join_requests_invite_unique_idx` | `inviteId` |
| `join_requests` | `join_requests_company_status_type_created_idx` | `companyId, status, requestType, createdAt, ` |
| `join_requests` | `join_requests_pending_human_user_uq` | `companyId, requestingUserId` |
| `join_requests` | `join_requests_pending_human_email_uq` | `companyId, requestEmailSnapshot}` |
| `labels` | `labels_company_idx` | `companyId` |
| `labels` | `labels_company_name_idx` | `companyId, name` |
| `pipeline_case_events` | `pipeline_case_events_case_created_idx` | `caseId, createdAt` |
| `pipeline_case_events` | `pipeline_case_events_company_case_idx` | `companyId, caseId` |
| `pipeline_cases` | `pipeline_cases_pipeline_case_key_uq` | `pipelineId, caseKey` |
| `pipeline_cases` | `pipeline_cases_parent_request_key_uq` | `parentCaseId, requestKey` |
| `pipeline_cases` | `pipeline_cases_company_idx` | `companyId` |
| `pipeline_cases` | `pipeline_cases_pipeline_stage_idx` | `pipelineId, stageId` |
| `pipeline_cases` | `pipeline_cases_parent_idx` | `parentCaseId` |
| `pipeline_cases` | `pipeline_cases_automation_attempt_idx` | `automationAttemptId` |
| `pipeline_cases` | `pipeline_cases_retired_idx` | `companyId, retiredAt` |
| `pipeline_cases` | `pipeline_cases_lease_expires_idx` | `leaseExpiresAt` |
| `pipeline_cases` | `pipeline_case_issue_links_case_issue_uq` | `caseId, issueId` |
| `pipeline_cases` | `pipeline_case_issue_links_issue_idx` | `issueId` |
| `pipeline_cases` | `pipeline_case_issue_links_company_case_idx` | `companyId, caseId` |
| `pipeline_cases` | `pipeline_case_issue_links_automation_attempt_idx` | `automationAttemptId` |
| `pipeline_cases` | `pipeline_case_blockers_case_blocked_by_uq` | `caseId, blockedByCaseId` |
| `pipeline_cases` | `pipeline_case_blockers_blocked_by_idx` | `blockedByCaseId` |
| `pipeline_cases` | `pipeline_case_blockers_company_case_idx` | `companyId, caseId` |
| `pipeline_cases` | `pipeline_documents_company_pipeline_key_uq` | `companyId, pipelineId, key, ` |
| `pipeline_cases` | `pipeline_documents_document_uq` | `documentId` |
| `pipeline_cases` | `pipeline_documents_company_pipeline_updated_idx` | `companyId, pipelineId, updatedAt, ` |
| `pipeline_cases` | `pipeline_case_documents_company_case_key_uq` | `companyId, caseId, key, ` |
| `pipeline_cases` | `pipeline_case_documents_document_uq` | `documentId` |
| `pipeline_cases` | `pipeline_case_documents_company_case_updated_idx` | `companyId, caseId, updatedAt, ` |
| `pipeline_cases` | `pipeline_automation_executions_idempotency_uq` | `caseId, automationId, triggeringEventId, ` |
| `pipeline_cases` | `pipeline_automation_executions_company_case_idx` | `companyId, caseId` |
| `pipeline_cases` | `pipeline_automation_executions_routine_idx` | `routineId` |
| `pipeline_cases` | `pipeline_automation_executions_execution_issue_idx` | `executionIssueId` |
| `pipeline_cases` | `pipeline_automation_executions_retry_of_execution_idx` | `retryOfExecutionId` |
| `pipelines` | `pipelines_company_key_uq` | `companyId, key` |
| `pipelines` | `pipelines_company_idx` | `companyId` |
| `pipelines` | `pipelines_company_project_idx` | `companyId, projectId` |
| `pipelines` | `pipeline_stages_pipeline_key_uq` | `pipelineId, key` |
| `pipelines` | `pipeline_stages_pipeline_position_idx` | `pipelineId, position` |
| `pipelines` | `pipeline_transitions_pipeline_edge_uq` | `pipelineId, fromStageId, toStageId, ` |
| `pipelines` | `pipeline_transitions_pipeline_from_idx` | `pipelineId, fromStageId` |
| `pipelines` | `pipeline_transitions_pipeline_to_idx` | `pipelineId, toStageId` |
| `plugin_company_settings` | `plugin_company_settings_company_idx` | `companyId` |
| `plugin_company_settings` | `plugin_company_settings_plugin_idx` | `pluginId` |
| `plugin_company_settings` | `plugin_company_settings_company_plugin_uq` | `companyId, pluginId, ` |
| `plugin_config` | `plugin_config_plugin_company_idx` | `pluginId, companyId, ` |
| `plugin_database_namespaces` | `plugin_database_namespaces_plugin_idx` | `pluginId` |
| `plugin_database_namespaces` | `plugin_database_namespaces_namespace_idx` | `namespaceName` |
| `plugin_database_namespaces` | `plugin_database_namespaces_status_idx` | `status` |
| `plugin_database_namespaces` | `plugin_migrations_plugin_key_idx` | `pluginId, migrationKey, ` |
| `plugin_database_namespaces` | `plugin_migrations_plugin_idx` | `pluginId` |
| `plugin_database_namespaces` | `plugin_migrations_status_idx` | `status` |
| `plugin_entities` | `plugin_entities_plugin_idx` | `pluginId` |
| `plugin_entities` | `plugin_entities_company_idx` | `companyId` |
| `plugin_entities` | `plugin_entities_type_idx` | `entityType` |
| `plugin_entities` | `plugin_entities_scope_idx` | `scopeKind, scopeId` |
| `plugin_jobs` | `plugin_jobs_plugin_idx` | `pluginId` |
| `plugin_jobs` | `plugin_jobs_next_run_idx` | `nextRunAt` |
| `plugin_jobs` | `plugin_jobs_unique_idx` | `pluginId, jobKey` |
| `plugin_jobs` | `plugin_job_runs_job_idx` | `jobId` |
| `plugin_jobs` | `plugin_job_runs_plugin_idx` | `pluginId` |
| `plugin_jobs` | `plugin_job_runs_company_idx` | `companyId` |
| `plugin_jobs` | `plugin_job_runs_status_idx` | `status` |
| `plugin_logs` | `plugin_logs_plugin_time_idx` | `pluginId, createdAt, ` |
| `plugin_logs` | `plugin_logs_company_idx` | `companyId` |
| `plugin_logs` | `plugin_logs_level_idx` | `level` |
| `plugin_managed_resources` | `plugin_managed_resources_company_idx` | `companyId` |
| `plugin_managed_resources` | `plugin_managed_resources_plugin_idx` | `pluginId` |
| `plugin_managed_resources` | `plugin_managed_resources_resource_idx` | `resourceKind, resourceId` |
| `plugin_managed_resources` | `plugin_managed_resources_company_plugin_resource_uq` | `companyId, pluginId, resourceKind, resourceKey, ` |
| `plugin_state` | `plugin_state_plugin_scope_idx` | `pluginId, scopeKind, ` |
| `plugin_webhook_deliveries` | `plugin_webhook_deliveries_plugin_idx` | `pluginId` |
| `plugin_webhook_deliveries` | `plugin_webhook_deliveries_company_idx` | `companyId` |
| `plugin_webhook_deliveries` | `plugin_webhook_deliveries_status_idx` | `status` |
| `plugin_webhook_deliveries` | `plugin_webhook_deliveries_key_idx` | `webhookKey` |
| `plugins` | `plugins_plugin_key_idx` | `pluginKey` |
| `plugins` | `plugins_status_idx` | `status` |
| `principal_permission_grants` | `principal_permission_grants_unique_idx` | `companyId, principalType, principalId, permissionKey, ` |
| `principal_permission_grants` | `principal_permission_grants_company_permission_idx` | `companyId, permissionKey, ` |
| `project_goals` | `project_goals_project_idx` | `projectId` |
| `project_goals` | `project_goals_goal_idx` | `goalId` |
| `project_goals` | `project_goals_company_idx` | `companyId` |
| `project_memberships` | `project_memberships_company_user_idx` | `companyId, userId` |
| `project_memberships` | `project_memberships_company_user_starred_idx` | `companyId, userId, starredAt, ` |
| `project_memberships` | `project_memberships_project_idx` | `projectId` |
| `project_memberships` | `project_memberships_company_user_project_uq` | `companyId, userId, projectId, ` |
| `project_workspaces` | `project_workspaces_company_project_idx` | `companyId, projectId` |
| `project_workspaces` | `project_workspaces_project_primary_idx` | `projectId, isPrimary` |
| `project_workspaces` | `project_workspaces_project_source_type_idx` | `projectId, sourceType` |
| `project_workspaces` | `project_workspaces_company_shared_key_idx` | `companyId, sharedWorkspaceKey` |
| `project_workspaces` | `project_workspaces_project_remote_ref_idx` | `projectId, remoteProvider, remoteWorkspaceRef` |
| `projects` | `projects_company_idx` | `companyId` |
| `routine_documents` | `routine_documents_company_routine_key_uq` | `companyId, routineId, key, ` |
| `routine_documents` | `routine_documents_document_uq` | `documentId` |
| `routine_documents` | `routine_documents_company_routine_updated_idx` | `companyId, routineId, updatedAt, ` |
| `routines` | `routines_company_status_idx` | `companyId, status` |
| `routines` | `routines_company_assignee_idx` | `companyId, assigneeAgentId` |
| `routines` | `routines_company_project_idx` | `companyId, projectId` |
| `routines` | `routines_company_folder_idx` | `companyId, folderId` |
| `routines` | `routines_company_responsible_user_idx` | `companyId, responsibleUserId` |
| `routines` | `routines_company_origin_idx` | `companyId, originKind, originId` |
| `routines` | `routine_revisions_routine_revision_uq` | `routineId, revisionNumber, ` |
| `routines` | `routine_revisions_company_routine_created_idx` | `companyId, routineId, createdAt, ` |
| `routines` | `routine_revisions_company_responsible_user_idx` | `companyId, responsibleUserId, createdAt, ` |
| `routines` | `routine_triggers_company_routine_idx` | `companyId, routineId` |
| `routines` | `routine_triggers_company_kind_idx` | `companyId, kind` |
| `routines` | `routine_triggers_next_run_idx` | `nextRunAt` |
| `routines` | `routine_triggers_public_id_idx` | `publicId` |
| `routines` | `routine_triggers_public_id_uq` | `publicId` |
| `routines` | `routine_runs_company_routine_idx` | `companyId, routineId, createdAt` |
| `routines` | `routine_runs_revision_idx` | `routineRevisionId` |
| `routines` | `routine_runs_company_responsible_user_idx` | `companyId, responsibleUserId, createdAt, ` |
| `routines` | `routine_runs_trigger_idx` | `triggerId, createdAt` |
| `routines` | `routine_runs_dispatch_fingerprint_idx` | `routineId, dispatchFingerprint` |
| `routines` | `routine_runs_linked_issue_idx` | `linkedIssueId` |
| `routines` | `routine_runs_trigger_idempotency_idx` | `triggerId, idempotencyKey` |
| `secret_access_events` | `secret_access_events_company_created_idx` | `companyId, createdAt` |
| `secret_access_events` | `secret_access_events_secret_created_idx` | `secretId, createdAt` |
| `secret_access_events` | `secret_access_events_user_definition_created_idx` | `userSecretDefinitionId, createdAt, ` |
| `secret_access_events` | `secret_access_events_company_credential_owner_idx` | `companyId, credentialOwnerUserId, createdAt, ` |
| `secret_access_events` | `secret_access_events_consumer_idx` | `companyId, consumerType, consumerId` |
| `secret_access_events` | `secret_access_events_run_idx` | `heartbeatRunId` |
| `smoke_runs` | `smoke_runs_company_started_idx` | `companyId, startedAt` |
| `smoke_runs` | `smoke_runs_company_status_idx` | `companyId, status` |
| `smoke_runs` | `smoke_run_steps_company_run_idx` | `companyId, runId` |
| `smoke_runs` | `smoke_run_steps_company_path_idx` | `companyId, path` |
| `status_cards` | `status_cards_company_archived_idx` | `companyId, archivedAt` |
| `status_cards` | `status_cards_company_next_eval_idx` | `companyId, nextEvalAt` |
| `status_cards` | `status_card_updates_card_started_idx` | `cardId, startedAt` |
| `status_cards` | `status_card_updates_generation_issue_idx` | `generationIssueId` |
| `summary_slots` | `summary_slots_document_uq` | `documentId` |
| `summary_slots` | `summary_slots_company_scope_idx` | `companyId, scopeKind, scopeId` |
| `summary_slots` | `summary_slots_company_generating_issue_idx` | `companyId, generatingIssueId, ` |
| `summary_slots` | `summary_slots_company_updated_idx` | `companyId, updatedAt` |
| `tool_applications` | `tool_applications_company_idx` | `companyId` |
| `tool_applications` | `tool_applications_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_applications_company_name_uq` | `companyId, name` |
| `tool_applications` | `tool_applications_company_key_uq` | `companyId, applicationKey` |
| `tool_applications` | `tool_connections_company_idx` | `companyId` |
| `tool_applications` | `tool_connections_application_idx` | `applicationId` |
| `tool_applications` | `tool_connections_company_enabled_idx` | `companyId, enabled` |
| `tool_applications` | `tool_connections_company_uid_uq` | `companyId, uid` |
| `tool_applications` | `connection_grants_company_connection_idx` | `companyId, connectionId` |
| `tool_applications` | `connection_grants_subject_user_idx` | `companyId, subjectUserId` |
| `tool_applications` | `connection_grants_user_uq` | `connectionId, subjectUserId` |
| `tool_applications` | `connection_grants_default_uq` | `connectionId` |
| `tool_applications` | `tool_connection_installs_company_target_idx` | `companyId, targetType, targetId` |
| `tool_applications` | `tool_connection_installs_connection_idx` | `companyId, connectionId` |
| `tool_applications` | `tool_connection_installs_target_uq` | `companyId, connectionId, targetType, targetId, ` |
| `tool_applications` | `tool_oauth_states_company_idx` | `companyId` |
| `tool_applications` | `tool_oauth_states_connection_idx` | `connectionId` |
| `tool_applications` | `tool_oauth_states_actor_idx` | `createdByActorType, createdByActorId` |
| `tool_applications` | `tool_oauth_states_expires_at_idx` | `expiresAt` |
| `tool_applications` | `tool_catalog_entries_company_idx` | `companyId` |
| `tool_applications` | `tool_catalog_entries_application_idx` | `applicationId` |
| `tool_applications` | `tool_catalog_entries_connection_idx` | `connectionId` |
| `tool_applications` | `tool_catalog_entries_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_catalog_entries_connection_name_uq` | `connectionId, name` |
| `tool_applications` | `tool_profiles_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_profiles_company_key_uq` | `companyId, profileKey` |
| `tool_applications` | `tool_profiles_company_name_uq` | `companyId, name` |
| `tool_applications` | `tool_profile_entries_company_profile_idx` | `companyId, profileId` |
| `tool_applications` | `tool_profile_entries_application_idx` | `companyId, applicationId` |
| `tool_applications` | `tool_profile_entries_connection_idx` | `companyId, connectionId` |
| `tool_applications` | `tool_profile_entries_catalog_entry_idx` | `companyId, catalogEntryId` |
| `tool_applications` | `tool_profile_bindings_company_target_idx` | `companyId, targetType, targetId` |
| `tool_applications` | `tool_profile_bindings_target_profile_uq` | `companyId, targetType, targetId, profileId, ` |
| `tool_applications` | `tool_mcp_gateways_company_idx` | `companyId` |
| `tool_applications` | `tool_mcp_gateways_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_mcp_gateways_profile_idx` | `companyId, profileId` |
| `tool_applications` | `tool_mcp_gateways_public_id_uq` | `gatewayPublicId` |
| `tool_applications` | `tool_mcp_gateways_company_slug_uq` | `companyId, slug` |
| `tool_applications` | `tool_mcp_gateways_company_name_uq` | `companyId, name` |
| `tool_applications` | `tool_mcp_gateway_tokens_token_hash_uq` | `tokenHash` |
| `tool_applications` | `tool_mcp_gateway_tokens_gateway_idx` | `companyId, gatewayId` |
| `tool_applications` | `tool_mcp_gateway_tokens_subject_idx` | `companyId, subjectType, subjectId` |
| `tool_applications` | `tool_mcp_gateway_tokens_company_expires_idx` | `companyId, expiresAt` |
| `tool_applications` | `tool_policies_company_enabled_idx` | `companyId, enabled` |
| `tool_applications` | `tool_policies_company_type_idx` | `companyId, policyType` |
| `tool_applications` | `tool_policies_company_name_uq` | `companyId, name` |
| `tool_applications` | `tool_runtime_slots_company_idx` | `companyId` |
| `tool_applications` | `tool_runtime_slots_connection_idx` | `connectionId` |
| `tool_applications` | `tool_runtime_slots_execution_workspace_idx` | `companyId, executionWorkspaceId` |
| `tool_applications` | `tool_runtime_slots_slot_key_uq` | `companyId, slotKey` |
| `tool_applications` | `tool_stdio_command_templates_company_idx` | `companyId` |
| `tool_applications` | `tool_stdio_command_templates_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_stdio_command_templates_company_key_uq` | `companyId, templateKey` |
| `tool_applications` | `tool_gateway_sessions_token_hash_uq` | `tokenHash` |
| `tool_applications` | `tool_gateway_sessions_company_agent_idx` | `companyId, agentId` |
| `tool_applications` | `tool_gateway_sessions_company_expires_idx` | `companyId, expiresAt` |
| `tool_applications` | `tool_gateway_sessions_run_idx` | `companyId, runId` |
| `tool_applications` | `tool_gateway_sessions_issue_idx` | `companyId, issueId` |
| `tool_applications` | `tool_gateway_sessions_gateway_idx` | `companyId, gatewayId` |
| `tool_applications` | `tool_gateway_rate_limit_counters_company_idx` | `companyId` |
| `tool_applications` | `tool_gateway_rate_limit_counters_window_uq` | `companyId, counterKey, windowStartAt, ` |
| `tool_applications` | `tool_invocations_company_created_idx` | `companyId, createdAt` |
| `tool_applications` | `tool_invocations_run_idx` | `companyId, runId` |
| `tool_applications` | `tool_invocations_issue_idx` | `companyId, issueId` |
| `tool_applications` | `tool_invocations_gateway_idx` | `companyId, gatewayId` |
| `tool_applications` | `tool_invocations_company_idempotency_uq` | `companyId, idempotencyKey` |
| `tool_applications` | `tool_action_requests_company_status_idx` | `companyId, status` |
| `tool_applications` | `tool_action_requests_invocation_idx` | `invocationId` |
| `tool_applications` | `tool_action_requests_issue_idx` | `companyId, issueId` |
| `tool_applications` | `tool_call_events_company_created_idx` | `companyId, createdAt` |
| `tool_applications` | `tool_call_events_run_idx` | `companyId, runId` |
| `tool_applications` | `tool_call_events_issue_idx` | `companyId, issueId` |
| `tool_applications` | `tool_call_events_invocation_idx` | `invocationId` |
| `tool_applications` | `tool_call_events_gateway_idx` | `companyId, gatewayId` |
| `tool_applications` | `connection_token_issuances_company_created_idx` | `companyId, createdAt` |
| `tool_applications` | `connection_token_issuances_connection_created_idx` | `companyId, connectionId, createdAt` |
| `tool_applications` | `connection_token_issuances_agent_connection_idx` | `companyId, agentId, connectionId, createdAt` |
| `tool_applications` | `connection_token_issuances_run_idx` | `companyId, runId` |
| `tool_applications` | `tool_rate_limit_counters_company_idx` | `companyId` |
| `tool_applications` | `tool_rate_limit_counters_window_uq` | `companyId, policyId, counterKey, windowKind, windowStartAt, ` |
| `tool_applications` | `tool_runtime_metric_counters_company_metric_idx` | `companyId, metric, bucketStartAt` |
| `tool_applications` | `tool_runtime_metric_counters_bucket_uq` | `companyId, metric, bucketStartAt` |
| `tool_applications` | `tool_access_audit_company_created_idx` | `companyId, createdAt` |
| `tool_applications` | `tool_access_audit_connection_idx` | `connectionId` |
| `tool_applications` | `tool_access_audit_gateway_idx` | `companyId, gatewayId` |
| `user_inbox_agent_policies` | `user_inbox_agent_policies_company_user_uq` | `companyId, userId, ` |
| `user_secret_declarations` | `user_secret_declarations_company_idx` | `companyId` |
| `user_secret_declarations` | `user_secret_declarations_definition_idx` | `userSecretDefinitionId` |
| `user_secret_declarations` | `user_secret_declarations_target_idx` | `companyId, targetType, targetId` |
| `user_secret_declarations` | `user_secret_declarations_company_required_idx` | `companyId, required` |
| `user_secret_declarations` | `user_secret_declarations_target_path_uq` | `companyId, targetType, targetId, configPath, ` |
| `user_secret_declarations` | `user_secret_declarations_required_override_idx` | `companyId, allowMissingOverride` |
| `user_secret_definitions` | `user_secret_definitions_company_status_idx` | `companyId, status` |
| `user_secret_definitions` | `user_secret_definitions_company_provider_idx` | `companyId, provider` |
| `user_secret_definitions` | `user_secret_definitions_provider_config_idx` | `providerConfigId` |
| `user_secret_definitions` | `user_secret_definitions_company_key_uq` | `companyId, key` |
| `user_sidebar_preferences` | `user_sidebar_preferences_user_uq` | `userId` |
| `workspace_operations` | `workspace_operations_company_run_started_idx` | `companyId, heartbeatRunId, startedAt, ` |
| `workspace_operations` | `workspace_operations_company_workspace_started_idx` | `companyId, executionWorkspaceId, startedAt, ` |
| `workspace_operations` | `workspace_operations_company_workspace_issue_started_idx` | `companyId, executionWorkspaceId, issueId, startedAt, ` |
| `workspace_runtime_services` | `workspace_runtime_services_company_workspace_status_idx` | `companyId, projectWorkspaceId, status, ` |
| `workspace_runtime_services` | `workspace_runtime_services_company_execution_workspace_status_idx` | `companyId, executionWorkspaceId, status, ` |
| `workspace_runtime_services` | `workspace_runtime_services_company_project_status_idx` | `companyId, projectId, status, ` |
| `workspace_runtime_services` | `workspace_runtime_services_run_idx` | `startedByRunId` |
| `workspace_runtime_services` | `workspace_runtime_services_company_updated_idx` | `companyId, updatedAt, ` |
