//! Decision service acceptance test — PAPERCLIP_MIGRATION_PLAN.md line 140.
//!
//! Aligns `DecisionServiceImpl` with Paperclip's `decisionService` core lifecycle
//! against the real `decisions` table: create (open, signed, issue-linked,
//! idempotent), decide (chosen option + responsible party), cancel, status
//! machine, and pending list. Runs against the live compile DB.

use serde_json::json;
use services::decision_service::{
    CreateDecisionRequest, DecisionService, DecisionServiceImpl, DecisionStatus,
};
use sqlx::PgPool;
use uuid::Uuid;

struct Fixture {
    pool: PgPool,
    company: Uuid,
    issue: Uuid,
    agent: Uuid,
    run: Uuid,
}

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database for decision_service acceptance test");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company = Uuid::new_v4();
    let issue = Uuid::new_v4();
    let agent = Uuid::new_v4();
    let run = Uuid::new_v4();
    let prefix = format!("RT{}", &company.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company)
        .bind("Decision Service Acceptance Co")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO issues (id, company_id, title, identifier) VALUES ($1, $2, $3, $4)")
        .bind(issue)
        .bind(company)
        .bind("Decision acceptance issue")
        .bind(format!("{}-1", &company.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert issue");
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(agent)
        .bind(company)
        .bind("Decision acceptance agent")
        .execute(pool)
        .await
        .expect("insert agent");

    Fixture {
        pool: pool.clone(),
        company,
        issue,
        agent,
        run,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM decisions WHERE company_id = $1")
        .bind(f.company)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(f.agent)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE id = $1")
        .bind(f.issue)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company)
        .execute(&f.pool)
        .await;
}

fn sample_request(f: &Fixture, idempotency_key: &str) -> CreateDecisionRequest {
    CreateDecisionRequest {
        company_id: f.company,
        origin_agent_id: f.agent,
        origin_issue_id: f.issue,
        origin_run_id: f.run,
        rule_key: Some("rule-ship".to_string()),
        title: "Should we ship?".to_string(),
        body: "Decide whether to ship the feature.".to_string(),
        options: json!([
            {"id": "opt-yes", "label": "Yes"},
            {"id": "opt-no", "label": "No"}
        ]),
        inputs: None,
        expires_at: None,
        idempotency_key: Some(idempotency_key.to_string()),
        continuation_policy: Some("none".to_string()),
        metadata: Some(json!({})),
    }
}

#[tokio::test]
async fn decision_service_lifecycle_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let svc = DecisionServiceImpl::new(pool.clone());

    // --- create: open, signed, issue-linked, expiring, idempotent ---
    let d = svc
        .create_decision(sample_request(&f, "idem-ship-1"))
        .await
        .expect("create decision");

    assert_eq!(d.status, DecisionStatus::Open, "new decision is open");
    assert!(
        !d.signed_spec.is_empty(),
        "decision carries a signed spec (Paperclip signing parity)"
    );
    assert_eq!(
        d.origin_issue_id, f.issue,
        "decision is linked to its origin issue"
    );
    assert!(
        d.expires_at > chrono::Utc::now(),
        "decision has a future expiry"
    );
    assert_eq!(
        d.idempotency_key.as_deref(),
        Some("idem-ship-1"),
        "idempotency key persisted"
    );
    assert_eq!(d.continuation_policy, "none");

    // --- idempotency: same key returns the existing row, not a duplicate ---
    let dup = svc
        .create_decision(sample_request(&f, "idem-ship-1"))
        .await
        .expect("idempotent re-create");
    assert_eq!(
        dup.id, d.id,
        "repeated idempotency key returns same decision"
    );

    // --- pending list sees the open decision ---
    let pending = svc
        .list_pending_decisions(f.company)
        .await
        .expect("list pending");
    assert!(
        pending.iter().any(|x| x.id == d.id),
        "open decision appears in pending list"
    );

    // --- decide: records chosen option and responsible party ---
    svc.make_decision(d.id, "opt-yes".to_string(), Some("user-42".to_string()))
        .await
        .expect("make decision");
    let decided = svc.get_decision(d.id).await.unwrap().unwrap();
    assert_eq!(decided.status, DecisionStatus::Decided);
    assert_eq!(
        decided.chosen_option_id.as_deref(),
        Some("opt-yes"),
        "chosen option recorded"
    );
    assert_eq!(
        decided.decided_by_user_id.as_deref(),
        Some("user-42"),
        "responsible party recorded"
    );
    assert!(decided.decided_at.is_some(), "decided_at stamped");

    // --- a fresh open decision can be cancelled ---
    let d3 = svc
        .create_decision(sample_request(&f, "idem-ship-2"))
        .await
        .expect("create second decision");
    svc.cancel_decision(d3.id).await.expect("cancel decision");
    let cancelled = svc.get_decision(d3.id).await.unwrap().unwrap();
    assert_eq!(cancelled.status, DecisionStatus::Cancelled);

    // --- deciding a non-open decision is rejected ---
    let err = svc.make_decision(d3.id, "opt-yes".to_string(), None).await;
    assert!(err.is_err(), "cannot decide a cancelled decision");

    cleanup_fixture(&f).await;
}
