//! Interaction continuation recovery parity tests.
//!
//! These tests exercise the real heartbeat service and recovery job against
//! PostgreSQL.  The normal interaction-resolution path is best-effort, so the
//! backstop must recover a lost wake without creating a second execution when
//! normal or concurrent callers already supplied the continuation.

use services::job_scheduler::{HeartbeatRecoveryJob, ScheduledJob};
use services::heartbeat_service::{DefaultHeartbeatService, HeartbeatService, HeartbeatWakeupOptions};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    agent_id: Uuid,
    issue_id: Uuid,
}

async fn seed(pool: &PgPool, issue_status: &str) -> Fixture {
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let prefix = format!("IC{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Interaction continuation parity Co")
        .bind(&prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status, adapter_type)
         VALUES ($1, $2, 'Interaction continuation agent', 'idle', 'process')",
    )
    .bind(agent_id)
    .bind(company_id)
    .execute(pool)
    .await
    .expect("insert agent");
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, assignee_agent_id)
         VALUES ($1, $2, 'Interaction continuation issue', $3, $4::issue_status, $5)",
    )
    .bind(issue_id)
    .bind(company_id)
    .bind(format!("{prefix}-1"))
    .bind(issue_status)
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("insert issue");

    Fixture {
        pool: pool.clone(),
        company_id,
        agent_id,
        issue_id,
    }
}

async fn insert_interaction(f: &Fixture, status: &str, policy: &str) -> Uuid {
    let interaction_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issue_thread_interactions
         (id, company_id, issue_id, kind, status, continuation_policy,
          requested_resolver_policy, effective_resolver_policy, payload, created_at, updated_at)
         VALUES ($1, $2, $3, 'question', $4, $5, 'board_only', 'board_only', '{}', NOW(), NOW())",
    )
    .bind(interaction_id)
    .bind(f.company_id)
    .bind(f.issue_id)
    .bind(status)
    .bind(policy)
    .execute(&f.pool)
    .await
    .expect("insert interaction");
    interaction_id
}

async fn insert_wake(
    f: &Fixture,
    interaction_id: Uuid,
    status: &str,
    idempotency_key: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO agent_wakeup_requests
         (company_id, agent_id, status, payload, source, reason, idempotency_key,
          requested_at, updated_at, finished_at)
         VALUES ($1, $2, $3::agent_wakeup_request_status, $4, 'interaction',
                 'issue_continuation_needed', $5, NOW(), NOW(),
                 CASE WHEN $3 IN ('completed', 'failed', 'cancelled') THEN NOW() ELSE NULL END)",
    )
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind(status)
    .bind(serde_json::json!({
        "issueId": f.issue_id,
        "interactionId": interaction_id,
    }))
    .bind(idempotency_key)
    .execute(&f.pool)
    .await
    .expect("insert wake");
}

async fn interaction_wake_count(f: &Fixture, interaction_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
          WHERE company_id = $1 AND agent_id = $2
            AND payload->>'interactionId' = $3",
    )
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind(interaction_id.to_string())
    .fetch_one(&f.pool)
    .await
    .expect("count interaction wakes")
}

async fn backstop_wake_count(f: &Fixture, interaction_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
          WHERE company_id = $1
            AND idempotency_key = 'interaction_continuation_backstop:' || $2::text",
    )
    .bind(f.company_id)
    .bind(interaction_id)
    .fetch_one(&f.pool)
    .await
    .expect("count backstop wakes")
}

async fn interaction_run_count(f: &Fixture, interaction_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs
          WHERE company_id = $1 AND agent_id = $2
            AND context_snapshot->>'interactionId' = $3",
    )
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind(interaction_id.to_string())
    .fetch_one(&f.pool)
    .await
    .expect("count interaction runs")
}

async fn cleanup(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM agent_wakeup_requests WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM heartbeat_runs WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issue_thread_interactions WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
}

#[sqlx::test]
async fn backstop_supports_both_continuation_policies(pool: PgPool) {
    migrate(&pool).await;
    let accepted = seed(&pool, "in_progress").await;
    let accepted_interaction = insert_interaction(&accepted, "accepted", "wake_assignee_on_accept").await;

    let heartbeat = DefaultHeartbeatService::new(pool.clone());
    assert_eq!(
        heartbeat
            .reconcile_interaction_continuation_wakeups()
            .await
            .expect("reconcile accepted interaction"),
        1
    );
    assert_eq!(backstop_wake_count(&accepted, accepted_interaction).await, 1);

    cleanup(&accepted).await;

    let rejected = seed(&pool, "in_progress").await;
    let rejected_interaction = insert_interaction(&rejected, "rejected", "wake_assignee_on_accept").await;
    let heartbeat = DefaultHeartbeatService::new(pool.clone());
    assert_eq!(
        heartbeat
            .reconcile_interaction_continuation_wakeups()
            .await
            .expect("reconcile rejected interaction"),
        0
    );
    assert_eq!(backstop_wake_count(&rejected, rejected_interaction).await, 0);
    cleanup(&rejected).await;
}

#[sqlx::test]
async fn normal_wake_states_suppress_backstop_but_failed_wake_is_retried(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool, "in_progress").await;
    let interaction_id = insert_interaction(&f, "answered", "wake_assignee").await;

    for status in ["queued", "dispatched", "running", "completed"] {
        insert_wake(&f, interaction_id, status, None).await;
        assert_eq!(
            DefaultHeartbeatService::new(pool.clone())
                .reconcile_interaction_continuation_wakeups()
                .await
                .expect("reconcile existing normal wake"),
            0,
            "{status} normal wake must suppress the backstop"
        );
        sqlx::query("DELETE FROM agent_wakeup_requests WHERE company_id = $1")
            .bind(f.company_id)
            .execute(&pool)
            .await
            .expect("delete normal wake");
    }

    insert_wake(&f, interaction_id, "failed", Some("normal-interaction-failed")).await;
    assert_eq!(
        DefaultHeartbeatService::new(pool.clone())
            .reconcile_interaction_continuation_wakeups()
            .await
            .expect("retry failed normal wake"),
        1
    );
    assert_eq!(backstop_wake_count(&f, interaction_id).await, 1);
    assert_eq!(interaction_wake_count(&f, interaction_id).await, 2);

    cleanup(&f).await;
}

#[sqlx::test]
async fn concurrent_recovery_creates_one_backstop_wake_and_one_run(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool, "in_progress").await;
    let interaction_id = insert_interaction(&f, "accepted", "wake_assignee").await;
    let first = DefaultHeartbeatService::new(pool.clone());
    let second = DefaultHeartbeatService::new(pool.clone());

    let _ = tokio::join!(
        first.reconcile_interaction_continuation_wakeups(),
        second.reconcile_interaction_continuation_wakeups(),
    );

    assert_eq!(backstop_wake_count(&f, interaction_id).await, 1);
    assert_eq!(interaction_run_count(&f, interaction_id).await, 1);
    cleanup(&f).await;
}

#[sqlx::test]
async fn failed_backstop_is_reactivated_and_recovery_job_calls_it(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool, "in_progress").await;
    let interaction_id = insert_interaction(&f, "cancelled", "wake_assignee").await;
    let key = format!("interaction_continuation_backstop:{interaction_id}");
    insert_wake(&f, interaction_id, "failed", Some(&key)).await;

    let heartbeat = Arc::new(DefaultHeartbeatService::new(pool.clone()));
    let result = HeartbeatRecoveryJob::new(heartbeat)
        .execute()
        .await
        .expect("execute heartbeat recovery job");
    assert!(result.contains("interaction continuation wakes"), "{result}");
    assert_eq!(backstop_wake_count(&f, interaction_id).await, 1);
    assert_eq!(interaction_run_count(&f, interaction_id).await, 1);

    cleanup(&f).await;
}

#[sqlx::test]
async fn live_retry_terminal_issue_and_pause_hold_are_excluded(pool: PgPool) {
    migrate(&pool).await;

    let live = seed(&pool, "in_progress").await;
    let _live_interaction = insert_interaction(&live, "accepted", "wake_assignee").await;
    sqlx::query(
        "INSERT INTO heartbeat_runs (company_id, agent_id, status, context_snapshot)
         VALUES ($1, $2, 'scheduled_retry', $3)",
    )
    .bind(live.company_id)
    .bind(live.agent_id)
    .bind(serde_json::json!({"issueId": live.issue_id}))
    .execute(&pool)
    .await
    .expect("insert scheduled retry");
    assert_eq!(
        DefaultHeartbeatService::new(pool.clone())
            .reconcile_interaction_continuation_wakeups()
            .await
            .expect("reconcile live retry"),
        0
    );
    cleanup(&live).await;

    let terminal = seed(&pool, "done").await;
    let terminal_interaction = insert_interaction(&terminal, "accepted", "wake_assignee").await;
    assert_eq!(
        DefaultHeartbeatService::new(pool.clone())
            .reconcile_interaction_continuation_wakeups()
            .await
            .expect("reconcile terminal issue"),
        0
    );
    assert_eq!(backstop_wake_count(&terminal, terminal_interaction).await, 0);
    cleanup(&terminal).await;

    let held = seed(&pool, "in_progress").await;
    let held_interaction = insert_interaction(&held, "accepted", "wake_assignee").await;
    let hold_id: Uuid = sqlx::query_scalar(
        "INSERT INTO issue_tree_holds (company_id, root_issue_id, mode, status)
         VALUES ($1, $2, 'pause', 'active') RETURNING id",
    )
    .bind(held.company_id)
    .bind(held.issue_id)
    .fetch_one(&pool)
    .await
    .expect("insert pause hold");
    sqlx::query(
        "INSERT INTO issue_tree_hold_members
         (company_id, hold_id, issue_id, depth, issue_title, issue_status)
         VALUES ($1, $2, $3, 0, 'Interaction continuation issue', 'in_progress')",
    )
    .bind(held.company_id)
    .bind(hold_id)
    .bind(held.issue_id)
    .execute(&pool)
    .await
    .expect("insert pause hold member");
    assert_eq!(
        DefaultHeartbeatService::new(pool.clone())
            .reconcile_interaction_continuation_wakeups()
            .await
            .expect("reconcile held issue"),
        0
    );
    assert_eq!(backstop_wake_count(&held, held_interaction).await, 0);
    cleanup(&held).await;
}

#[sqlx::test]
async fn backlog_is_active_for_continuation_recovery(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool, "backlog").await;
    let interaction_id = insert_interaction(&f, "rejected", "wake_assignee").await;

    assert_eq!(
        DefaultHeartbeatService::new(pool.clone())
            .reconcile_interaction_continuation_wakeups()
            .await
            .expect("reconcile backlog issue"),
        1
    );
    assert_eq!(backstop_wake_count(&f, interaction_id).await, 1);

    cleanup(&f).await;
}

#[sqlx::test]
async fn duplicate_wakeup_key_after_completion_does_not_create_second_run(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool, "in_progress").await;
    let key = "same-interaction-wake";
    let heartbeat = DefaultHeartbeatService::new(pool.clone());
    let options = HeartbeatWakeupOptions {
        source: Some("interaction".to_string()),
        reason: Some("issue_continuation_needed".to_string()),
        idempotency_key: Some(key.to_string()),
        payload: Some(serde_json::json!({"issueId": f.issue_id})),
        context_snapshot: Some(serde_json::json!({"issueId": f.issue_id})),
        ..Default::default()
    };
    heartbeat
        .wakeup_with_options(f.agent_id, f.issue_id, f.company_id, options.clone())
        .await
        .expect("first wake");
    heartbeat
        .wakeup_with_options(f.agent_id, f.issue_id, f.company_id, options)
        .await
        .expect("replayed wake");

    let wake_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(f.company_id)
    .bind(key)
    .fetch_one(&pool)
    .await
    .expect("count idempotent wakes");
    assert_eq!(wake_count, 1);
    let run_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs
          WHERE company_id = $1 AND agent_id = $2
            AND context_snapshot->>'issueId' = $3",
    )
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind(f.issue_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("count idempotent runs");
    assert_eq!(run_count, 1);

    cleanup(&f).await;
}
