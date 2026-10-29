-- Typed references for the first recoverable task-orchestration wake events.
-- The action UUID points back to the durable orchestration outbox; the resume
-- row intentionally does not copy its arbitrary JSON payload.
ALTER TABLE task_budget_wake_resumes
    ADD COLUMN IF NOT EXISTS resume_kind TEXT NOT NULL DEFAULT 'generic',
    ADD COLUMN IF NOT EXISTS source_task_action_id UUID;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'task_budget_wake_resumes'::regclass
           AND conname = 'task_budget_wake_resumes_resume_kind_check'
    ) THEN
        ALTER TABLE task_budget_wake_resumes
            ADD CONSTRAINT task_budget_wake_resumes_resume_kind_check
            CHECK (resume_kind IN ('generic', 'subtask_assigned', 'subtask_accepted'));
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'task_budget_wake_resumes'::regclass
           AND conname = 'task_budget_wake_resumes_action_reference_check'
    ) THEN
        ALTER TABLE task_budget_wake_resumes
            ADD CONSTRAINT task_budget_wake_resumes_action_reference_check
            CHECK ((resume_kind IN ('subtask_assigned', 'subtask_accepted')) =
                   (source_task_action_id IS NOT NULL));
    END IF;
END
$$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_task_budget_wake_resumes_source_task_action
    ON task_budget_wake_resumes (company_id, source_task_action_id)
    WHERE source_task_action_id IS NOT NULL;
