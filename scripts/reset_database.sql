-- ============================================
-- Parrot Agent Database Reset Script
-- ============================================
-- 此脚本会清空所有表的数据，但保留 schema
-- 用于端到端测试前的数据清理
-- 
-- 使用方法:
--   psql -U postgres -d parrot_agent_dev -f scripts/reset_database.sql
--   或者在代码中执行此脚本
-- ============================================

BEGIN;

-- 禁用外键约束检查（提高性能）
SET session_replication_role = 'replica';

-- ============================================
-- 按照依赖关系顺序清空表（从依赖表到被依赖表）
-- ============================================

-- 1. 清空最上层的依赖表
TRUNCATE TABLE activity_logs CASCADE;
TRUNCATE TABLE agent_api_keys CASCADE;
TRUNCATE TABLE agent_config_revisions CASCADE;
TRUNCATE TABLE agent_memberships CASCADE;
TRUNCATE TABLE agent_wakeup_requests CASCADE;
TRUNCATE TABLE annotation_comments CASCADE;
TRUNCATE TABLE annotation_threads CASCADE;
TRUNCATE TABLE approval_comments CASCADE;
TRUNCATE TABLE assets CASCADE;
TRUNCATE TABLE attachments CASCADE;
TRUNCATE TABLE board_api_keys CASCADE;
TRUNCATE TABLE budget_incidents CASCADE;
TRUNCATE TABLE budget_policies CASCADE;
TRUNCATE TABLE case_attachments CASCADE;
TRUNCATE TABLE case_documents CASCADE;
TRUNCATE TABLE case_events CASCADE;
TRUNCATE TABLE case_issue_links CASCADE;
TRUNCATE TABLE case_labels CASCADE;
TRUNCATE TABLE cli_auth_challenges CASCADE;
TRUNCATE TABLE cloud_upstream_runs CASCADE;
TRUNCATE TABLE company_memberships CASCADE;
TRUNCATE TABLE company_secret_bindings CASCADE;
TRUNCATE TABLE company_secret_proposals CASCADE;
TRUNCATE TABLE company_secret_versions CASCADE;
TRUNCATE TABLE company_skill_policies CASCADE;
TRUNCATE TABLE company_team_installs CASCADE;
TRUNCATE TABLE company_user_sidebar_preferences CASCADE;
TRUNCATE TABLE cost_events CASCADE;
TRUNCATE TABLE decision_archive_notification_outbox CASCADE;
TRUNCATE TABLE decision_bundles CASCADE;
TRUNCATE TABLE decision_effect_executions CASCADE;
TRUNCATE TABLE decision_queue_items CASCADE;
TRUNCATE TABLE decision_retention CASCADE;
TRUNCATE TABLE decision_target_issues CASCADE;
TRUNCATE TABLE decision_training_examples CASCADE;
TRUNCATE TABLE decision_triage CASCADE;
TRUNCATE TABLE decision_triage_events CASCADE;
TRUNCATE TABLE decisions CASCADE;
TRUNCATE TABLE document_annotation_comments CASCADE;
TRUNCATE TABLE document_annotation_threads CASCADE;
TRUNCATE TABLE document_revisions CASCADE;
TRUNCATE TABLE environment_custom_image_setup_sessions CASCADE;
TRUNCATE TABLE environment_leases CASCADE;
TRUNCATE TABLE feedback_traces CASCADE;
TRUNCATE TABLE feedback_votes CASCADE;
TRUNCATE TABLE finance_events CASCADE;
TRUNCATE TABLE folder_items CASCADE;
TRUNCATE TABLE heartbeat_run_watchdog_decisions CASCADE;
TRUNCATE TABLE heartbeat_runs CASCADE;
TRUNCATE TABLE inbox_dismissals CASCADE;
TRUNCATE TABLE instance_user_roles CASCADE;
TRUNCATE TABLE issue_approvals CASCADE;
TRUNCATE TABLE issue_comments CASCADE;
TRUNCATE TABLE issue_documents CASCADE;
TRUNCATE TABLE issue_inbox_archives CASCADE;
TRUNCATE TABLE issue_labels CASCADE;
TRUNCATE TABLE issue_plan_decompositions CASCADE;
TRUNCATE TABLE issue_read_status CASCADE;
TRUNCATE TABLE issue_relations CASCADE;
TRUNCATE TABLE issue_thread_interactions CASCADE;
TRUNCATE TABLE issue_tree_hold_members CASCADE;
TRUNCATE TABLE issue_tree_holds CASCADE;
TRUNCATE TABLE issue_watchdogs CASCADE;
TRUNCATE TABLE issue_work_products CASCADE;
TRUNCATE TABLE pipeline_case_events CASCADE;
TRUNCATE TABLE pipeline_cases CASCADE;
TRUNCATE TABLE pipeline_transitions CASCADE;
TRUNCATE TABLE plan_decompositions CASCADE;
TRUNCATE TABLE plugin_data CASCADE;
TRUNCATE TABLE plugin_job_runs CASCADE;
TRUNCATE TABLE plugin_jobs CASCADE;
TRUNCATE TABLE plugin_logs CASCADE;
TRUNCATE TABLE plugin_managed_resources CASCADE;
TRUNCATE TABLE principal_permission_grants CASCADE;
TRUNCATE TABLE project_goals CASCADE;
TRUNCATE TABLE project_memberships CASCADE;
TRUNCATE TABLE project_workspaces CASCADE;
TRUNCATE TABLE recovery_actions CASCADE;
TRUNCATE TABLE routine_documents CASCADE;
TRUNCATE TABLE routine_revisions CASCADE;
TRUNCATE TABLE routine_runs CASCADE;
TRUNCATE TABLE routine_triggers CASCADE;
TRUNCATE TABLE secret_access_events CASCADE;
TRUNCATE TABLE skill_comments CASCADE;
TRUNCATE TABLE skill_files CASCADE;
TRUNCATE TABLE skill_stars CASCADE;
TRUNCATE TABLE skill_test_inputs CASCADE;
TRUNCATE TABLE skill_test_run_templates CASCADE;
TRUNCATE TABLE skill_test_runs CASCADE;
TRUNCATE TABLE smoke_run_steps CASCADE;
TRUNCATE TABLE smoke_runs CASCADE;
TRUNCATE TABLE status_card_summary_revisions CASCADE;
TRUNCATE TABLE status_card_update_runs CASCADE;
TRUNCATE TABLE status_card_updates CASCADE;
TRUNCATE TABLE thread_interactions CASCADE;
TRUNCATE TABLE tool_action_requests CASCADE;
TRUNCATE TABLE tool_call_events CASCADE;
TRUNCATE TABLE status_card_summary_revisions CASCADE;
TRUNCATE TABLE tool_gateway_sessions CASCADE;
TRUNCATE TABLE tool_invocations CASCADE;
TRUNCATE TABLE tool_mcp_gateway_tokens CASCADE;
TRUNCATE TABLE tool_profile_bindings CASCADE;
TRUNCATE TABLE tool_profile_entries CASCADE;
TRUNCATE TABLE user_secret_declarations CASCADE;
TRUNCATE TABLE user_sidebar_preferences CASCADE;
TRUNCATE TABLE workspace_operations CASCADE;

-- 2. 清空中层表
TRUNCATE TABLE approvals CASCADE;
TRUNCATE TABLE auth_sessions CASCADE;
TRUNCATE TABLE cases CASCADE;
TRUNCATE TABLE cloud_upstream_connections CASCADE;
TRUNCATE TABLE company_secret_provider_configs CASCADE;
TRUNCATE TABLE company_secrets CASCADE;
TRUNCATE TABLE company_skills CASCADE;
TRUNCATE TABLE decision_queues CASCADE;
TRUNCATE TABLE documents CASCADE;
TRUNCATE TABLE environment_custom_image_templates CASCADE;
TRUNCATE TABLE execution_workspaces CASCADE;
TRUNCATE TABLE folders CASCADE;
TRUNCATE TABLE instruction_templates CASCADE;
TRUNCATE TABLE invites CASCADE;
TRUNCATE TABLE issues CASCADE;
TRUNCATE TABLE join_requests CASCADE;
TRUNCATE TABLE labels CASCADE;
TRUNCATE TABLE pipeline_stages CASCADE;
TRUNCATE TABLE plugins CASCADE;
TRUNCATE TABLE projects CASCADE;
TRUNCATE TABLE routines CASCADE;
TRUNCATE TABLE skill_catalogs CASCADE;
TRUNCATE TABLE skill_versions CASCADE;
TRUNCATE TABLE status_cards CASCADE;
TRUNCATE TABLE summary_slot_revisions CASCADE;
TRUNCATE TABLE summary_slots CASCADE;
TRUNCATE TABLE tool_applications CASCADE;
TRUNCATE TABLE tool_connections CASCADE;
TRUNCATE TABLE tool_mcp_gateways CASCADE;
TRUNCATE TABLE tool_policies CASCADE;
TRUNCATE TABLE tool_profiles CASCADE;
TRUNCATE TABLE user_preferences CASCADE;
TRUNCATE TABLE user_secret_definitions CASCADE;

-- 3. 清空核心表（agents, environments, pipelines, goals）
TRUNCATE TABLE agents CASCADE;
TRUNCATE TABLE environments CASCADE;
TRUNCATE TABLE goals CASCADE;
TRUNCATE TABLE pipelines CASCADE;

-- 4. 清空基础表（auth_users, companies, instance_settings）
TRUNCATE TABLE auth_users CASCADE;
TRUNCATE TABLE instance_settings CASCADE;

-- 5. 最后清空 companies（几乎所有表都依赖它）
TRUNCATE TABLE companies CASCADE;

-- 恢复外键约束检查
SET session_replication_role = 'origin';

-- ============================================
-- 重新插入必要的初始数据
-- ============================================


-- 1. 创建默认公司（local trusted 模式需要）
INSERT INTO companies (
    id, 
    name,
    issue_prefix,
    require_board_approval_for_new_agents,
    created_at,
    updated_at
) VALUES (
    '00000000-0000-0000-0000-000000000000',
    'Default Company',
    'CMP',
    false,
    NOW(),
    NOW()
) ON CONFLICT (id) DO NOTHING;

INSERT INTO auth_users (
    id,
    email,
    name,
    created_at,
    updated_at
) VALUES (
    '48592512-465a-4ed7-9b12-ca554ee636e8',
    'board@local.dev',
    'Local Board User',
    NOW(),
    NOW()
) ON CONFLICT (id) DO NOTHING;
-- 3. 将 board 用户添加到默认公司
INSERT INTO company_memberships (
    company_id,
    principal_type,
    principal_id,
    membership_role,
    status,
    created_at,
    updated_at
) VALUES (
    '00000000-0000-0000-0000-000000000000',
    'user',
    '48592512-465a-4ed7-9b12-ca554ee636e8',
    'admin',
    'active',
    NOW(),
    NOW()
) ON CONFLICT (company_id, principal_type, principal_id) DO NOTHING;

COMMIT;

-- 输出确认信息
DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE 'Database reset completed successfully!';
    RAISE NOTICE '========================================';
    RAISE NOTICE 'Default company: 00000000-0000-0000-0000-000000000000';
    RAISE NOTICE 'Default board user: 48592512-465a-4ed7-9b12-ca554ee636e8';
    RAISE NOTICE '========================================';
END $$;
