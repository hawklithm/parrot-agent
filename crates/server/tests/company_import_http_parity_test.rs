//! HTTP parity integration tests for Company Import (#116, #117).
//!
//! The Paperclip company import surface (routes/companies.ts
//! `/:companyId/imports/preview` and `/:companyId/imports/apply`) is
//! exercised: preview builds a real plan (create/update/skip per entity by
//! name, conflicts from the collision strategy, errors for missing names and
//! for import roots that escape the workspace), apply writes agents /
//! projects / issues in a single transaction and is idempotent under the
//! skip strategy. Cross-company callers get 403.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test company_import_http_parity_test

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
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Company Import Co")
        .bind(format!("CI{}", &company_a.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");
    Fixture {
        pool: pool.clone(),
        company_a,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM projects WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
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
        .expect("connect database for company import HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

fn import_body(entities: Value) -> Value {
    json!({
        "source": { "type": "inline", "rootPath": "imports/team-a" },
        "target": { "companyId": null },
        "entities": entities,
    })
}

/// #116 import paths + #117 preview/apply/conflict/rollback acceptance.
#[tokio::test]
async fn company_import_preview_apply_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = company_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);

    // 1. Preview of a fresh bundle: everything is create, root path is
    //    normalised, no conflicts, valid.
    let entities = json!({
        "agents": [
            { "slug": "ops-worker", "name": "Ops Worker", "adapterType": "process" },
            { "adapterType": "process" }
        ],
        "projects": [{ "slug": "core", "name": "Core" }],
        "issues": [{ "identifier": "CI-1", "title": "Ship the import", "status": "in_progress" }],
    });
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/imports/preview", f.company_a),
        Some(import_body(entities.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "preview → 200");
    let preview = parse(&body);
    assert_eq!(preview["rootPath"], "imports/team-a", "root path normalised");
    assert_eq!(preview["entityCount"], 4);
    assert_eq!(
        preview["plan"]["agentPlans"][0]["action"], "create",
        "new agent plans create"
    );
    assert_eq!(preview["plan"]["projectPlans"][0]["action"], "create");
    assert_eq!(preview["plan"]["issuePlans"][0]["action"], "create");
    assert!(
        preview["errors"].as_array().map(|a| a.len()).unwrap_or(0) >= 1,
        "entry without a name/slug/title is reported as an error"
    );
    assert_eq!(preview["valid"], false, "errors make the preview invalid");

    // 2. Apply writes the bundle in one transaction.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/imports/apply", f.company_a),
        Some(import_body(entities.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "apply → 200");
    let applied = parse(&body);
    assert_eq!(applied["applied"], true);
    assert_eq!(applied["entityCount"], 3, "valid entries applied");
    assert_eq!(applied["agents"][0]["action"], "created");
    assert_eq!(applied["projects"][0]["action"], "created");
    assert_eq!(applied["issues"][0]["action"], "created");

    // 3. Re-applying the same bundle with the default skip strategy is
    //    idempotent: everything is skipped.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/imports/apply", f.company_a),
        Some(import_body(json!({
            "agents": [{ "name": "Ops Worker" }],
            "projects": [{ "name": "Core" }],
            "issues": [{ "title": "Ship the import" }],
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "re-apply → 200");
    let second = parse(&body);
    assert_eq!(second["agents"][0]["action"], "skipped");
    assert_eq!(second["projects"][0]["action"], "skipped");
    assert_eq!(second["issues"][0]["action"], "skipped");

    // 4. Preview of the existing bundle shows conflicts under skip, and
    //    updates under the overwrite strategy.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/imports/preview", f.company_a),
        Some(import_body(json!({
            "agents": [{ "name": "Ops Worker" }],
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "conflict preview → 200");
    let conflict_preview = parse(&body);
    assert_eq!(
        conflict_preview["plan"]["agentPlans"][0]["action"], "skip",
        "skip strategy conflicts with existing"
    );
    assert!(
        !conflict_preview["conflicts"].as_array().map(|a| a.is_empty()).unwrap_or(true),
        "conflicts listed"
    );
    let mut overwrite_body = import_body(json!({ "agents": [{ "name": "Ops Worker", "adapterType": "codex_local" }] }));
    overwrite_body["collisionStrategy"] = json!("overwrite");
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/imports/preview", f.company_a),
        Some(overwrite_body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "overwrite preview → 200");
    assert_eq!(parse(&body)["plan"]["agentPlans"][0]["action"], "update");

    // 5. Apply with overwrite updates the existing agent.
    let mut apply_overwrite = import_body(json!({ "agents": [{ "name": "Ops Worker", "adapterType": "codex_local" }] }));
    apply_overwrite["collisionStrategy"] = json!("overwrite");
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/imports/apply", f.company_a),
        Some(apply_overwrite),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "overwrite apply → 200");
    assert_eq!(parse(&body)["agents"][0]["action"], "updated");
    let adapter: String = sqlx::query_scalar(
        "SELECT adapter_type FROM agents WHERE company_id = $1 AND name = 'Ops Worker'",
    )
    .bind(f.company_a)
    .fetch_one(&pool)
    .await
    .expect("read adapter type");
    assert_eq!(adapter, "codex_local", "overwrite applied the new adapter type");

    // 6. An absolute import root is rejected (400) — portable paths must
    //    stay relative to the workspace (#116).
    let (status, _) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/imports/preview", f.company_a),
        Some(json!({
            "source": { "type": "inline", "rootPath": "/etc/hosts" },
            "entities": { "agents": [] },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "absolute root → 400");
    let (status, _) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/imports/preview", f.company_a),
        Some(json!({
            "source": { "type": "inline", "rootPath": "../escape" },
            "entities": { "agents": [] },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "traversal root → 400");

    // 7. Cross-company boards cannot preview or apply (403).
    let outsider = board_actor(Uuid::new_v4(), Uuid::new_v4());
    for path in ["/imports/preview", "/imports/apply"] {
        let (status, _) = send(
            &app,
            &outsider,
            "POST",
            &format!("/companies/{}{path}", f.company_a),
            Some(import_body(json!({ "agents": [{ "name": "x" }] }))),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "cross-company {path} → 403");
    }

    cleanup_fixture(&f).await;
}
