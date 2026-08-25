//! Integration test: Heartbeat self-healing scheduled-retry (§4B.2 Run Liveness/
//! Continuation / Self-healing). Verifies `maybe_schedule_retry` transitions a
//! recoverable `failed` run to `scheduled_retry`, and `promote_due_scheduled_retries`
//! re-wakes due retries into a queued wakeup request. Skips when no live DB.

use services::HeartbeatService;
use sqlx::PgPool;
use repositories::{
    budget_repository::{PgBudgetIncidentRepository, PgBudgetPolicyRepository},
    company_repository::CompanyRepository,
    cost_event_repository::PgCostEventRepository,
};
use std::sync::Arc;
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
    // The parent run does not retry itself; the link to it is written on the
    // promoted retry run (verified by the promotion test below).
    assert_eq!(row.3, None, "parent run must NOT self-reference retry_of_run_id");

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
         VALUES ($1, $2, $3, 'on_demand', 'scheduled_retry', $4::jsonb, NOW() - INTERVAL '1 minute', 2, NULL)",
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

    // The promoted retry run must carry retry_of_run_id = parent run id, so the
    // dashboard `recovered` counter can identify retry-succeeded runs.
    let linked: bool = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM heartbeat_runs
           WHERE company_id = $1 AND agent_id = $2 AND retry_of_run_id = $3
         )",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .expect("query promoted run link");
    assert!(linked, "promoted retry run must point retry_of_run_id at the parent run");

    // The run-continuation ledger must record the promotion (run -> parent).
    let continuation: Option<(String,)> = sqlx::query_as(
        "SELECT reason FROM run_continuations
         WHERE parent_run_id = $1 AND continuation_point = 'scheduled_retry'",
    )
    .bind(run_id)
    .fetch_optional(&pool)
    .await
    .expect("query continuation");
    assert!(
        continuation.is_some(),
        "promotion must write a run_continuations ledger row"
    );

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

/// Build a `DefaultBudgetService` over the live pool (mirrors the production
/// composition root) so the budget hard-stop path in `wakeup_with_options` can
/// consult `get_invocation_block`.
fn build_budget_service(pool: &PgPool) -> Arc<dyn services::BudgetService> {
    Arc::new(services::DefaultBudgetService::new(
        Arc::new(repositories::cost_event_repository::PgCostEventRepository::new(pool.clone())),
        Arc::new(repositories::budget_repository::PgBudgetPolicyRepository::new(pool.clone())),
        Arc::new(repositories::budget_repository::PgBudgetIncidentRepository::new(pool.clone())),
        Arc::new(repositories::company_repository::CompanyRepository::new(pool.clone())),
    ))
}

#[tokio::test]
async fn wakeup_blocked_by_budget_hard_stop() {
    let Some(pool) = connect().await else { return; };
    let (company_id, agent_id, issue_id, _responsible_user_id) =
        seed_with_issue_responsible(&pool).await;

    // Simulate a budget hard-stop at the company scope: a hard-stop policy whose
    // amount is dwarfed by observed month-to-date spend. get_invocation_block sums
    // cost_events for the company window and returns a block when observed >= amount
    // with hard_stop_enabled. This is the primary "硬停止" path Paperclip uses.
    let policy_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO budget_policies (id, company_id, scope_type, scope_id, metric, window_kind, amount, hard_stop_enabled, is_active)
         VALUES ($1, $2, 'company', $2, 'billed_cents', 'calendar_month_utc', 100, true, true)",
    )
    .bind(policy_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .expect("insert budget policy");
    // Observed spend far exceeds the 100-cent policy amount.
    sqlx::query(
        "INSERT INTO cost_events (company_id, agent_id, amount_cents, cost_cents, event_type, occurred_at)
         VALUES ($1, $2, 10000000, 10000000, 'usage', NOW())",
    )
        .bind(company_id)
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("insert cost event");

    let idempotency_key = format!("budget_hard_stop_test:{}", issue_id);
    let svc = services::DefaultHeartbeatService::new(pool.clone())
        .with_budget_service(build_budget_service(&pool));

    svc.wakeup_with_options(
        agent_id,
        issue_id,
        company_id,
        services::HeartbeatWakeupOptions {
            source: Some("test".to_string()),
            reason: Some("budget_hard_stop_invariant".to_string()),
            idempotency_key: Some(idempotency_key.clone()),
            context_snapshot: Some(serde_json::json!({ "issueId": issue_id })),
            ..Default::default()
        },
    )
    .await
    .expect("wakeup should not error even when blocked");

    // No heartbeat run may have been created — the hard stop must prevent new work.
    let run_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM heartbeat_runs WHERE company_id = $1 AND agent_id = $2 AND context_snapshot->>'issueId' = $3",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(issue_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("count runs");
    assert_eq!(run_count, 0, "budget hard-stop must not create a run");

    // The wakeup request must be recorded as skipped for the budget hard stop.
    let (status, reason, skip_reason): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT status::text, reason, payload->'heartbeatSkip'->>'reason' FROM agent_wakeup_requests WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(company_id)
    .bind(&idempotency_key)
    .fetch_one(&pool)
    .await
    .expect("read wakeup request");
    assert_eq!(status, "skipped", "blocked wakeup must be skipped");
    assert_eq!(reason.as_deref(), Some("budget_hard_stop"));
    assert_eq!(skip_reason.as_deref(), Some("budget_hard_stop"));
    // Cleanup (cost_events / budget_policies before agents/company FKs)
    sqlx::query("DELETE FROM cost_events WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM budget_policies WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();

    // Cleanup
    sqlx::query("DELETE FROM agent_wakeup_requests WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM heartbeat_runs WHERE company_id = $1")
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

/// Agent-scope budget hard-stop must persist pause_reason='budget' + paused_at on
/// the agents row (PAPERCLIP_MIGRATION_PLAN §4B.2 line 316). The columns were
/// missing from the live schema (companies/projects had them, agents did not), so
/// this UPDATE used to fail with 42703 and was silently swallowed by `let _ =`.
#[tokio::test]
async fn budget_pause_writes_agent_pause_columns() {
    let Some(pool) = connect().await else { return; };
    let (company_id, agent_id) = seed(&pool).await;

    let policy_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO budget_policies (id, company_id, scope_type, scope_id, metric, window_kind, amount, hard_stop_enabled, is_active)
         VALUES ($1, $2, 'agent', $3, 'billed_cents', 'calendar_month_utc', 100, true, true)",
    )
    .bind(policy_id)
    .bind(company_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("insert agent-scope budget policy");
    sqlx::query(
        "INSERT INTO cost_events (company_id, agent_id, amount_cents, cost_cents, event_type, occurred_at)
         VALUES ($1, $2, 10000000, 10000000, 'usage', NOW())",
    )
    .bind(company_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("insert cost event");

    let budget_service = build_budget_service(&pool);
    budget_service
        .evaluate_cost_event(company_id, agent_id, None)
        .await
        .expect("evaluate_cost_event");

    let (status, pause_reason, paused_at): (String, Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT status, pause_reason, paused_at FROM agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_one(&pool)
        .await
        .expect("read agent");
    assert_eq!(status, "paused", "agent must be paused by hard-stop enforcement");
    assert_eq!(pause_reason.as_deref(), Some("budget"), "pause_reason must be 'budget'");
    assert!(paused_at.is_some(), "paused_at must be set");

    sqlx::query("DELETE FROM cost_events WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM budget_policies WHERE company_id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(agent_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .ok();
}
