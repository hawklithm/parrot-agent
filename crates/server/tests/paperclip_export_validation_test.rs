//! Paperclip export validation: verifies that exporting a company returns the expected
//! structure with correct metadata, counts, and entity arrays.
//!
//! Uses `#[sqlx::test]` for isolated DB per test case.
//!
//! L358 — 建立 Paperclip 导出数据导入 Parrot 后的行数、关联、权限和业务不变量校验。
//!
//! Run: `cargo test -p parrot-server --test paperclip_export_validation_test`

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;


mod common;
use common::migrate;


/// Seed a company with agents, projects, issues, skills, routines matching the
/// existing export test pattern (company_export_http_parity_test.rs).
async fn seed_company(pool: &PgPool) -> (Uuid, Uuid) {
    let company_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let prefix = format!("EV{}", &company_id.simple().to_string()[..6]);

    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix, budget_monthly_cents) VALUES ($1, $2, $3, $4)",
    )
    .bind(company_id)
    .bind("ExportValidationCo")
    .bind(&prefix)
    .bind(500_000i64)
    .execute(pool)
    .await
    .expect("insert company");
    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(format!("ev{}@test.com", Uuid::new_v4()))
    .bind("EV User")
    .execute(pool)
    .await
    .expect("insert user");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, 'user', $2, 'owner', 'active') ON CONFLICT DO NOTHING",
    )
    .bind(company_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("insert membership");

    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("EV Agent")
    .bind("codex_local")
    .execute(pool)
    .await
    .expect("insert agent");

    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, company_id, name) VALUES ($1, $2, $3)",
    )
    .bind(project_id)
    .bind(company_id)
    .bind("EV Project")
    .execute(pool)
    .await
    .expect("insert project");

    sqlx::query(
        "INSERT INTO issues (id, company_id, project_id, title, status, priority, identifier) VALUES ($1, $2, $3, $4, 'in_progress', 'high', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(project_id)
    .bind("EV Issue")
    .bind(format!("{}-1", prefix))
    .execute(pool)
    .await
    .expect("insert issue");

    sqlx::query(
        "INSERT INTO routines (id, company_id, name, title, description, agent_id, assignee_agent_id, status) VALUES ($1, $2, $3, $4, $5, $6, $6, 'active')",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind("EV Routine")
    .bind("Runs the export.")
    .bind("Runs the export routine.")
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("insert routine");

    sqlx::query(
        "INSERT INTO company_skills (id, company_id, name, slug, version, status, category, install_count) VALUES ($1, $2, $3, $4, '1.0.0', 'active', 'ops', 3)",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind("EV Skill")
    .bind("ev-skill")
    .execute(pool)
    .await
    .expect("insert skill");

    (company_id, user_id)
}

fn board_actor(user_id: Uuid, company_id: Uuid) -> services::auth::AuthorizationActor {
    services::auth::AuthorizationActor::board(user_id, company_id)
}

async fn send(
    app: &Router,
    a: &services::auth::AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let req_body = match body {
        Some(v) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    let mut req = builder.body(req_body).unwrap();
    req.extensions_mut().insert(a.clone());
    let resp = app.clone().oneshot(req).await.unwrap();
    let st = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let val = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (st, val)
}

#[sqlx::test]
async fn export_returns_expected_structure(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed_company(&pool).await;
    let a = board_actor(user_id, company_id);
    let app = Router::new()
        .merge(api::routes::companies::company_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    // Export the company
    let (st, export) = send(
        &app,
        &a,
        "POST",
        &format!("/companies/{}/export", company_id),
        Some(json!({ "format": "json" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "export should return 200: {:?}", export);

    // Verify top-level structure
    assert!(export.get("company").is_some(), "export should have company");
    assert!(export.get("agents").is_some(), "export should have agents");
    assert!(export.get("projects").is_some(), "export should have projects");
    assert!(export.get("issues").is_some(), "export should have issues");
    assert!(export.get("skills").is_some(), "export should have skills");
    assert!(export.get("routines").is_some(), "export should have routines");
    assert!(export.get("counts").is_some(), "export should have counts");

    // Verify company metadata
    assert_eq!(
        export["company"]["name"],
        "ExportValidationCo",
        "company name should match"
    );
    assert_eq!(
        export["company"]["budgetMonthlyCents"],
        500_000,
        "budget should be preserved"
    );

    // Verify counts match actual data
    assert_eq!(
        export["counts"]["agents"],
        1,
        "should have 1 agent"
    );
    assert_eq!(
        export["counts"]["projects"],
        1,
        "should have 1 project"
    );
    assert_eq!(
        export["counts"]["issues"],
        1,
        "should have 1 issue"
    );
    assert_eq!(
        export["counts"]["skills"],
        1,
        "should have 1 skill"
    );
    assert_eq!(
        export["counts"]["routines"],
        1,
        "should have 1 routine"
    );

    // Verify agent data
    assert_eq!(
        export["agents"][0]["name"],
        "EV Agent",
        "agent name should match"
    );
    assert_eq!(
        export["agents"][0]["adapterType"],
        "codex_local",
        "agent adapter type should match"
    );

    // Verify issue data
    assert_eq!(
        export["issues"][0]["title"],
        "EV Issue",
        "issue title should match"
    );
    assert_eq!(
        export["issues"][0]["identifier"],
        format!("EV{}-1", &company_id.simple().to_string()[..6]),
        "issue identifier should match"
    );
}

#[sqlx::test]
async fn export_issue_prefix_unique(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed_company(&pool).await;
    let a = board_actor(user_id, company_id);
    let app = Router::new()
        .merge(api::routes::companies::company_routes())
        .with_state(build_app_state(pool.clone()).await.expect("build state"));

    // Export
    let (_, export) = send(
        &app,
        &a,
        "POST",
        &format!("/companies/{}/export", company_id),
        Some(json!({ "format": "json" })),
    )
    .await;
    assert_eq!(
        export["company"]["name"],
        "ExportValidationCo",
        "export should succeed"
    );

    // Verify issue_prefix is unique in the database
    let prefixes: Vec<String> = sqlx::query_scalar(
        "SELECT issue_prefix FROM companies ORDER BY issue_prefix",
    )
    .fetch_all(&pool)
    .await
    .expect("fetch prefixes");
    let unique: std::collections::HashSet<&str> = prefixes.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        prefixes.len(),
        unique.len(),
        "duplicate issue_prefix found: {:?}",
        prefixes
    );
}

#[sqlx::test]
async fn export_fidelity_endpoint(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed_company(&pool).await;
    let a = board_actor(user_id, company_id);
    let app = Router::new()
        .merge(api::routes::companies::company_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    // Check fidelity endpoint
    let (st, fidelity) = send(
        &app,
        &a,
        "GET",
        &format!("/companies/{}/export/fidelity", company_id),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "fidelity should return 200: {:?}", fidelity);

    eprintln!("DEBUG fidelity: {:?}", fidelity);
    // Fidelity should contain relation counts
    assert!(
        fidelity.is_object(),
        "fidelity should be an object"
    );
}

