-- A task cap is owned by the root Issue so it is committed with issue creation
-- before any assignment wake can be dispatched. Child issues inherit the root
-- cap at enforcement time; they never store independent task caps.
ALTER TABLE issues
    ADD COLUMN IF NOT EXISTS task_budget_cents BIGINT,
    ADD COLUMN IF NOT EXISTS task_budget_revision BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS task_orchestration_required BOOLEAN NOT NULL DEFAULT FALSE;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'issues'::regclass
           AND conname = 'issues_task_budget_root_nonnegative_check'
    ) THEN
        ALTER TABLE issues
            ADD CONSTRAINT issues_task_budget_root_nonnegative_check
            CHECK (task_budget_cents IS NULL OR (task_budget_cents >= 0 AND parent_id IS NULL));
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'issues'::regclass
           AND conname = 'issues_task_budget_revision_nonnegative_check'
    ) THEN
        ALTER TABLE issues
            ADD CONSTRAINT issues_task_budget_revision_nonnegative_check
            CHECK (task_budget_revision >= 0);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'issues'::regclass
           AND conname = 'issues_task_orchestration_required_root_check'
    ) THEN
        ALTER TABLE issues
            ADD CONSTRAINT issues_task_orchestration_required_root_check
            CHECK (NOT task_orchestration_required OR parent_id IS NULL);
    END IF;
END
$$;
