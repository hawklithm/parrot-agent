-- Persistent task-driven team orchestration.
--
-- Every root issue that is routed to a CEO gets an orchestration instance: a
-- resumable, versioned workflow that records the current stage, the task-team
-- plan (reused/hired/exited members plus task-scope reporting edges), pending
-- clarification questions, required subtasks, deliverable versions and
-- acceptance decisions, and a durable outbox of side effects that must survive
-- a crash. All identifiers are company-scoped UUIDs consistent with the rest
-- of the schema.
--
-- Re-runnable against databases where an earlier draft of this migration was
-- applied: every DDL statement is `IF NOT EXISTS` / existence-guarded, and the
-- hardening steps (composite foreign keys, non-NULL idempotency keys, unique
-- deliverable decisions, single approved plan) are added in `DO` blocks that
-- no-op when already present.

CREATE TABLE IF NOT EXISTS task_orchestrations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    root_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    -- The CEO agent that owns this task's orchestration (role='ceo').
    ceo_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
    stage TEXT NOT NULL DEFAULT 'assessing'
        CHECK (stage IN (
            'assessing', 'team_planning', 'awaiting_team_approval', 'clarifying',
            'planning', 'executing', 'reviewing', 'reporting',
            'completed', 'failed', 'cancelled'
        )),
    -- Monotonically increasing workflow version; a stage change that does not
    -- advance the version is a no-op replay.
    stage_version BIGINT NOT NULL DEFAULT 0,
    -- Snapshot the stage to return to once a clarification batch resolves or a
    -- failure is retried.
    recovery_stage TEXT
        CHECK (recovery_stage IS NULL OR recovery_stage IN (
            'assessing', 'team_planning', 'awaiting_team_approval', 'clarifying',
            'planning', 'executing', 'reviewing', 'reporting'
        )),
    -- Latest submitted CEO decision contract.
    decision_contract JSONB NOT NULL DEFAULT '{}'::jsonb,
    decision_contract_version INT NOT NULL DEFAULT 0,
    -- Failure / cancellation diagnostics surfaced to the user.
    final_report TEXT,
    error_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    UNIQUE (company_id, root_issue_id)
);

CREATE INDEX IF NOT EXISTS task_orchestrations_company_stage_idx
    ON task_orchestrations(company_id, stage);
CREATE INDEX IF NOT EXISTS task_orchestrations_root_issue_idx
    ON task_orchestrations(root_issue_id);

-- Cross-company reference guard. `root_issue_id` and `ceo_agent_id` are plain
-- single-column FKs, so PostgreSQL alone cannot stop a row from pointing at an
-- issue or agent owned by a *different* company. The fix is the
-- database-native option: a composite FK. `UNIQUE (id, company_id)` is added
-- to `issues` and `agents` (both are multi-tenant rows keyed by UUID; a row's
-- `company_id` is immutable in practice). The composite FK then carries the
-- existing delete semantics exactly: issue CASCADE, CEO agent RESTRICT.
DO $$
DECLARE
    composite_fk_count BIGINT;
BEGIN
    CREATE UNIQUE INDEX IF NOT EXISTS issues_id_company_id_idx
        ON issues (id, company_id);
    CREATE UNIQUE INDEX IF NOT EXISTS agents_id_company_id_idx
        ON agents (id, company_id);

    SELECT COUNT(*)
      INTO composite_fk_count
      FROM pg_constraint
     WHERE conrelid = 'task_orchestrations'::regclass
       AND contype = 'f'
       AND confrelid IN ('issues'::regclass, 'agents'::regclass)
       AND array_length(conkey, 1) = 2;

    IF composite_fk_count = 0 THEN
        ALTER TABLE task_orchestrations
            DROP CONSTRAINT IF EXISTS task_orchestrations_root_issue_id_fkey,
            DROP CONSTRAINT IF EXISTS task_orchestrations_ceo_agent_id_fkey;

        ALTER TABLE task_orchestrations
            ADD CONSTRAINT fk_task_orchestrations_root_issue
                FOREIGN KEY (root_issue_id, company_id)
                REFERENCES issues (id, company_id) ON DELETE CASCADE,
            ADD CONSTRAINT fk_task_orchestrations_ceo_agent
                FOREIGN KEY (ceo_agent_id, company_id)
                REFERENCES agents (id, company_id) ON DELETE RESTRICT;
    END IF;
END
$$;

-- Hardening for databases where an earlier draft of this migration already
-- installed the single-column FKs and the trigger-based check: the composite
-- FKs above are authoritative, so the trigger and its function go away.
DROP TRIGGER IF EXISTS task_orchestrations_same_company ON task_orchestrations;
DROP FUNCTION IF EXISTS task_orchestrations_same_company_check();

-- Versioned task-team plan. Each plan revision captures the members that were
-- considered (reused, hired, exited) and the task-scope reporting edges. Old
-- revisions and exit timestamps stay readable forever.
CREATE TABLE IF NOT EXISTS task_team_plans (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    orchestration_id UUID NOT NULL REFERENCES task_orchestrations(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    revision INT NOT NULL,
    status TEXT NOT NULL DEFAULT 'proposed'
        CHECK (status IN ('proposed', 'approved', 'rejected', 'superseded')),
    -- The agent that authored the plan (the CEO).
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    proposed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    decided_at TIMESTAMPTZ,
    decision_note TEXT,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (orchestration_id, revision)
);

CREATE INDEX IF NOT EXISTS task_team_plans_company_idx ON task_team_plans(company_id);
-- Only one plan revision may be approved at a time; a later approval supersedes
-- the previous row first (the service does that transactionally, the index is
-- the crash-safe backstop).
CREATE UNIQUE INDEX IF NOT EXISTS task_team_plans_approved_uq
    ON task_team_plans(orchestration_id) WHERE status = 'approved';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'task_team_plans'::regclass
           AND conname = 'fk_task_team_plans_author'
    ) THEN
        ALTER TABLE task_team_plans
            ADD CONSTRAINT fk_task_team_plans_author
                FOREIGN KEY (created_by_agent_id, company_id)
                REFERENCES agents (id, company_id) ON DELETE SET NULL;
    END IF;
END
$$;

-- One row per member decision within a plan revision: reused / hired / exited.
-- `agent_id` names the existing member for reuse/exit; `hired_agent_id`
-- records the pending agent created for a hire. Both stay NULL until the hire
-- lands.
CREATE TABLE IF NOT EXISTS task_team_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plan_id UUID NOT NULL REFERENCES task_team_plans(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    action TEXT NOT NULL CHECK (action IN ('reuse', 'hire', 'exit')),
    role TEXT,
    responsibilities TEXT,
    reason TEXT,
    -- Task-scope direct superior: NULL means the CEO owns this member.
    reports_to_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    -- A hire references an approval (agent-hires) created during planning.
    approval_id UUID REFERENCES approvals(id) ON DELETE SET NULL,
    hire_status TEXT CHECK (hire_status IN ('not_required', 'pending_approval', 'hired', 'rejected')),
    hired_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    -- When the member exits the task team. Exit removes task membership only;
    -- it never terminates the shared company agent.
    exited_at TIMESTAMPTZ,
    exit_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS task_team_members_company_idx ON task_team_members(company_id);
CREATE INDEX IF NOT EXISTS task_team_members_plan_idx ON task_team_members(plan_id);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'task_team_members'::regclass
           AND conname = 'fk_task_team_members_agent'
    ) THEN
        ALTER TABLE task_team_members
            ADD CONSTRAINT fk_task_team_members_agent
                FOREIGN KEY (agent_id, company_id)
                REFERENCES agents (id, company_id) ON DELETE SET NULL,
            ADD CONSTRAINT fk_task_team_members_reports_to
                FOREIGN KEY (reports_to_agent_id, company_id)
                REFERENCES agents (id, company_id) ON DELETE SET NULL,
            ADD CONSTRAINT fk_task_team_members_hired_agent
                FOREIGN KEY (hired_agent_id, company_id)
                REFERENCES agents (id, company_id) ON DELETE SET NULL;
    END IF;
END
$$;

-- Durable, resumable side effects. The worker claims a pending action, applies
-- it, and records the outcome; a crash between enqueue and apply is recovered
-- by replaying the still-pending row. Idempotent by (orchestration, key) —
-- the key is mandatory so a NULL cannot smuggle in a duplicate.
CREATE TABLE IF NOT EXISTS task_orchestration_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    orchestration_id UUID NOT NULL REFERENCES task_orchestrations(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    action_type TEXT NOT NULL
        CHECK (action_type IN (
            'hire_agent', 'exit_member', 'assign_subtask', 'wake_assignee',
            'accept_deliverable', 'reject_deliverable', 'submit_summary',
            'final_report', 'cancel', 'resume'
        )),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'applied', 'failed', 'skipped')),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Stable dedupe key for this (type, reference).
    idempotency_key TEXT,
    attempts INT NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    UNIQUE (orchestration_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS task_orchestration_actions_pending_idx
    ON task_orchestration_actions(orchestration_id, status) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS task_orchestration_actions_company_idx ON task_orchestration_actions(company_id);

-- Hardening: the key must exist so ON CONFLICT DO NOTHING is a real dedupe and
-- two workers cannot both claim "the same unnamed action".
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_name = 'task_orchestration_actions'
           AND column_name = 'idempotency_key'
           AND is_nullable = 'YES'
    ) THEN
        UPDATE task_orchestration_actions
           SET idempotency_key = action_type || ':' || id::text
         WHERE idempotency_key IS NULL;
        ALTER TABLE task_orchestration_actions
            ALTER COLUMN idempotency_key SET NOT NULL;
    END IF;
END
$$;

-- Required subtasks for the orchestration, linked to a real child issue.
CREATE TABLE IF NOT EXISTS task_orchestration_subtasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    orchestration_id UUID NOT NULL REFERENCES task_orchestrations(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    -- The member (agent) that owns execution of this subtask.
    owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    -- The task-scope direct superior that reviews this subtask; NULL = CEO.
    reviewer_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    acceptance_criteria TEXT,
    -- pending -> submitted -> accepted | rejected. A rejected submission stays
    -- on this row's history; the row returns to pending for a re-run.
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'submitted', 'accepted', 'rejected')),
    required BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (orchestration_id, issue_id)
);

CREATE INDEX IF NOT EXISTS task_orchestration_subtasks_company_idx ON task_orchestration_subtasks(company_id);
CREATE INDEX IF NOT EXISTS task_orchestration_subtasks_status_idx ON task_orchestration_subtasks(orchestration_id, status);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'task_orchestration_subtasks'::regclass
           AND conname = 'fk_task_orchestration_subtasks_issue'
    ) THEN
        ALTER TABLE task_orchestration_subtasks
            DROP CONSTRAINT IF EXISTS task_orchestration_subtasks_issue_id_fkey,
            ADD CONSTRAINT fk_task_orchestration_subtasks_issue
                FOREIGN KEY (issue_id, company_id)
                REFERENCES issues (id, company_id) ON DELETE CASCADE,
            ADD CONSTRAINT fk_task_orchestration_subtasks_owner
                FOREIGN KEY (owner_agent_id, company_id)
                REFERENCES agents (id, company_id) ON DELETE SET NULL,
            ADD CONSTRAINT fk_task_orchestration_subtasks_reviewer
                FOREIGN KEY (reviewer_agent_id, company_id)
                REFERENCES agents (id, company_id) ON DELETE SET NULL;
    END IF;
END
$$;

-- Deliverable versions: an immutable, independently reviewable artifact.
CREATE TABLE IF NOT EXISTS task_deliverables (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    orchestration_id UUID NOT NULL REFERENCES task_orchestrations(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    -- 'subtask' rows point at a task_orchestration_subtasks.id; 'final' is the
    -- orchestration-level summary.
    kind TEXT NOT NULL CHECK (kind IN ('subtask', 'final')),
    subtask_id UUID REFERENCES task_orchestration_subtasks(id) ON DELETE CASCADE,
    version INT NOT NULL,
    submitted_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    -- The issue work product / document that is the artifact evidence.
    work_product_id UUID,
    issue_document_key TEXT,
    body TEXT,
    evidence JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (subtask_id, version)
);

CREATE INDEX IF NOT EXISTS task_deliverables_company_idx ON task_deliverables(company_id);
CREATE INDEX IF NOT EXISTS task_deliverables_subtask_idx ON task_deliverables(subtask_id);
CREATE UNIQUE INDEX IF NOT EXISTS task_deliverables_final_uq
    ON task_deliverables(orchestration_id) WHERE kind = 'final';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'task_deliverables'::regclass
           AND conname = 'fk_task_deliverables_submitter'
    ) THEN
        ALTER TABLE task_deliverables
            ADD CONSTRAINT fk_task_deliverables_submitter
                FOREIGN KEY (submitted_by_agent_id, company_id)
                REFERENCES agents (id, company_id) ON DELETE SET NULL;
    END IF;
END
$$;

-- Acceptance decisions: who reviewed, the verdict, and the feedback on reject.
-- A deliverable version receives exactly one verdict: a re-submission is a new
-- version, so history is never overwritten.
CREATE TABLE IF NOT EXISTS task_acceptance_decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    orchestration_id UUID NOT NULL REFERENCES task_orchestrations(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    deliverable_id UUID NOT NULL REFERENCES task_deliverables(id) ON DELETE CASCADE,
    -- Actors are soft-deleted in Parrot; RESTRICT keeps the audit trail
    -- readable rather than nulling out who made the decision.
    reviewer_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
    verdict TEXT NOT NULL CHECK (verdict IN ('accepted', 'rejected')),
    -- The acceptance criteria this verdict was judged against.
    criteria TEXT,
    feedback TEXT,
    decided_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (deliverable_id)
);

CREATE INDEX IF NOT EXISTS task_acceptance_decisions_company_idx ON task_acceptance_decisions(company_id);
CREATE INDEX IF NOT EXISTS task_acceptance_decisions_deliverable_idx ON task_acceptance_decisions(deliverable_id);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'task_acceptance_decisions'::regclass
           AND conname = 'fk_task_acceptance_decisions_reviewer'
    ) THEN
        ALTER TABLE task_acceptance_decisions
            DROP CONSTRAINT IF EXISTS task_acceptance_decisions_reviewer_agent_id_fkey,
            ADD CONSTRAINT fk_task_acceptance_decisions_reviewer
                FOREIGN KEY (reviewer_agent_id, company_id)
                REFERENCES agents (id, company_id) ON DELETE RESTRICT;
    END IF;
END
$$;

-- Audit log: every applied event, in append-only order.
CREATE TABLE IF NOT EXISTS task_orchestration_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    orchestration_id UUID NOT NULL REFERENCES task_orchestrations(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    actor_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    actor_user_id UUID,
    stage_before TEXT,
    stage_after TEXT,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS task_orchestration_events_company_idx ON task_orchestration_events(company_id);
CREATE INDEX IF NOT EXISTS task_orchestration_events_orchestration_idx ON task_orchestration_events(orchestration_id);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'task_orchestration_events'::regclass
           AND conname = 'fk_task_orchestration_events_actor'
    ) THEN
        ALTER TABLE task_orchestration_events
            ADD CONSTRAINT fk_task_orchestration_events_actor
                FOREIGN KEY (actor_agent_id, company_id)
                REFERENCES agents (id, company_id) ON DELETE SET NULL;
    END IF;
END
$$;
