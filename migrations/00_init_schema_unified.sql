-- Parrot Agent Database Schema
-- 统一初始化脚本
-- 由所有迁移文件合并生成
-- 生成时间: 2026-08-15
-- 源文件数: 108

-- ============================================
-- 1. 创建扩展
-- ============================================



CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- ========================================
-- ENUM Types
-- ========================================

DO $$ BEGIN
    CREATE TYPE agent_wakeup_request_status AS ENUM ('queued', 'dispatched', 'running', 'completed', 'failed', 'cancelled');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE allowed_join_types AS ENUM ('human', 'agent', 'both');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE annotation_thread_status AS ENUM ('open', 'resolved');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE approval_status AS ENUM ('pending', 'approved', 'rejected', 'revision_requested');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE approval_type AS ENUM ('hire_agent', 'spend_credits', 'create_resource', 'deploy_agent');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE budget_incident_status AS ENUM ('open', 'resolved', 'dismissed');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE budget_metric AS ENUM ('billed_cents');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE budget_scope_type AS ENUM ('company', 'agent', 'project');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE budget_threshold_type AS ENUM ('soft', 'hard');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE budget_window_kind AS ENUM ('calendar_month_utc', 'lifetime');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE case_event_kind AS ENUM ('created', 'updated', 'status_changed', 'document_revised', 'issue_linked', 'issue_unlinked');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE case_issue_link_role AS ENUM ('origin', 'work', 'reference');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE case_status AS ENUM ('draft', 'in_progress', 'in_review', 'approved', 'done', 'cancelled');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE catch_up_policy AS ENUM ('run_missed', 'skip_missed');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE comment_actor_type AS ENUM ('user', 'agent', 'system');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE company_membership_status AS ENUM ('active', 'inactive');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE company_status AS ENUM ('active', 'paused', 'archived');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE concurrency_policy AS ENUM ('coalesce_if_active', 'parallel', 'skip_if_active');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE execution_workspace_policy AS ENUM ('shared', 'isolated_per_issue', 'isolated_per_agent');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE finance_direction AS ENUM ('debit', 'credit');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE goal_level AS ENUM ('company', 'project', 'task');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE goal_priority AS ENUM ('low', 'medium', 'high', 'critical');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE goal_status AS ENUM ('planned', 'active', 'completed', 'archived');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE heartbeat_run_status AS ENUM (
'queued', 'running', 'succeeded', 'failed', 'cancelled', 'timed_out'
);
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE hold_release_strategy AS ENUM ('manual', 'all_done', 'first_done');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE interaction_type AS ENUM ('question', 'clarification', 'approval', 'feedback');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE invite_type AS ENUM ('company_join', 'bootstrap_ceo');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE issue_monitor_scheduled_by AS ENUM ('assignee', 'board');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE issue_priority AS ENUM ('critical', 'high', 'medium', 'low');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE issue_status AS ENUM ('backlog', 'todo', 'in_progress', 'in_review', 'blocked', 'done', 'cancelled');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE issue_thread_interaction_status AS ENUM ('pending', 'resolved', 'cancelled');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE issue_tree_control_mode AS ENUM ('pause', 'resume', 'cancel', 'restore');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE issue_tree_hold_status AS ENUM ('active', 'released');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE issue_watchdog_status AS ENUM ('active', 'paused', 'resolved', 'archived');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE issue_work_mode AS ENUM ('standard', 'ask', 'planning', 'skill_test');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE join_request_status AS ENUM ('pending_approval', 'approved', 'rejected');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE membership_role AS ENUM ('owner', 'admin', 'operator', 'viewer');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE membership_state AS ENUM ('joined', 'left');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE membership_status AS ENUM ('active', 'archived');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE pipeline_stage_kind AS ENUM ('open', 'working', 'review', 'done', 'cancelled');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE principal_type AS ENUM ('user', 'agent');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE project_status AS ENUM ('backlog', 'todo', 'in_progress', 'in_review', 'blocked', 'done');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE routine_status AS ENUM ('active', 'paused', 'draft');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE run_source AS ENUM ('schedule', 'manual', 'webhook', 'api');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE run_status AS ENUM ('received', 'queued', 'dispatched', 'coalesced', 'skipped', 'succeeded', 'failed');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE terminal_kind AS ENUM ('done', 'cancelled');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE trigger_kind AS ENUM ('schedule', 'webhook', 'manual');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;



-- ============================================
-- 2. 创建表（按依赖关系排序）
-- ============================================

CREATE TABLE IF NOT EXISTS auth_users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL UNIQUE,
    email_verified BOOLEAN NOT NULL DEFAULT false,
    name VARCHAR(255),
    avatar_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS auth_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    token VARCHAR(255) NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS cli_auth_challenges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    challenge_code VARCHAR(50) NOT NULL UNIQUE,
    user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    approved BOOLEAN NOT NULL DEFAULT false,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS companies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    status company_status NOT NULL DEFAULT 'active',
    pause_reason TEXT,
    paused_at TIMESTAMPTZ,
    issue_prefix VARCHAR(10) NOT NULL,
    issue_counter INTEGER NOT NULL DEFAULT 0,
    budget_monthly_cents BIGINT,
    spent_monthly_cents BIGINT NOT NULL DEFAULT 0,
    attachment_max_bytes BIGINT NOT NULL DEFAULT 10485760, -- 10MB default
    default_responsible_user_id UUID,
    require_board_approval_for_new_agents BOOLEAN NOT NULL DEFAULT false,
    feedback_data_sharing_enabled BOOLEAN NOT NULL DEFAULT false,
    feedback_data_sharing_consent_at TIMESTAMPTZ,
    feedback_data_sharing_consent_by_user_id UUID,
    feedback_data_sharing_terms_version VARCHAR(50),
    brand_color VARCHAR(7), -- hex color
    logo_asset_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_prefix)
);

CREATE TABLE IF NOT EXISTS activity_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    actor_type VARCHAR(50) NOT NULL,
    actor_id UUID NOT NULL,
    resource_type VARCHAR(50) NOT NULL,
    resource_id UUID NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- 新增字段：链接到 heartbeat_runs 和 agents (外键约束稍后添加)
    run_id UUID,
    agent_id UUID
);

CREATE TABLE IF NOT EXISTS agents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'general',
    status TEXT NOT NULL DEFAULT 'idle',
    adapter_type TEXT NOT NULL DEFAULT 'process',
    adapter_config JSONB NOT NULL DEFAULT '{}',
    runtime_config JSONB NOT NULL DEFAULT '{}',
    permissions JSONB NOT NULL DEFAULT '{"can_create_agents":false,"can_create_skills":false,"trust_preset":"standard","authorization_policy":"manual"}',
    metadata JSONB NOT NULL DEFAULT '{}',
    budget_monthly_cents INTEGER NOT NULL DEFAULT 0,
    reports_to UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_role CHECK (role IN ('ceo', 'vp', 'manager', 'researcher', 'general')),
    CONSTRAINT valid_status CHECK (status IN ('idle', 'running', 'paused', 'pending_approval', 'terminated'))
);

CREATE TABLE IF NOT EXISTS agent_api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    key_hash TEXT NOT NULL UNIQUE,
    name TEXT,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS agent_config_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    snapshot JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS agent_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,  -- Board user ID (not a FK to avoid coupling)
    state TEXT NOT NULL DEFAULT 'joined' CHECK (state IN ('joined', 'left')),
    starred_at TIMESTAMPTZ,  -- NULL = not starred, non-NULL = starred
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS agent_wakeup_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    status agent_wakeup_request_status NOT NULL DEFAULT 'queued',
    payload JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS approvals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    approval_type approval_type NOT NULL,
    requested_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    requested_by_user_id UUID,
    status approval_status NOT NULL DEFAULT 'pending',
    payload JSONB NOT NULL,
    decision_note TEXT,
    decided_by_user_id UUID,
    decided_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS approval_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    approval_id UUID NOT NULL REFERENCES approvals(id) ON DELETE CASCADE,
    author_user_id UUID NOT NULL,
    body TEXT NOT NULL CHECK (length(trim(body)) > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS assets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE NO ACTION,
    provider TEXT NOT NULL,
    object_key TEXT NOT NULL,
    content_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    original_filename TEXT,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE NO ACTION,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS attachments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    parent_type TEXT NOT NULL,
    parent_id UUID NOT NULL,
    asset_id UUID NOT NULL,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    created_by_type TEXT,
    created_by_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS board_api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    key_hash VARCHAR(255) NOT NULL,
    key_prefix VARCHAR(20) NOT NULL,
    name VARCHAR(255),
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS budget_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    scope_type budget_scope_type NOT NULL,
    scope_id UUID NOT NULL,
    metric budget_metric NOT NULL DEFAULT 'billed_cents',
    window_kind budget_window_kind NOT NULL DEFAULT 'calendar_month_utc',
    amount BIGINT NOT NULL DEFAULT 0,
    warn_percent INTEGER NOT NULL DEFAULT 80,
    hard_stop_enabled BOOLEAN NOT NULL DEFAULT true,
    notify_enabled BOOLEAN NOT NULL DEFAULT true,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_by_user_id UUID,
    updated_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, scope_type, scope_id, metric, window_kind)
);

CREATE TABLE IF NOT EXISTS budget_incidents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    policy_id UUID NOT NULL REFERENCES budget_policies(id) ON DELETE CASCADE,
    scope_type budget_scope_type NOT NULL,
    scope_id UUID NOT NULL,
    metric budget_metric NOT NULL DEFAULT 'billed_cents',
    window_kind budget_window_kind NOT NULL DEFAULT 'calendar_month_utc',
    window_start TIMESTAMPTZ NOT NULL,
    window_end TIMESTAMPTZ NOT NULL,
    threshold_type budget_threshold_type NOT NULL,
    amount_limit BIGINT NOT NULL,
    amount_observed BIGINT NOT NULL,
    status budget_incident_status NOT NULL DEFAULT 'open',
    approval_id UUID REFERENCES approvals(id) ON DELETE SET NULL,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS cloud_upstream_connections (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    remote_url TEXT NOT NULL,
    status TEXT NOT NULL,
    source_instance_id TEXT,
    source_instance_fingerprint TEXT,
    source_public_key TEXT,
    private_key_pem TEXT,
    token_status TEXT NOT NULL DEFAULT 'pending',
    scopes TEXT[] NOT NULL DEFAULT '{}',
    target_stack_id TEXT,
    target_stack_slug TEXT,
    target_company_id TEXT,
    target_origin TEXT,
    target_schema_major INTEGER,
    pending_state TEXT,
    pending_code_verifier TEXT,
    pending_redirect_uri TEXT,
    pending_token_url TEXT,
    access_token TEXT,
    token_id TEXT,
    token_expires_at TIMESTAMPTZ,
    authorized_global_user_id TEXT,
    last_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS cloud_upstream_runs (
    id UUID PRIMARY KEY,
    connection_id UUID NOT NULL REFERENCES cloud_upstream_connections(id) ON DELETE CASCADE,
    company_id UUID REFERENCES companies(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    active_step TEXT,
    progress_percent INTEGER NOT NULL DEFAULT 0,
    dry_run BOOLEAN NOT NULL DEFAULT false,
    summary JSONB NOT NULL DEFAULT '{}',
    warnings JSONB NOT NULL DEFAULT '[]',
    conflicts JSONB NOT NULL DEFAULT '[]',
    events JSONB NOT NULL DEFAULT '[]',
    report JSONB NOT NULL DEFAULT '{}',
    idempotency_key TEXT,
    manifest_hash TEXT,
    target_url TEXT,
    remote_run_id TEXT,
    retry_of_run_id UUID REFERENCES cloud_upstream_runs(id),
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS company_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    principal_type principal_type NOT NULL,
    principal_id UUID NOT NULL,
    status company_membership_status NOT NULL DEFAULT 'active',
    membership_role membership_role NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, principal_type, principal_id)
);

CREATE TABLE IF NOT EXISTS company_secret_provider_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'ready',
    is_default BOOLEAN NOT NULL DEFAULT false,
    config JSONB NOT NULL DEFAULT '{}',
    health_status TEXT,
    health_checked_at TIMESTAMPTZ,
    health_message TEXT,
    health_details JSONB,
    disabled_at TIMESTAMPTZ,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS company_secrets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE NO ACTION,
    scope TEXT NOT NULL DEFAULT 'company',
    owner_user_id TEXT,
    user_secret_definition_id UUID,
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    provider TEXT NOT NULL DEFAULT 'local_encrypted',
    status TEXT NOT NULL DEFAULT 'active',
    managed_mode TEXT NOT NULL DEFAULT 'paperclip_managed',
    external_ref TEXT,
    provider_config_id UUID,
    provider_metadata JSONB,
    latest_version INTEGER NOT NULL DEFAULT 1,
    description TEXT,
    last_resolved_at TIMESTAMPTZ,
    last_rotated_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT company_secrets_scope_shape_check CHECK (
        (scope = 'company' AND owner_user_id IS NULL AND user_secret_definition_id IS NULL)
        OR
        (scope = 'user' AND owner_user_id IS NOT NULL AND user_secret_definition_id IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS company_secret_bindings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE NO ACTION,
    secret_id UUID NOT NULL REFERENCES company_secrets(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    config_path TEXT NOT NULL,
    version_selector TEXT NOT NULL DEFAULT 'latest',
    required BOOLEAN NOT NULL DEFAULT true,
    label TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS company_secret_proposals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,                          -- secret | binding
    status TEXT NOT NULL DEFAULT 'pending',      -- pending|approved|rejected|withdrawn|expired
    proposed_name TEXT,
    proposed_key TEXT,
    proposed_description TEXT,
    justification TEXT NOT NULL,
    value_ciphertext JSONB,
    value_fingerprint_sha256 TEXT,
    value_length INTEGER,
    secret_id UUID REFERENCES company_secrets(id) ON DELETE SET NULL,
    target_type TEXT,
    target_id UUID,
    config_path TEXT,
    proposed_by_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    origin_issue_id UUID,
    origin_run_id UUID,
    resolved_by_user_id TEXT,
    resolved_at TIMESTAMPTZ,
    resolution_reason TEXT,
    created_secret_id UUID REFERENCES company_secrets(id) ON DELETE SET NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS company_secret_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    secret_id UUID NOT NULL REFERENCES company_secrets(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    material JSONB NOT NULL,
    value_sha256 TEXT NOT NULL,
    provider_version_ref TEXT,
    status TEXT NOT NULL DEFAULT 'current',
    fingerprint_sha256 TEXT,
    rotation_job_id TEXT,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS company_skill_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    policy JSONB NOT NULL DEFAULT '{}'::jsonb,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (company_id)
);

CREATE TABLE IF NOT EXISTS company_team_installs (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    catalog_id TEXT NOT NULL,
    catalog_key TEXT,
    content_hash TEXT NOT NULL,
    agent_ids UUID[] NOT NULL DEFAULT '{}',
    agent_count INTEGER NOT NULL DEFAULT 0,
    installed_by_user_id UUID,
    installed_by_agent_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT company_team_installs_company_catalog_unique UNIQUE (company_id, catalog_id)
);

CREATE TABLE IF NOT EXISTS company_user_sidebar_preferences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    project_order JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (company_id, user_id)
);

CREATE TABLE IF NOT EXISTS cost_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    amount_cents INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS decision_queues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    created_by_type TEXT NOT NULL,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_by_run_id UUID,
    created_by_agent_api_key_id UUID,
    retention_days INTEGER,
    seed_rules JSONB NOT NULL DEFAULT '[]'::jsonb,
    seed_rules_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decision_queues_created_by_type_check
        CHECK (created_by_type IN ('agent', 'user', 'system')),
    CONSTRAINT decision_queues_creator_check CHECK (
        (created_by_type = 'agent' AND created_by_agent_id IS NOT NULL AND created_by_user_id IS NULL)
        OR (created_by_type = 'user' AND created_by_user_id IS NOT NULL AND created_by_agent_id IS NULL)
        OR (created_by_type = 'system' AND created_by_agent_id IS NULL AND created_by_user_id IS NULL)
    ),
    CONSTRAINT decision_queues_retention_days_check
        CHECK (retention_days IS NULL OR (retention_days >= 1 AND retention_days <= 3650)),
    CONSTRAINT uniq_decision_queues_company_key UNIQUE (company_id, key)
,
    UNIQUE (id, company_id)
);

CREATE TABLE IF NOT EXISTS decision_queue_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    queue_id UUID NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    added_by_type TEXT NOT NULL,
    added_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    added_by_user_id TEXT,
    added_by_run_id UUID,
    added_by_agent_api_key_id UUID,
    responsible_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decision_queue_items_added_by_type_check
        CHECK (added_by_type IN ('agent', 'user', 'system')),
    CONSTRAINT decision_queue_items_actor_check CHECK (
        (added_by_type = 'agent' AND added_by_agent_id IS NOT NULL AND added_by_user_id IS NULL)
        OR (added_by_type = 'user' AND added_by_user_id IS NOT NULL AND added_by_agent_id IS NULL)
        OR (added_by_type = 'system' AND added_by_agent_id IS NULL AND added_by_user_id IS NULL)
    ),
    CONSTRAINT fk_decision_queue_items_queue
        FOREIGN KEY (queue_id, company_id)
        REFERENCES decision_queues(id, company_id) ON DELETE CASCADE,
    CONSTRAINT uniq_decision_queue_items_source
        UNIQUE (queue_id, source_kind, source_id)
);

CREATE TABLE IF NOT EXISTS decision_retention (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_activity_at TIMESTAMPTZ NOT NULL,
    keep BOOLEAN NOT NULL DEFAULT FALSE,
    archived_at TIMESTAMPTZ,
    archived_reason TEXT,
    archived_by_type TEXT,
    archived_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    archived_by_user_id TEXT,
    archived_by_run_id UUID,
    version INTEGER NOT NULL DEFAULT 1,
    archive_version INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decision_retention_archive_actor_check CHECK (
        archived_at IS NULL
        OR (archived_by_type = 'agent' AND archived_by_agent_id IS NOT NULL AND archived_by_user_id IS NULL)
        OR (archived_by_type = 'user' AND archived_by_user_id IS NOT NULL AND archived_by_agent_id IS NULL)
        OR (archived_by_type = 'system' AND archived_by_agent_id IS NULL AND archived_by_user_id IS NULL)
    ),
    CONSTRAINT uniq_decision_retention_source
        UNIQUE (company_id, source_kind, source_id)
);

CREATE TABLE IF NOT EXISTS decision_triage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    decide_by TEXT,
    decide_by_date DATE,
    snoozed_until TIMESTAMPTZ,
    set_by_type TEXT NOT NULL,
    set_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    set_by_user_id TEXT,
    set_by_run_id UUID,
    set_by_agent_api_key_id UUID,
    responsible_user_id TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decision_triage_set_by_type_check
        CHECK (set_by_type IN ('agent', 'user', 'system')),
    CONSTRAINT decision_triage_decide_by_check CHECK (
        (decide_by IS NULL AND decide_by_date IS NULL)
        OR (decide_by IN ('today', 'this_week', 'whenever') AND decide_by_date IS NULL)
        OR (decide_by = 'date' AND decide_by_date IS NOT NULL)
    ),
    CONSTRAINT uniq_decision_triage_source
        UNIQUE (company_id, source_kind, source_id)
);

CREATE TABLE IF NOT EXISTS decision_triage_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    queue_id UUID REFERENCES decision_queues(id) ON DELETE SET NULL,
    source_kind TEXT,
    source_id TEXT,
    action TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    actor_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    actor_user_id TEXT,
    actor_run_id UUID,
    agent_api_key_id UUID,
    responsible_user_id TEXT,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decision_triage_events_actor_type_check
        CHECK (actor_type IN ('agent', 'user', 'system'))
);

CREATE TABLE IF NOT EXISTS documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    content TEXT NOT NULL,
    content_type TEXT,
    locked_by_type TEXT,
    locked_by_id UUID,
    locked_at TIMESTAMPTZ,
    locked_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS annotation_threads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    position JSONB NOT NULL,
    status annotation_thread_status NOT NULL DEFAULT 'open',
    created_by_type TEXT,
    created_by_id UUID,
    resolved_by_type TEXT,
    resolved_by_id UUID,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS annotation_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    thread_id UUID NOT NULL REFERENCES annotation_threads(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    actor_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS document_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL,
    content TEXT NOT NULL,
    created_by_type TEXT,
    created_by_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(document_id, revision_number)
);

CREATE TABLE IF NOT EXISTS environments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    driver TEXT NOT NULL DEFAULT 'local',
    status TEXT NOT NULL DEFAULT 'active',
    config JSONB NOT NULL DEFAULT '{}',
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS environment_custom_image_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    template_kind TEXT NOT NULL DEFAULT 'snapshot',
    template_ref TEXT NOT NULL,
    source_template_ref TEXT,
    source_environment_config_fingerprint TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    superseded_by_template_id UUID REFERENCES environment_custom_image_templates(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_by_agent_id UUID,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS folders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,                        -- routine | skill
    parent_id UUID REFERENCES folders(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    system_key TEXT,
    path TEXT NOT NULL,
    depth INTEGER NOT NULL DEFAULT 1,
    color TEXT,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (company_id, kind, slug)
);

CREATE TABLE IF NOT EXISTS folder_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    folder_id UUID NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    item_kind TEXT NOT NULL,                   -- routine | skill
    item_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (folder_id, item_kind, item_id)
);

CREATE TABLE IF NOT EXISTS goals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    level goal_level NOT NULL,
    status goal_status NOT NULL DEFAULT 'planned',
    parent_id UUID REFERENCES goals(id) ON DELETE SET NULL,
    owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS heartbeat_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    invocation_source TEXT NOT NULL DEFAULT 'on_demand',
    status heartbeat_run_status NOT NULL DEFAULT 'queued',
    responsible_user_id TEXT,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    error TEXT,
    exit_code INTEGER,
    context_snapshot JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_invocation_source CHECK (invocation_source IN ('on_demand', 'scheduled', 'watchdog'))
);

CREATE TABLE IF NOT EXISTS inbox_dismissals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    item_key TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'dismiss',          -- dismiss | snooze
    dismissed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    snoozed_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (company_id, user_id, item_key)
);

CREATE TABLE IF NOT EXISTS instance_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    instance_name TEXT NOT NULL DEFAULT 'Parrot Agent',
    version TEXT NOT NULL DEFAULT '0.1.0',
    general JSONB NOT NULL DEFAULT '{"timezone":"UTC","language":"en"}',
    experimental JSONB NOT NULL DEFAULT '{"issueGraphLivenessAutoRecovery":false,"enableCloudSync":false,"enableBuiltInAgents":true,"enableCases":true}',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS instance_user_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_user_role UNIQUE (user_id, role)
);

CREATE TABLE IF NOT EXISTS instruction_templates (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    variables TEXT[] NOT NULL DEFAULT '{}',
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ,
    
    -- 名称唯一约束
    CONSTRAINT uq_instruction_templates_name UNIQUE (name)
);

CREATE TABLE IF NOT EXISTS invites (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    invite_type invite_type NOT NULL,
    invited_email VARCHAR(255),
    invited_by_user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    token VARCHAR(255) NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    accepted BOOLEAN NOT NULL DEFAULT false,
    accepted_by_user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    accepted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS join_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    requester_user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    status join_request_status NOT NULL DEFAULT 'pending_approval',
    message TEXT,
    reviewed_by_user_id UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    reviewed_at TIMESTAMPTZ,
    rejection_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS labels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    name TEXT NOT NULL,
    color TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, name)
);

CREATE TABLE IF NOT EXISTS plugins (
    id UUID PRIMARY KEY,
    plugin_key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '0.0.0',
    api_version INTEGER NOT NULL DEFAULT 1,
    categories JSONB NOT NULL DEFAULT '[]',
    install_order INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'installed',
    package_name TEXT,
    install_path TEXT,
    manifest JSONB NOT NULL DEFAULT '{}',
    config JSONB NOT NULL DEFAULT '{}',
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS plugin_data (
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    data_key TEXT NOT NULL,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(plugin_id, data_key)
);

CREATE TABLE IF NOT EXISTS plugin_jobs (
    id UUID PRIMARY KEY,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    job_key TEXT NOT NULL,
    name TEXT NOT NULL,
    schedule TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    definition JSONB NOT NULL DEFAULT '{}',
    UNIQUE(plugin_id, job_key)
);

CREATE TABLE IF NOT EXISTS plugin_job_runs (
    id UUID PRIMARY KEY,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    job_id UUID NOT NULL REFERENCES plugin_jobs(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'queued',
    result JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS plugin_logs (
    id UUID PRIMARY KEY,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    level TEXT NOT NULL DEFAULT 'info',
    message TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS plugin_managed_resources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    plugin_key TEXT NOT NULL,
    resource_kind TEXT NOT NULL,  -- e.g., 'project', 'issue', 'agent'
    resource_key TEXT NOT NULL,    -- plugin-specific identifier
    resource_id UUID NOT NULL,     -- actual resource UUID in our DB
    defaults_json JSONB NOT NULL DEFAULT '{}',  -- plugin-specific metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS principal_permission_grants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    principal_type principal_type NOT NULL,
    principal_id UUID NOT NULL,
    permission_key VARCHAR(100) NOT NULL,
    scope JSONB NOT NULL DEFAULT '{}',
    granted_by_user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_principal_permission UNIQUE (company_id, principal_type, principal_id, permission_key)
);

CREATE TABLE IF NOT EXISTS projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    goal_id UUID,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    status project_status NOT NULL DEFAULT 'backlog',
    lead_agent_id UUID,
    target_date TIMESTAMPTZ,
    color VARCHAR(7), -- hex color
    icon VARCHAR(50),
    env JSONB,
    pause_reason TEXT,
    paused_at TIMESTAMPTZ,
    execution_workspace_policy execution_workspace_policy NOT NULL DEFAULT 'shared',
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS cases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    project_id UUID REFERENCES projects(id),
    case_number INTEGER NOT NULL,
    identifier TEXT NOT NULL,
    case_type TEXT NOT NULL,
    key TEXT,
    title TEXT NOT NULL,
    summary TEXT,
    status case_status NOT NULL DEFAULT 'draft',
    fields JSONB NOT NULL DEFAULT '{}'::jsonb,
    parent_case_id UUID REFERENCES cases(id),
    created_by_agent_id UUID REFERENCES agents(id),
    created_by_user_id UUID,
    created_by_run_id UUID,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Unique constraint: company_id + case_type + key (when key is not null)
    UNIQUE(company_id, case_type, key),
    UNIQUE(company_id, case_number)
);

CREATE TABLE IF NOT EXISTS case_attachments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    asset_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS case_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(case_id, key)
);

CREATE TABLE IF NOT EXISTS case_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    kind case_event_kind NOT NULL,
    actor_type TEXT,
    actor_id UUID,
    actor_run_id UUID,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS case_labels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    label_id UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(case_id, label_id)
);

CREATE TABLE IF NOT EXISTS issues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    project_id UUID REFERENCES projects(id),
    project_workspace_id UUID,
    goal_id UUID,
    parent_id UUID REFERENCES issues(id),

    title TEXT NOT NULL,
    description TEXT,
    status issue_status NOT NULL DEFAULT 'backlog',
    work_mode issue_work_mode NOT NULL DEFAULT 'standard',
    priority issue_priority NOT NULL DEFAULT 'medium',

    assignee_agent_id UUID REFERENCES agents(id),
    assignee_user_id UUID,
    checkout_run_id UUID,
    execution_run_id UUID,
    execution_agent_name_key TEXT,
    execution_locked_at TIMESTAMPTZ,

    created_by_agent_id UUID REFERENCES agents(id),
    created_by_user_id UUID,
    responsible_user_id UUID,

    issue_number INTEGER,
    identifier TEXT UNIQUE,

    origin_kind TEXT NOT NULL DEFAULT 'manual',
    origin_id TEXT,
    origin_run_id UUID,
    origin_fingerprint TEXT NOT NULL DEFAULT 'default',
    request_depth INTEGER NOT NULL DEFAULT 0,

    billing_code TEXT,
    assignee_adapter_overrides JSONB,
    execution_policy JSONB,
    execution_state JSONB,

    monitor_next_check_at TIMESTAMPTZ,
    monitor_last_triggered_at TIMESTAMPTZ,
    monitor_attempt_count INTEGER NOT NULL DEFAULT 0,
    monitor_notes TEXT,
    monitor_scheduled_by issue_monitor_scheduled_by,

    execution_workspace_id UUID,
    execution_workspace_preference TEXT,
    execution_workspace_settings JSONB,

    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    hidden_at TIMESTAMPTZ,
    source_trust JSONB,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS case_issue_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    role case_issue_link_role NOT NULL,
    created_by_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(case_id, issue_id)
);

CREATE TABLE IF NOT EXISTS decision_archive_notification_outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    archive_version INTEGER NOT NULL,
    origin_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    origin_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_attempt_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decision_archive_outbox_status_check
        CHECK (status IN ('pending', 'delivered', 'failed')),
    CONSTRAINT uniq_decision_archive_outbox_source_version
        UNIQUE (company_id, source_kind, source_id, archive_version)
);

CREATE TABLE IF NOT EXISTS decision_bundles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    summary TEXT,
    origin_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    origin_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    origin_run_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS decision_training_examples (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_id UUID NOT NULL,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    cutoff_at TIMESTAMPTZ NOT NULL,
    notes TEXT NOT NULL DEFAULT '',
    notes_history JSONB NOT NULL DEFAULT '[]'::jsonb,
    decision_outcome TEXT,
    retention_policy TEXT NOT NULL DEFAULT 'scrub_deleted_comments_v1',
    snapshot JSONB NOT NULL,
    created_by_user_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decision_training_examples_source_kind_check
        CHECK (source_kind IN ('interaction', 'approval', 'execution_decision')),
    CONSTRAINT uniq_decision_training_examples_source_author
        UNIQUE (source_kind, source_id, created_by_user_id)
);

CREATE TABLE IF NOT EXISTS decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    bundle_id UUID REFERENCES decision_bundles(id) ON DELETE SET NULL,
    origin_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    origin_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    origin_run_id UUID NOT NULL,
    rule_key TEXT,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    options JSONB NOT NULL,
    inputs JSONB,
    status TEXT NOT NULL DEFAULT 'open',
    execution_status TEXT,
    chosen_option_id TEXT,
    input_values JSONB,
    decided_by_user_id TEXT,
    decided_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL,
    idempotency_key TEXT,
    signed_spec TEXT NOT NULL DEFAULT '',
    target_snapshots JSONB NOT NULL DEFAULT '{}'::jsonb,
    continuation_policy TEXT NOT NULL DEFAULT 'none',
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decisions_status_check
        CHECK (status IN ('open', 'decided', 'cancelled', 'expired')),
    CONSTRAINT decisions_continuation_policy_check
        CHECK (continuation_policy IN ('none', 'wake_origin_agent'))
);

CREATE TABLE IF NOT EXISTS decision_effect_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    decision_id UUID NOT NULL REFERENCES decisions(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    effect_index INTEGER NOT NULL,
    effect_type TEXT NOT NULL,
    target_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'claimed',
    result JSONB,
    error TEXT,
    activity_log_id UUID,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT decision_effect_executions_status_check
        CHECK (status IN ('claimed', 'succeeded', 'failed', 'skipped'))
);

CREATE TABLE IF NOT EXISTS decision_target_issues (
    decision_id UUID NOT NULL REFERENCES decisions(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    PRIMARY KEY (decision_id, issue_id)
);

CREATE TABLE IF NOT EXISTS feedback_votes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    voter_id UUID NOT NULL,
    voter_type TEXT NOT NULL DEFAULT 'user',
    vote TEXT NOT NULL CHECK (vote IN ('up', 'down')),
    reason TEXT,
    shared_with_labs BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, voter_id, voter_type)
);

CREATE TABLE IF NOT EXISTS feedback_traces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    vote_id UUID NOT NULL REFERENCES feedback_votes(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,
    target_id UUID,
    payload JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'local_only' CHECK (status IN ('local_only', 'pending', 'sent', 'failed')),
    failure_reason TEXT,
    shared_with_labs BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS finance_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    goal_id UUID REFERENCES goals(id) ON DELETE SET NULL,
    heartbeat_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    cost_event_id UUID REFERENCES cost_events(id) ON DELETE SET NULL,
    biller TEXT NOT NULL DEFAULT 'unknown',
    event_kind TEXT NOT NULL,
    direction finance_direction NOT NULL DEFAULT 'debit',
    amount_cents INTEGER NOT NULL DEFAULT 0,
    currency TEXT NOT NULL DEFAULT 'USD',
    estimated BOOLEAN NOT NULL DEFAULT false,
    description TEXT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS heartbeat_run_watchdog_decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES heartbeat_runs(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    decision TEXT NOT NULL
        CHECK (decision IN ('snooze', 'continue', 'dismissed_false_positive')),
    evaluation_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    reason TEXT,
    snoozed_until TIMESTAMPTZ,
    created_by_type TEXT,
    created_by_id UUID,
    created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS issue_approvals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    approval_id UUID NOT NULL REFERENCES approvals(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    CONSTRAINT unique_approval_issue UNIQUE (approval_id, issue_id)
);

CREATE TABLE IF NOT EXISTS issue_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    actor_type comment_actor_type NOT NULL,
    actor_id UUID,
    actor_run_id UUID,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS issue_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, key)
);

CREATE TABLE IF NOT EXISTS issue_inbox_archives (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, user_id)
);

CREATE TABLE IF NOT EXISTS issue_labels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    label_id UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, label_id)
);

CREATE TABLE IF NOT EXISTS issue_read_status (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    read_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(issue_id, user_id)
);

CREATE TABLE IF NOT EXISTS issue_relations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    related_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    type TEXT NOT NULL DEFAULT 'blocks' CHECK (type = 'blocks'),
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT issue_relations_company_edge_uq
        UNIQUE (company_id, issue_id, related_issue_id, type),
    CONSTRAINT issue_relations_no_self_edge CHECK (issue_id <> related_issue_id)
);

CREATE TABLE IF NOT EXISTS issue_thread_interactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'question',
    status TEXT NOT NULL DEFAULT 'pending',
    continuation_policy TEXT NOT NULL DEFAULT 'wake_assignee',
    requested_resolver_policy TEXT NOT NULL DEFAULT 'board_only',
    effective_resolver_policy TEXT NOT NULL DEFAULT 'board_only',
    idempotency_key TEXT,
    source_comment_id UUID REFERENCES issue_comments(id) ON DELETE SET NULL,
    source_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    title TEXT,
    summary TEXT,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    addressee_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    resolved_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    resolved_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    resolved_by_user_id TEXT,
    payload JSONB NOT NULL,
    result JSONB,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_interaction_kind CHECK (kind IN ('question', 'approval', 'review'))
);

CREATE TABLE IF NOT EXISTS issue_plan_decompositions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    accepted_plan_revision_id UUID NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
    accepted_interaction_id UUID REFERENCES issue_thread_interactions(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'in_flight',
    request_fingerprint TEXT NOT NULL,
    requested_child_count INTEGER NOT NULL DEFAULT 0,
    requested_children JSONB NOT NULL DEFAULT '[]'::jsonb,
    child_issue_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    owner_user_id TEXT,
    owner_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT valid_decomposition_status CHECK (status IN ('in_flight', 'completed', 'cancelled'))
);

CREATE TABLE IF NOT EXISTS issue_tree_holds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    root_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    mode issue_tree_control_mode NOT NULL,
    status issue_tree_hold_status NOT NULL DEFAULT 'active',
    reason TEXT,
    release_policy JSONB NOT NULL DEFAULT '{"strategy":"manual"}'::jsonb,
    metadata JSONB,
    actor_type TEXT,
    actor_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    released_at TIMESTAMPTZ,
    released_by_type TEXT,
    released_by_id UUID
);

CREATE TABLE IF NOT EXISTS issue_tree_hold_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    hold_id UUID NOT NULL REFERENCES issue_tree_holds(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    parent_issue_id UUID REFERENCES issues(id),
    depth INTEGER NOT NULL,
    issue_identifier TEXT,
    issue_title TEXT NOT NULL,
    issue_status TEXT NOT NULL,
    assignee_agent_id UUID,
    assignee_user_id UUID,
    active_run_id UUID,
    active_run_status TEXT,
    skipped BOOLEAN NOT NULL DEFAULT false,
    skip_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(hold_id, issue_id)
);

CREATE TABLE IF NOT EXISTS issue_watchdogs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    watchdog_agent_id UUID NOT NULL REFERENCES agents(id),
    instructions TEXT,
    status issue_watchdog_status NOT NULL DEFAULT 'active',
    watchdog_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    last_observed_fingerprint TEXT,
    last_reviewed_fingerprint TEXT,
    last_triggered_at TIMESTAMPTZ,
    last_completed_at TIMESTAMPTZ,
    trigger_count INTEGER NOT NULL DEFAULT 0,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    updated_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    updated_by_user_id TEXT,
    updated_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT issue_watchdogs_company_issue_uq UNIQUE (company_id, issue_id),
    CONSTRAINT issue_watchdogs_company_watchdog_issue_uq UNIQUE (company_id, watchdog_issue_id)
);

CREATE TABLE IF NOT EXISTS issue_work_products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    artifact JSONB NOT NULL,
    created_by_agent_id UUID REFERENCES agents(id),
    created_by_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS pipelines (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    key VARCHAR(100) NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    enforce_transitions BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_pipeline_key_per_company UNIQUE (company_id, key)
);

CREATE TABLE IF NOT EXISTS pipeline_stages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    key VARCHAR(100) NOT NULL,
    name VARCHAR(255) NOT NULL,
    kind pipeline_stage_kind NOT NULL,
    position INTEGER NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_stage_key_per_pipeline UNIQUE (pipeline_id, key),
    CONSTRAINT unique_stage_position_per_pipeline UNIQUE (pipeline_id, position)
);

CREATE TABLE IF NOT EXISTS pipeline_cases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    pipeline_id UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    stage_id UUID NOT NULL REFERENCES pipeline_stages(id) ON DELETE RESTRICT,
    case_key VARCHAR(50) NOT NULL,
    title VARCHAR(500) NOT NULL,
    summary TEXT,
    fields JSONB NOT NULL DEFAULT '{}',
    terminal_kind terminal_kind,
    version INTEGER NOT NULL DEFAULT 1,
    pending_suggestion JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_case_key_per_company UNIQUE (company_id, case_key)
);

CREATE TABLE IF NOT EXISTS pipeline_case_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES pipeline_cases(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    actor_type VARCHAR(50),
    actor_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS pipeline_transitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    from_stage_id UUID NOT NULL REFERENCES pipeline_stages(id) ON DELETE CASCADE,
    to_stage_id UUID NOT NULL REFERENCES pipeline_stages(id) ON DELETE CASCADE,
    label VARCHAR(255),
    conditions JSONB NOT NULL DEFAULT '{}',
    CONSTRAINT unique_pipeline_transition UNIQUE (pipeline_id, from_stage_id, to_stage_id)
);

CREATE TABLE IF NOT EXISTS plan_decompositions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    plan JSONB NOT NULL,
    accepted_at TIMESTAMPTZ,
    accepted_by_type TEXT,
    accepted_by_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS project_goals (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    goal_id UUID NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (project_id, goal_id)
);

CREATE TABLE IF NOT EXISTS project_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,  -- Board user ID (not a FK to avoid coupling)
    state TEXT NOT NULL DEFAULT 'joined' CHECK (state IN ('joined', 'left')),
    starred_at TIMESTAMPTZ,  -- NULL = not starred, non-NULL = starred
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS project_workspaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    is_primary BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(project_id, name)
);

CREATE TABLE IF NOT EXISTS execution_workspaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE NO ACTION,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    project_workspace_id UUID REFERENCES project_workspaces(id) ON DELETE SET NULL,
    source_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    mode TEXT NOT NULL,
    strategy_type TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    cwd TEXT,
    repo_url TEXT,
    base_ref TEXT,
    branch_name TEXT,
    provider_type TEXT NOT NULL DEFAULT 'local_fs',
    provider_ref TEXT,
    derived_from_execution_workspace_id UUID REFERENCES execution_workspaces(id) ON DELETE SET NULL,
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    opened_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at TIMESTAMPTZ,
    cleanup_eligible_at TIMESTAMPTZ,
    cleanup_reason TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS environment_leases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    execution_workspace_id UUID REFERENCES execution_workspaces(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    heartbeat_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'active',
    lease_policy TEXT NOT NULL DEFAULT 'ephemeral',
    provider TEXT,
    provider_lease_id TEXT,
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ,
    released_at TIMESTAMPTZ,
    failure_reason TEXT,
    cleanup_status TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS environment_custom_image_setup_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    template_id UUID,
    promoted_template_id UUID,
    provider TEXT NOT NULL,
    provider_lease_id TEXT,
    environment_lease_id UUID REFERENCES environment_leases(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'starting',
    started_by_user_id TEXT,
    started_by_agent_id UUID,
    base_template_ref TEXT,
    expires_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    failure_reason TEXT,
    connection_summary JSONB,
    connection_secret_ref TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS recovery_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    action_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'in_progress', 'resolved', 'failed')),
    description TEXT,
    metadata JSONB,
    triggered_by_issue_id UUID REFERENCES issues(id),
    triggered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS routines (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    goal_id UUID,
    parent_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    assignee_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    priority INTEGER NOT NULL DEFAULT 50,
    status routine_status NOT NULL DEFAULT 'draft',
    concurrency_policy concurrency_policy NOT NULL DEFAULT 'coalesce_if_active',
    catch_up_policy catch_up_policy NOT NULL DEFAULT 'skip_missed',
    variables JSONB NOT NULL DEFAULT '[]',
    env JSONB NOT NULL DEFAULT '{}',
    latest_revision_id UUID,
    latest_revision_number INTEGER NOT NULL DEFAULT 0,
    responsible_user_id UUID,
    last_triggered_at TIMESTAMPTZ,
    last_enqueued_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS document_annotation_threads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    routine_id UUID REFERENCES routines(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    document_key TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    anchor_state TEXT NOT NULL DEFAULT 'active',
    original_revision_id UUID,
    original_revision_number INTEGER NOT NULL,
    current_revision_id UUID,
    current_revision_number INTEGER NOT NULL,
    selected_text TEXT NOT NULL,
    prefix_text TEXT NOT NULL DEFAULT '',
    suffix_text TEXT NOT NULL DEFAULT '',
    normalized_start INTEGER NOT NULL,
    normalized_end INTEGER NOT NULL,
    markdown_start INTEGER NOT NULL,
    markdown_end INTEGER NOT NULL,
    anchor_confidence TEXT NOT NULL DEFAULT 'exact',
    anchor_selector JSONB NOT NULL,
    created_by_agent_id UUID,
    created_by_user_id TEXT,
    resolved_by_agent_id UUID,
    resolved_by_user_id TEXT,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS document_annotation_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    thread_id UUID NOT NULL REFERENCES document_annotation_threads(id) ON DELETE CASCADE,
    routine_id UUID REFERENCES routines(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    author_type TEXT NOT NULL DEFAULT 'user',
    author_agent_id UUID,
    author_user_id TEXT,
    created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS routine_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    routine_id UUID NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    document_key TEXT NOT NULL,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    UNIQUE (routine_id, document_key)
);

CREATE TABLE IF NOT EXISTS routine_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    routine_id UUID NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    snapshot JSONB NOT NULL,
    change_summary TEXT,
    restored_from_revision_id UUID REFERENCES routine_revisions(id) ON DELETE SET NULL,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_routine_revision_number UNIQUE (routine_id, revision_number)
);

CREATE TABLE IF NOT EXISTS routine_triggers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    routine_id UUID NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    kind trigger_kind NOT NULL,
    label VARCHAR(255),
    enabled BOOLEAN NOT NULL DEFAULT true,
    cron_expression VARCHAR(255),
    timezone VARCHAR(100),
    next_run_at TIMESTAMPTZ,
    last_fired_at TIMESTAMPTZ,
    public_id VARCHAR(100) UNIQUE,
    secret_id VARCHAR(100),
    signing_mode VARCHAR(50),
    replay_window_sec INTEGER,
    last_rotated_at TIMESTAMPTZ,
    last_result JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS routine_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    routine_id UUID NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    trigger_id UUID REFERENCES routine_triggers(id) ON DELETE SET NULL,
    source run_source NOT NULL,
    status run_status NOT NULL DEFAULT 'received',
    triggered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    routine_revision_id UUID REFERENCES routine_revisions(id) ON DELETE SET NULL,
    idempotency_key VARCHAR(255),
    trigger_payload JSONB,
    dispatch_fingerprint VARCHAR(255),
    linked_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    coalesced_into_run_id UUID REFERENCES routine_runs(id) ON DELETE SET NULL,
    failure_reason TEXT,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_trigger_idempotency UNIQUE (trigger_id, idempotency_key)
);

CREATE TABLE IF NOT EXISTS skill_catalogs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    category VARCHAR(255),
    metadata JSONB NOT NULL DEFAULT '{}',
    is_paperclip_managed BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS company_skills (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    catalog_id UUID REFERENCES skill_catalogs(id) ON DELETE SET NULL,
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    category VARCHAR(255),
    version VARCHAR(50) NOT NULL DEFAULT '1.0.0',
    tags JSONB NOT NULL DEFAULT '[]',
    config JSONB NOT NULL DEFAULT '{}',
    is_paperclip_managed BOOLEAN NOT NULL DEFAULT false,
    is_fork BOOLEAN NOT NULL DEFAULT false,
    forked_from_skill_id UUID REFERENCES company_skills(id) ON DELETE SET NULL,
    forked_from_catalog_id UUID REFERENCES skill_catalogs(id) ON DELETE SET NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    update_available BOOLEAN NOT NULL DEFAULT false,
    latest_version VARCHAR(50),
    created_by_agent_id UUID,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, slug)
);

CREATE TABLE IF NOT EXISTS skill_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    parent_comment_id UUID REFERENCES skill_comments(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    author_agent_id UUID,
    author_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS skill_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    path VARCHAR(1024) NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    mime_type VARCHAR(255),
    size_bytes BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(skill_id, path)
);

CREATE TABLE IF NOT EXISTS skill_stars (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    agent_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, skill_id, user_id),
    UNIQUE(company_id, skill_id, agent_id)
);

CREATE TABLE IF NOT EXISTS skill_test_inputs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_by_agent_id UUID,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS skill_test_run_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    created_by_agent_id UUID,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS skill_test_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    template_id UUID REFERENCES skill_test_run_templates(id) ON DELETE SET NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    result JSONB,
    started_by_agent_id UUID,
    started_by_user_id UUID,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS skill_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES company_skills(id) ON DELETE CASCADE,
    version VARCHAR(50) NOT NULL,
    files JSONB NOT NULL DEFAULT '{}',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_by_agent_id UUID,
    created_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(skill_id, version)
);

CREATE TABLE IF NOT EXISTS smoke_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    trigger TEXT NOT NULL DEFAULT 'manual',
    status TEXT NOT NULL DEFAULT 'running',   -- running|passed|failed|cancelled
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    summary JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS smoke_run_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES smoke_runs(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    scenario_step TEXT NOT NULL,
    status TEXT NOT NULL,                    -- passed|failed|skipped|running
    detail TEXT,
    screenshot_artifact_ref JSONB,
    duration_ms INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS status_cards (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    created_by_user_id TEXT,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    title TEXT,
    title_pinned BOOLEAN NOT NULL DEFAULT false,
    interest_prompt TEXT NOT NULL DEFAULT '',
    queries JSONB NOT NULL DEFAULT '[]',
    query_version INTEGER NOT NULL DEFAULT 0,
    query_compiled_at TIMESTAMPTZ,
    query_compiled_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    refresh_policy JSONB NOT NULL DEFAULT '{}',
    state TEXT NOT NULL DEFAULT 'compiling',   -- compiling|active|error|paused_budget|paused_hours
    pending_change_count INTEGER NOT NULL DEFAULT 0,
    archived_at TIMESTAMPTZ,
    archived_by_user_id TEXT,
    generating_issue_id UUID,
    summary_markdown TEXT,
    summary_compiled_at TIMESTAMPTZ,
    summary_compiled_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS status_card_summary_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    card_id UUID NOT NULL REFERENCES status_cards(id) ON DELETE CASCADE,
    markdown TEXT NOT NULL,
    compiled_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS status_card_update_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    card_id UUID NOT NULL REFERENCES status_cards(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,                    -- compile|full|incremental
    trigger TEXT NOT NULL DEFAULT 'manual',-- manual|interval|reactive|restore
    generation_issue_id UUID,
    run_id UUID,
    changes JSONB NOT NULL DEFAULT '[]',
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cost_cents INTEGER NOT NULL DEFAULT 0,
    model TEXT,
    query_version INTEGER,
    change_summary TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'running',-- running|ok|failed
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS status_card_updates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    card_id UUID NOT NULL REFERENCES status_cards(id) ON DELETE CASCADE,
    issue_id UUID,
    identifier TEXT,
    from_status TEXT,
    to_status TEXT,
    change_kind TEXT NOT NULL DEFAULT 'status',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS summary_slots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    scope_kind TEXT NOT NULL,
    scope_id UUID,
    slot_key TEXT NOT NULL,
    document_id UUID,
    status TEXT NOT NULL DEFAULT 'idle',       -- idle|generating|error
    failure_reason TEXT,
    generating_issue_id UUID,
    last_generated_at TIMESTAMPTZ,
    last_generated_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    last_model TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (company_id, scope_kind, scope_id, slot_key)
);

CREATE TABLE IF NOT EXISTS summary_slot_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slot_id UUID NOT NULL REFERENCES summary_slots(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL DEFAULT 1,
    markdown TEXT NOT NULL,
    title TEXT,
    change_summary TEXT,
    base_revision_id UUID,
    generation_issue_id UUID,
    model TEXT,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (slot_id, revision_number)
);

CREATE TABLE IF NOT EXISTS thread_interactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    interaction_type interaction_type NOT NULL,
    actor_type comment_actor_type NOT NULL,
    actor_id UUID,
    body TEXT NOT NULL,
    metadata JSONB,
    resolved_at TIMESTAMPTZ,
    resolved_by_type comment_actor_type,
    resolved_by_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS tool_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    application_id UUID,
    name TEXT NOT NULL,
    uid TEXT NOT NULL,
    connection_kind TEXT NOT NULL DEFAULT 'managed',
    ownership TEXT NOT NULL DEFAULT 'customer',
    transport TEXT NOT NULL DEFAULT 'mcp_remote',
    auth_kind TEXT NOT NULL DEFAULT 'none',
    status TEXT NOT NULL DEFAULT 'active',
    transport_config JSONB NOT NULL DEFAULT '{}',
    credential_secret_refs JSONB NOT NULL DEFAULT '[]',
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_by_agent_id UUID,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, uid)
);

CREATE TABLE IF NOT EXISTS tool_applications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
    connection_id UUID REFERENCES tool_connections(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending',   -- pending|approved|denied
    justification TEXT,
    reviewed_by_user_id TEXT,
    reviewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (agent_id, connection_id)
);

CREATE TABLE IF NOT EXISTS tool_connection_grants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    connection_id UUID NOT NULL REFERENCES tool_connections(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    grant_type TEXT NOT NULL DEFAULT 'explicit',
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (connection_id, agent_id)
);

CREATE TABLE IF NOT EXISTS tool_gateway_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES heartbeat_runs(id) ON DELETE CASCADE,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    project_id UUID,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS tool_invocations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    idempotency_key TEXT,
    actor_type TEXT NOT NULL DEFAULT 'system',
    actor_id TEXT,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    application_id UUID,
    connection_id UUID,
    catalog_entry_id UUID,
    tool_name TEXT NOT NULL,
    arguments_hash TEXT,
    arguments_summary JSONB,
    policy_decision TEXT,
    matched_policy_ids JSONB NOT NULL DEFAULT '[]',
    approval_state TEXT NOT NULL DEFAULT 'not_required',
    status TEXT NOT NULL DEFAULT 'pending',
    upstream_request_id TEXT,
    result_hash TEXT,
    result_summary JSONB,
    result_size_bytes INTEGER,
    result_artifact_id UUID,
    error_code TEXT,
    error_message TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, idempotency_key)
);

CREATE TABLE IF NOT EXISTS tool_action_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    invocation_id UUID NOT NULL REFERENCES tool_invocations(id) ON DELETE CASCADE,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    interaction_id UUID,
    approval_id UUID,
    status TEXT NOT NULL DEFAULT 'pending',
    canonical_arguments_hash TEXT NOT NULL,
    canonical_arguments_summary JSONB NOT NULL,
    signed_arguments TEXT,
    preview_markdown TEXT,
    requested_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    requested_by_user_id TEXT,
    resolved_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    resolved_by_user_id TEXT,
    decided_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    decided_by_user_id TEXT,
    decided_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS tool_call_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    actor_type TEXT NOT NULL DEFAULT 'system',
    actor_id TEXT,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    application_id UUID,
    connection_id UUID,
    catalog_entry_id UUID,
    invocation_id UUID REFERENCES tool_invocations(id) ON DELETE SET NULL,
    action_request_id UUID REFERENCES tool_action_requests(id) ON DELETE SET NULL,
    runtime_slot_id UUID,
    tool_name TEXT,
    decision TEXT,
    matched_policy_ids JSONB NOT NULL DEFAULT '[]',
    reason_code TEXT,
    outcome TEXT NOT NULL DEFAULT 'pending',
    latency_ms INTEGER,
    arguments_summary JSONB,
    request_hash TEXT,
    request_summary JSONB,
    result_hash TEXT,
    result_summary JSONB,
    result_size_bytes INTEGER,
    redaction_plan JSONB,
    rate_limit_state JSONB,
    metadata JSONB,
    error_code TEXT,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS tool_mcp_gateways (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    gateway_public_id TEXT NOT NULL UNIQUE DEFAULT ('gw_' || replace(gen_random_uuid()::text, '-', '')),
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    profile_id UUID,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, name),
    UNIQUE(company_id, slug)
);

CREATE TABLE IF NOT EXISTS tool_mcp_gateway_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    gateway_id UUID NOT NULL REFERENCES tool_mcp_gateways(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL DEFAULT '',
    allowed_actions JSONB NOT NULL DEFAULT '["tools/list", "tools/call"]',
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS tool_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    policy_type TEXT NOT NULL DEFAULT 'allow',
    priority INTEGER NOT NULL DEFAULT 0,
    enabled BOOLEAN NOT NULL DEFAULT true,
    selectors JSONB NOT NULL DEFAULT '{}',
    conditions JSONB,
    config JSONB,
    created_by_agent_id UUID,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, name)
);

CREATE TABLE IF NOT EXISTS tool_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    profile_key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    default_action TEXT NOT NULL DEFAULT 'deny',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, profile_key),
    UNIQUE(company_id, name)
);

CREATE TABLE IF NOT EXISTS tool_profile_bindings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    profile_id UUID NOT NULL REFERENCES tool_profiles(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,
    target_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, target_type, target_id)
);

CREATE TABLE IF NOT EXISTS tool_profile_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    profile_id UUID NOT NULL REFERENCES tool_profiles(id) ON DELETE CASCADE,
    selector_type TEXT NOT NULL,
    selector_value TEXT NOT NULL,
    effect TEXT NOT NULL DEFAULT 'allow',
    connection_id UUID REFERENCES tool_connections(id) ON DELETE CASCADE,
    tool_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_preferences (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    preferences JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, company_id)
);

CREATE TABLE IF NOT EXISTS user_secret_definitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    provider TEXT NOT NULL DEFAULT 'local_encrypted',
    managed_mode TEXT NOT NULL DEFAULT 'paperclip_managed',
    provider_config_id UUID REFERENCES company_secret_provider_configs(id) ON DELETE SET NULL,
    provider_metadata JSONB,
    usage_guidance TEXT,
    required BOOLEAN NOT NULL DEFAULT false,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id UUID,
    updated_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    updated_by_user_id UUID,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS secret_access_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE NO ACTION,
    secret_id UUID REFERENCES company_secrets(id) ON DELETE CASCADE,
    user_secret_definition_id UUID REFERENCES user_secret_definitions(id) ON DELETE SET NULL,
    version INTEGER,
    provider TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    actor_id TEXT,
    consumer_type TEXT NOT NULL,
    consumer_id TEXT NOT NULL,
    config_path TEXT,
    secret_scope TEXT NOT NULL DEFAULT 'company',
    responsible_user_id TEXT,
    credential_owner_user_id TEXT,
    credential_subject_type TEXT,
    credential_subject_id TEXT,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    heartbeat_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    plugin_id UUID,
    outcome TEXT NOT NULL,
    error_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS user_secret_declarations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_secret_definition_id UUID NOT NULL REFERENCES user_secret_definitions(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    config_path TEXT NOT NULL,
    env_key TEXT NOT NULL,
    version_selector TEXT NOT NULL DEFAULT 'latest',
    required BOOLEAN NOT NULL DEFAULT true,
    allow_missing_override BOOLEAN NOT NULL DEFAULT false,
    label TEXT,
    -- parrot extension: encrypted user-provided value (paperclip stores values via provider/UI).
    value_material JSONB,
    value_sha256 TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS user_sidebar_preferences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    company_order JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id)
);

CREATE TABLE IF NOT EXISTS workspace_operations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    execution_workspace_id UUID REFERENCES execution_workspaces(id) ON DELETE SET NULL,
    heartbeat_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    phase TEXT NOT NULL,
    command TEXT,
    cwd TEXT,
    status TEXT NOT NULL DEFAULT 'running',
    exit_code INTEGER,
    log_store TEXT,
    log_ref TEXT,
    log_bytes BIGINT,
    log_sha256 TEXT,
    log_compressed BOOLEAN NOT NULL DEFAULT FALSE,
    stdout_excerpt TEXT,
    stderr_excerpt TEXT,
    metadata JSONB,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);



-- ============================================
-- 3. 创建索引
-- ============================================

CREATE INDEX IF NOT EXISTS idx_companies_status ON companies(status);

CREATE INDEX IF NOT EXISTS idx_company_memberships_company_id ON company_memberships(company_id);

CREATE INDEX IF NOT EXISTS idx_company_memberships_principal ON company_memberships(principal_type, principal_id);

CREATE INDEX IF NOT EXISTS idx_company_memberships_role ON company_memberships(membership_role);

CREATE INDEX IF NOT EXISTS idx_projects_company_id ON projects(company_id);

CREATE INDEX IF NOT EXISTS idx_projects_status ON projects(status);

CREATE INDEX IF NOT EXISTS idx_projects_goal_id ON projects(goal_id);

CREATE INDEX IF NOT EXISTS idx_projects_lead_agent_id ON projects(lead_agent_id);

CREATE INDEX IF NOT EXISTS idx_project_workspaces_project_id ON project_workspaces(project_id);

CREATE INDEX IF NOT EXISTS idx_project_workspaces_is_primary ON project_workspaces(is_primary);

CREATE INDEX IF NOT EXISTS idx_project_memberships_company_id ON project_memberships(company_id);

CREATE INDEX IF NOT EXISTS idx_project_memberships_project_id ON project_memberships(project_id);

CREATE INDEX IF NOT EXISTS idx_project_memberships_user_id ON project_memberships(user_id);

CREATE INDEX IF NOT EXISTS idx_project_memberships_state ON project_memberships(state);

CREATE INDEX IF NOT EXISTS idx_project_memberships_starred ON project_memberships(starred_at) WHERE starred_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_agent_memberships_company_id ON agent_memberships(company_id);

CREATE INDEX IF NOT EXISTS idx_agent_memberships_agent_id ON agent_memberships(agent_id);

CREATE INDEX IF NOT EXISTS idx_agent_memberships_user_id ON agent_memberships(user_id);

CREATE INDEX IF NOT EXISTS idx_agent_memberships_state ON agent_memberships(state);

CREATE INDEX IF NOT EXISTS idx_agent_memberships_starred ON agent_memberships(starred_at) WHERE starred_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_activity_logs_company_id ON activity_logs(company_id);

CREATE INDEX IF NOT EXISTS idx_activity_logs_event_type ON activity_logs(event_type);

CREATE INDEX IF NOT EXISTS idx_activity_logs_resource ON activity_logs(resource_type, resource_id);

CREATE INDEX IF NOT EXISTS idx_activity_logs_created_at ON activity_logs(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_activity_logs_actor ON activity_logs(actor_type, actor_id);

CREATE INDEX IF NOT EXISTS idx_activity_logs_run_id ON activity_logs(run_id) WHERE run_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_activity_logs_agent_id ON activity_logs(agent_id) WHERE agent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_activity_logs_run_resource ON activity_logs(company_id, run_id, resource_type, resource_id) WHERE run_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_agents_company_id ON agents(company_id);

CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);

CREATE INDEX IF NOT EXISTS idx_agents_reports_to ON agents(reports_to);

CREATE INDEX IF NOT EXISTS idx_agent_config_revisions_agent_id ON agent_config_revisions(agent_id);

CREATE INDEX IF NOT EXISTS idx_agent_config_revisions_created_at ON agent_config_revisions(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_cost_events_agent_id ON cost_events(agent_id);

CREATE INDEX IF NOT EXISTS idx_cost_events_created_at ON cost_events(created_at DESC);

CREATE INDEX IF NOT EXISTS issues_company_status_idx ON issues(company_id, status);

CREATE INDEX IF NOT EXISTS issues_company_assignee_status_idx ON issues(company_id, assignee_agent_id, status);

CREATE INDEX IF NOT EXISTS issues_company_assignee_user_status_idx ON issues(company_id, assignee_user_id, status);

CREATE INDEX IF NOT EXISTS issues_company_responsible_user_idx ON issues(company_id, responsible_user_id);

CREATE INDEX IF NOT EXISTS issues_company_parent_idx ON issues(company_id, parent_id);

CREATE INDEX IF NOT EXISTS issues_company_project_idx ON issues(company_id, project_id);

CREATE INDEX IF NOT EXISTS issues_company_origin_idx ON issues(company_id, origin_kind, origin_id);

CREATE INDEX IF NOT EXISTS issues_company_execution_workspace_idx ON issues(company_id, execution_workspace_id);

CREATE INDEX IF NOT EXISTS issues_company_monitor_due_idx ON issues(company_id, monitor_next_check_at);

CREATE INDEX IF NOT EXISTS issues_company_updated_idx ON issues(company_id, updated_at);

CREATE INDEX IF NOT EXISTS issues_company_created_idx ON issues(company_id, created_at);

CREATE INDEX IF NOT EXISTS issues_company_priority_idx ON issues(company_id, priority);

CREATE INDEX IF NOT EXISTS issues_title_search_idx ON issues USING gin(title gin_trgm_ops);

CREATE INDEX IF NOT EXISTS issues_identifier_search_idx ON issues USING gin(identifier gin_trgm_ops);

CREATE INDEX IF NOT EXISTS issues_description_search_idx ON issues USING gin(description gin_trgm_ops);

CREATE INDEX IF NOT EXISTS issue_comments_issue_idx ON issue_comments(issue_id);

CREATE INDEX IF NOT EXISTS issue_comments_company_idx ON issue_comments(company_id);

CREATE INDEX IF NOT EXISTS thread_interactions_issue_idx ON thread_interactions(issue_id);

CREATE INDEX IF NOT EXISTS thread_interactions_resolved_idx ON thread_interactions(issue_id, resolved_at);

CREATE INDEX IF NOT EXISTS documents_company_idx ON documents(company_id);

CREATE INDEX IF NOT EXISTS documents_locked_idx ON documents(locked_by_id, locked_at);

CREATE INDEX IF NOT EXISTS issue_documents_issue_idx ON issue_documents(issue_id);

CREATE INDEX IF NOT EXISTS issue_documents_document_idx ON issue_documents(document_id);

CREATE INDEX IF NOT EXISTS annotation_threads_document_idx ON annotation_threads(document_id);

CREATE INDEX IF NOT EXISTS annotation_threads_status_idx ON annotation_threads(document_id, status);

CREATE INDEX IF NOT EXISTS annotation_comments_thread_idx ON annotation_comments(thread_id);

CREATE INDEX IF NOT EXISTS issue_work_products_issue_idx ON issue_work_products(issue_id);

CREATE INDEX IF NOT EXISTS issue_work_products_company_idx ON issue_work_products(company_id);

CREATE INDEX IF NOT EXISTS labels_company_idx ON labels(company_id);

CREATE INDEX IF NOT EXISTS issue_labels_issue_idx ON issue_labels(issue_id);

CREATE INDEX IF NOT EXISTS issue_labels_label_idx ON issue_labels(label_id);

CREATE INDEX IF NOT EXISTS attachments_parent_idx ON attachments(parent_type, parent_id);

CREATE INDEX IF NOT EXISTS attachments_company_idx ON attachments(company_id);

CREATE INDEX IF NOT EXISTS issue_tree_holds_root_issue_idx ON issue_tree_holds(root_issue_id);

CREATE INDEX IF NOT EXISTS issue_tree_holds_company_status_idx ON issue_tree_holds(company_id, status);

CREATE INDEX IF NOT EXISTS issue_tree_holds_mode_idx ON issue_tree_holds(mode, status);

CREATE INDEX IF NOT EXISTS issue_tree_hold_members_hold_idx ON issue_tree_hold_members(hold_id);

CREATE INDEX IF NOT EXISTS issue_tree_hold_members_issue_idx ON issue_tree_hold_members(issue_id);

CREATE INDEX IF NOT EXISTS issue_tree_hold_members_company_idx ON issue_tree_hold_members(company_id);

CREATE INDEX IF NOT EXISTS cases_company_idx ON cases(company_id);

CREATE INDEX IF NOT EXISTS cases_company_status_idx ON cases(company_id, status);

CREATE INDEX IF NOT EXISTS cases_company_type_idx ON cases(company_id, case_type);

CREATE INDEX IF NOT EXISTS cases_project_idx ON cases(project_id);

CREATE INDEX IF NOT EXISTS cases_parent_case_idx ON cases(parent_case_id);

CREATE INDEX IF NOT EXISTS cases_identifier_idx ON cases(identifier);

CREATE INDEX IF NOT EXISTS cases_company_updated_idx ON cases(company_id, updated_at);

CREATE INDEX IF NOT EXISTS case_events_case_idx ON case_events(case_id, created_at DESC);

CREATE INDEX IF NOT EXISTS case_events_kind_idx ON case_events(case_id, kind);

CREATE INDEX IF NOT EXISTS case_issue_links_case_idx ON case_issue_links(case_id);

CREATE INDEX IF NOT EXISTS case_issue_links_issue_idx ON case_issue_links(issue_id);

CREATE INDEX IF NOT EXISTS case_issue_links_role_idx ON case_issue_links(case_id, role);

CREATE INDEX IF NOT EXISTS case_documents_case_idx ON case_documents(case_id);

CREATE INDEX IF NOT EXISTS case_documents_document_idx ON case_documents(document_id);

CREATE INDEX IF NOT EXISTS document_revisions_document_idx ON document_revisions(document_id, revision_number DESC);

CREATE INDEX IF NOT EXISTS case_attachments_case_idx ON case_attachments(case_id);

CREATE INDEX IF NOT EXISTS case_attachments_asset_idx ON case_attachments(asset_id);

CREATE INDEX IF NOT EXISTS case_labels_case_idx ON case_labels(case_id);

CREATE INDEX IF NOT EXISTS case_labels_label_idx ON case_labels(label_id);

CREATE INDEX IF NOT EXISTS idx_agent_api_keys_agent_id ON agent_api_keys(agent_id);

CREATE INDEX IF NOT EXISTS idx_agent_api_keys_key_hash ON agent_api_keys(key_hash);

CREATE INDEX IF NOT EXISTS idx_agent_api_keys_last_used_at ON agent_api_keys(last_used_at DESC);

CREATE INDEX IF NOT EXISTS idx_routines_company_id ON routines(company_id);

CREATE INDEX IF NOT EXISTS idx_routines_project_id ON routines(project_id) WHERE project_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_routines_goal_id ON routines(goal_id) WHERE goal_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_routines_assignee_agent_id ON routines(assignee_agent_id);

CREATE INDEX IF NOT EXISTS idx_routines_status ON routines(status);

CREATE INDEX IF NOT EXISTS idx_routines_created_at ON routines(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_routine_triggers_routine_id ON routine_triggers(routine_id);

CREATE INDEX IF NOT EXISTS idx_routine_triggers_company_id ON routine_triggers(company_id);

CREATE INDEX IF NOT EXISTS idx_routine_triggers_next_run_at ON routine_triggers(next_run_at) WHERE enabled = true AND kind = 'schedule';

CREATE INDEX IF NOT EXISTS idx_routine_triggers_public_id ON routine_triggers(public_id) WHERE public_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_routine_revisions_routine_id ON routine_revisions(routine_id, revision_number DESC);

CREATE INDEX IF NOT EXISTS idx_routine_revisions_company_id ON routine_revisions(company_id);

CREATE INDEX IF NOT EXISTS idx_routine_revisions_created_at ON routine_revisions(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_routine_runs_routine_id ON routine_runs(routine_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_routine_runs_company_id ON routine_runs(company_id);

CREATE INDEX IF NOT EXISTS idx_routine_runs_status ON routine_runs(status);

CREATE INDEX IF NOT EXISTS idx_routine_runs_trigger_id ON routine_runs(trigger_id) WHERE trigger_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_routine_runs_linked_issue_id ON routine_runs(linked_issue_id) WHERE linked_issue_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_routine_runs_dispatch_fingerprint ON routine_runs(dispatch_fingerprint) WHERE dispatch_fingerprint IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_goals_company_id ON goals(company_id);

CREATE INDEX IF NOT EXISTS idx_goals_parent_id ON goals(parent_id) WHERE parent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_goals_owner_agent_id ON goals(owner_agent_id) WHERE owner_agent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_goals_level_status ON goals(level, status);

CREATE INDEX IF NOT EXISTS idx_goals_created_at ON goals(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_approvals_company_id ON approvals(company_id);

CREATE INDEX IF NOT EXISTS idx_approvals_status ON approvals(status);

CREATE INDEX IF NOT EXISTS idx_approvals_requested_by_agent_id ON approvals(requested_by_agent_id) WHERE requested_by_agent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_approvals_created_at ON approvals(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_issue_approvals_approval_id ON issue_approvals(approval_id);

CREATE INDEX IF NOT EXISTS idx_issue_approvals_issue_id ON issue_approvals(issue_id);

CREATE INDEX IF NOT EXISTS idx_auth_users_email ON auth_users(email);

CREATE INDEX IF NOT EXISTS idx_auth_users_created_at ON auth_users(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_auth_sessions_user_id ON auth_sessions(user_id);

CREATE INDEX IF NOT EXISTS idx_auth_sessions_token ON auth_sessions(token);

CREATE INDEX IF NOT EXISTS idx_auth_sessions_expires_at ON auth_sessions(expires_at);

CREATE INDEX IF NOT EXISTS idx_auth_sessions_cleanup ON auth_sessions(expires_at);

CREATE INDEX IF NOT EXISTS idx_board_api_keys_company_id ON board_api_keys(company_id);

CREATE INDEX IF NOT EXISTS idx_board_api_keys_user_id ON board_api_keys(user_id);

CREATE INDEX IF NOT EXISTS idx_board_api_keys_key_hash ON board_api_keys(key_hash);

CREATE INDEX IF NOT EXISTS idx_board_api_keys_expires_at ON board_api_keys(expires_at) WHERE expires_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_principal_permission_grants_company_id ON principal_permission_grants(company_id);

CREATE INDEX IF NOT EXISTS idx_principal_permission_grants_principal ON principal_permission_grants(principal_type, principal_id);

CREATE INDEX IF NOT EXISTS idx_principal_permission_grants_permission_key ON principal_permission_grants(permission_key);

CREATE INDEX IF NOT EXISTS idx_invites_company_id ON invites(company_id);

CREATE INDEX IF NOT EXISTS idx_invites_token ON invites(token);

CREATE INDEX IF NOT EXISTS idx_invites_expires_at ON invites(expires_at);

CREATE INDEX IF NOT EXISTS idx_invites_accepted ON invites(accepted);

CREATE INDEX IF NOT EXISTS idx_join_requests_company_id ON join_requests(company_id);

CREATE INDEX IF NOT EXISTS idx_join_requests_requester_user_id ON join_requests(requester_user_id);

CREATE INDEX IF NOT EXISTS idx_join_requests_status ON join_requests(status);

CREATE INDEX IF NOT EXISTS idx_cli_auth_challenges_challenge_code ON cli_auth_challenges(challenge_code);

CREATE INDEX IF NOT EXISTS idx_cli_auth_challenges_expires_at ON cli_auth_challenges(expires_at);

CREATE INDEX IF NOT EXISTS idx_instance_user_roles_user_id ON instance_user_roles(user_id);

CREATE INDEX IF NOT EXISTS idx_pipelines_company_id ON pipelines(company_id);

CREATE INDEX IF NOT EXISTS idx_pipelines_project_id ON pipelines(project_id) WHERE project_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_pipelines_key ON pipelines(key);

CREATE INDEX IF NOT EXISTS idx_pipeline_stages_pipeline_id ON pipeline_stages(pipeline_id);

CREATE INDEX IF NOT EXISTS idx_pipeline_stages_position ON pipeline_stages(pipeline_id, position);

CREATE INDEX IF NOT EXISTS idx_pipeline_stages_kind ON pipeline_stages(kind);

CREATE INDEX IF NOT EXISTS idx_pipeline_transitions_pipeline_id ON pipeline_transitions(pipeline_id);

CREATE INDEX IF NOT EXISTS idx_pipeline_transitions_from_stage ON pipeline_transitions(from_stage_id);

CREATE INDEX IF NOT EXISTS idx_pipeline_transitions_to_stage ON pipeline_transitions(to_stage_id);

CREATE INDEX IF NOT EXISTS idx_pipeline_cases_company_id ON pipeline_cases(company_id);

CREATE INDEX IF NOT EXISTS idx_pipeline_cases_pipeline_stage ON pipeline_cases(pipeline_id, stage_id);

CREATE INDEX IF NOT EXISTS idx_pipeline_cases_terminal_kind ON pipeline_cases(terminal_kind) WHERE terminal_kind IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_pipeline_cases_case_key ON pipeline_cases(case_key);

CREATE INDEX IF NOT EXISTS idx_company_secrets_company ON company_secrets(company_id);

CREATE INDEX IF NOT EXISTS idx_company_secrets_company_provider ON company_secrets(company_id, provider);

CREATE INDEX IF NOT EXISTS idx_company_secrets_company_scope ON company_secrets(company_id, scope);

CREATE INDEX IF NOT EXISTS idx_company_secrets_company_owner ON company_secrets(company_id, owner_user_id);

CREATE INDEX IF NOT EXISTS idx_company_secrets_user_definition_owner ON company_secrets(company_id, user_secret_definition_id, owner_user_id);

CREATE INDEX IF NOT EXISTS idx_company_secrets_provider_config ON company_secrets(provider_config_id);

CREATE UNIQUE INDEX IF NOT EXISTS company_secrets_company_key_uq
    ON company_secrets(company_id, key) WHERE scope = 'company' AND deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS company_secrets_company_name_uq
    ON company_secrets(company_id, name) WHERE scope = 'company' AND deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS company_secrets_user_definition_owner_uq
    ON company_secrets(company_id, user_secret_definition_id, owner_user_id) WHERE scope = 'user' AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_pipeline_case_events_case_id ON pipeline_case_events(case_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_pipeline_case_events_event_type ON pipeline_case_events(event_type);

CREATE INDEX IF NOT EXISTS issue_read_status_company_idx ON issue_read_status(company_id);

CREATE INDEX IF NOT EXISTS issue_read_status_issue_idx ON issue_read_status(issue_id);

CREATE INDEX IF NOT EXISTS issue_read_status_user_idx ON issue_read_status(company_id, user_id);

CREATE INDEX IF NOT EXISTS issue_inbox_archives_company_idx ON issue_inbox_archives(company_id);

CREATE INDEX IF NOT EXISTS issue_inbox_archives_issue_idx ON issue_inbox_archives(issue_id);

CREATE INDEX IF NOT EXISTS issue_inbox_archives_user_idx ON issue_inbox_archives(company_id, user_id);

CREATE INDEX IF NOT EXISTS issue_inbox_archives_company_issue_idx ON issue_inbox_archives(company_id, issue_id);

CREATE INDEX IF NOT EXISTS feedback_votes_company_idx ON feedback_votes(company_id);

CREATE INDEX IF NOT EXISTS feedback_votes_issue_idx ON feedback_votes(issue_id);

CREATE INDEX IF NOT EXISTS feedback_votes_voter_idx ON feedback_votes(company_id, voter_id);

CREATE INDEX IF NOT EXISTS feedback_traces_company_idx ON feedback_traces(company_id);

CREATE INDEX IF NOT EXISTS feedback_traces_issue_idx ON feedback_traces(issue_id);

CREATE INDEX IF NOT EXISTS feedback_traces_vote_idx ON feedback_traces(vote_id);

CREATE INDEX IF NOT EXISTS feedback_traces_status_idx ON feedback_traces(status);

CREATE INDEX IF NOT EXISTS recovery_actions_company_idx ON recovery_actions(company_id);

CREATE INDEX IF NOT EXISTS recovery_actions_issue_idx ON recovery_actions(issue_id);

CREATE INDEX IF NOT EXISTS recovery_actions_status_idx ON recovery_actions(status);

CREATE INDEX IF NOT EXISTS recovery_actions_triggered_idx ON recovery_actions(company_id, status, triggered_at);

CREATE INDEX IF NOT EXISTS plan_decompositions_company_idx ON plan_decompositions(company_id);

CREATE INDEX IF NOT EXISTS plan_decompositions_issue_idx ON plan_decompositions(issue_id);

CREATE INDEX IF NOT EXISTS heartbeat_runs_company_agent_started_idx
    ON heartbeat_runs(company_id, agent_id, started_at);

CREATE INDEX IF NOT EXISTS heartbeat_runs_company_status_idx
    ON heartbeat_runs(company_id, status);

CREATE INDEX IF NOT EXISTS issue_watchdogs_company_status_idx ON issue_watchdogs(company_id, status);

CREATE INDEX IF NOT EXISTS issue_watchdogs_company_agent_idx ON issue_watchdogs(company_id, watchdog_agent_id);

CREATE INDEX IF NOT EXISTS agent_wakeup_requests_company_status_idx ON agent_wakeup_requests(company_id, status);

CREATE INDEX IF NOT EXISTS agent_wakeup_requests_company_agent_idx ON agent_wakeup_requests(company_id, agent_id);

CREATE INDEX IF NOT EXISTS issue_thread_interactions_issue_idx ON issue_thread_interactions(issue_id);

CREATE INDEX IF NOT EXISTS issue_thread_interactions_company_issue_idx ON issue_thread_interactions(company_id, issue_id);

CREATE INDEX IF NOT EXISTS environments_company_status_idx ON environments(company_id, status);

CREATE UNIQUE INDEX IF NOT EXISTS environments_company_driver_idx ON environments(company_id, driver);

CREATE INDEX IF NOT EXISTS environments_company_name_idx ON environments(company_id, name);

CREATE INDEX IF NOT EXISTS execution_workspaces_company_project_status_idx
    ON execution_workspaces(company_id, project_id, status);

CREATE INDEX IF NOT EXISTS execution_workspaces_company_project_workspace_status_idx
    ON execution_workspaces(company_id, project_workspace_id, status);

CREATE INDEX IF NOT EXISTS execution_workspaces_company_source_issue_idx
    ON execution_workspaces(company_id, source_issue_id);

CREATE INDEX IF NOT EXISTS execution_workspaces_company_last_used_idx
    ON execution_workspaces(company_id, last_used_at);

CREATE INDEX IF NOT EXISTS execution_workspaces_company_branch_idx
    ON execution_workspaces(company_id, branch_name);

CREATE INDEX IF NOT EXISTS environment_leases_company_environment_status_idx
    ON environment_leases(company_id, environment_id, status);

CREATE INDEX IF NOT EXISTS environment_leases_company_execution_workspace_idx
    ON environment_leases(company_id, execution_workspace_id);

CREATE INDEX IF NOT EXISTS environment_leases_company_issue_idx
    ON environment_leases(company_id, issue_id);

CREATE INDEX IF NOT EXISTS environment_leases_heartbeat_run_idx
    ON environment_leases(heartbeat_run_id);

CREATE INDEX IF NOT EXISTS environment_leases_company_last_used_idx
    ON environment_leases(company_id, last_used_at);

CREATE INDEX IF NOT EXISTS environment_leases_provider_lease_idx
    ON environment_leases(provider_lease_id);

CREATE INDEX IF NOT EXISTS assets_company_created_idx ON assets(company_id, created_at);

CREATE INDEX IF NOT EXISTS assets_company_provider_idx ON assets(company_id, provider);

CREATE UNIQUE INDEX IF NOT EXISTS assets_company_object_key_uq ON assets(company_id, object_key);

CREATE INDEX IF NOT EXISTS company_secret_provider_configs_company_idx
    ON company_secret_provider_configs(company_id);

CREATE INDEX IF NOT EXISTS company_secret_provider_configs_company_provider_idx
    ON company_secret_provider_configs(company_id, provider);

CREATE UNIQUE INDEX IF NOT EXISTS company_secret_provider_configs_default_uq
    ON company_secret_provider_configs(company_id, provider) WHERE is_default = true;

CREATE INDEX IF NOT EXISTS company_secret_versions_secret_idx
    ON company_secret_versions(secret_id, created_at);

CREATE INDEX IF NOT EXISTS company_secret_versions_value_sha256_idx
    ON company_secret_versions(value_sha256);

CREATE UNIQUE INDEX IF NOT EXISTS company_secret_versions_secret_version_uq
    ON company_secret_versions(secret_id, version);

CREATE INDEX IF NOT EXISTS company_secret_versions_fingerprint_idx
    ON company_secret_versions(fingerprint_sha256);

CREATE INDEX IF NOT EXISTS company_secret_bindings_company_idx ON company_secret_bindings(company_id);

CREATE INDEX IF NOT EXISTS company_secret_bindings_secret_idx ON company_secret_bindings(secret_id);

CREATE INDEX IF NOT EXISTS company_secret_bindings_target_idx
    ON company_secret_bindings(company_id, target_type, target_id);

CREATE UNIQUE INDEX IF NOT EXISTS company_secret_bindings_target_path_uq
    ON company_secret_bindings(company_id, target_type, target_id, config_path);

CREATE INDEX IF NOT EXISTS user_secret_definitions_company_status_idx
    ON user_secret_definitions(company_id, status);

CREATE INDEX IF NOT EXISTS user_secret_definitions_company_provider_idx
    ON user_secret_definitions(company_id, provider);

CREATE INDEX IF NOT EXISTS user_secret_definitions_provider_config_idx
    ON user_secret_definitions(provider_config_id);

CREATE UNIQUE INDEX IF NOT EXISTS user_secret_definitions_company_key_uq
    ON user_secret_definitions(company_id, key) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS user_secret_declarations_company_idx
    ON user_secret_declarations(company_id);

CREATE INDEX IF NOT EXISTS user_secret_declarations_definition_idx
    ON user_secret_declarations(user_secret_definition_id);

CREATE INDEX IF NOT EXISTS user_secret_declarations_target_idx
    ON user_secret_declarations(company_id, target_type, target_id);

CREATE INDEX IF NOT EXISTS user_secret_declarations_company_required_idx
    ON user_secret_declarations(company_id, required);

CREATE UNIQUE INDEX IF NOT EXISTS user_secret_declarations_target_path_uq
    ON user_secret_declarations(company_id, target_type, target_id, config_path);

CREATE INDEX IF NOT EXISTS user_secret_declarations_required_override_idx
    ON user_secret_declarations(company_id, allow_missing_override) WHERE allow_missing_override = true;

CREATE INDEX IF NOT EXISTS secret_access_events_company_created_idx
    ON secret_access_events(company_id, created_at);

CREATE INDEX IF NOT EXISTS secret_access_events_secret_created_idx
    ON secret_access_events(secret_id, created_at);

CREATE INDEX IF NOT EXISTS secret_access_events_consumer_idx
    ON secret_access_events(company_id, consumer_type, consumer_id);

CREATE INDEX IF NOT EXISTS secret_access_events_run_idx
    ON secret_access_events(heartbeat_run_id);

CREATE INDEX IF NOT EXISTS secret_access_events_user_definition_created_idx
    ON secret_access_events(user_secret_definition_id, created_at);

CREATE INDEX IF NOT EXISTS secret_access_events_company_credential_owner_idx
    ON secret_access_events(company_id, credential_owner_user_id, created_at);

CREATE INDEX IF NOT EXISTS idx_company_skills_company_id ON company_skills(company_id);

CREATE INDEX IF NOT EXISTS idx_company_skills_catalog_id ON company_skills(catalog_id);

CREATE INDEX IF NOT EXISTS idx_company_skills_name ON company_skills(name);

CREATE INDEX IF NOT EXISTS idx_skill_versions_skill_id ON skill_versions(skill_id);

CREATE INDEX IF NOT EXISTS idx_skill_test_inputs_skill_id ON skill_test_inputs(skill_id);

CREATE INDEX IF NOT EXISTS idx_skill_test_runs_skill_id ON skill_test_runs(skill_id);

CREATE INDEX IF NOT EXISTS idx_skill_stars_skill_id ON skill_stars(skill_id);

CREATE INDEX IF NOT EXISTS idx_skill_comments_skill_id ON skill_comments(skill_id);

CREATE INDEX IF NOT EXISTS idx_skill_files_skill_id ON skill_files(skill_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_cost_events_company_id ON cost_events(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_cost_events_occurred_at ON cost_events(occurred_at DESC);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_cost_events_provider ON cost_events(provider);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_cost_events_biller ON cost_events(biller);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_cost_events_model ON cost_events(model);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_cost_events_project_id ON cost_events(project_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_cost_events_issue_id ON cost_events(issue_id);

CREATE INDEX IF NOT EXISTS idx_budget_policies_company_id ON budget_policies(company_id);

CREATE INDEX IF NOT EXISTS idx_budget_policies_scope ON budget_policies(scope_type, scope_id);

CREATE INDEX IF NOT EXISTS idx_budget_incidents_company_id ON budget_incidents(company_id);

CREATE INDEX IF NOT EXISTS idx_budget_incidents_policy_id ON budget_incidents(policy_id);

CREATE INDEX IF NOT EXISTS idx_budget_incidents_status ON budget_incidents(status);

CREATE INDEX IF NOT EXISTS idx_finance_events_company_id ON finance_events(company_id);

CREATE INDEX IF NOT EXISTS idx_finance_events_agent_id ON finance_events(agent_id);

CREATE INDEX IF NOT EXISTS idx_finance_events_occurred_at ON finance_events(occurred_at DESC);

-- CREATE INDEX IF NOT EXISTS agent_api_keys_scope_type_idx  -- FIXME: column "scope" does not exist in agent_api_keys
--     ON agent_api_keys ((scope->>'scope_type'));  -- FIXME: column "scope" does not exist in agent_api_keys
--   -- FIXME: column "scope" does not exist in agent_api_keys
CREATE INDEX IF NOT EXISTS cloud_upstream_connections_company_idx ON cloud_upstream_connections(company_id);

CREATE INDEX IF NOT EXISTS cloud_upstream_runs_connection_idx ON cloud_upstream_runs(connection_id);

CREATE INDEX IF NOT EXISTS plugins_status_idx ON plugins(status);

CREATE UNIQUE INDEX IF NOT EXISTS issue_inbox_archives_company_issue_user_idx ON issue_inbox_archives(company_id, issue_id, user_id);

CREATE INDEX IF NOT EXISTS folders_company_kind_position_idx ON folders(company_id, kind, position, name);

CREATE INDEX IF NOT EXISTS folders_company_kind_parent_idx ON folders(company_id, kind, parent_id, position, name);

-- [REMOVED] CREATE INDEX IF NOT EXISTS routines_company_folder_idx ON routines(company_id, folder_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS company_skills_company_folder_idx ON company_skills(company_id, folder_id);

CREATE INDEX IF NOT EXISTS tool_invocations_company_created_idx
    ON tool_invocations(company_id, created_at);

CREATE INDEX IF NOT EXISTS tool_invocations_run_idx
    ON tool_invocations(company_id, run_id);

CREATE INDEX IF NOT EXISTS tool_action_requests_invocation_idx
    ON tool_action_requests(invocation_id);

CREATE INDEX IF NOT EXISTS tool_call_events_run_idx
    ON tool_call_events(company_id, run_id);

CREATE INDEX IF NOT EXISTS tool_call_events_invocation_idx
    ON tool_call_events(invocation_id);

CREATE INDEX IF NOT EXISTS tool_gateway_sessions_run_idx
    ON tool_gateway_sessions(company_id, run_id);

CREATE INDEX IF NOT EXISTS tool_mcp_gateways_company_idx ON tool_mcp_gateways(company_id, status);

CREATE INDEX IF NOT EXISTS tool_mcp_gateway_tokens_gateway_idx ON tool_mcp_gateway_tokens(company_id, gateway_id);

CREATE INDEX IF NOT EXISTS workspace_operations_company_run_started_idx ON workspace_operations(company_id, heartbeat_run_id, started_at);

CREATE INDEX IF NOT EXISTS workspace_operations_company_workspace_started_idx ON workspace_operations(company_id, execution_workspace_id, started_at);

CREATE INDEX IF NOT EXISTS document_annotation_threads_routine_idx ON document_annotation_threads(company_id, routine_id, status);

CREATE INDEX IF NOT EXISTS document_annotation_comments_thread_idx ON document_annotation_comments(company_id, thread_id, created_at);

CREATE INDEX IF NOT EXISTS environment_custom_image_setup_sessions_environment_status_idx ON environment_custom_image_setup_sessions(environment_id, status);

CREATE INDEX IF NOT EXISTS environment_custom_image_setup_sessions_expires_idx ON environment_custom_image_setup_sessions(expires_at);

CREATE INDEX IF NOT EXISTS document_annotation_threads_routine_document_idx
    ON document_annotation_threads(routine_id, document_id, status);

CREATE INDEX IF NOT EXISTS document_annotation_comments_routine_thread_idx
    ON document_annotation_comments(routine_id, thread_id, created_at);

CREATE INDEX IF NOT EXISTS idx_approval_comments_approval_created
    ON approval_comments (approval_id, created_at ASC);

CREATE INDEX IF NOT EXISTS environment_custom_image_templates_environment_status_idx
    ON environment_custom_image_templates(environment_id, status);

CREATE UNIQUE INDEX IF NOT EXISTS environment_custom_image_templates_environment_active_uq
    ON environment_custom_image_templates(environment_id) WHERE status = 'active';

CREATE INDEX IF NOT EXISTS environment_custom_image_setup_sessions_environment_status_idx
    ON environment_custom_image_setup_sessions(environment_id, status);


CREATE INDEX IF NOT EXISTS issue_relations_company_issue_idx
    ON issue_relations(company_id, issue_id);

CREATE INDEX IF NOT EXISTS issue_relations_company_related_issue_idx
    ON issue_relations(company_id, related_issue_id);

CREATE INDEX IF NOT EXISTS issue_relations_company_type_idx
    ON issue_relations(company_id, type);

-- [REMOVED] CREATE INDEX IF NOT EXISTS document_annotation_threads_issue_document_idx
-- [REMOVED]     ON document_annotation_threads(issue_id, document_id, status);

-- [REMOVED] CREATE INDEX IF NOT EXISTS document_annotation_comments_issue_thread_idx
-- [REMOVED]     ON document_annotation_comments(issue_id, thread_id, created_at);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_project_workspaces_company_id ON project_workspaces(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_project_workspaces_source_type ON project_workspaces(source_type);

CREATE INDEX IF NOT EXISTS idx_project_goals_project_id ON project_goals(project_id);

CREATE INDEX IF NOT EXISTS idx_project_goals_goal_id ON project_goals(goal_id);

CREATE INDEX IF NOT EXISTS idx_project_goals_company_id ON project_goals(company_id);

CREATE INDEX IF NOT EXISTS idx_plugin_managed_resources_company_id ON plugin_managed_resources(company_id);

CREATE INDEX IF NOT EXISTS idx_plugin_managed_resources_plugin_id ON plugin_managed_resources(plugin_id);

CREATE INDEX IF NOT EXISTS idx_plugin_managed_resources_resource_id ON plugin_managed_resources(resource_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_plugin_managed_resources_lookup 
    ON plugin_managed_resources(company_id, plugin_id, resource_kind, resource_key);

CREATE INDEX IF NOT EXISTS idx_project_memberships_company_user ON project_memberships(company_id, user_id);

CREATE INDEX IF NOT EXISTS idx_project_memberships_company_user_starred ON project_memberships(company_id, user_id, starred_at);

CREATE INDEX IF NOT EXISTS idx_project_memberships_project ON project_memberships(project_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_project_memberships_company_user_project 
            ON project_memberships(company_id, user_id, project_id);

CREATE INDEX IF NOT EXISTS idx_agent_memberships_company_user ON agent_memberships(company_id, user_id);

CREATE INDEX IF NOT EXISTS idx_agent_memberships_company_user_starred ON agent_memberships(company_id, user_id, starred_at);

CREATE INDEX IF NOT EXISTS idx_agent_memberships_agent ON agent_memberships(agent_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_memberships_company_user_agent 
            ON agent_memberships(company_id, user_id, agent_id);

CREATE INDEX IF NOT EXISTS issue_plan_decompositions_company_source_status_idx 
    ON issue_plan_decompositions(company_id, source_issue_id, status);

CREATE UNIQUE INDEX IF NOT EXISTS issue_plan_decompositions_source_revision_uq 
    ON issue_plan_decompositions(company_id, source_issue_id, accepted_plan_revision_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS issue_thread_interactions_created_by_agent_idx 
-- [REMOVED]     ON issue_thread_interactions(created_by_agent_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS issue_thread_interactions_resolved_by_agent_idx 
-- [REMOVED]     ON issue_thread_interactions(resolved_by_agent_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS issue_thread_interactions_source_comment_idx 
-- [REMOVED]     ON issue_thread_interactions(source_comment_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_invites_allowed_join_types ON invites(allowed_join_types);

-- CREATE INDEX IF NOT EXISTS idx_companies_interaction_resolver_governance 
--     ON companies USING gin(interaction_resolver_governance);

CREATE INDEX IF NOT EXISTS heartbeat_run_watchdog_decisions_run_idx
    ON heartbeat_run_watchdog_decisions(run_id);

CREATE INDEX IF NOT EXISTS heartbeat_run_watchdog_decisions_company_idx
    ON heartbeat_run_watchdog_decisions(company_id);

CREATE INDEX IF NOT EXISTS idx_decision_bundles_company_created
    ON decision_bundles(company_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_decision_bundles_origin_issue
    ON decision_bundles(origin_issue_id);

CREATE INDEX IF NOT EXISTS idx_decisions_company_status_expires
    ON decisions(company_id, status, expires_at);

CREATE INDEX IF NOT EXISTS idx_decisions_bundle
    ON decisions(bundle_id);

CREATE INDEX IF NOT EXISTS idx_decisions_origin_issue
    ON decisions(origin_issue_id);

CREATE INDEX IF NOT EXISTS idx_decisions_origin_agent
    ON decisions(origin_agent_id);

CREATE INDEX IF NOT EXISTS idx_decisions_company_rule_key
    ON decisions(company_id, rule_key);

CREATE INDEX IF NOT EXISTS idx_decision_target_issues_issue
    ON decision_target_issues(issue_id);

CREATE INDEX IF NOT EXISTS idx_decision_target_issues_company
    ON decision_target_issues(company_id);

CREATE UNIQUE INDEX IF NOT EXISTS uniq_decision_effect_executions_decision_index
    ON decision_effect_executions(decision_id, effect_index);

CREATE INDEX IF NOT EXISTS idx_decision_effect_executions_company
    ON decision_effect_executions(company_id);

CREATE INDEX IF NOT EXISTS idx_decision_effect_executions_target_issue
    ON decision_effect_executions(target_issue_id);

-- CREATE UNIQUE INDEX IF NOT EXISTS uniq_decision_queues_id_company
--     ON decision_queues(id, company_id); -- Already defined as table constraint

CREATE INDEX IF NOT EXISTS idx_decision_queues_company
    ON decision_queues(company_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_decision_queue_items_company_source
    ON decision_queue_items(company_id, source_kind, source_id);

CREATE INDEX IF NOT EXISTS idx_decision_queue_items_queue
    ON decision_queue_items(queue_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_decision_triage_company_snoozed
    ON decision_triage(company_id, snoozed_until);

CREATE INDEX IF NOT EXISTS idx_decision_triage_company_decide_by
    ON decision_triage(company_id, decide_by, decide_by_date);

CREATE INDEX IF NOT EXISTS idx_decision_triage_events_company_created
    ON decision_triage_events(company_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_decision_triage_events_source
    ON decision_triage_events(company_id, source_kind, source_id);

CREATE INDEX IF NOT EXISTS idx_decision_triage_events_queue
    ON decision_triage_events(queue_id);

CREATE INDEX IF NOT EXISTS idx_decision_retention_company_archived
    ON decision_retention(company_id, archived_at);

CREATE INDEX IF NOT EXISTS idx_decision_retention_company_activity
    ON decision_retention(company_id, source_activity_at DESC);

CREATE INDEX IF NOT EXISTS idx_decision_archive_outbox_status
    ON decision_archive_notification_outbox(status, created_at);

CREATE INDEX IF NOT EXISTS idx_decision_archive_outbox_company
    ON decision_archive_notification_outbox(company_id);

CREATE INDEX IF NOT EXISTS idx_decision_training_examples_company_created
    ON decision_training_examples(company_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_decision_training_examples_issue
    ON decision_training_examples(issue_id);

CREATE INDEX IF NOT EXISTS idx_decision_training_examples_author
    ON decision_training_examples(company_id, created_by_user_id);

CREATE INDEX IF NOT EXISTS idx_status_cards_company ON status_cards(company_id);

CREATE INDEX IF NOT EXISTS idx_status_card_updates_card ON status_card_updates(card_id);

CREATE INDEX IF NOT EXISTS idx_status_card_summary_revisions_card ON status_card_summary_revisions(card_id);

CREATE INDEX IF NOT EXISTS idx_summary_slots_company ON summary_slots(company_id, scope_kind, scope_id);

CREATE INDEX IF NOT EXISTS idx_smoke_runs_company ON smoke_runs(company_id, started_at);

CREATE INDEX IF NOT EXISTS idx_smoke_run_steps_company_run ON smoke_run_steps(company_id, run_id);

CREATE INDEX IF NOT EXISTS idx_tool_applications_company ON tool_applications(company_id, status);

CREATE INDEX IF NOT EXISTS idx_tool_connection_grants_company ON tool_connection_grants(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_agent_api_keys_company ON agent_api_keys(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_agent_config_revisions_company ON agent_config_revisions(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_cost_events_company ON cost_events(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_document_revisions_company ON document_revisions(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_issue_approvals_company ON issue_approvals(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_approval_comments_company ON approval_comments(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_project_workspaces_company ON project_workspaces(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_pipeline_case_events_company ON pipeline_case_events(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_plugin_job_runs_company ON plugin_job_runs(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_plugin_logs_company ON plugin_logs(company_id);

CREATE INDEX IF NOT EXISTS idx_invites_company_id ON invites(company_id);

CREATE INDEX IF NOT EXISTS idx_inbox_dismissals_company_user ON inbox_dismissals(company_id, user_id);

CREATE INDEX IF NOT EXISTS idx_company_user_sidebar_company
    ON company_user_sidebar_preferences(company_id);

CREATE INDEX IF NOT EXISTS company_skill_policies_company_idx
    ON company_skill_policies(company_id);

CREATE INDEX IF NOT EXISTS idx_folders_company_kind ON folders(company_id, kind);

CREATE INDEX IF NOT EXISTS idx_folder_items_item ON folder_items(item_kind, item_id);

CREATE INDEX IF NOT EXISTS idx_company_team_installs_company
    ON company_team_installs (company_id);

CREATE INDEX IF NOT EXISTS idx_secret_proposals_company_status
    ON company_secret_proposals(company_id, status);

CREATE INDEX IF NOT EXISTS idx_secret_proposals_proposer_status
    ON company_secret_proposals(proposed_by_agent_id, status);

CREATE INDEX IF NOT EXISTS idx_company_secret_provider_configs_company_id ON company_secret_provider_configs(company_id);

CREATE INDEX IF NOT EXISTS idx_decision_bundles_company_id ON decision_bundles(company_id);

CREATE INDEX IF NOT EXISTS idx_decision_queues_company_id ON decision_queues(company_id);

CREATE INDEX IF NOT EXISTS idx_decision_training_examples_company_id ON decision_training_examples(company_id);

CREATE INDEX IF NOT EXISTS idx_environment_leases_company_id ON environment_leases(company_id);

CREATE INDEX IF NOT EXISTS idx_execution_workspaces_company_id ON execution_workspaces(company_id);

CREATE INDEX IF NOT EXISTS idx_heartbeat_run_watchdog_decisions_company_id ON heartbeat_run_watchdog_decisions(company_id);

CREATE INDEX IF NOT EXISTS idx_heartbeat_runs_company_id ON heartbeat_runs(company_id);

CREATE INDEX IF NOT EXISTS idx_issue_plan_decompositions_company_id ON issue_plan_decompositions(company_id);

CREATE INDEX IF NOT EXISTS idx_issue_relations_company_id ON issue_relations(company_id);

CREATE INDEX IF NOT EXISTS idx_secret_access_events_company_id ON secret_access_events(company_id);

CREATE INDEX IF NOT EXISTS idx_user_secret_declarations_company_id ON user_secret_declarations(company_id);

CREATE INDEX IF NOT EXISTS idx_user_secret_definitions_company_id ON user_secret_definitions(company_id);

-- [REMOVED] CREATE INDEX IF NOT EXISTS idx_status_cards_next_eval
-- [REMOVED]     ON status_cards(archived_at, generating_issue_id, next_eval_at);

CREATE INDEX IF NOT EXISTS idx_status_card_update_runs_card
    ON status_card_update_runs(card_id, started_at);

CREATE INDEX IF NOT EXISTS idx_status_card_update_runs_gen_issue
    ON status_card_update_runs(generation_issue_id);

CREATE INDEX IF NOT EXISTS idx_instruction_templates_name 
    ON instruction_templates(name);

CREATE INDEX IF NOT EXISTS idx_instruction_templates_created_at 
    ON instruction_templates(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_instruction_templates_version 
    ON instruction_templates(version);

-- ============================================
-- 4. 创建触发器和函数
-- ============================================

CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Apply updated_at trigger to companies

CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Add updated_at triggers

-- ============================================
-- 5. ALTER 语句
-- ============================================

ALTER TABLE company_secret_versions ALTER COLUMN fingerprint_sha256 SET NOT NULL;

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS company_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS issue_id UUID;

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS project_id UUID;

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS goal_id UUID;

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS heartbeat_run_id UUID;

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS billing_code TEXT;

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS provider TEXT NOT NULL DEFAULT 'unknown';

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS biller TEXT NOT NULL DEFAULT 'unknown';

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS billing_type TEXT NOT NULL DEFAULT 'usage';

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS model TEXT NOT NULL DEFAULT 'unknown';

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS input_tokens INTEGER NOT NULL DEFAULT 0;

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS cached_input_tokens INTEGER NOT NULL DEFAULT 0;

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS output_tokens INTEGER NOT NULL DEFAULT 0;

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS cost_cents INTEGER NOT NULL DEFAULT 0;

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

ALTER TABLE agents ADD COLUMN IF NOT EXISTS spent_monthly_cents BIGINT NOT NULL DEFAULT 0;

ALTER TABLE cloud_upstream_connections ADD COLUMN IF NOT EXISTS target_schema_major INTEGER;

ALTER TABLE plugins ADD COLUMN IF NOT EXISTS api_version INTEGER NOT NULL DEFAULT 1;

ALTER TABLE plugins ADD COLUMN IF NOT EXISTS categories JSONB NOT NULL DEFAULT '[]';

ALTER TABLE plugins ADD COLUMN IF NOT EXISTS install_order INTEGER NOT NULL DEFAULT 0;

ALTER TABLE plugins ADD COLUMN IF NOT EXISTS config JSONB NOT NULL DEFAULT '{}';

ALTER TABLE IF EXISTS saga_instances ADD COLUMN IF NOT EXISTS initiator_id UUID;

ALTER TABLE agent_api_keys ALTER COLUMN name SET DEFAULT 'Agent key';

ALTER TABLE agent_api_keys ALTER COLUMN name SET NOT NULL;

ALTER TABLE routines ADD COLUMN IF NOT EXISTS folder_id UUID REFERENCES folders(id) ON DELETE SET NULL;

ALTER TABLE company_skills ADD COLUMN IF NOT EXISTS folder_id UUID REFERENCES folders(id) ON DELETE SET NULL;

ALTER TABLE issue_thread_interactions DROP CONSTRAINT IF EXISTS valid_interaction_kind;

ALTER TABLE agent_api_keys ADD COLUMN IF NOT EXISTS company_id UUID;

ALTER TABLE agent_api_keys ALTER COLUMN company_id SET NOT NULL;

ALTER TABLE agent_config_revisions ADD COLUMN IF NOT EXISTS company_id UUID;

ALTER TABLE agent_config_revisions ALTER COLUMN company_id SET NOT NULL;

ALTER TABLE cost_events ADD COLUMN IF NOT EXISTS company_id UUID;

ALTER TABLE cost_events ALTER COLUMN company_id SET NOT NULL;

ALTER TABLE document_revisions ADD COLUMN IF NOT EXISTS company_id UUID;

ALTER TABLE document_revisions ALTER COLUMN company_id SET NOT NULL;

ALTER TABLE issue_approvals ADD COLUMN IF NOT EXISTS company_id UUID;

ALTER TABLE issue_approvals ALTER COLUMN company_id SET NOT NULL;

ALTER TABLE approval_comments ADD COLUMN IF NOT EXISTS company_id UUID;

ALTER TABLE approval_comments ALTER COLUMN company_id SET NOT NULL;

ALTER TABLE project_workspaces ADD COLUMN IF NOT EXISTS company_id UUID;

ALTER TABLE project_workspaces ALTER COLUMN company_id SET NOT NULL;

ALTER TABLE pipeline_case_events ADD COLUMN IF NOT EXISTS company_id UUID;

ALTER TABLE pipeline_case_events ALTER COLUMN company_id SET NOT NULL;

ALTER TABLE plugin_job_runs ADD COLUMN IF NOT EXISTS company_id UUID;

ALTER TABLE plugin_logs ADD COLUMN IF NOT EXISTS company_id UUID;

ALTER TABLE invites ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ;




























-- ============================================
-- 6. 注释
-- ============================================

COMMENT ON TABLE project_goals IS 'Many-to-many relationship between projects and goals';

COMMENT ON TABLE plugin_managed_resources IS 'Tracks resources managed by plugins (e.g., GitHub issues, Linear projects)';

COMMENT ON COLUMN plugin_managed_resources.plugin_key IS 'Plugin identifier for validation';

COMMENT ON COLUMN plugin_managed_resources.resource_kind IS 'Type of resource: project, issue, agent, etc.';

COMMENT ON COLUMN plugin_managed_resources.resource_key IS 'Plugin-specific key (e.g., GitHub repo name)';

COMMENT ON COLUMN plugin_managed_resources.resource_id IS 'UUID of the actual resource in our database';

COMMENT ON COLUMN plugin_managed_resources.defaults_json IS 'Plugin-specific configuration and metadata';

COMMENT ON TABLE project_memberships IS 'User membership state and starred status for projects';

COMMENT ON COLUMN project_memberships.state IS 'joined = user is a member, left = user has left';

COMMENT ON COLUMN project_memberships.starred_at IS 'Non-NULL if user has starred this project';

COMMENT ON COLUMN project_memberships.user_id IS 'Board user ID (external, not a FK)';

COMMENT ON TABLE agent_memberships IS 'User membership state and starred status for agents';

COMMENT ON COLUMN agent_memberships.state IS 'joined = user is a member, left = user has left';

COMMENT ON COLUMN agent_memberships.starred_at IS 'Non-NULL if user has starred this agent';

COMMENT ON COLUMN agent_memberships.user_id IS 'Board user ID (external, not a FK)';

-- ============================================
-- Foreign Key Constraints (added after all tables are created)
-- ============================================

-- Add foreign keys for activity_logs.run_id and activity_logs.agent_id
-- These are added here because heartbeat_runs and agents tables are created after activity_logs
DO $$ 
BEGIN
    -- Add run_id foreign key if not exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'activity_logs_run_id_fkey' 
        AND table_name = 'activity_logs'
    ) THEN
        ALTER TABLE activity_logs 
        ADD CONSTRAINT activity_logs_run_id_fkey 
        FOREIGN KEY (run_id) REFERENCES heartbeat_runs(id) ON DELETE CASCADE;
    END IF;
    
    -- Add agent_id foreign key if not exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'activity_logs_agent_id_fkey' 
        AND table_name = 'activity_logs'
    ) THEN
        ALTER TABLE activity_logs 
        ADD CONSTRAINT activity_logs_agent_id_fkey 
        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE SET NULL;
    END IF;
END $$;

-- ============================================
-- 7. 性能优化和Bug修复索引
-- ============================================
-- 添加时间: 2026-08-16
-- 修复问题:
--   1. Agent重复启动问题 (唯一索引)
--   2. 慢SQL查询优化 (性能索引)

-- 修复1: 防止Agent在同一issue上创建多个active run
-- 使用部分唯一索引，只对queued/running状态生效
CREATE UNIQUE INDEX IF NOT EXISTS idx_heartbeat_runs_unique_active_agent_issue
ON heartbeat_runs (
    agent_id, 
    company_id,
    COALESCE(context_snapshot->>'issueId', context_snapshot->>'taskId')
)
WHERE status IN ('queued', 'running')
  AND (context_snapshot->>'issueId' IS NOT NULL OR context_snapshot->>'taskId' IS NOT NULL);

COMMENT ON INDEX idx_heartbeat_runs_unique_active_agent_issue IS 
    '防止同一agent在同一issue上创建多个active run - 修复重复启动问题';

-- 修复2: 优化慢SQL查询性能

-- issues表索引 (优化assignee查询和状态过滤)
CREATE INDEX IF NOT EXISTS idx_issues_assignee_agent_id 
    ON issues(assignee_agent_id) 
    WHERE assignee_agent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_issues_status 
    ON issues(status);

CREATE INDEX IF NOT EXISTS idx_issues_company_status 
    ON issues(company_id, status);

-- approvals表索引 (优化审批查询)
CREATE INDEX IF NOT EXISTS idx_approvals_status 
    ON approvals(status);

CREATE INDEX IF NOT EXISTS idx_approvals_company_status 
    ON approvals(company_id, status);

-- project_memberships表索引 (优化项目成员查询)
CREATE INDEX IF NOT EXISTS idx_project_memberships_project_id 
    ON project_memberships(project_id);

CREATE INDEX IF NOT EXISTS idx_project_memberships_user_id 
    ON project_memberships(user_id);

-- agent_memberships表索引 (优化agent成员查询)
CREATE INDEX IF NOT EXISTS idx_agent_memberships_agent_id 
    ON agent_memberships(agent_id);

CREATE INDEX IF NOT EXISTS idx_agent_memberships_company_id 
    ON agent_memberships(company_id);

-- heartbeat_runs表索引 (优化wakeup查询性能)
CREATE INDEX IF NOT EXISTS idx_heartbeat_runs_agent_status 
    ON heartbeat_runs(agent_id, status, created_at DESC) 
    WHERE status IN ('queued', 'running');

-- 添加索引说明
COMMENT ON INDEX idx_issues_assignee_agent_id IS '优化按agent查询issues';
COMMENT ON INDEX idx_issues_company_status IS '优化按公司和状态查询issues';
COMMENT ON INDEX idx_approvals_company_status IS '优化按公司和状态查询approvals - 解决1.06秒慢查询';
COMMENT ON INDEX idx_project_memberships_project_id IS '优化项目成员查询 - 解决1.47秒慢查询';
COMMENT ON INDEX idx_agent_memberships_agent_id IS '优化agent成员查询 - 解决1.16秒慢查询';
COMMENT ON INDEX idx_heartbeat_runs_agent_status IS '优化wakeup查询 - 提升agent启动性能';
