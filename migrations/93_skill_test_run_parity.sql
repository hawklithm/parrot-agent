-- Skill Studio run parity.
--
-- Paperclip's Skills Studio drives a real harness run: `POST
-- /companies/:id/skills/:skillId/test-runs` snapshots the skill revision and
-- the agent config, opens a `skill_test` issue, and then tracks the run
-- through it. Parrot had the `skill_test_runs` table but nothing that could
-- write it, so the run projection carried only seven columns and the version
-- projection had to derive `revisionNumber` in the read query.
--
-- Three shapes are missing, all of them Paperclip's:
--
--   * A version needs a stable identity. Parrot derived `revisionNumber` with
--     `ROW_NUMBER()` in the SELECT, which cannot allocate the next revision
--     atomically when two restores race. It is a column now, with a unique
--     index per skill.
--   * A run needs the snapshot fields the Studio reads back (input, skill
--     version, agent config, template, harness issue, output). `template_id`
--     also has to widen: Paperclip's built-in template is the string
--     `built-in:default-test-template`, which a uuid column cannot hold.
--   * A template needs `body` — the harness instructions — rather than only
--     the opaque `config` blob, and templates are soft-deleted so a run that
--     references one keeps resolving.

-- ─── skill_versions ───────────────────────────────────────────

-- Paperclip's `companySkillVersions.revisionNumber`: monotonic per skill.
ALTER TABLE skill_versions
    ADD COLUMN IF NOT EXISTS revision_number integer;

-- Backfill in creation order so existing rows get the revision they were
-- already being displayed as.
UPDATE skill_versions sv
SET revision_number = ranked.rn
FROM (
    SELECT id, ROW_NUMBER() OVER (PARTITION BY skill_id ORDER BY created_at, id) AS rn
    FROM skill_versions
) ranked
WHERE sv.id = ranked.id;

ALTER TABLE skill_versions
    ALTER COLUMN revision_number SET NOT NULL;

-- Guards the `MAX(revision_number) + 1` allocation in `create_version`: two
-- concurrent restores of the same skill must not land on the same revision.
CREATE UNIQUE INDEX IF NOT EXISTS skill_versions_skill_revision_idx
    ON skill_versions (skill_id, revision_number);

-- Paperclip's free-text `label`, distinct from `version`. `version` is the
-- unique identity Parrot already enforced; `label` is the note the Studio
-- writes when restoring ("Restore of v2").
ALTER TABLE skill_versions
    ADD COLUMN IF NOT EXISTS label text;

-- The revision's own copy of the skill's files.
--
-- Paperclip stores `{path, kind, content}` on the version so history, diff and
-- restore keep working after the live files move on. Parrot kept the bytes only
-- in `skill_files`, which holds the *current* content, so every historical
-- version would read back as the present-day files.
ALTER TABLE skill_versions
    ADD COLUMN IF NOT EXISTS file_inventory jsonb NOT NULL DEFAULT '[]'::jsonb;

-- ─── skill_test_runs ──────────────────────────────────────────

-- The built-in template id is a string, not a uuid. Widen before adding the
-- built-in template to the list projection.
ALTER TABLE skill_test_runs
    DROP CONSTRAINT IF EXISTS skill_test_runs_template_id_fkey;

ALTER TABLE skill_test_runs
    ALTER COLUMN template_id TYPE text USING template_id::text;

ALTER TABLE skill_test_runs
    ADD COLUMN IF NOT EXISTS input_id uuid REFERENCES skill_test_inputs (id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS input_snapshot text NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS skill_version_id uuid REFERENCES skill_versions (id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS agent_id uuid REFERENCES agents (id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS agent_config_snapshot jsonb NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS issue_id uuid REFERENCES issues (id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS template_name text,
    ADD COLUMN IF NOT EXISTS template_body text,
    ADD COLUMN IF NOT EXISTS rendered_template_body text,
    ADD COLUMN IF NOT EXISTS harness_issue_description text NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS output_document_key text NOT NULL DEFAULT 'output',
    ADD COLUMN IF NOT EXISTS output_snapshot text NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS error text,
    ADD COLUMN IF NOT EXISTS deleted_at timestamptz,
    ADD COLUMN IF NOT EXISTS superseded_at timestamptz,
    ADD COLUMN IF NOT EXISTS harness_issue_expires_at timestamptz,
    ADD COLUMN IF NOT EXISTS harness_issue_deleted_at timestamptz;

-- The Studio lists a skill's runs newest-first.
CREATE INDEX IF NOT EXISTS idx_skill_test_runs_company_skill_created
    ON skill_test_runs (company_id, skill_id, created_at DESC);

-- One live run per harness issue: the issue drives run completion, so a
-- second run bound to the same issue would never be finalized.
CREATE UNIQUE INDEX IF NOT EXISTS idx_skill_test_runs_company_issue
    ON skill_test_runs (company_id, issue_id)
    WHERE issue_id IS NOT NULL;

-- Supersede scans previous runs of the same test input.
CREATE INDEX IF NOT EXISTS idx_skill_test_runs_company_input_created
    ON skill_test_runs (company_id, input_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_skill_test_runs_company_status
    ON skill_test_runs (company_id, status);

-- Retention sweep for expired harness issues.
CREATE INDEX IF NOT EXISTS idx_skill_test_runs_harness_issue_expires
    ON skill_test_runs (company_id, harness_issue_expires_at);

-- ─── skill_test_run_templates ─────────────────────────────────

-- `body` holds the harness instructions the run renders. `config` is kept:
-- it is the existing persisted payload and stays part of the response.
ALTER TABLE skill_test_run_templates
    ADD COLUMN IF NOT EXISTS description text,
    ADD COLUMN IF NOT EXISTS body text NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS deleted_at timestamptz,
    ADD COLUMN IF NOT EXISTS updated_by_agent_id uuid,
    ADD COLUMN IF NOT EXISTS updated_by_user_id uuid;

-- Soft delete keeps a superseded run's template snapshot resolvable.
CREATE INDEX IF NOT EXISTS idx_skill_test_run_templates_company_active
    ON skill_test_run_templates (company_id, name)
    WHERE deleted_at IS NULL;
