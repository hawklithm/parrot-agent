-- Migration: Add env_vars column to environments
--
-- Aligns with Paperclip environments schema which includes:
--   env_vars JSONB NOT NULL DEFAULT '{}' — environment variables available to
--   agents running in this environment.
--
-- The column was omitted from the unified init schema, but
-- PgEnvironmentRepository::list_by_company already selects it, so every
-- `GET /companies/:company_id/environments` failed with
-- `column "env_vars" does not exist`. `ExecutionEnvironment.env_vars` is a
-- non-optional JsonValue, so the column must be NOT NULL with a default to
-- keep existing rows decodable.

ALTER TABLE environments
    ADD COLUMN IF NOT EXISTS env_vars JSONB NOT NULL DEFAULT '{}';
