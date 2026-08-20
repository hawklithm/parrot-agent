ALTER TABLE decision_archive_notification_outbox
    DROP CONSTRAINT IF EXISTS decision_archive_outbox_status_check;

ALTER TABLE decision_archive_notification_outbox
    ADD CONSTRAINT decision_archive_outbox_status_check
    CHECK (status IN ('pending', 'delivering', 'delivered', 'failed'));

CREATE INDEX IF NOT EXISTS idx_decision_archive_outbox_delivery
    ON decision_archive_notification_outbox(status, last_attempt_at, created_at);
