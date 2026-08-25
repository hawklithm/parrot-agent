//! Integration test: Heartbeat self-healing scheduled-retry (§4B.2 Run Liveness/
//! Continuation / Self-healing). Verifies `maybe_schedule_retry` transitions a
//! recoverable `failed` run to `scheduled_retry`, and `promote_due_scheduled_retries`
//! re-wakes due retries into a queued wakeup request. Skips when no live DB.

use services::HeartbeatService;
use sqlx::PgPool;
use uuid::Uuid;
async fn connect() -> Option<PgPool> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:admin123@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    match PgPool::connect(&database_url).await {
        Ok(p) => Some(p),
        Err(_) => {
            eprintln!("Skipping heartbeat_scheduled_retry test: no DATABASE_URL reachable");
            None
        }
    }
}

/// Insert an isolated company + agent and return (company_id, agent_id).
async fn seed(pool: &PgPool) -> (Uuid, Uuid) {
    let company_id = Uuid::new_v4();
    let issue_prefix = format!("R{}", &company_id.simple().to_string()[..6]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, 'SR Test Co', $2)")
        .bind(company_id)
        .bind(&issue_prefix)
        .execute(pool)
        .await
        .expect("insert company");
    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'SR Test Agent')")
        .bind(agent_id)
        .bind(company_id)
        .execute(pool)
        .await
        .expect("insert agent");
    (company_id, agent_id)
}

async fn cleanup(pool: &PgPool, company_id: Uuid, run_id: Uuid) {
    sqlx::query("DELETE FROM heartbeat_runs WHERE id = $1")
        .bind(run_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM agent_wakeup_requests WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM agents WHERE id IN (SELECT agent_id FROM heartbeat_runs WHERE company_id = $1)")
        .bind(company_id)
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

#[tokio::test]
async fn maybe_schedule_retry_transitions_recoverable_failure() {
    let Some(pool) = connect().await else { return; };
    let (company_id, agent_id) = seed(&pool).await;
    let issue_id = Uuid::new_v4();
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status, context_snapshot, error_code, error_family)
         VALUES ($1, $2, 'on_demand', 'failed', $3::jsonb, 'claude_transient_upstream', 'transient_upstream') RETURNING id",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(serde_json::json!({ "issueId": issue_id.to_string() }))
    .fetch_one(&pool)
    .await
    .expect("insert failed run");

    // Build a DefaultHeartbeatService via the public constructor path used by tests.
    let svc = services::DefaultHeartbeatService::new(pool.clone());

    let rescheduled = svc
        .maybe_schedule_retry(
            run_id,
            agent_id,
            issue_id,
            company_id,
            Some("claude_transient_upstream"),
            Some("transient_upstream"),
            "recoverable failure: claude_transient_upstream",
        )
        .await
        .expect("maybe_schedule_retry");
    assert!(rescheduled, "recoverable failure should be rescheduled");

    let row: (String, Option<chrono::DateTime<chrono::Utc>>, i32, Option<String>) = sqlx::query_as(
        "SELECT status::text, scheduled_retry_at, scheduled_retry_attempt, retry_of_run_id::text
         FROM heartbeat_runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .expect("read back run");
    assert_eq!(row.0, "scheduled_retry");
    assert!(row.1.is_some(), "scheduled_retry_at must be set");
    assert_eq!(row.2, 1, "first attempt");
    assert_eq!(row.3.as_deref(), Some(run_id.to_string().as_str()));

    cleanup(&pool, company_id, run_id).await;
}

#[tokio::test]
async fn maybe_schedule_retry_ignores_permanent_failure() {
    let Some(pool) = connect().await else { return; };
    let (company_id, agent_id) = seed(&pool).await;
    let issue_id = Uuid::new_v4();
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status, context_snapshot, error_code, error_family)
         VALUES ($1, $2, 'on_demand', 'failed', $3::jsonb, NULL, NULL) RETURNING id",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(serde_json::json!({ "issueId": issue_id.to_string() }))
    .fetch_one(&pool)
    .await
    .expect("insert failed run");

    let svc = services::DefaultHeartbeatService::new(pool.clone());
    let rescheduled = svc
        .maybe_schedule_retry(
            run_id,
            agent_id,
            issue_id,
            company_id,
            None,
            None,
            "business failure",
        )
        .await
        .expect("maybe_schedule_retry");
    assert!(!rescheduled, "permanent failure must NOT be rescheduled");

    let status: String = sqlx::query_scalar("SELECT status::text FROM heartbeat_runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .expect("read status");
    assert_eq!(status, "failed");

    cleanup(&pool, company_id, run_id).await;
}

#[tokio::test]
async fn promote_due_scheduled_retries_rewakes_run() {
    let Some(pool) = connect().await else { return; };
    let (company_id, agent_id) = seed(&pool).await;
    let issue_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, invocation_source, status, context_snapshot, scheduled_retry_at, scheduled_retry_attempt, retry_of_run_id)
         VALUES ($1, $2, $3, 'on_demand', 'scheduled_retry', $4::jsonb, NOW() - INTERVAL '1 minute', 2, $1)",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .bind(serde_json::json!({ "issueId": issue_id.to_string() }))
    .execute(&pool)
    .await
    .expect("insert scheduled_retry run");
    let svc = services::DefaultHeartbeatService::new(pool.clone());
    let promoted = svc
        .promote_due_scheduled_retries()
        .await
        .expect("promote_due_scheduled_retries");
    assert!(promoted >= 1, "due scheduled retry should be promoted");

    // A wakeup request for this issue must now exist. Promotion dispatches the
    // retry (status progresses queued -> dispatched -> running/completed); any
    // of these confirms the due retry was re-woken.
    let wake: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM agent_wakeup_requests WHERE company_id = $1 AND agent_id = $2 AND payload->>'issueId' = $3 AND status IN ('queued','dispatched','completed','running') LIMIT 1",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(issue_id.to_string())
    .fetch_optional(&pool)
    .await
    .expect("query wakeup");
    assert!(wake.is_some(), "promotion must enqueue/dispatch a wakeup");

    // The run's scheduled_retry_at marker is cleared after promotion.
    let still_due: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM heartbeat_runs WHERE id = $1 AND scheduled_retry_at IS NOT NULL)",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .expect("read marker");
    assert!(!still_due, "scheduled_retry_at cleared after promotion");

    cleanup(&pool, company_id, run_id).await;
}

/// Insert an isolated company + agent + issue whose `responsible_user_id` is
/// set, and return (company_id, agent_id, issue_id, responsible_user_id).
async fn seed_with_issue_responsible(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid) {
    let company_id = Uuid::new_v4();
    let issue_prefix = format!("U{}", &company_id.simple().to_string()[..6]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, 'RU Test Co', $2)")
        .bind(company_id)
        .bind(&issue_prefix)
        .execute(pool)
        .await
        .expect("insert company");
    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'RU Test Agent')")
        .bind(agent_id)
        .bind(company_id)
        .execute(pool)
        .await
        .expect("insert agent");
    let responsible_user_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, responsible_user_id) VALUES ($1, $2, 'RU Issue', $3, 'todo', $4)",
    )
    .bind(issue_id)
    .bind(company_id)
    .bind(format!("{}-1", &company_id.simple().to_string()[..8]))
    .bind(responsible_user_id)
    .execute(pool)
    .await
    .expect("insert issue");
    (company_id, agent_id, issue_id, responsible_user_id)
}

#[tokio::test]
async fn run_records_responsible_user_from_issue() {
    let Some(pool) = connect().await else { return; };
    let (company_id, agent_id, issue_id, responsible_user_id) =
        seed_with_issue_responsible(&pool).await;

    // Wake the agent for the issue; the heartbeat run must carry the issue's
    // responsible_user_id (责任用户不变量: the run is accountable to the human
    // responsible for the issue being worked).
    let svc = services::DefaultHeartbeatService::new(pool.clone());
    svc.wakeup_with_options(
        agent_id,
        issue_id,
        company_id,
        services::HeartbeatWakeupOptions {
            source: Some("test".to_string()),
            reason: Some("responsible_user_invariant".to_string()),
            idempotency_key: Some(format!("responsible_user_test:{}", issue_id)),
            context_snapshot: Some(serde_json::json!({ "issueId": issue_id })),
            ..Default::default()
        },
    )
    .await
    .expect("wakeup");

    let recorded: Option<String> = sqlx::query_scalar(
        "SELECT responsible_user_id::text FROM heartbeat_runs WHERE company_id = $1 AND agent_id = $2 AND context_snapshot->>'issueId' = $3 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(issue_id.to_string())
    .fetch_optional(&pool)
    .await
    .expect("read run");
    assert_eq!(
        recorded.as_deref(),
        Some(responsible_user_id.to_string().as_str()),
        "heartbeat run must carry the issue's responsible user"
    );

    sqlx::query("DELETE FROM heartbeat_runs WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM agent_wakeup_requests WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
}
