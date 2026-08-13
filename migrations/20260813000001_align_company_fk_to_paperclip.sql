-- 对齐 company_id FK onDelete 策略到 Paperclip 设计（自动生成）
-- 规则：Paperclip 的 company_id FK onDelete 怎么设计，Parrot 就怎么设计。
-- #3：FK 策略（ADD_CASCADE / CHANGE_TO_RESTRICT）；#5：company_id 索引（幂等）。
-- #4（补 company_id 列）需回填设计，未纳入本迁移，见 MIGRATION_ALIGNMENT_PLAN.md。

-- activity_logs (003_create_activity_logs.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'activity_logs'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE activity_logs DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE activity_logs
  ADD CONSTRAINT activity_logs_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- agent_wakeup_requests (20260712000004_create_agent_wakeup_requests.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'agent_wakeup_requests'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE agent_wakeup_requests DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE agent_wakeup_requests
  ADD CONSTRAINT agent_wakeup_requests_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- agents (20260711000001_create_agents.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'agents'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE agents DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE agents
  ADD CONSTRAINT agents_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- approvals (20260711000011_create_approvals.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'approvals'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE approvals DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE approvals
  ADD CONSTRAINT approvals_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- budget_incidents (20260719000002_create_budget_and_finance_tables.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'budget_incidents'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE budget_incidents DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE budget_incidents
  ADD CONSTRAINT budget_incidents_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- budget_policies (20260719000002_create_budget_and_finance_tables.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'budget_policies'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE budget_policies DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE budget_policies
  ADD CONSTRAINT budget_policies_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- cases (20260711000004_create_cases.sql): Paperclip=cascade, Parrot 当前 no-action
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'cases'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE cases DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE cases
  ADD CONSTRAINT cases_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE;

-- company_memberships (001_create_companies.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'company_memberships'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE company_memberships DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE company_memberships
  ADD CONSTRAINT company_memberships_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- company_skills (20260712000017_create_skill_tables.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'company_skills'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE company_skills DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE company_skills
  ADD CONSTRAINT company_skills_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- decision_bundles (20260813000002_create_decisions.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'decision_bundles'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE decision_bundles DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE decision_bundles
  ADD CONSTRAINT decision_bundles_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- documents (20260711000002_create_issues.sql): Paperclip=cascade, Parrot 当前 no-action
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'documents'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE documents DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE documents
  ADD CONSTRAINT documents_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE;

-- execution_workspaces (20260712000007_create_execution_workspaces.sql): Paperclip=cascade, Parrot 当前 no-action
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'execution_workspaces'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE execution_workspaces DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE execution_workspaces
  ADD CONSTRAINT execution_workspaces_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE;

-- finance_events (20260719000002_create_budget_and_finance_tables.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'finance_events'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE finance_events DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE finance_events
  ADD CONSTRAINT finance_events_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- goals (20260711000010_create_goals.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'goals'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE goals DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE goals
  ADD CONSTRAINT goals_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- heartbeat_run_watchdog_decisions (20260813000001_create_heartbeat_run_watchdog_decisions.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'heartbeat_run_watchdog_decisions'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE heartbeat_run_watchdog_decisions DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE heartbeat_run_watchdog_decisions
  ADD CONSTRAINT heartbeat_run_watchdog_decisions_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- heartbeat_runs (20260712000002_create_heartbeat_runs.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'heartbeat_runs'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE heartbeat_runs DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE heartbeat_runs
  ADD CONSTRAINT heartbeat_runs_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- invites (20260711000016_create_invites_join_requests.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'invites'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE invites DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE invites
  ADD CONSTRAINT invites_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- issue_labels (20260711000002_create_issues.sql): Paperclip=cascade, Parrot 当前 no-action
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'issue_labels'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE issue_labels DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE issue_labels
  ADD CONSTRAINT issue_labels_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE;

-- issue_plan_decompositions (20260808000006_create_issue_plan_decompositions.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'issue_plan_decompositions'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE issue_plan_decompositions DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE issue_plan_decompositions
  ADD CONSTRAINT issue_plan_decompositions_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- issue_relations (20260805000004_create_issue_relations.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'issue_relations'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE issue_relations DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE issue_relations
  ADD CONSTRAINT issue_relations_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- issue_thread_interactions (20260712000005_create_issue_thread_interactions.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'issue_thread_interactions'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE issue_thread_interactions DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE issue_thread_interactions
  ADD CONSTRAINT issue_thread_interactions_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- join_requests (20260711000016_create_invites_join_requests.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'join_requests'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE join_requests DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE join_requests
  ADD CONSTRAINT join_requests_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- labels (20260711000002_create_issues.sql): Paperclip=cascade, Parrot 当前 no-action
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'labels'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE labels DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE labels
  ADD CONSTRAINT labels_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE;

-- principal_permission_grants (20260711000015_create_permission_grants.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'principal_permission_grants'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE principal_permission_grants DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE principal_permission_grants
  ADD CONSTRAINT principal_permission_grants_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- project_goals (20260808000001_create_project_goals.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'project_goals'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE project_goals DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE project_goals
  ADD CONSTRAINT project_goals_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- projects (002_create_projects.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'projects'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE projects DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE projects
  ADD CONSTRAINT projects_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- routine_documents (20260728000005_create_routine_document_annotations.sql): Paperclip=restrict, Parrot 当前 cascade
DO $$
DECLARE fk_name text;
BEGIN
  SELECT con.conname INTO fk_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  WHERE rel.relname = 'routine_documents'
    AND con.contype = 'f'
    AND con.confrelid = 'companies'::regclass
    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k
                WHERE a.attname = 'company_id')
  LIMIT 1;
  IF fk_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE routine_documents DROP CONSTRAINT %I', fk_name);
  END IF;
END $$;
ALTER TABLE routine_documents
  ADD CONSTRAINT routine_documents_company_id_fkey
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;

-- company_secret_provider_configs: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_company_secret_provider_configs_company_id ON company_secret_provider_configs(company_id);

-- decision_bundles: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_decision_bundles_company_id ON decision_bundles(company_id);

-- decision_queues: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_decision_queues_company_id ON decision_queues(company_id);

-- decision_training_examples: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_decision_training_examples_company_id ON decision_training_examples(company_id);

-- environment_leases: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_environment_leases_company_id ON environment_leases(company_id);

-- execution_workspaces: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_execution_workspaces_company_id ON execution_workspaces(company_id);

-- heartbeat_run_watchdog_decisions: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_heartbeat_run_watchdog_decisions_company_id ON heartbeat_run_watchdog_decisions(company_id);

-- heartbeat_runs: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_heartbeat_runs_company_id ON heartbeat_runs(company_id);

-- issue_plan_decompositions: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_issue_plan_decompositions_company_id ON issue_plan_decompositions(company_id);

-- issue_relations: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_issue_relations_company_id ON issue_relations(company_id);

-- secret_access_events: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_secret_access_events_company_id ON secret_access_events(company_id);

-- user_secret_declarations: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_user_secret_declarations_company_id ON user_secret_declarations(company_id);

-- user_secret_definitions: Paperclip 有 company_id 索引，Parrot 缺失（#5）
CREATE INDEX IF NOT EXISTS idx_user_secret_definitions_company_id ON user_secret_definitions(company_id);
