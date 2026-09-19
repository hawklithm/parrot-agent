//! Integration test: a task Chat comment reaches the agent's next run.
//!
//! Pins the three defects this change fixes, all in one live-DB scenario:
//!
//! 1. a comment wake always carries `commentId` / `wakeCommentIds`, so the
//!    comment can be loaded while the run prompt is built;
//! 2. a comment that arrives while the issue's run is already `running` is
//!    deferred into a follow-up `queued` run rather than silently dropped;
//! 3. a second comment coalesces into that same pending run instead of
//!    queueing a third turn.
//!
//! Skips when no live DB is reachable.

use services::wake_prompt_service::{
    extract_wake_comment_ids, merge_wake_context, render_run_prompt, PromptSource,
};
use services::{DefaultHeartbeatService, HeartbeatService, HeartbeatWakeupOptions};
use sqlx::PgPool;
use uuid::Uuid;

async fn connect() -> Option<PgPool> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping task chat wake test: DATABASE_URL is not set");
        return None;
    };
    match PgPool::connect(&database_url).await {
        Ok(pool) => Some(pool),
        Err(_) => {
            eprintln!("skipping task chat wake test: no DATABASE_URL reachable");
            None
        }
    }
}

async fn seed(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let company_id = Uuid::new_v4();
    let issue_prefix = format!("TC{}", &company_id.simple().to_string()[..6]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, 'TC Test Co', $2)")
        .bind(company_id)
        .bind(&issue_prefix)
        .execute(pool)
        .await
        .expect("insert company");

    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'TC Test Agent')")
        .bind(agent_id)
        .bind(company_id)
        .execute(pool)
        .await
        .expect("insert agent");

    let issue_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, priority)
         VALUES ($1, $2, 'Task Chat Issue', 'in_progress', 'medium')",
    )
    .bind(issue_id)
    .bind(company_id)
    .execute(pool)
    .await
    .expect("insert issue");

    (company_id, agent_id, issue_id)
}

async fn seed_comment(pool: &PgPool, company_id: Uuid, issue_id: Uuid, body: &str) -> Uuid {
    let comment_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issue_comments (id, company_id, issue_id, body, actor_type, actor_id)
         VALUES ($1, $2, $3, $4, 'user', NULL)",
    )
    .bind(comment_id)
    .bind(company_id)
    .bind(issue_id)
    .bind(body)
    .execute(pool)
    .await
    .expect("insert comment");
    comment_id
}

fn comment_wake_options(comment_id: Uuid, issue_id: Uuid) -> HeartbeatWakeupOptions {
    HeartbeatWakeupOptions {
        source: Some("automation".to_string()),
        trigger_detail: Some("system".to_string()),
        reason: Some("issue_commented".to_string()),
        idempotency_key: Some(format!("issue-comment:{comment_id}")),
        payload: Some(serde_json::json!({ "issueId": issue_id, "commentId": comment_id })),
        context_snapshot: Some(serde_json::json!({
            "issueId": issue_id,
            "taskId": issue_id,
            "source": "issue.comment",
            "wakeReason": "issue_commented",
            "commentId": comment_id,
            "wakeCommentId": comment_id,
            "wakeCommentIds": [comment_id],
        })),
        ..Default::default()
    }
}

async fn cleanup(pool: &PgPool, company_id: Uuid, issue_id: Uuid) {
    sqlx::query("DELETE FROM agent_task_sessions WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM agent_runtime_states WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM heartbeat_runs WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM agent_wakeup_requests WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM issue_comments WHERE issue_id = $1")
        .bind(issue_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM issues WHERE id = $1")
        .bind(issue_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(pool)
        .await
        .ok();
}

/// A comment wake persists `commentId` and the batch, and a comment arriving
/// while that turn is still pending extends the same run.
#[tokio::test]
async fn second_comment_coalesces_into_the_pending_run() {
    let Some(pool) = connect().await else { return };
    let (company_id, agent_id, issue_id) = seed(&pool).await;
    let service = DefaultHeartbeatService::new(pool.clone());

    // A turn that has been queued but has not built its prompt yet. Seeded
    // directly so the assertion does not race the background executor.
    let first = seed_comment(&pool, company_id, issue_id, "first follow-up").await;
    let pending_run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status, context_snapshot)
         VALUES ($1, $2, 'automation', 'queued', $3)
         RETURNING id",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(serde_json::json!({
        "issueId": issue_id,
        "taskKey": format!("issue:{issue_id}"),
        "source": "issue.comment",
        "wakeReason": "issue_commented",
        "commentId": first,
        "wakeCommentId": first,
        "wakeCommentIds": [first],
    }))
    .fetch_one(&pool)
    .await
    .expect("insert pending run");

    let second = seed_comment(&pool, company_id, issue_id, "second follow-up").await;
    service
        .wakeup_with_options(
            agent_id,
            issue_id,
            company_id,
            comment_wake_options(second, issue_id),
        )
        .await
        .expect("second comment wake");

    let runs: Vec<(Uuid, String, serde_json::Value)> = sqlx::query_as(
        "SELECT id, status::text, COALESCE(context_snapshot, '{}'::jsonb)
         FROM heartbeat_runs WHERE company_id = $1 AND agent_id = $2 ORDER BY created_at",
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_all(&pool)
    .await
    .expect("read runs");

    assert_eq!(runs.len(), 1, "a pending turn absorbs a second comment");
    assert_eq!(runs[0].0, pending_run_id, "the pending turn is reused");
    assert_eq!(runs[0].1, "queued", "coalescing does not start the turn");
    let ids = extract_wake_comment_ids(&runs[0].2);
    assert_eq!(ids, vec![first, second], "both comments ride the same turn");
    assert_eq!(
        runs[0].2["commentId"],
        serde_json::json!(second.to_string()),
        "the latest comment drives the follow-up"
    );

    cleanup(&pool, company_id, issue_id).await;
}

/// A comment arriving while the run is genuinely executing cannot be injected
/// into the live child process, and cannot start a second active run either
/// (`idx_heartbeat_runs_unique_active_agent_issue`). It is parked on its wakeup
/// request and replayed once the blocking run ends.
#[tokio::test]
async fn comment_during_running_run_is_deferred_then_replayed() {
    let Some(pool) = connect().await else { return };
    let (company_id, agent_id, issue_id) = seed(&pool).await;
    let service = DefaultHeartbeatService::new(pool.clone());

    // Simulate a run that has already consumed its prompt.
    let running_run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status, context_snapshot)
         VALUES ($1, $2, 'assignment', 'running', $3)
         RETURNING id",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(serde_json::json!({
        "issueId": issue_id,
        "taskKey": format!("issue:{issue_id}"),
        "wakeReason": "issue_reopened",
    }))
    .fetch_one(&pool)
    .await
    .expect("insert running run");

    let comment_id = seed_comment(&pool, company_id, issue_id, "但是agent并没有按照预期正常创建").await;
    service
        .wakeup_with_options(
            agent_id,
            issue_id,
            company_id,
            comment_wake_options(comment_id, issue_id),
        )
        .await
        .expect("comment wake during running run");

    // The live run is untouched and nothing else became active, so the unique
    // active run index still holds.
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs
         WHERE company_id = $1 AND agent_id = $2 AND status IN ('queued','running')",
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_one(&pool)
    .await
    .expect("count active runs");
    assert_eq!(active_count, 1, "the running run keeps the agent+issue slot");

    let (request_status, parked_behind, parked_payload): (String, Option<Uuid>, serde_json::Value) =
        sqlx::query_as(
            "SELECT status::text, run_id, payload FROM agent_wakeup_requests
             WHERE company_id = $1 AND agent_id = $2 AND status = 'queued'
             ORDER BY requested_at DESC LIMIT 1",
        )
        .bind(company_id)
        .bind(agent_id)
        .fetch_one(&pool)
        .await
        .expect("read parked wake request");

    assert_eq!(request_status, "queued", "the wake stays pending, not consumed");
    assert_eq!(
        parked_behind,
        Some(running_run_id),
        "the parked wake names the run that blocked it"
    );
    assert_eq!(
        extract_wake_comment_ids(&parked_payload["deferredContext"]),
        vec![comment_id],
        "the comment is carried on the parked wake"
    );

    // The blocking run terminates; the parked comment becomes a real turn.
    // Drive the same two steps `execute_run` runs at its tail, in order: the
    // completion sweep must leave the parked row alone for the replay to find.
    sqlx::query("UPDATE heartbeat_runs SET status = 'succeeded', finished_at = NOW() WHERE id = $1")
        .bind(running_run_id)
        .execute(&pool)
        .await
        .expect("complete running run");

    service
        .complete_run_wakeup_requests(company_id, agent_id, issue_id, running_run_id)
        .await;

    let (status_after_sweep,): (String,) = sqlx::query_as(
        "SELECT status::text FROM agent_wakeup_requests WHERE run_id = $1 ORDER BY requested_at DESC LIMIT 1",
    )
    .bind(running_run_id)
    .fetch_one(&pool)
    .await
    .expect("read parked wake after the completion sweep");
    assert_eq!(
        status_after_sweep, "queued",
        "the completion sweep must not consume a parked comment wake"
    );

    let replayed = service
        .reconcile_deferred_comment_wakes(agent_id, company_id)
        .await
        .expect("replay deferred comment wake");
    assert_eq!(replayed, 1, "the parked comment is replayed exactly once");

    let (new_run_id, snapshot): (Uuid, serde_json::Value) = sqlx::query_as(
        "SELECT id, COALESCE(context_snapshot, '{}'::jsonb) FROM heartbeat_runs
         WHERE company_id = $1 AND agent_id = $2 AND id <> $3
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(running_run_id)
    .fetch_one(&pool)
    .await
    .expect("read replayed run");

    assert_ne!(new_run_id, running_run_id);
    assert_eq!(
        extract_wake_comment_ids(&snapshot),
        vec![comment_id],
        "the follow-up run carries the operator's comment"
    );
    assert_eq!(
        snapshot["wakeReason"],
        serde_json::json!("issue_commented"),
        "the follow-up carries the comment reason, not the stale assignment reason"
    );

    // The replay must render the operator's comment, not the brief.
    let comments =
        services::wake_prompt_service::load_wake_comments(&pool, company_id, &[comment_id])
            .await
            .expect("load wake comments");
    let rendered = render_run_prompt(
        "Task: Create a new agent".to_string(),
        true,
        snapshot["wakeReason"].as_str(),
        Some("MAR-12"),
        "Create a new agent",
        &comments,
        &[comment_id],
    );
    assert_eq!(rendered.source, PromptSource::ResumeDelta);
    assert!(rendered.prompt.contains("但是agent并没有按照预期正常创建"));
    assert!(!rendered.prompt.contains("Task: Create a new agent"));

    // Replaying again must not deliver the same comment a second time.
    let replayed_again = service
        .reconcile_deferred_comment_wakes(agent_id, company_id)
        .await
        .expect("second replay scan");
    assert_eq!(replayed_again, 0, "the replay is idempotent");

    cleanup(&pool, company_id, issue_id).await;
}

/// The stored snapshot must survive a merge with the context-less assignment
/// wake that the issue service spawns on every comment against a closed issue.
#[tokio::test]
async fn assignment_wake_does_not_erase_the_comment_delta() {
    let Some(pool) = connect().await else { return };
    let (company_id, _agent_id, issue_id) = seed(&pool).await;
    let comment_id = seed_comment(&pool, company_id, issue_id, "recreate it").await;

    // The comment wake committed first, then the context-less reopen wake.
    let after_comment = serde_json::json!({
        "issueId": issue_id,
        "taskKey": format!("issue:{issue_id}"),
        "source": "issue.comment.reopen",
        "wakeReason": "issue_reopened_via_comment",
        "commentId": comment_id,
        "wakeCommentId": comment_id,
        "wakeCommentIds": [comment_id],
    });
    let assignment = serde_json::json!({
        "issueId": issue_id,
        "taskKey": format!("issue:{issue_id}"),
    });
    let merged = merge_wake_context(&after_comment, &assignment);

    assert_eq!(
        extract_wake_comment_ids(&merged),
        vec![comment_id],
        "the comment survives the later assignment wake"
    );
    assert_eq!(
        merged["wakeReason"],
        serde_json::json!("issue_reopened_via_comment")
    );

    cleanup(&pool, company_id, issue_id).await;
}

/// Two different task keys must never resume the same provider session.
///
/// The defect this pins: a task-scoped run with no `agent_task_sessions` row of
/// its own fell back to the agent's global `agent_runtime_states.session_id`, so
/// three unrelated issues all resumed Claude session `1e5a1e18` and shared
/// conversation history.
#[tokio::test]
async fn task_run_never_inherits_the_agent_global_session() {
    let Some(pool) = connect().await else { return };
    let (company_id, agent_id, issue_id) = seed(&pool).await;
    let service = DefaultHeartbeatService::new(pool.clone());

    let global_session = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO agent_runtime_states (agent_id, company_id, adapter_type, session_id)
         VALUES ($1, $2, 'claude_local', $3)",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind(&global_session)
    .execute(&pool)
    .await
    .expect("insert runtime state");

    let task_key = format!("issue:{issue_id}");
    let task_scoped_run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status, context_snapshot)
         VALUES ($1, $2, 'assignment', 'queued', $3) RETURNING id",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(serde_json::json!({ "issueId": issue_id, "taskKey": task_key }))
    .fetch_one(&pool)
    .await
    .expect("insert task-scoped run");

    let resolved = service
        .resolve_persisted_session(company_id, agent_id, "claude_local", &task_key, task_scoped_run_id)
        .await
        .expect("resolve task session");
    assert_eq!(
        resolved, None,
        "a task-scoped run with no task session of its own must start fresh, \
         not borrow the agent's global session"
    );

    // An agent-level run has no task of its own, so the global session is its
    // legitimate resume target.
    let agent_level_run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status)
         VALUES ($1, $2, 'on_demand', 'queued') RETURNING id",
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_one(&pool)
    .await
    .expect("insert agent-level run");

    let resolved = service
        .resolve_persisted_session(company_id, agent_id, "claude_local", "agent:global", agent_level_run_id)
        .await
        .expect("resolve agent-level session");
    assert_eq!(resolved.as_deref(), Some(global_session.as_str()));

    sqlx::query(
        "INSERT INTO agent_task_sessions (company_id, agent_id, adapter_type, task_key, session_display_id)
         VALUES ($1, $2, 'claude_local', $3, $4)",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(&task_key)
    .bind(&global_session)
    .execute(&pool)
    .await
    .expect("insert task session");

    let resolved = service
        .resolve_persisted_session(company_id, agent_id, "claude_local", &task_key, task_scoped_run_id)
        .await
        .expect("resolve task session");
    assert_eq!(
        resolved.as_deref(),
        Some(global_session.as_str()),
        "the same task key still resumes its own session"
    );

    cleanup(&pool, company_id, issue_id).await;
}

/// A task key whose session belongs to another task key is not resumed.
#[tokio::test]
async fn a_task_key_does_not_borrow_another_tasks_session() {
    let Some(pool) = connect().await else { return };
    let (company_id, agent_id, issue_id) = seed(&pool).await;
    let service = DefaultHeartbeatService::new(pool.clone());

    let other_session = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO agent_task_sessions (company_id, agent_id, adapter_type, task_key, session_display_id)
         VALUES ($1, $2, 'claude_local', $3, $4)",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(format!("issue:{}", Uuid::new_v4()))
    .bind(&other_session)
    .execute(&pool)
    .await
    .expect("insert other task session");

    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status, context_snapshot)
         VALUES ($1, $2, 'assignment', 'queued', $3) RETURNING id",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(serde_json::json!({
        "issueId": issue_id,
        "taskKey": format!("issue:{issue_id}"),
    }))
    .fetch_one(&pool)
    .await
    .expect("insert run");

    let resolved = service
        .resolve_persisted_session(
            company_id,
            agent_id,
            "claude_local",
            &format!("issue:{issue_id}"),
            run_id,
        )
        .await
        .expect("resolve task session");
    assert_eq!(
        resolved, None,
        "this task key has no session of its own and must not adopt another task's"
    );

    cleanup(&pool, company_id, issue_id).await;
}

/// A later comment extends the parked wake instead of parking a second one.
///
/// Only one row may be parked behind a running run: the replay turns each parked
/// row into a run, and a second active run for the same agent+issue is forbidden.
/// The extended row must also stop claiming the comments the running run already
/// received, or the replay delivers them a second time.
#[tokio::test]
async fn a_third_comment_extends_the_parked_wake_without_replaying_delivered_ones() {
    let Some(pool) = connect().await else { return };
    let (company_id, agent_id, issue_id) = seed(&pool).await;
    let service = DefaultHeartbeatService::new(pool.clone());

    let first = seed_comment(&pool, company_id, issue_id, "first").await;
    let running_run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status, context_snapshot)
         VALUES ($1, $2, 'assignment', 'running', $3) RETURNING id",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(comment_wake_options(first, issue_id)
        .context_snapshot
        .expect("snapshot"))
    .fetch_one(&pool)
    .await
    .expect("insert running run");

    let second = seed_comment(&pool, company_id, issue_id, "second").await;
    service
        .wakeup_with_options(
            agent_id,
            issue_id,
            company_id,
            comment_wake_options(second, issue_id),
        )
        .await
        .expect("second comment parks");

    let third = seed_comment(&pool, company_id, issue_id, "third").await;
    service
        .wakeup_with_options(
            agent_id,
            issue_id,
            company_id,
            comment_wake_options(third, issue_id),
        )
        .await
        .expect("third comment extends the parked wake");

    let parked: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT payload FROM agent_wakeup_requests
         WHERE company_id = $1 AND agent_id = $2 AND status = 'queued' AND run_id = $3",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(running_run_id)
    .fetch_all(&pool)
    .await
    .expect("read parked wakes");

    assert_eq!(parked.len(), 1, "only one wake may be parked per blocking run");
    assert_eq!(
        extract_wake_comment_ids(&parked[0].0["deferredContext"]),
        vec![second, third],
        "the parked delta carries the comments the running run never saw, in order"
    );

    cleanup(&pool, company_id, issue_id).await;
}
