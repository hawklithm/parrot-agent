-- Migration: Create agent_start_locks table
--
-- Provides the per-agent mutual-exclusion lock used by AgentStartLockService
-- (crates/services/src/agent_start_lock_service.rs). The service was implemented
-- against this table but the matching DDL was never written or applied, leaving it
-- dead code and the table absent from every environment. This migration closes that
-- gap so the Heartbeat Start Lock (PAPERCLIP_MIGRATION_PLAN §4B.2 line 324) can be
-- enforced: only one adapter process may start for a given agent at a time.
--
-- Columns mirror the INSERT/DELETE performed by AgentStartLockService.acquire_lock /
-- release_lock / cleanup_expired_locks exactly:
--   id          lock row id (also the release handle)
--   agent_id    UNIQUE — at most one held lock per agent (ON CONFLICT (agent_id) DO NOTHING)
--   acquired_at lock creation time
--   expires_at  auto-expiry (30s by default) so a crashed holder cannot wedge the agent
--   holder      opaque string identifying the owner (heartbeat run id)

CREATE TABLE IF NOT EXISTS agent_start_locks (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id    UUID NOT NULL REFERENCES agents (id) ON DELETE CASCADE,
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ NOT NULL,
    holder      TEXT NOT NULL,
    CONSTRAINT agent_start_locks_agent_unique UNIQUE (agent_id)
);

CREATE INDEX IF NOT EXISTS agent_start_locks_expires_at_idx
    ON agent_start_locks (expires_at);

COMMENT ON TABLE agent_start_locks IS 'Per-agent mutual-exclusion lock guarding concurrent adapter process starts';
COMMENT ON COLUMN agent_start_locks.holder IS 'Opaque owner token (heartbeat run id) of the current lock holder';
