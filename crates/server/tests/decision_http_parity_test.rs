//! HTTP parity integration tests for Paperclip's Decision Retention and
//! Decision Training routes.
//!
//! These tests stand up the *real* Axum router (`decision_routes()`) wired to a
//! *real* `AppState` built by `parrot_server::build_app_state` against a live
//! PostgreSQL pool. The global auth middleware is intentionally bypassed: each
//! request injects an `AuthorizationActor` via a request extension, exactly as
//! the auth middleware would after resolving the caller. This lets us assert
//! the route behaviour (status codes, idempotency, company scoping) matches
//! Paperclip without standing up the full auth stack.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test decision_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::decisions::decision_routes;
use parrot_server::build_app_state;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

/// Send one request with `actor` injected as the `AuthorizationActor` extension
/// and return `(status, body_bytes)`.
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
    // LocalImplicit board actor: the canonical "local trusted" caller.
    AuthorizationActor::board(user_id, company_id)
}

fn session_board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    // Session-sourced board actor with an explicit membership, so that company
    // scoping is actually enforced (LocalImplicit actors bypass scoping in the
    // single-user local mode).
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
    company_b: Uuid,
    issue_a: Uuid,
    agent_a: Uuid,
    interaction_x: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let company_b = Uuid::new_v4();
    let issue_a = Uuid::new_v4();
    let agent_a = Uuid::new_v4();
    let interaction_x = Uuid::new_v4();
    let prefix_a = format!("RT{}", &company_a.simple().to_string()[..8]);
    let prefix_b = format!("RT{}", &company_b.simple().to_string()[..8]);

    for (id, name, prefix) in [
        (company_a, "HTTP Parity Company A", prefix_a),
        (company_b, "HTTP Parity Company B", prefix_b),
    ] {
        sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(name)
            .bind(prefix)
            .execute(pool)
            .await
            .expect("insert company");
    }
    sqlx::query("INSERT INTO issues (id, company_id, title, identifier) VALUES ($1, $2, $3, $4)")
        .bind(issue_a)
        .bind(company_a)
        .bind("HTTP parity issue")
        .bind(format!("{}-1", &company_a.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert issue");
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(agent_a)
        .bind(company_a)
        .bind("HTTP parity agent")
        .execute(pool)
        .await
        .expect("insert agent");
    sqlx::query(
        "INSERT INTO issue_thread_interactions
            (id, company_id, issue_id, kind, status, created_by_agent_id, payload)
         VALUES ($1, $2, $3, 'question', 'pending', $4, $5)",
    )
    .bind(interaction_x)
    .bind(company_a)
    .bind(issue_a)
    .bind(agent_a)
    .bind(json!({"question": "http parity"}))
    .execute(pool)
    .await
    .expect("insert interaction");

    Fixture {
        pool: pool.clone(),
        company_a,
        company_b,
        issue_a,
        agent_a,
        interaction_x,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM decision_triage_events WHERE company_id = ANY($1)")
        .bind(&[f.company_a, f.company_b])
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM decision_retention WHERE company_id = ANY($1)")
        .bind(&[f.company_a, f.company_b])
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM decision_training_examples WHERE company_id = ANY($1)")
        .bind(&[f.company_a, f.company_b])
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issue_thread_interactions WHERE id = $1")
        .bind(f.interaction_x)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(f.agent_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE id = $1")
        .bind(f.issue_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = ANY($1)")
        .bind(&[f.company_a, f.company_b])
        .execute(&f.pool)
        .await;
}

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database for HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

// ===========================================================================
// Decision Retention — PAPERCLIP_MIGRATION_PLAN.md line 146
// ===========================================================================

#[tokio::test]
async fn retention_http_state_machine_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone())
        .await
        .expect("build_app_state");
    let app = decision_routes().with_state(state);

    let actor_a = board_actor(Uuid::new_v4(), f.company_a);
    let actor_b = board_actor(Uuid::new_v4(), f.company_b);
    let base = format!(
        "/companies/{}/decision-retention/issue_thread_interaction/{}",
        f.company_a, f.interaction_x
    );

    // 1) keep = true → created, version 1, not archived
    let (status, body) = send(&app, &actor_a, "PATCH", &base, Some(json!({"keep": true}))).await;
    assert_eq!(status, StatusCode::OK, "keep=true should succeed");
    let kept = parse(&body);
    assert_eq!(kept["keep"], true);
    assert_eq!(kept["version"], 1);
    assert!(kept["archivedAt"].is_null(), "fresh retention is not archived");

    // 2) keep = false → toggled, version 2
    let (status, body) = send(&app, &actor_a, "PATCH", &base, Some(json!({"keep": false}))).await;
    assert_eq!(status, StatusCode::OK, "keep=false should succeed");
    let unkept = parse(&body);
    assert_eq!(unkept["keep"], false);
    assert_eq!(unkept["version"], 2);

    // 3) archive → archived, version 3, archiveVersion 1, archivedByType "user"
    let (status, body) = send(
        &app,
        &actor_a,
        "POST",
        &format!("{base}/archive"),
        Some(json!({"reason": "test archive"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "archive should succeed");
    let archived = parse(&body);
    assert!(!archived["archivedAt"].is_null(), "archived_at must be set");
    assert_eq!(archived["archivedReason"], "test archive");
    assert_eq!(archived["archivedByType"], "user");
    assert_eq!(archived["version"], 3);
    assert_eq!(archived["archiveVersion"], 1);

    // 4) repeated archive is idempotent (version + reason unchanged)
    let (status, body) = send(
        &app,
        &actor_a,
        "POST",
        &format!("{base}/archive"),
        Some(json!({"reason": "ignored"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "repeated archive should still succeed");
    let re_archived = parse(&body);
    assert_eq!(re_archived["version"], 3, "repeat archive must not bump version");
    assert_eq!(
        re_archived["archivedReason"], "test archive",
        "repeat archive must keep original reason"
    );

    // 5) revive → un-archived, version 4, archiveVersion still 1
    let (status, body) = send(&app, &actor_a, "POST", &format!("{base}/revive"), None).await;
    assert_eq!(status, StatusCode::OK, "revive should succeed");
    let revived = parse(&body);
    assert!(revived["archivedAt"].is_null(), "revived must clear archived_at");
    assert_eq!(revived["version"], 4);
    assert_eq!(revived["archiveVersion"], 1);

    // 6) repeated revive is idempotent (version unchanged)
    let (status, body) = send(&app, &actor_a, "POST", &format!("{base}/revive"), None).await;
    assert_eq!(status, StatusCode::OK, "repeated revive should still succeed");
    let re_revived = parse(&body);
    assert_eq!(re_revived["version"], 4, "repeat revive must not bump version");

    // 7) invalid source kind → 400 validation
    let (status, _) = send(
        &app,
        &actor_a,
        "PATCH",
        &format!(
            "/companies/{}/decision-retention/bogus_kind/{}",
            f.company_a, f.interaction_x
        ),
        Some(json!({"keep": true})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "invalid sourceKind → 400");

    // 8) non-existent source (valid kind) → 404
    let missing = Uuid::new_v4();
    let (status, _) = send(
        &app,
        &actor_a,
        "PATCH",
        &format!(
            "/companies/{}/decision-retention/issue_thread_interaction/{}",
            f.company_a, missing
        ),
        Some(json!({"keep": true})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "missing source → 404");

    // 9) cross-company visibility: interaction lives in company A, requested
    //    under company B (actor is a company-B board principal) → 404.
    let (status, _) = send(
        &app,
        &actor_b,
        "PATCH",
        &format!(
            "/companies/{}/decision-retention/issue_thread_interaction/{}",
            f.company_b, f.interaction_x
        ),
        Some(json!({"keep": true})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "cross-company source must be invisible");

    cleanup_fixture(&f).await;
}

// ===========================================================================
// Decision Training — PAPERCLIP_MIGRATION_PLAN.md line 150
// ===========================================================================

#[tokio::test]
async fn training_http_lifecycle_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone())
        .await
        .expect("build_app_state");
    let app = decision_routes().with_state(state);

    let owner = session_board_actor(Uuid::new_v4(), f.company_a);
    let attacker = session_board_actor(Uuid::new_v4(), f.company_b);
    let agent_actor = AuthorizationActor::agent(f.agent_a, f.company_a, None);

    // 1) create → 201 with full snapshot parity
    let create_uri = format!("/companies/{}/decision-training", f.company_a);
    let create_body = json!({
        "sourceKind": "interaction",
        "sourceId": f.interaction_x,
        "issueId": f.issue_a,
        "notes": "note1",
        "tags": ["review"],
        "qualityScore": 0.5,
    });
    let (status, body) = send(&app, &owner, "POST", &create_uri, Some(create_body)).await;
    assert_eq!(status, StatusCode::CREATED, "training create → 201");
    let created = parse(&body);
    let example_id = created["id"]
        .as_str()
        .expect("created example has id")
        .to_string();
    assert_eq!(created["sourceKind"], "interaction");
    assert_eq!(created["sourceId"], f.interaction_x.to_string());
    assert_eq!(created["issueId"], f.issue_a.to_string());
    assert_eq!(created["notes"], "note1");
    assert_eq!(created["tags"], json!(["review"]));
    assert!(
        (created["qualityScore"].as_f64().unwrap() - 0.5).abs() < 1e-6,
        "qualityScore should round-trip"
    );
    assert!(
        created["retentionPolicy"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
        "retentionPolicy should be set"
    );
    assert!(created["snapshot"].is_object(), "snapshot must be captured");

    // 1b) duplicate create (same source, same owner) → 409 conflict
    let (status, _) = send(
        &app,
        &owner,
        "POST",
        &create_uri,
        Some(json!({
            "sourceKind": "interaction",
            "sourceId": f.interaction_x,
            "issueId": f.issue_a,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "duplicate training → 409");

    // 2) list → 200 array containing the example
    let (status, body) = send(
        &app,
        &owner,
        "GET",
        &format!("/companies/{}/decision-training", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "training list → 200");
    let list = parse(&body);
    assert!(list.is_array(), "list returns an array");
    let found = list
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["example"]["id"] == example_id);
    assert!(found, "list must include the created example");

    // 3) export.jsonl → 200 ndjson, one line with state + label + retentionPolicy
    let (status, body) = send(
        &app,
        &owner,
        "GET",
        &format!("/companies/{}/decision-training/export.jsonl", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "training export → 200");
    let text = String::from_utf8(body).expect("export is utf-8");
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 1, "export must contain exactly one example line");
    let line: Value = serde_json::from_str(lines[0]).expect("export line is JSON");
    assert!(line["state"].is_object(), "export line carries state");
    assert!(line["label"].is_object(), "export line carries label");
    assert!(
        line["retentionPolicy"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
        "export line carries retentionPolicy"
    );

    // 4) get single → 200 with notes
    let (status, body) = send(
        &app,
        &owner,
        "GET",
        &format!("/decision-training/{}", example_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "training get → 200");
    let got = parse(&body);
    assert_eq!(got["id"], example_id);
    assert_eq!(got["notes"], "note1");

    // 5) update → 200, notes + tags + qualityScore merged
    let (status, body) = send(
        &app,
        &owner,
        "PATCH",
        &format!("/decision-training/{}", example_id),
        Some(json!({
            "notes": "note2",
            "tags": ["review", "urgent"],
            "qualityScore": 0.9,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "training update → 200");
    let updated = parse(&body);
    assert_eq!(updated["notes"], "note2");
    assert_eq!(updated["tags"], json!(["review", "urgent"]), "tags are sorted");
    assert!(
        (updated["qualityScore"].as_f64().unwrap() - 0.9).abs() < 1e-6,
        "qualityScore should round-trip"
    );

    // 6) cross-company get (attacker from company B) → 404
    let (status, _) = send(
        &app,
        &attacker,
        "GET",
        &format!("/decision-training/{}", example_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "cross-company training → 404");

    // 7) agent principal cannot create (human-only) → 403
    let (status, _) = send(
        &app,
        &agent_actor,
        "POST",
        &create_uri,
        Some(json!({
            "sourceKind": "interaction",
            "sourceId": f.interaction_x,
            "issueId": f.issue_a,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "agent create → 403");

    // 8) invalid sourceKind → 400
    let (status, _) = send(
        &app,
        &owner,
        "POST",
        &create_uri,
        Some(json!({
            "sourceKind": "bogus",
            "sourceId": f.interaction_x,
            "issueId": f.issue_a,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "invalid sourceKind → 400");

    // 9) delete → 204, then get → 404
    let (status, _) = send(
        &app,
        &owner,
        "DELETE",
        &format!("/decision-training/{}", example_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "training delete → 204");
    let (status, _) = send(
        &app,
        &owner,
        "GET",
        &format!("/decision-training/{}", example_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "deleted training → 404");

    cleanup_fixture(&f).await;
}
