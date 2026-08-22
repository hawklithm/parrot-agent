//! HTTP parity integration tests for Paperclip's Dashboard summary (#108).
//!
//! `GET /companies/:company_id/dashboard` returns agent buckets, task buckets,
//! month costs, pending approvals, budget summary and a 14-day run activity
//! window, mirroring Paperclip `services/dashboard.ts` summary.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test dashboard_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::dashboard::dashboard_routes;
use parrot_server::build_app_state;
use services::auth::{ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType};

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("build request");
    let mut req = req;
    req.extensions_mut().insert(actor.clone());
    let resp = app.clone().oneshot(req).await.expect("dispatch request");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, bytes.to_vec())
}

fn parse(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("response body must be JSON")
}

fn board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board(user_id, company_id)
}

fn session_board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        user_id,
        company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Operator,
        )],
        false,
    )
}

struct Fixture {
    pool: PgPool,
    company_a: Uuid,
    agent_running: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let agent_idle = Uuid::new_v4();
    let agent_running = Uuid::new_v4();
    let agent_paused = Uuid::new_v4();
    let prefix = format!("DB{}", &company_a.simple().to_string()[..8]);

    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix, budget_monthly_cents) VALUES ($1, $2, $3, $4)",
    )
    .bind(company_a)
    .bind("Dashboard Parity Co")
    .bind(&prefix)
    .bind(200_000i64)
    .execute(pool)
    .await
    .expect("insert company");

    for (id, status) in [
        (agent_idle, "idle"),
        (agent_running, "running"),
        (agent_paused, "paused"),
    ] {
        sqlx::query("INSERT INTO agents (id, company_id, name, status) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(company_a)
            .bind(format!("Agent {status}"))
            .bind(status)
            .execute(pool)
            .await
            .expect("insert agent");
    }

    for (index, status) in [
        "backlog", "in_progress", "blocked", "done", "cancelled",
    ]
    .iter()
    .enumerate()
    {
        sqlx::query("INSERT INTO issues (id, company_id, title, identifier, status) VALUES ($1, $2, $3, $4, $5::issue_status)")
            .bind(Uuid::new_v4())
            .bind(company_a)
            .bind(format!("Task {status}"))
            .bind(format!("{}-{}", &prefix, index + 1))
            .bind(status)
            .execute(pool)
            .await
            .expect("insert issue");
    }

    // One pending approval.
    sqlx::query(
        "INSERT INTO approvals (id, company_id, approval_type, status, payload) \
         VALUES ($1, $2, 'create_resource', 'pending', $3)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind(json!({}))
    .execute(pool)
    .await
    .expect("insert approval");

    // A month-to-date cost event of 50_000 cents.
    sqlx::query(
        "INSERT INTO cost_events (id, agent_id, amount_cents, event_type) \
         VALUES ($1, $2, 50000, 'run')",
    )
    .bind(Uuid::new_v4())
    .bind(agent_running)
    .execute(pool)
    .await
    .expect("insert cost event");

    // Today's run activity: one succeeded, one failed.
    for status in ["succeeded", "failed"] {
        sqlx::query(
            "INSERT INTO heartbeat_runs (id, company_id, agent_id, status) VALUES ($1, $2, $3, $4::heartbeat_run_status)",
        )
        .bind(Uuid::new_v4())
        .bind(company_a)
        .bind(agent_running)
        .bind(status)
        .execute(pool)
        .await
        .expect("insert heartbeat run");
    }

    // One open budget incident (no approval link).
    let policy_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO budget_policies \
            (id, company_id, scope_type, scope_id, metric, amount) \
         VALUES ($1, $2, 'company', $3, 'billed_cents', 100000)",
    )
    .bind(policy_id)
    .bind(company_a)
    .bind(company_a)
    .execute(pool)
    .await
    .expect("insert budget policy");
    sqlx::query(
        "INSERT INTO budget_incidents \
            (id, company_id, policy_id, scope_type, scope_id, amount_limit, amount_observed, status, \
             threshold_type, window_start, window_end) \
         VALUES ($1, $2, $3, 'company', $4, 100000, 120000, 'open', 'hard', NOW(), NOW() + INTERVAL '1 day')",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind(policy_id)
    .bind(company_a)
    .execute(pool)
    .await
    .expect("insert budget incident");

    Fixture {
        pool: pool.clone(),
        company_a,
        agent_running,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM budget_incidents WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM budget_policies WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM cost_events WHERE agent_id = $1")
        .bind(f.agent_running)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM approvals WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM heartbeat_runs WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
}

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database for dashboard HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #108 dashboard summary acceptance.
#[tokio::test]
async fn dashboard_summary_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = dashboard_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);

    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/dashboard", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "dashboard → 200");
    let summary = parse(&body);

    // Agents: idle counts as active; Parrot has no agent "error" status, so
    // the error bucket is 0.
    assert_eq!(summary["agents"], json!({ "active": 1, "running": 1, "paused": 1, "error": 0 }));

    // Tasks: cancelled is excluded from open.
    assert_eq!(
        summary["tasks"],
        json!({ "open": 1, "inProgress": 1, "blocked": 1, "done": 1 })
    );

    // Costs: 50_000 of a 200_000 budget → 25%.
    assert_eq!(summary["costs"]["monthSpendCents"], 50_000);
    assert_eq!(summary["costs"]["monthBudgetCents"], 200_000);
    assert!(
        (summary["costs"]["monthUtilizationPercent"].as_f64().unwrap() - 25.0).abs() < 1e-6,
        "utilization is 25%"
    );

    assert_eq!(summary["pendingApprovals"], 1);

    // Budgets: one open incident, no pending approval.
    assert_eq!(summary["budgets"]["activeIncidents"], 1);
    assert_eq!(summary["budgets"]["pendingApprovals"], 0);
    assert_eq!(summary["budgets"]["pausedAgents"], 0);
    assert_eq!(summary["budgets"]["pausedProjects"], 0);

    // Run activity: the newest day carries today's succeeded + failed runs.
    let run_activity = summary["runActivity"].as_array().expect("runActivity array");
    assert_eq!(run_activity.len(), 14, "14-day window");
    let today = run_activity[0].clone();
    assert_eq!(today["succeeded"], 1);
    assert_eq!(today["failed"], 1);
    assert_eq!(today["total"], 2);

    // A board from another company cannot read the dashboard.
    let outsider = session_board_actor(Uuid::new_v4(), Uuid::new_v4());
    let (status, _) = send(
        &app,
        &outsider,
        "GET",
        &format!("/companies/{}/dashboard", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company dashboard → 403");

    cleanup_fixture(&f).await;
}
