//! HTTP parity integration tests for Company Export (#121).
//!
//! The Paperclip export surface (routes/companies.ts `POST /:companyId/export`,
//! `POST /:companyId/exports/preview` and `GET /:companyId/export/fidelity`)
//! is exercised: export returns company metadata plus agents / projects /
//! issues / skills / routines arrays with include filtering, preview reports
//! filtered counts, and the fidelity report counts related data
//! (labels, issue labels, relations, documents, approvals, cost events,
//! activity logs). Cross-company callers get 403.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test company_export_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::companies::company_routes;
use parrot_server::build_app_state;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    let req_body = match body {
        Some(ref value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(value).expect("serialize request body"))
        }
        None => Body::empty(),
    };
    let mut req = builder.body(req_body).expect("build request");
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
    agent_a: Uuid,
    issue_a: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let agent_a = Uuid::new_v4();
    let issue_a = Uuid::new_v4();
    let project_a = Uuid::new_v4();
    let label_a = Uuid::new_v4();
    let prefix = format!("CE{}", &company_a.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix, budget_monthly_cents) VALUES ($1, $2, $3, $4)")
        .bind(company_a)
        .bind("Company Export Co")
        .bind(&prefix)
        .bind(500_000i64)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO agents (id, company_id, name, adapter_type) VALUES ($1, $2, $3, $4)")
        .bind(agent_a)
        .bind(company_a)
        .bind("Export Agent")
        .bind("codex_local")
        .execute(pool)
        .await
        .expect("insert agent");
    sqlx::query("INSERT INTO projects (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(project_a)
        .bind(company_a)
        .bind("Export Project")
        .execute(pool)
        .await
        .expect("insert project");
    sqlx::query(
        "INSERT INTO issues (id, company_id, identifier, title, status, priority) \
         VALUES ($1, $2, $3, $4, 'in_progress', 'high')",
    )
    .bind(issue_a)
    .bind(company_a)
    .bind(format!("{prefix}-1"))
    .bind("Export the company")
    .execute(pool)
    .await
    .expect("insert issue");
    sqlx::query(
        "INSERT INTO routines (id, company_id, name, title, description, agent_id, assignee_agent_id, status) \
         VALUES ($1, $2, $3, $3, $4, $5, $5, 'active')",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind("Export Routine")
    .bind("Runs the export.")
    .bind(agent_a)
    .execute(pool)
    .await
    .expect("insert routine");
    sqlx::query(
        "INSERT INTO company_skills (id, company_id, name, slug, version, status, category, install_count) \
         VALUES ($1, $2, $3, $4, '1.0.0', 'active', 'ops', 3)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind("Export Skill")
    .bind("export-skill")
    .execute(pool)
    .await
    .expect("insert skill");
    // Fidelity related rows.
    sqlx::query("INSERT INTO labels (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(label_a)
        .bind(company_a)
        .bind("release")
        .execute(pool)
        .await
        .expect("insert label");
    sqlx::query("INSERT INTO issue_labels (id, company_id, issue_id, label_id) VALUES ($1, $2, $3, $4)")
        .bind(Uuid::new_v4())
        .bind(company_a)
        .bind(issue_a)
        .bind(label_a)
        .execute(pool)
        .await
        .expect("insert issue label");
    sqlx::query(
        "INSERT INTO approvals (id, company_id, approval_type, requested_by_agent_id, status, payload) \
         VALUES ($1, $2, 'create_resource', $3, 'pending', '{}'::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind(agent_a)
    .execute(pool)
    .await
    .expect("insert approval");
    sqlx::query(
        "INSERT INTO cost_events (id, company_id, agent_id, amount_cents, event_type) \
         VALUES ($1, $2, $3, $4, 'llm_call')",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind(agent_a)
    .bind(120i64)
    .execute(pool)
    .await
    .expect("insert cost event");

    Fixture {
        pool: pool.clone(),
        company_a,
        agent_a,
        issue_a,
    }
}

async fn cleanup_fixture(f: &Fixture) {
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
        .expect("connect database for company export HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #121 company export / preview / fidelity acceptance.
#[tokio::test]
async fn company_export_preview_and_fidelity_match_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = company_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);

    // 1. Export returns company metadata plus all entity arrays and counts.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/export", f.company_a),
        Some(json!({ "format": "json" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "export → 200");
    let export = parse(&body);
    assert_eq!(export["company"]["name"], "Company Export Co");
    assert_eq!(export["company"]["budgetMonthlyCents"], 500_000);
    assert_eq!(export["counts"]["agents"], 1);
    assert_eq!(export["counts"]["projects"], 1);
    assert_eq!(export["counts"]["issues"], 1);
    assert_eq!(export["counts"]["skills"], 1);
    assert_eq!(export["counts"]["routines"], 1);
    assert_eq!(export["agents"][0]["name"], "Export Agent");
    assert_eq!(export["agents"][0]["adapterType"], "codex_local");
    assert_eq!(export["issues"][0]["identifier"], format!("CE{}", &f.company_a.simple().to_string()[..8]).to_string() + "-1");
    assert_eq!(export["routines"][0]["name"], "Export Routine");
    assert_eq!(export["skills"][0]["installCount"], 3);

    // 2. Include filtering drops the excluded surface.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/export", f.company_a),
        Some(json!({ "include": { "agents": false, "skills": false } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "filtered export → 200");
    let filtered = parse(&body);
    assert_eq!(filtered["counts"]["agents"], 0);
    assert_eq!(filtered["agents"].as_array().map(|a| a.len()).unwrap_or(9), 0);
    assert_eq!(filtered["counts"]["skills"], 0);
    assert!(
        filtered["company"]["name"].as_str().is_some(),
        "company still included by default"
    );

    // 3. Preview reports include-aware counts.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/exports/preview", f.company_a),
        Some(json!({ "include": { "issues": false } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "export preview → 200");
    let preview = parse(&body);
    assert_eq!(preview["counts"]["issues"], 0);
    assert_eq!(preview["counts"]["agents"], 1);

    // 4. Fidelity counts the related data (labels, issue labels, approvals,
    //    cost events, activity logs) alongside the core counts.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/export/fidelity", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "fidelity → 200");
    let fidelity = parse(&body);
    assert_eq!(fidelity["counts"]["core"]["agents"], 1);
    assert_eq!(fidelity["counts"]["core"]["issues"], 1);
    let relations = fidelity["counts"]["relations"].as_array().expect("relations array");
    let rel_count = |kind: &str| -> i64 {
        relations
            .iter()
            .find(|r| r["kind"] == kind)
            .map(|r| r["count"].as_i64().unwrap_or(0))
            .unwrap_or(0)
    };
    assert_eq!(rel_count("labels"), 1, "one label counted");
    assert_eq!(rel_count("issueLabels"), 1, "one issue label counted");
    assert_eq!(rel_count("approvals"), 1, "one approval counted");
    assert_eq!(rel_count("costEvents"), 1, "one cost event counted");
    assert_eq!(rel_count("activityLogs"), 0, "no activity logs in fixture");

    // 5. Cross-company boards cannot export or read fidelity (403).
    let outsider = board_actor(Uuid::new_v4(), Uuid::new_v4());
    let (status, _) = send(
        &app,
        &outsider,
        "POST",
        &format!("/companies/{}/export", f.company_a),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company export → 403");
    let (status, _) = send(
        &app,
        &outsider,
        "GET",
        &format!("/companies/{}/export/fidelity", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company fidelity → 403");

    cleanup_fixture(&f).await;
}
