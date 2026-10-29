-- Durable intent to replay a wake skipped by the task-budget gate.
-- The source wake request remains an immutable audit record in `skipped` state;
-- replay attempts use a separate stable idempotency key derived from this row.
-- The source UUID is deliberately not a foreign key so this intent is not
-- deleted if wake-request history is pruned.
CREATE TABLE IF NOT EXISTS task_budget_wake_resumes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    root_issue_id UUID,
    issue_id UUID NOT NULL,
    agent_id UUID NOT NULL,
    source_wakeup_request_id UUID NOT NULL,
    budget_revision BIGINT NOT NULL DEFAULT 0 CHECK (budget_revision >= 0),
    requested_by_actor_type TEXT,
    requested_by_actor_id UUID,
    resume_source TEXT,
    resume_trigger_detail TEXT,
    resume_reason TEXT,
    original_idempotency_key TEXT,
    resume_idempotency_key TEXT NOT NULL,
    resume_payload JSONB,
    resume_context_snapshot JSONB,
    retry_of_run_id UUID,
    scheduled_retry_attempt INTEGER,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'applied', 'cancelled', 'unsupported')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claim_token UUID,
    claim_expires_at TIMESTAMPTZ,
    last_error TEXT,
    resume_wakeup_request_id UUID REFERENCES agent_wakeup_requests(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    UNIQUE (company_id, source_wakeup_request_id),
    UNIQUE (company_id, resume_idempotency_key),
    CHECK ((status = 'processing') = (claim_token IS NOT NULL AND claim_expires_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_task_budget_wake_resumes_pending
    ON task_budget_wake_resumes (next_attempt_at, created_at)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_task_budget_wake_resumes_expired_claim
    ON task_budget_wake_resumes (claim_expires_at, created_at)
    WHERE status = 'processing';

CREATE INDEX IF NOT EXISTS idx_task_budget_wake_resumes_root
    ON task_budget_wake_resumes (company_id, root_issue_id, status);
