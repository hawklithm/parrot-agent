//! HTTP parity integration tests for user profiles (#125).
//!
//! GET /companies/:company_id/users/:user_slug/profile mirrors Paperclip
//! routes/user-profiles.ts: basic profile (masked email, canonical slug),
//! window stats (last7/last30/all), recent assigned issues and recent
//! activity. Cross-company callers get 403.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test user_profiles_http_parity_test

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
    user_a: Uuid,
    issue_a: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let user_a = Uuid::new_v4();
    let issue_a = Uuid::new_v4();
    let prefix = format!("UP{}", &company_a.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("User Profiles Co")
        .bind(&prefix)
        .execute(pool)
        .await
        .expect("insert company");
    let email = format!("alice-{}@example.com", &user_a.simple().to_string()[..8]);
    sqlx::query(
        "INSERT INTO auth_users (id, email, name, avatar_url) VALUES ($1, $2, $3, $4)",
    )
    .bind(user_a)
    .bind(&email)
    .bind("Alice")
    .bind("https://example.com/alice.png")
    .execute(pool)
    .await
    .expect("insert auth user");
    sqlx::query(
        "INSERT INTO issues (id, company_id, identifier, title, status, assignee_user_id, updated_at) \
         VALUES ($1, $2, $3, $4, 'in_progress', $5, NOW() - INTERVAL '2 days')",
    )
    .bind(issue_a)
    .bind(company_a)
    .bind(format!("{prefix}-1"))
    .bind("Alice's issue")
    .bind(user_a)
    .execute(pool)
    .await
    .expect("insert issue");
    sqlx::query(
        "INSERT INTO issues (id, company_id, identifier, title, status, assignee_user_id, updated_at) \
         VALUES ($1, $2, $3, $4, 'done', $5, NOW() - INTERVAL '10 days')",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind(format!("{prefix}-2"))
    .bind("Done issue")
    .bind(user_a)
    .execute(pool)
    .await
    .expect("insert done issue");
    sqlx::query(
        "INSERT INTO activity_logs (id, company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata) \
         VALUES ($1, $2, $3, 'user', $4, 'issue', $5, '{}'::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind("issue.updated")
    .bind(user_a)
    .bind(issue_a)
    .execute(pool)
    .await
    .expect("insert activity log");

    Fixture {
        pool: pool.clone(),
        company_a,
        user_a,
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
        .expect("connect database for user profile HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #125 user profile acceptance.
#[tokio::test]
async fn user_profile_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = company_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);
    let user_email = format!("alice-{}@example.com", &f.user_a.simple().to_string()[..8]);

    // 1. Resolve by email returns the profile with masked email and stats.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/users/{}/profile", f.company_a, user_email),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "profile → 200");
    let profile = parse(&body);
    assert_eq!(profile["canonicalSlug"], user_email);
    assert_eq!(profile["profile"]["name"], "Alice");
    assert_eq!(profile["profile"]["avatarUrl"], "https://example.com/alice.png");
    let email = profile["profile"]["email"].as_str().expect("email string");
    assert!(!email.contains(user_email.as_str()), "email is masked: {email}");
    assert!(email.contains("***"), "email masked with ***");

    // 2. Window stats: last7 has the in-progress issue (updated 2d ago) as
    //    touched + open; all time has 2 touched, 1 done.
    let stats = profile["stats"].as_array().expect("stats array");
    let by_key = |key: &str| -> &Value {
        stats
            .iter()
            .find(|s| s["key"] == key)
            .expect("window present")
    };
    let last7 = by_key("last7");
    assert_eq!(last7["touchedIssues"], 1, "one issue updated within 7 days");
    assert_eq!(last7["assignedOpenIssues"], 1, "one open assigned issue");
    assert_eq!(last7["completedIssues"], 0);
    let all_time = by_key("all");
    assert_eq!(all_time["touchedIssues"], 2, "both issues touched all time");
    assert_eq!(all_time["completedIssues"], 1, "one done issue");

    // 3. Recent issues list the assigned issues (newest first).
    let recent = profile["recentIssues"].as_array().expect("recent issues array");
    assert_eq!(recent.len(), 2, "two assigned issues");
    assert_eq!(recent[0]["title"], "Alice's issue", "newest first");

    // 4. Recent activity lists the user's activity.
    let activity = profile["recentActivity"].as_array().expect("recent activity array");
    assert_eq!(activity.len(), 1, "one activity entry");
    assert_eq!(activity[0]["eventType"], "issue.updated");

    // 5. Resolve by id also works.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/users/{}/profile", f.company_a, f.user_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "profile by id → 200");
    assert_eq!(parse(&body)["profile"]["name"], "Alice");

    // 6. Unknown user → 404.
    let (status, _) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/users/nobody@example.com/profile", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown user → 404");

    // 7. Cross-company board → 403.
    let outsider = board_actor(Uuid::new_v4(), Uuid::new_v4());
    let (status, _) = send(
        &app,
        &outsider,
        "GET",
        &format!("/companies/{}/users/{}/profile", f.company_a, user_email),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company profile → 403");

    cleanup_fixture(&f).await;
}
