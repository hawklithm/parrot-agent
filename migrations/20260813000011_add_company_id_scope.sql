-- 对齐 Paperclip：为 10 张表补 company_id（含历史数据回填）。
-- onDelete 策略按 PAPERCLIP_SCHEMA_BASELINE.md。
-- 注意：plugin_jobs/plugin_logs 的父表 plugins 尚无 company_id，暂保持
-- company_id NULLABLE + FK（plugins 自身 company scope 为独立待办）。

-- 1) agent_api_keys（restrict）← agents
ALTER TABLE agent_api_keys ADD COLUMN IF NOT EXISTS company_id UUID;
UPDATE agent_api_keys k SET company_id = a.company_id
  FROM agents a WHERE a.id = k.agent_id AND k.company_id IS NULL;
ALTER TABLE agent_api_keys ALTER COLUMN company_id SET NOT NULL;
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'agent_api_keys_company_id_fkey') THEN
    ALTER TABLE agent_api_keys ADD CONSTRAINT agent_api_keys_company_id_fkey
      FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;
  END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_agent_api_keys_company ON agent_api_keys(company_id);

-- 2) agent_config_revisions（restrict）← agents
ALTER TABLE agent_config_revisions ADD COLUMN IF NOT EXISTS company_id UUID;
UPDATE agent_config_revisions r SET company_id = a.company_id
  FROM agents a WHERE a.id = r.agent_id AND r.company_id IS NULL;
ALTER TABLE agent_config_revisions ALTER COLUMN company_id SET NOT NULL;
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'agent_config_revisions_company_id_fkey') THEN
    ALTER TABLE agent_config_revisions ADD CONSTRAINT agent_config_revisions_company_id_fkey
      FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;
  END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_agent_config_revisions_company ON agent_config_revisions(company_id);

-- 3) cost_events（restrict）← agents
ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS company_id UUID;
UPDATE cost_events e SET company_id = a.company_id
  FROM agents a WHERE a.id = e.agent_id AND e.company_id IS NULL;
ALTER TABLE cost_events ALTER COLUMN company_id SET NOT NULL;
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'cost_events_company_id_fkey') THEN
    ALTER TABLE cost_events ADD CONSTRAINT cost_events_company_id_fkey
      FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;
  END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_cost_events_company ON cost_events(company_id);

-- 4) document_revisions（cascade）← documents
ALTER TABLE document_revisions ADD COLUMN IF NOT EXISTS company_id UUID;
UPDATE document_revisions r SET company_id = d.company_id
  FROM documents d WHERE d.id = r.document_id AND r.company_id IS NULL;
ALTER TABLE document_revisions ALTER COLUMN company_id SET NOT NULL;
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'document_revisions_company_id_fkey') THEN
    ALTER TABLE document_revisions ADD CONSTRAINT document_revisions_company_id_fkey
      FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE;
  END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_document_revisions_company ON document_revisions(company_id);

-- 5) issue_approvals（restrict）← issues
ALTER TABLE issue_approvals ADD COLUMN IF NOT EXISTS company_id UUID;
UPDATE issue_approvals ia SET company_id = i.company_id
  FROM issues i WHERE i.id = ia.issue_id AND ia.company_id IS NULL;
ALTER TABLE issue_approvals ALTER COLUMN company_id SET NOT NULL;
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'issue_approvals_company_id_fkey') THEN
    ALTER TABLE issue_approvals ADD CONSTRAINT issue_approvals_company_id_fkey
      FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;
  END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_issue_approvals_company ON issue_approvals(company_id);

-- 6) approval_comments（restrict）← approvals
ALTER TABLE approval_comments ADD COLUMN IF NOT EXISTS company_id UUID;
UPDATE approval_comments c SET company_id = a.company_id
  FROM approvals a WHERE a.id = c.approval_id AND c.company_id IS NULL;
ALTER TABLE approval_comments ALTER COLUMN company_id SET NOT NULL;
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'approval_comments_company_id_fkey') THEN
    ALTER TABLE approval_comments ADD CONSTRAINT approval_comments_company_id_fkey
      FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;
  END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_approval_comments_company ON approval_comments(company_id);

-- 7) project_workspaces（restrict）← projects
ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS company_id UUID;
UPDATE project_workspaces w SET company_id = p.company_id
  FROM projects p WHERE p.id = w.project_id AND w.company_id IS NULL;
ALTER TABLE project_workspaces ALTER COLUMN company_id SET NOT NULL;
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'project_workspaces_company_id_fkey') THEN
    ALTER TABLE project_workspaces ADD CONSTRAINT project_workspaces_company_id_fkey
      FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE NO ACTION;
  END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_project_workspaces_company ON project_workspaces(company_id);

-- 8) pipeline_case_events（cascade）← pipeline_cases
ALTER TABLE pipeline_case_events ADD COLUMN IF NOT EXISTS company_id UUID;
UPDATE pipeline_case_events e SET company_id = c.company_id
  FROM pipeline_cases c WHERE c.id = e.case_id AND e.company_id IS NULL;
ALTER TABLE pipeline_case_events ALTER COLUMN company_id SET NOT NULL;
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pipeline_case_events_company_id_fkey') THEN
    ALTER TABLE pipeline_case_events ADD CONSTRAINT pipeline_case_events_company_id_fkey
      FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE;
  END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_pipeline_case_events_company ON pipeline_case_events(company_id);

-- 9) plugin_jobs（cascade）← plugins；plugins 暂无 company_id，保持 NULLABLE
ALTER TABLE plugin_jobs ADD COLUMN IF NOT EXISTS company_id UUID;
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'plugin_jobs_company_id_fkey') THEN
    ALTER TABLE plugin_jobs ADD CONSTRAINT plugin_jobs_company_id_fkey
      FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE;
  END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_plugin_jobs_company ON plugin_jobs(company_id);

-- 10) plugin_logs（cascade）← plugins；同上 NULLABLE
ALTER TABLE plugin_logs ADD COLUMN IF NOT EXISTS company_id UUID;
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'plugin_logs_company_id_fkey') THEN
    ALTER TABLE plugin_logs ADD CONSTRAINT plugin_logs_company_id_fkey
      FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE;
  END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_plugin_logs_company ON plugin_logs(company_id);
