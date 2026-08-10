-- 清空所有表数据（保留表结构）
-- 按照外键依赖顺序删除，避免约束冲突
TRUNCATE TABLE 
  routine_runs,
  routine_triggers,
  routine_revisions,
  routines,
  issue_tree_hold_members,
  issue_tree_holds,
  issue_plan_decompositions,
  issue_thread_interactions,
  issue_relations,
  issue_watchdogs,
  issue_read_status,
  issue_documents,
  annotation_comments,
  annotation_threads,
  document_revisions,
  routine_documents,
  documents,
  issues,
  case_documents,
  case_issue_links,
  cases,
  agent_memberships,
  project_memberships,
  projects,
  goals,
  agents,
  agent_api_keys,
  approvals,
  activity_logs,
  plugin_managed_resources,
  board_api_keys,
  invites,
  companies,
  auth_sessions,
  auth_users
CASCADE;

-- 显示结果
SELECT 'Database cleared successfully!' as status;
