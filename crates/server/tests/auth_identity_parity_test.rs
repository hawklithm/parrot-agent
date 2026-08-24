//! HTTP parity integration tests for identity consistency (plan 3.1):
//! Agent JWT, Board Session and Agent API Key must resolve to the same
//! `AuthorizationActor` semantics through the real AuthMiddleware resolver
//! chain (BearerTokenResolver + SessionCookieResolver), and behave
//! consistently on company-scoped endpoints.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test auth_identity_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::app_state::create_router;
use parrot_server::build_app_state;
use repositories::board_api_key_repository::hash_api_key;
use services::auth::jwt::create_local_agent_jwt;
use services::auth::{
    auth_cookie_prefix, ActorSource, AuthorizationActor, CompanyMembership, JwtConfig,
    MembershipRole, PrincipalType,
};

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let req = builder.body(Body::empty()).expect("build request");
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

struct Fixture {
    pool: PgPool,
    company_a: Uuid,
    agent_a: Uuid,
    user_a: Uuid,
    session_token: String,
    agent_key_token: String,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let agent_a = Uuid::new_v4();
    let user_a = Uuid::new_v4();
    let session_token = format!("sess-{}", &Uuid::new_v4().simple().to_string()[..24]);
    let agent_key_token = format!("aak_{}", &Uuid::new_v4().simple().to_string());
    let prefix = format!("AI{}", &company_a.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Identity Co")
        .bind(&prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO agents (id, company_id, name, role, status) VALUES ($1, $2, $3, 'general', 'running')")
        .bind(agent_a)
        .bind(company_a)
        .bind("Identity Agent")
        .execute(pool)
        .await
        .expect("insert agent");
    let email = format!("identity-{}@example.com", &user_a.simple().to_string()[..8]);
    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3)")
        .bind(user_a)
        .bind(&email)
        .bind("Identity User")
        .execute(pool)
        .await
        .expect("insert auth user");
    sqlx::query(
        "INSERT INTO auth_sessions (id, user_id, token, expires_at) \
         VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour')",
    )
    .bind(Uuid::new_v4())
    .bind(user_a)
    .bind(&session_token)
    .execute(pool)
    .await
    .expect("insert auth session");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role) \
         VALUES ($1, 'user', $2, 'operator')",
    )
    .bind(company_a)
    .bind(user_a)
    .execute(pool)
    .await
    .expect("insert company membership");
    sqlx::query(
        "INSERT INTO agent_api_keys (id, company_id, agent_id, key_hash, name) VALUES ($1, $2, $3, $4, 'test')",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind(agent_a)
    .bind(hash_api_key(&agent_key_token))
    .execute(pool)
    .await
    .expect("insert agent api key");

    Fixture {
        pool: pool.clone(),
        company_a,
        agent_a,
        user_a,
        session_token,
        agent_key_token,
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
        .expect("connect database for auth identity HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// Agent JWT / Board Session / Agent API Key consistency (plan 3.1).
#[tokio::test]
async fn three_identities_resolve_consistently() {
    // The auth middleware reads JWT config from the environment at
    // build_app_state time; set the same secret the test signs with.
    std::env::set_var("JWT_SECRET", "identity-test-secret-123456");
    std::env::set_var("INSTANCE_ID", "identity-test-instance");
    std::env::set_var("DEPLOYMENT_MODE", "authenticated");

    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    // companies route group carries the AuthMiddleware layer in app_state; use
    // the full /api router shape via company_routes + app_state.
    let app = create_router(state);

    // Sign an Agent JWT with the same config the middleware derives from env.
    let jwt_config = JwtConfig::new(
        "identity-test-secret-123456".to_string(),
        3600,
        "parrot-agent".to_string(),
        "agent-runtime".to_string(),
        "identity-test-instance".to_string(),
    );
    let jwt = create_local_agent_jwt(
        &jwt_config,
        f.agent_a,
        f.company_a,
        "process".to_string(),
        None,
        None,
        None,
    )
    .expect("sign agent jwt");

    // 1. Agent JWT resolves to an Agent actor (source agent_jwt).
    let (status, body) = send(
        &app,
        "GET",
        "/api/auth/get-session",
        &[("authorization", &format!("Bearer {jwt}"))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "agent jwt session → 200");
    let session = parse(&body);
    assert_eq!(session["session"]["agentId"], f.agent_a.to_string());
    assert_eq!(session["session"]["source"], "agent_jwt");
    assert!(session["user"].is_null(), "agent session has no user");

    // 2. Agent API Key resolves to an Agent actor (source agent_key).
    let (status, body) = send(
        &app,
        "GET",
        "/api/auth/get-session",
        &[("authorization", &format!("Bearer {}", f.agent_key_token))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "agent key session → 200");
    let session = parse(&body);
    assert_eq!(session["session"]["agentId"], f.agent_a.to_string());
    assert_eq!(session["session"]["source"], "agent_key");

    // 3. Board Session (cookie) resolves to a Board actor with the user.
    let cookie_name = format!("{}-session", auth_cookie_prefix("identity-test-instance"));
    let cookie_header = format!("{cookie_name}={}", f.session_token);
    let (status, body) = send(
        &app,
        "GET",
        "/api/auth/get-session",
        &[("cookie", &cookie_header)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "board session → 200");
    let session = parse(&body);
    assert_eq!(session["session"]["userId"], f.user_a.to_string());
    assert_eq!(session["user"]["id"], f.user_a.to_string());
    assert_eq!(session["user"]["name"], "Identity User");

    // 4. Unauthenticated requests resolve to no session (null).
    let (status, body) = send(&app, "GET", "/api/auth/get-session", &[]).await;
    assert_eq!(status, StatusCode::OK, "anonymous session → 200");
    assert!(parse(&body)["session"].is_null(), "anonymous session is null");

    // 5. All three identities can read the company (same company-scope Read
    //    semantics): the org tree is company-scoped and read-only.
    let org_uri = format!("/api/companies/{}/org", f.company_a);
    let (status, _) = send(&app, "GET", &org_uri, &[("authorization", &format!("Bearer {jwt}"))]).await;
    assert_eq!(status, StatusCode::OK, "agent jwt company read → 200");
    let (status, _) = send(&app, "GET", &org_uri, &[("authorization", &format!("Bearer {}", f.agent_key_token))]).await;
    assert_eq!(status, StatusCode::OK, "agent key company read → 200");
    let (status, _) = send(&app, "GET", &org_uri, &[("cookie", &cookie_header)]).await;
    assert_eq!(status, StatusCode::OK, "board session company read → 200");

    // 6. Cross-company Read is denied for the Board session (company-scope
    //    enforcement is identity-agnostic).
    let outsider_company = Uuid::new_v4();
    let (status, _) = send(
        &app,
        "GET",
        &format!("/api/companies/{outsider_company}/org"),
        &[("cookie", &cookie_header)],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company board read → 403");
    let (status, _) = send(
        &app,
        "GET",
        &format!("/api/companies/{outsider_company}/org"),
        &[("authorization", &format!("Bearer {jwt}"))],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company agent read → 403");

    cleanup_fixture(&f).await;
    std::env::remove_var("JWT_SECRET");
    std::env::remove_var("INSTANCE_ID");
    std::env::remove_var("DEPLOYMENT_MODE");
}
