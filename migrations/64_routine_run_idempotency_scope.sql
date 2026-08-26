-- Manual routine dispatches may not have a trigger_id. PostgreSQL UNIQUE
-- constraints do not deduplicate NULL values, so scope idempotency by routine.
CREATE UNIQUE INDEX IF NOT EXISTS routine_runs_routine_idempotency_key_uq
    ON routine_runs (routine_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
