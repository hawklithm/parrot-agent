-- Align legacy skill_versions with Paperclip's release-pinning contract.
-- Existing versions remain valid and are treated as unreleased until a release
-- id/name/date is explicitly assigned.

ALTER TABLE skill_versions
    ADD COLUMN IF NOT EXISTS release_id TEXT,
    ADD COLUMN IF NOT EXISTS release_name TEXT,
    ADD COLUMN IF NOT EXISTS released_at TIMESTAMPTZ;

CREATE UNIQUE INDEX IF NOT EXISTS skill_versions_skill_release_idx
    ON skill_versions(skill_id, release_id)
    WHERE release_id IS NOT NULL;
