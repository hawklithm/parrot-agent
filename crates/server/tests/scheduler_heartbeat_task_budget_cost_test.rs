//! Exercises the same scheduler heartbeat factory used by the server entry
//! point. A recovery-shaped wake must record its real adapter cost before the
//! task budget gate decides whether a later wake may start.

use axum::{
    http::StatusCode,
    routing::post,
    Router,
};
use services::{HeartbeatRecoveryJob, HeartbeatService, HeartbeatWakeupOptions, ScheduledJob};
use sqlx::PgPool;
use uuid::Uuid;

#[path = "../src/main.rs"]
mod main_entry;

mod common;

async fn fake_costing_adapter() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake adapter");
    let address = listener.local_addr().expect("fake adapter address");
    let app = Router::new().route(
        "/run",
        post(|| async {
            (
                StatusCode::OK,
                r#"{"type":"result","subtype":"success","result":"done","provider":"test-provider","model":"test-model","usage":{"input_tokens":100,"output_tokens":50},"total_cost_usd":1.25}"#,
            )
        }),
    );
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    address
}

async fn wait_for_terminal_run(
    pool: &PgPool,
    company_id: Uuid,
    issue_id: Uuid,
) -> (Uuid, String) {
    for _ in 0..120 {
        let run: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id, status::text FROM heartbeat_runs
              WHERE company_id = $1
                AND context_snapshot->>'issueId' = $2
                AND status IN ('succeeded', 'failed', 'cancelled')
              ORDER BY created_at DESC
              LIMIT 1",
        )
        .bind(company_id)
        .bind(issue_id.to_string())
        .fetch_optional(pool)
        .await
        .expect("read terminal heartbeat run");
        if let Some(run) = run {
            return run;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!("no terminal heartbeat run for {issue_id}");
}

async fn wait_for_cost_event(
    pool: &PgPool,
    company_id: Uuid,
    issue_id: Uuid,
    run_id: Uuid,
) -> (i32, i32, String) {
    for _ in 0..120 {
        let event: Option<(i32, i32, String)> = sqlx::query_as(
            "SELECT amount_cents, cost_cents, event_type FROM cost_events
              WHERE company_id = $1 AND issue_id = $2 AND heartbeat_run_id = $3",
        )
        .bind(company_id)
        .bind(issue_id)
        .bind(run_id)
        .fetch_optional(pool)
        .await
        .expect("poll cost event for terminal heartbeat run");
        if let Some(event) = event {
            return event;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    let run_diagnostic: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT status::text, error FROM heartbeat_runs
          WHERE company_id = $1 AND id = $2",
    )
    .bind(company_id)
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .expect("read heartbeat run after cost-event timeout");
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cost_events
          WHERE company_id = $1 AND issue_id = $2 AND heartbeat_run_id = $3",
    )
    .bind(company_id)
    .bind(issue_id)
    .bind(run_id)
    .fetch_one(pool)
    .await
    .expect("count cost events after timeout");
    panic!(
        "cost event for issue {issue_id}, run {run_id} did not appear within 30s; run={run_diagnostic:?}, event_count={event_count}"
    );
}

fn recovery_wake(issue_id: Uuid, idempotency_key: String) -> HeartbeatWakeupOptions {
    HeartbeatWakeupOptions {
        source: Some("automation".to_string()),
        trigger_detail: Some("recovery_action_restored".to_string()),
        reason: Some("issue_recovery_action_restored".to_string()),
        idempotency_key: Some(idempotency_key),
        context_snapshot: Some(serde_json::json!({
            "issueId": issue_id,
            "wakeReason": "issue_recovery_action_restored",
            "source": "issue.recovery_action.restored",
        })),
        ..Default::default()
    }
}

#[sqlx::test]
async fn scheduler_recovery_run_records_cost_and_blocks_at_task_budget(pool: PgPool) {
    common::migrate(&pool).await;

    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let endpoint_secret_id = Uuid::new_v4();
    let issue_prefix = format!("B{}", &company_id.simple().to_string()[..8]);
    let adapter_address = fake_costing_adapter().await;

    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix)
         VALUES ($1, 'Scheduler cost budget test', $2)",
    )
    .bind(company_id)
    .bind(&issue_prefix)
    .execute(&pool)
    .await
    .expect("insert test company");

    let (endpoint_material, endpoint_sha) = services::secret_provider::encrypt_secret_material(
        &format!("http://{adapter_address}/run"),
    )
    .expect("encrypt adapter endpoint");
    sqlx::query(
        "INSERT INTO company_secrets
            (id, company_id, scope, key, name, provider, status, managed_mode, latest_version)
         VALUES ($1, $2, 'company', 'SCHEDULER_TEST_ENDPOINT', 'Scheduler test endpoint',
                 'local_encrypted', 'active', 'paperclip_managed', 1)",
    )
    .bind(endpoint_secret_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .expect("insert adapter endpoint secret");
    sqlx::query(
        "INSERT INTO company_secret_versions
            (secret_id, version, material, value_sha256, fingerprint_sha256, status)
         VALUES ($1, 1, $2, $3, $3, 'current')",
    )
    .bind(endpoint_secret_id)
    .bind(endpoint_material)
    .bind(endpoint_sha)
    .execute(&pool)
    .await
    .expect("insert adapter endpoint secret version");

    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role, status, adapter_type, adapter_config)
         VALUES ($1, $2, 'Recovery worker', 'general', 'idle', 'http', $3)",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind(serde_json::json!({
        "url": {
            "type": "secret_ref",
            "secretId": endpoint_secret_id,
            "version": "latest",
        },
        "method": "POST",
    }))
    .execute(&pool)
    .await
    .expect("insert test agent");

    sqlx::query(
        "INSERT INTO issues
            (id, company_id, title, identifier, status, assignee_agent_id, task_budget_cents)
         VALUES ($1, $2, 'Recovery task', $3, 'todo', $4, 100)",
    )
    .bind(issue_id)
    .bind(company_id)
    .bind(format!("{issue_prefix}-1"))
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("insert budgeted root issue");

    // This factory is shared with main.rs so the test covers the actual
    // scheduler heartbeat dependency graph, not only the request heartbeat.
    let heartbeat = main_entry::build_scheduler_heartbeat(&pool);
    HeartbeatRecoveryJob::new(heartbeat.clone())
        .execute()
        .await
        .expect("execute scheduled heartbeat recovery");

    let (run_id, run_status) = wait_for_terminal_run(&pool, company_id, issue_id).await;
    assert_eq!(run_status, "succeeded");

    // `execute_run` persists terminal status before calling CostService, so
    // the terminal run alone does not prove its ledger write has settled.
    let (amount_cents, cost_cents, event_type) =
        wait_for_cost_event(&pool, company_id, issue_id, run_id).await;
    assert_eq!(amount_cents, 125);
    assert_eq!(cost_cents, 125);
    assert_eq!(event_type, "usage");

    let spent_cents = services::task_orchestration_service::task_tree_spend_cents(
        &pool,
        company_id,
        issue_id,
    )
    .await
    .expect("aggregate task tree spend");
    assert_eq!(spent_cents, 125);

    heartbeat
        .wakeup_with_options(
            agent_id,
            issue_id,
            company_id,
            recovery_wake(issue_id, format!("recovery-cost-after-cap:{issue_id}")),
        )
        .await
        .expect("budget-blocked wake is recorded as skipped");

    let (wake_status, wake_reason): (String, Option<String>) = sqlx::query_as(
        "SELECT status::text, reason FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(company_id)
    .bind(format!("recovery-cost-after-cap:{issue_id}"))
    .fetch_one(&pool)
    .await
    .expect("read skipped wake request");
    assert_eq!(wake_status, "skipped");
    assert_eq!(wake_reason.as_deref(), Some("task_budget_hard_stop"));

    let run_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs
          WHERE company_id = $1 AND context_snapshot->>'issueId' = $2",
    )
    .bind(company_id)
    .bind(issue_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("count task runs");
    assert_eq!(run_count, 1);
}
