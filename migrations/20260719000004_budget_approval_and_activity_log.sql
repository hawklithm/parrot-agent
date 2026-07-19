-- Budget hard-stops require an explicit override approval.
ALTER TYPE approval_type ADD VALUE IF NOT EXISTS 'budget_override_required';

-- The pre-existing activity log stores the Paperclip event name in event_type.
-- Budget events use resource_type='budget' and structured metadata.
CREATE INDEX IF NOT EXISTS idx_activity_logs_budget_events
    ON activity_logs(company_id, event_type, created_at DESC)
    WHERE resource_type = 'budget';
