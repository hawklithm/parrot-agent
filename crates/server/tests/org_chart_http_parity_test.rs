//! HTTP parity integration tests for org-chart-svg (plan 4.3 org chart).
//!
//! GET /companies/:companyId/org returns the real agent hierarchy built from
//! agents.reports_to; /org-chart.svg (and the /org.svg alias) render the
//! hierarchy as an SVG image; /org.png returns a rendered PNG. All endpoints
//! enforce company access (cross-company 403).
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test org_chart_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::org_chart::org_chart_routes;
use parrot_server::build_app_state;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
) -> (StatusCode, Vec<u8>, Option<String>) {
    let mut req = Request::builder().method(method).uri(uri).body(Body::empty()).expect("build request");
    req.extensions_mut().insert(actor.clone());
    let resp = app.clone().oneshot(req).await.expect("dispatch request");
    let status = resp.status();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, bytes.to_vec(), content_type)
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
    ceo: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let ceo = Uuid::new_v4();
    let cto = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Org Chart Co")
        .bind(format!("OC{}", &company_a.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role) VALUES ($1, $2, $3, 'ceo')",
    )
    .bind(ceo)
    .bind(company_a)
    .bind("Alice CEO")
    .execute(pool)
    .await
    .expect("insert ceo agent");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role, reports_to) VALUES ($1, $2, $3, 'vp', $4)",
    )
    .bind(cto)
    .bind(company_a)
    .bind("Bob CTO")
    .bind(ceo)
    .execute(pool)
    .await
    .expect("insert cto agent");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role, reports_to) VALUES ($1, $2, $3, 'manager', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind("Carol Engineer")
    .bind(cto)
    .execute(pool)
    .await
    .expect("insert engineer agent");
    Fixture {
        pool: pool.clone(),
        company_a,
        ceo,
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
        .expect("connect database for org chart HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// org-chart-svg acceptance.
#[tokio::test]
async fn org_chart_tree_svg_and_png_match_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = org_chart_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);

    // 1. The org tree reflects the real reports_to hierarchy.
    let (status, body, _) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/org", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "org tree → 200");
    let tree: Value = serde_json::from_slice(&body).expect("org tree JSON");
    assert_eq!(tree.as_array().map(|a| a.len()).unwrap_or(0), 1, "one root");
    assert_eq!(tree[0]["name"], "Alice CEO");
    assert_eq!(tree[0]["role"], "CEO", "role label applied");
    let reports = tree[0]["reports"].as_array().expect("reports array");
    assert_eq!(reports.len(), 1, "CEO has one direct report");
    assert_eq!(reports[0]["name"], "Bob CTO");
    let inner = reports[0]["reports"].as_array().expect("inner reports");
    assert_eq!(inner.len(), 1, "CTO has one direct report");
    assert_eq!(inner[0]["name"], "Carol Engineer");

    // 2. The SVG chart is served with the image/svg+xml content type and
    //    carries the company name and the agent names.
    let (status, body, content_type) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/org-chart.svg", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "org-chart.svg → 200");
    assert_eq!(content_type.as_deref(), Some("image/svg+xml"), "svg content type");
    let svg = String::from_utf8_lossy(&body);
    assert!(svg.contains("<svg"), "svg root element");
    assert!(svg.contains("Org Chart Co"), "company name in svg");
    assert!(svg.contains("Alice CEO"), "agent name in svg");

    // 3. The /org.svg alias and the style query both work.
    let (status, body, _) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/org.svg?style=dark", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "org.svg alias → 200");
    assert!(String::from_utf8_lossy(&body).contains("<svg"));

    // 4. The PNG endpoint returns a real PNG image.
    let (status, body, content_type) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/org.png", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "org.png → 200");
    assert_eq!(content_type.as_deref(), Some("image/png"), "png content type");
    assert_eq!(&body[..4], [0x89, b'P', b'N', b'G'], "png magic bytes");

    // 5. Cross-company boards get 403 on all three surfaces.
    let outsider = board_actor(Uuid::new_v4(), Uuid::new_v4());
    for path in ["/org", "/org-chart.svg", "/org.png"] {
        let (status, _, _) = send(
            &app,
            &outsider,
            "GET",
            &format!("/companies/{}{path}", f.company_a),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "cross-company {path} → 403");
    }

    cleanup_fixture(&f).await;
}
