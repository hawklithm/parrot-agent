-- Migration: Add Decision System
-- Description: Adds 13 tables for CEO decision-making, queuing, triage, and training
-- Date: 2026-08-18
-- Tables: Core decisions (4), Queue management (6), Training & monitoring (3)

-- ============================================================================
-- Core Decision Tables
-- ============================================================================

-- Decision bundles (grouped decisions)
CREATE TABLE decision_bundles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    origin_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    origin_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    origin_run_id UUID NOT NULL REFERENCES heartbeat_runs(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX decision_bundles_company_created_at_idx ON decision_bundles(company_id, created_at);

-- Decisions (individual decision records)
CREATE TABLE decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    bundle_id UUID REFERENCES decision_bundles(id) ON DELETE SET NULL,
    origin_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    origin_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    origin_run_id UUID NOT NULL REFERENCES heartbeat_runs(id) ON DELETE CASCADE,
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
    signed_spec TEXT NOT NULL,
    target_snapshots JSONB NOT NULL,
    continuation_policy TEXT NOT NULL DEFAULT 'none',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT decisions_status_check CHECK (status IN ('open', 'decided', 'expired', 'cancelled')),
    CONSTRAINT decisions_execution_status_check CHECK (execution_status IS NULL OR execution_status IN ('pending', 'executing', 'completed', 'failed')),
    CONSTRAINT decisions_continuation_policy_check CHECK (continuation_policy IN ('none', 'auto_continue', 'require_approval'))
);

CREATE INDEX decisions_company_status_expires_at_idx ON decisions(company_id, status, expires_at);
CREATE INDEX decisions_bundle_idx ON decisions(bundle_id);
CREATE INDEX decisions_origin_issue_idx ON decisions(origin_issue_id);
CREATE UNIQUE INDEX decisions_company_idempotency_uq ON decisions(company_id, idempotency_key) WHERE idempotency_key IS NOT NULL;

-- Decision target issues (which issues are affected)
CREATE TABLE decision_target_issues (
    decision_id UUID NOT NULL REFERENCES decisions(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    
   KEY (decision_id, issue_id)
);

CREATE INDEX decision_target_issues_decision_idx ON decision_target_issues(decision_id);
CREATE INDEX decision_target_issues_issue_idx ON decision_target_issues(issue_id);

-- Decision effect executions (tracking decision outcomes)
CREATE TABLE decision_effect_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    decision_id UUID NOT NULL REFERENCES decisions(id) ON DELETE CASCADE,
    effect_index INTEGER NOT NULL,
    effect_type TEXT NOT NULL,
    target_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    status TEXT NOT N 'claimed',
    result JSONB,
    error TEXT,
    activity_log_id UUID REFERENCES activity_log(id) ON DELETE SET NULL,
    executed_at TIMESTAMPTZ,
    
    CONSTRAINT decision_effect_executions_status_check CHECK (status IN ('claimed', 'executing', 'succeeded', 'failed'))
);

CREATE UNIQUE INDEX decision_effect_executions_decision_effect_uq ON decision_effect_executions(decision_id, effect_index);
CREATE INDEX decision_effect_executions_target_issue_idx ON decision_effect_executions(target_issue_id);

-- ============================================================================
-- Queue ManTables
-- ============================================================================

-- Decision queues (organized decision lists)
CREATE TABLE decision_queues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    created_by_type TEXT NOT NULL,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_by_run_id UUID REFERENCES heartbeat_runs(TE SET NULL,
    created_by_agent_api_key_id UUID REFERENCES agent_api_keys(id) ON DELETE SET NULL,
    retention_days INTEGER,
    seed_rules JSONB NOT NULL DEFAULT '[]',
    seed_rules_enabled BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT decision_queues_created_by_type_check CHECK (created_by_type IN ('agent', 'user', 'system')),
    CONSTRAINT decision_queues_creator_check CHECK (
        (created_by_type = 'agent' AND created_by_agent_id IS NOT NULL AND created_by_user_id IS NULL)
        OR (created_by_type = 'user' AND created_by_agent_id IS NULL AND created_by_user_id IS NOT NULL)
        OR (created_by_type = 'system' AND created_by_agent_id IS NULL AND created_by_user_id IS NULL)
    ),
    CONSTRAINT decision_queues_retention_days_check CHECK (retention_days IS NULL OR (retention_days >= 1 AND retention_days <= 3650))
);

CREATE UNIQUE INDEX decision_queues_company_key_uq ON decision_queues(company_id, key);
CREATE INDEX decision_queues_company_updated_idx ON decision_queues(company_id, updated_at);
CREATE UNIQUE INDEX decision_queues_id_company_uq ON decision_queues(id, company_id);

-- Decision queue items (items in queues)
CREATE TABLE decision_queue_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    queue_id UUID NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    added_by_type TEXT NOT NULL,
    added_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    added_by_user_id TEXT,
    added_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    added_by_agent_api_key_id UUID REFERENCES agent_api_keys(id) ON DELETE SET NULL,
    responsible_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT decision_queue_items_added_by_type_check CHECK (added_by_type IN ('agent', 'user', 'system')),
    CONSTRAINT decision_queue_items_actor_check CHECK (
        (added_by_type = 'agent' AND added_by_agent_id IS NOT NULL AND added_by_user_id IS NULL)
        OR (added_by_type = 'user' AND added_by_agent_id IS NULL AND added_by_user_id IS NOT NULL)
        OR (added_by_type = 'system' AND added_by_agent_id IS NULL AND added_by_user_id IS NULL)
    ),
    CONSTRAINT decision_queue_items_queue_company_fk FOREIGN KEY (queue_id, company_id) REFERENCES decision_queues(id, company_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX decision_queue_items_queue_source_uq ON decision_queue_items(queue_id, source_kind, source_id);
CREATE INDEX decision_queue_items_company_source_idx ON decision_queue_items(company_id, source_kind, source_id);

-- Decision triage (priority and timing for decisions)
CREATE TABLE decision_triage (
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
    set_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    set_by_agent_api_key_id UUID REFERENCES agent_api_keys(id) ON DELETE SET NULL,
    responsible_user_id TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT decision_triage_set_by_type_check CHECK (set_by_type IN ('agent', 'user')),
    CONSTRAINT decision_triage_actor_check CHECK (
        (set_by_type = 'agent' AND set_by_agent_id IS NOT NULL AND set_by_user_id IS NULL)
        OR (set_by_type = 'user' AND set_by_agent_id IS NULL AND set_by_user_id IS NOT NULL)
    ),
    CONSTRAINT decision_triage_decide_by_check CHECK (
        (decide_by IS NULL AND decide_by_date IS NULL)
        OR (decide_by IN ('today', 'this_week', 'whenever') AND decide_by_date IS NULL)
        OR (decide_by = 'date' AND decide_by_date IS NOT NULL)
    )
);

CREATE UNIQUE INDEX decision_triage_company_source_uq ON decision_triage(company_id, source_kind, source_id);
CREATE INDEX decision_triage_company_decide_by_idx ON decision_triage(company_id, decide_by);

-- Decision triage events (history of triage changes)
CREATE TABLE decision_triage_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    queue_id UUID REFERENCES decision_queues(id) ON DELETE SET NULL,
    source_kind TEXT,
    source_id TEXT,
    action TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    actor_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    actor_user_id TEXT,
    actor_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    agent_api_key_id UUID REFERENCES agent_api_keys(id) ON DELETE SET NULL,
    responsible_user_id TEXT,
    details JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT decision_triage_events_actor_type_check CHECK (actor_type IN ('agent', 'user', 'system')),
    CONSTRAINT decision_triage_events_actor_check CHECK (
        (actor_type = 'agent' AND actor_agent_id IS NOT NULL AND actor_user_id IS NULL)
        OR (actor_type = 'user' AND actor_agent_id IS NULL AND actor_user_id IS NOT NULL)
        OR (actor_type = 'system' AND actor_agent_id IS NULL AND actor_user_id IS NULL)
    )
);

CREATE INDEX decision_triage_events_company_source_created_idx ON decision_triage_events(company_id, source_kind, source_id, created_at);
CREATE INDEX decision_triage_events_queue_created_idx ON decision_triage_events(queue_id, created_at);

-- Decision retention (archival policies)
CREATE TABLE decision_retention (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_activity_at TIMESTAMPTZ NOT NULL,
    keep BOOLEAN NOT NULL DEFAULT false,
    archived_at TIMESTAMPTZ,
    archived_reason TEXT,
    archived_by_type TEXT,
    archived_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    archived_by_user_id TEXT,
    archived_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    version INTEGER NOT NULL DEFAULT 1,
    archive_version INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT decision_retention_archived_by_type_check CHECK (archived_by_type IS NULL OR archived_by_type IN ('system', 'agent', 'user')),
    CONSTRAINT decision_retention_archive_actor_check CHECK (
        (archived_at IS NULL AND archived_by_type IS NULL AND archived_by_agent_id IS NULL AND archived_by_user_id IS NULL)
        OR (archived_at IS NOT NULL AND archived_by_type = 'system' AND archived_by_agent_id IS NULL AND archived_by_user_id IS NULL)
        OR (archived_at IS NOT NULL AND archived_by_type = 'agent' AND archived_by_agent_id IS NOT NULL AND archived_by_user_id IS NULL)
        OR (archived_at IS NOT NULL AND archived_by_type = 'user' AND archived_by_agent_id IS NULL AND archived_by_user_id IS NOT NULL)
    )
);

CREATE UNIQUE INDEX decision_retention_company_source_uq ON decision_retention(company_id, source_kind, source_id);
CREATE INDEX decision_retention_company_archived_idx ON decision_retention(company_id, archived_at);

-- Decision archive notification outbox (async notifications)
CREATE TABLE decision_archive_notification_outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    archive_version INTEGER NOT NULL,
    origin_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    origin_issue_id UUID NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_attempt_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT decision_archive_notification_outbox_status_check CHECK (status IN ('pending', 'delivering', 'delivered'))
);

CREATE UNIQUE INDEX decision_archive_notification_outbox_uq ON decision_archive_notification_outbox(company_id, source_kind, source_id, archive_version, origin_agent_id);
CREATE INDEX decision_archive_notification_outbox_pending_idx ON decision_archive_notification_outbox(status, created_at);

-- ============================================================================
-- Training and Monitoring Tables
-- ============================================================================

-- Decision training examples (CEO learning data)
CREATE TABLE decision_training_examples (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_id UUID NOT NULL,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    cutoff_at TIMESTAMPTZ NOT NULL,
    notes TEXT NOT NULL DEFAULT '',
    notes_history JSONB NOT NULL DEFAULT '[]',
    decision_outcome TEXT,
    retention_policy TEXT NOT NULL DEFAULT 'scrub_deleted_comments_v1',
    snapshot JSONB NOT NULL,
    created_by_user_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT decision_training_examples_source_kind_check CHECK (source_kind IN ('decision', 'approval', 'manual')),
    CONSTRAINT decision_training_examples_retention_policy_check CHECK (retention_policy IN ('scrub_deleted_comments_v1', 'keep_all', 'anonymize'))
);

CREATE INDEX decision_training_examples_company_created_at_idx ON decision_training_examples(company_id, created_at);
CREATE INDEX decision_training_examples_issue_idx ON decision_training_examples(issue_id);
CREATE UNIQUE INDEX decision_training_examples_source_author_uq ON decision_training_examples(source_kind, source_id, created_by_user_id);

-- Heartbeat run watchdog decisions (monitoring agent runs)
CREATE TABLE heartbeat_run_watchdog_decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES heartbeat_runs(id) ON DELETE CASCADE,
    evaluation_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    decision TEXT NOT NULL,
    snoozed_until TIMESTAMPTZ,
    reason TEXT,
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    created_by_user_id TEXT,
    created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT heartbeat_run_watchdog_decisions_decision_check CHECK (decision IN ('continue', 'pause', 'stop', 'escalate', 'snooze'))
);

CREATE INDEX heartbeat_run_watchdog_decisions_company_run_created_idx ON heartbeat_run_watchdog_decisions(company_id, run_id, created_at);
CREATE INDEX heartbeat_run_watchdog_decisions_company_run_snooze_idx ON heartbeat_run_watchdog_decisions(company_id, run_id, snoozed_until);

-- Issue execution decisions (execution stage decisions)
CREATE TABLE issue_execution_decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    stage_id UUID NOT NULL,
    stage_type TEXT NOT NULL,
    actor_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    actor_user_id TEXT,
    outcome TEXT NOT NULL,
    body TEXT NOT NULL,
    created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    CONSTRAINT issue_execution_decisions_stage_type_check CHECK (stage_type IN ('planning', 'execution', 'review', 'approval')),
    CONSTRAINT issue_execution_decisions_outcome_check CHECK (outcome IN ('approved', 'rejected', 'deferred', 'escalated', 'cancelled'))
);

CREATE INDEX issue_execution_decisions_company_issue_idx ON issue_execution_decisions(company_id, issue_id);
CREATE INDEX issue_execution_decisions_stage_idx ON issue_execution_decisions(issue_id, stage_id, created_at);

-- ============================================================================
-- Triggers
-- ============================================================================

CREATE TRIGGER update_decisions_updated_at BEFORE UPDATE ON decisions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_decision_queues_updated_at BEFORE UPDATE ON decision_queues
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_decision_triage_updated_at BEFORE UPDATE ON decision_triage
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_decision_retention_updated_at BEFORE UPDATE ON decision_retention
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_decision_archive_notification_outbox_updated_at BEFORE UPDATE ON decision_archive_notification_outbox
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_decision_training_examples_updated_at BEFORE UPDATE ON decision_training_examples
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_issue_execution_decisions_updated_at BEFORE UPDATE ON issue_execution_decisions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON TABLE decision_bundles IS 'Grouped decisions from a single agent run';
COMMENT ON TABLE decisions IS 'Individual CEO decisions with options and execution tracking';
COMMENT ON TABLE decision_target_issues IS 'Join table: which issues are affected by a decision';
COMMENT ON TABLE decision_effect_executions IS 'Execution tracking for decision effects';

COMMENT ON TABLE decision_queues IS 'Organized decision lists with retention policies';
COMMENT ON TABLE decision_queue_items IS 'Items in decision queues';
COMMENT ON TABLE decision_triage IS 'Decision priority and timing metadata';
COMMENT ON TABLE decision_triage_events IS 'Audit log of triage changes';
COMMENT ON TABLE decision_retention IS 'Archival policy and status for decisions';
COMMENT ON TABLE decision_archive_notification_outbox IS 'Async notification queue for archived decisions';

COMMENT ON TABLE decision_training_examples IS 'Training data for CEO decision-making model';
COMMENT ON TABLE heartbeat_run_watchdog_decisions IS 'Monitoring decisions for agent runs';
COMMENT ON TABLE issue_execution_decisions IS 'Decisions made during issue execution stages';

COMMENT ON COLUMN decisions.signed_spec IS 'Cryptographic signature of decision specification';
COMMENT ON COLUMN decisions.continuation_policy IS 'How to handle decision chaining';
COMMENT ON COLUMN decision_queues.seed_rules IS 'Auto-population rules for queue';
COMMENT ON COLUMN decision_training_examples.snapshot IS 'Immutable snapshot of decision context';
COMMENT ON COLUMN decision_retention.keep IS 'Override retention policy to preserve forever';
