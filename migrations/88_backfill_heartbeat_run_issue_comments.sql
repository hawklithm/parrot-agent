-- Backfill the task conversation entry that Paperclip creates when a
-- heartbeat run succeeds. Older Parrot runs only persisted their transcript
-- and result_json, so their Chat tab had no normal agent comment to render.
-- The actor_run_id guard makes this migration safe to run more than once.

WITH successful_runs AS (
    SELECT
        r.id AS run_id,
        r.company_id,
        r.agent_id,
        i.id AS issue_id,
        NULLIF(
            BTRIM(COALESCE(r.result_json->>'summary', r.result_json->>'resultSummary', r.result_json->>'result')),
            ''
        ) AS summary
    FROM heartbeat_runs r
    JOIN issues i
      ON i.id::text = COALESCE(r.context_snapshot->>'issueId', r.context_snapshot->>'taskId')
     AND i.company_id = r.company_id
    WHERE r.status = 'succeeded'
      AND NOT EXISTS (
          SELECT 1
          FROM issue_comments c
          WHERE c.issue_id = i.id
            AND c.actor_run_id = r.id
      )
)
INSERT INTO issue_comments (
    company_id,
    issue_id,
    body,
    actor_type,
    actor_id,
    actor_run_id,
    metadata,
    author_type
)
SELECT
    company_id,
    issue_id,
    CASE
        WHEN char_length(summary) > 1200
          OR lower(summary) LIKE 'let me %'
          OR lower(summary) LIKE 'i''ll %'
          OR lower(summary) LIKE 'i''m %'
          OR lower(summary) LIKE 'i can see%'
          OR lower(summary) LIKE 'now i''ll %'
          OR lower(summary) LIKE 'next i''ll %'
          OR lower(summary) LIKE 'looking at%'
          OR lower(summary) LIKE 'fetching%'
          OR lower(summary) LIKE 'checking%'
          OR lower(summary) LIKE 'first,%'
        THEN 'Run completed. Agent did not post a summary comment this run (transcript withheld — see run log).'
        ELSE summary
    END,
    'agent'::comment_actor_type,
    agent_id,
    run_id,
    jsonb_build_object('source', 'heartbeat_run'),
    'agent'
FROM successful_runs
WHERE summary IS NOT NULL;
