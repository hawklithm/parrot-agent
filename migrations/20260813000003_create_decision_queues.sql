-- P0.1 Decision domain: queues, queue items, triage, triage events, retention, archive outbox
-- Ported from Paperclip packages/db/src/schema/decision_queues.ts

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
);

-- Composite unique required by the (queue_id, company_id) composite FK on items
CREATE UNIQUE INDEX IF NOT EXISTS uniq_decision_queues_id_company
    ON decision_queues(id, company_id);
CREATE INDEX IF NOT EXISTS idx_decision_queues_company
    ON decision_queues(company_id, created_at DESC);

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

CREATE INDEX IF NOT EXISTS idx_decision_queue_items_company_source
    ON decision_queue_items(company_id, source_kind, source_id);
CREATE INDEX IF NOT EXISTS idx_decision_queue_items_queue
    ON decision_queue_items(queue_id, created_at DESC);

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

CREATE INDEX IF NOT EXISTS idx_decision_triage_company_snoozed
    ON decision_triage(company_id, snoozed_until);
CREATE INDEX IF NOT EXISTS idx_decision_triage_company_decide_by
    ON decision_triage(company_id, decide_by, decide_by_date);

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

CREATE INDEX IF NOT EXISTS idx_decision_triage_events_company_created
    ON decision_triage_events(company_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_decision_triage_events_source
    ON decision_triage_events(company_id, source_kind, source_id);
CREATE INDEX IF NOT EXISTS idx_decision_triage_events_queue
    ON decision_triage_events(queue_id);

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

CREATE INDEX IF NOT EXISTS idx_decision_retention_company_archived
    ON decision_retention(company_id, archived_at);
CREATE INDEX IF NOT EXISTS idx_decision_retention_company_activity
    ON decision_retention(company_id, source_activity_at DESC);

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

CREATE INDEX IF NOT EXISTS idx_decision_archive_outbox_status
    ON decision_archive_notification_outbox(status, created_at);
CREATE INDEX IF NOT EXISTS idx_decision_archive_outbox_company
    ON decision_archive_notification_outbox(company_id);
