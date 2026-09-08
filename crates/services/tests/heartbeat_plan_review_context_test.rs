//! Integration test: plan review context is wired into the heartbeat wake path.
//!
//! §4B.1「对齐深度规划、Plan Revision、Plan Review Context、审批与恢复路径」。
//! `plan_review_context_service` used to be an orphan module (declared in
//! `lib.rs` with no callers). This test pins the wiring into
//! `wakeup_with_options`, and pins Paperclip's inclusion rule:
//!
//! - context is built when the wake carries an interaction / comment / annotation
//!   marker, or the issue is in `planning` work mode;
//! - context is only produced when the issue actually has a `plan` document
//!   (Paperclip returns `null` without one).
//!
//! Skips when no live DB is reachable.

use services::{DefaultHeartbeatService, HeartbeatService, HeartbeatWakeupOptions};
use sqlx::PgPool;
use uuid::Uuid;

async fn connect() -> Option<PgPool> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:admin123@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    match PgPool::connect(&database_url).await {
        Ok(p) => Some(p),
        Err(_) => {
            eprintln!("Skipping heartbeat_plan_review_context test: no DATABASE_URL reachable");
            None
        }
    }
}

async fn seed(pool: &PgPool, work_mode: &str) -> (Uuid, Uuid, Uuid) {
    let company_id = Uuid::new_v4();
    let issue_prefix = format!("PR{}", &company_id.simple().to_string()[..6]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, 'PR Test Co', $2)")
        .bind(company_id)
        .bind(&issue_prefix)
        .execute(pool)
        .await
        .expect("insert company");

    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'PR Test Agent')")
        .bind(agent_id)
        .bind(company_id)
        .execute(pool)
        .await
        .expect("insert agent");

    let issue_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, priority, work_mode)
         VALUES ($1, $2, 'PR Test Issue', 'todo', 'medium', $3::issue_work_mode)",
    )
    .bind(issue_id)
    .bind(company_id)
    .bind(work_mode)
    .execute(pool)
    .await
    .expect("insert issue");

    (company_id, agent_id, issue_id)
}

/// Attach a `plan` document to the issue, which is what makes plan review
/// context producible.
async fn seed_plan_document(pool: &PgPool, company_id: Uuid, issue_id: Uuid) -> Uuid {
    let document_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO documents (id, company_id, title, content, content_type)
         VALUES ($1, $2, 'Plan', '# Plan', 'text/markdown')",
    )
    .bind(document_id)
    .bind(company_id)
    .execute(pool)
    .await
    .expect("insert document");

    sqlx::query(
        "INSERT INTO issue_documents (company_id, issue_id, document_id, key)
         VALUES ($1, $2, $3, 'plan')",
    )
    .bind(company_id)
    .bind(issue_id)
    .bind(document_id)
    .execute(pool)
    .await
    .expect("link plan document");

    document_id
}

async fn latest_snapshot(pool: &PgPool, company_id: Uuid, agent_id: Uuid) -> serde_json::Value {
    let snapshot: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT context_snapshot FROM heartbeat_runs
          WHERE company_id = $1 AND agent_id = $2
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_one(pool)
    .await
    .expect("read run context_snapshot");
    snapshot.expect("context_snapshot should be persisted")
}

async fn cleanup(pool: &PgPool, company_id: Uuid, issue_id: Uuid) {
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
    sqlx::query("DELETE FROM issue_documents WHERE issue_id = $1")
        .bind(issue_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM documents WHERE company_id = $1")
        .bind(company_id)
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

#[tokio::test]
async fn planning_wake_persists_plan_review_context() {
    let Some(pool) = connect().await else {
        return;
    };
    let (company_id, agent_id, issue_id) = seed(&pool, "planning").await;
    seed_plan_document(&pool, company_id, issue_id).await;

    let svc = DefaultHeartbeatService::new(pool.clone());
    svc.wakeup_with_options(
        agent_id,
        issue_id,
        company_id,
        HeartbeatWakeupOptions {
            source: Some("test".to_string()),
            context_snapshot: Some(serde_json::json!({ "issueId": issue_id })),
            ..Default::default()
        },
    )
    .await
    .expect("wakeup_with_options");

    let snapshot = latest_snapshot(&pool, company_id, agent_id).await;
    assert_eq!(
        snapshot.get("issueId").and_then(|v| v.as_str()),
        Some(issue_id.to_string()).as_deref(),
        "issueId must survive context enrichment"
    );
    let plan_review = snapshot
        .get("planReviewContext")
        .unwrap_or_else(|| panic!("planReviewContext missing from {snapshot}"));
    assert_eq!(
        plan_review.get("issueId").and_then(|v| v.as_str()),
        Some(issue_id.to_string()).as_deref()
    );
    // `limits` is what the 7/7 unit tests pin against Paperclip; it must survive
    // the round trip so downstream consumers see Paperclip's caps.
    let limits = plan_review
        .get("limits")
        .expect("plan review limits must be serialized");
    assert_eq!(limits.get("maxThreads").and_then(|v| v.as_u64()), Some(20));
    assert_eq!(limits.get("maxComments").and_then(|v| v.as_u64()), Some(80));

    cleanup(&pool, company_id, issue_id).await;
}

#[tokio::test]
async fn interaction_wake_without_plan_document_omits_context() {
    let Some(pool) = connect().await else {
        return;
    };
    let (company_id, agent_id, issue_id) = seed(&pool, "standard").await;
    // No plan document: Paperclip's `buildPlanReviewContext` returns null here.

    let svc = DefaultHeartbeatService::new(pool.clone());
    svc.wakeup_with_options(
        agent_id,
        issue_id,
        company_id,
        HeartbeatWakeupOptions {
            source: Some("test".to_string()),
            context_snapshot: Some(serde_json::json!({
                "issueId": issue_id,
                "interactionId": Uuid::new_v4(),
                "wakeReason": "issue_reopened_via_comment",
            })),
            ..Default::default()
        },
    )
    .await
    .expect("wakeup_with_options");

    let snapshot = latest_snapshot(&pool, company_id, agent_id).await;
    assert!(
        snapshot.get("planReviewContext").is_none(),
        "no plan document means no plan review context; got {snapshot}"
    );

    cleanup(&pool, company_id, issue_id).await;
}

#[tokio::test]
async fn standard_wake_without_markers_omits_context() {
    let Some(pool) = connect().await else {
        return;
    };
    let (company_id, agent_id, issue_id) = seed(&pool, "standard").await;
    seed_plan_document(&pool, company_id, issue_id).await;

    let svc = DefaultHeartbeatService::new(pool.clone());
    svc.wakeup_with_options(
        agent_id,
        issue_id,
        company_id,
        HeartbeatWakeupOptions {
            source: Some("test".to_string()),
            context_snapshot: Some(serde_json::json!({ "issueId": issue_id })),
            ..Default::default()
        },
    )
    .await
    .expect("wakeup_with_options");

    let snapshot = latest_snapshot(&pool, company_id, agent_id).await;
    assert!(
        snapshot.get("planReviewContext").is_none(),
        "standard work mode with no comment/annotation/interaction marker must not build context; got {snapshot}"
    );

    cleanup(&pool, company_id, issue_id).await;
}
