//! HTTP parity integration tests for Paperclip's Attention feed (#94).
//!
//! End-to-end data acceptance over the real router: the board-scoped
//! `GET /companies/:company_id/attention` aggregates approvals, pending issue
//! thread interactions, open decisions and failed runs into Paperclip-shaped
//! items, resolves the interaction resolver audience, flags plan-targeted
//! interactions, and feeds `materializeSeededQueues` so seeded queues surface
//! in each item's `queues` refs.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test attention_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use sqlx::Row;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::decisions::decision_routes;
use parrot_server::build_app_state;
use services::auth::AuthorizationActor;

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
    AuthorizationActor::board(user_id, company_id)
}

struct Fixture {
    pool: PgPool,
    company_a: Uuid,
    agent_a: Uuid,
    addressee_a: Uuid,
    issue_a: Uuid,
    approval_id: Uuid,
    interaction_id: Uuid,
    decision_id: Uuid,
    run_failed: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let agent_a = Uuid::new_v4();
    let addressee_a = Uuid::new_v4();
    let issue_a = Uuid::new_v4();
    let approval_id = Uuid::new_v4();
    let interaction_id = Uuid::new_v4();
    let decision_id = Uuid::new_v4();
    let run_failed = Uuid::new_v4();
    let prefix = format!("AT{}", &company_a.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Attention Parity Co")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");

    for (id, name) in [
        (agent_a, "Origin Agent"),
        (addressee_a, "Addressee Agent"),
    ] {
        sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(company_a)
            .bind(name)
            .execute(pool)
            .await
            .expect("insert agent");
    }

    sqlx::query("INSERT INTO issues (id, company_id, title, identifier) VALUES ($1, $2, $3, $4)")
        .bind(issue_a)
        .bind(company_a)
        .bind("Attention seed issue")
        .bind(format!("{}-1", &company_a.simple().to_string()[..6]))
        .execute(pool)
        .await
        .expect("insert issue");

    // The issue carries a pull-request work product → every candidate linked
    // to it is routed into the seeded `prs` queue.
    sqlx::query(
        "INSERT INTO issue_work_products \
            (id, company_id, issue_id, name, artifact, type, provider, title, status) \
         VALUES ($1, $2, $3, $4, $5, 'pull_request', 'github', $6, 'open')",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind(issue_a)
    .bind("PR 42")
    .bind(json!({}))
    .bind("PR 42")
    .execute(pool)
    .await
    .expect("insert pull-request work product");

    // Pending approval linked to the issue.
    sqlx::query(
        "INSERT INTO approvals (id, company_id, approval_type, status, payload) \
         VALUES ($1, $2, 'create_resource', 'pending', $3)",
    )
    .bind(approval_id)
    .bind(company_a)
    .bind(json!({ "summary": "Create the rollout resource" }))
    .execute(pool)
    .await
    .expect("insert approval");
    sqlx::query(
        "INSERT INTO issue_approvals (approval_id, issue_id, company_id) VALUES ($1, $2, $3)",
    )
    .bind(approval_id)
    .bind(issue_a)
    .bind(company_a)
    .execute(pool)
    .await
    .expect("link approval to issue");

    // Pending interaction addressed to the addressee agent, targeting the
    // issue's plan document (drives the `plans` seed) with a legacy
    // board_or_agents resolver policy (canonicalizes to `anyone`).
    sqlx::query(
        "INSERT INTO issue_thread_interactions \
            (id, company_id, issue_id, kind, status, created_by_agent_id, addressee_agent_id, \
             requested_resolver_policy, effective_resolver_policy, title, payload) \
         VALUES ($1, $2, $3, 'review', 'pending', $4, $5, 'board_or_agents', 'board_or_agents', $6, $7)",
    )
    .bind(interaction_id)
    .bind(company_a)
    .bind(issue_a)
    .bind(agent_a)
    .bind(addressee_a)
    .bind("Review the rollout")
    .bind(json!({ "target": { "type": "issue_document", "key": "plan" } }))
    .execute(pool)
    .await
    .expect("insert interaction");

    // Open decision waiting for a board response.
    sqlx::query(
        "INSERT INTO decisions \
            (id, company_id, origin_agent_id, origin_issue_id, origin_run_id, \
             title, body, options, status, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'open', NOW() + INTERVAL '1 day')",
    )
    .bind(decision_id)
    .bind(company_a)
    .bind(agent_a)
    .bind(issue_a)
    .bind(Uuid::new_v4())
    .bind("Proceed with rollout?")
    .bind("Board must choose.")
    .bind(json!([{ "id": "go", "label": "Go" }]))
    .execute(pool)
    .await
    .expect("insert decision");

    // A failed heartbeat run → failed_run attention item.
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status) VALUES ($1, $2, $3, 'failed')",
    )
    .bind(run_failed)
    .bind(company_a)
    .bind(agent_a)
    .execute(pool)
    .await
    .expect("insert failed run");

    Fixture {
        pool: pool.clone(),
        company_a,
        agent_a,
        addressee_a,
        issue_a,
        approval_id,
        interaction_id,
        decision_id,
        run_failed,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM decision_triage_events WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM decision_queue_items WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM decision_queues WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM decisions WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issue_approvals WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM approvals WHERE id = $1")
        .bind(f.approval_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issue_thread_interactions WHERE id = $1")
        .bind(f.interaction_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issue_work_products WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM heartbeat_runs WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE id = $1")
        .bind(f.issue_a)
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
        .expect("connect database for attention HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// End-to-end attention feed acceptance (#94): multi-source aggregation,
/// resolver audience, plan-target flag, seeded-queue wiring, and filtering.
#[tokio::test]
async fn attention_feed_aggregates_sources_and_resolves_audience_and_seeds() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = decision_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);
    let base = format!("/companies/{}/attention", f.company_a);

    // 1. Full feed aggregates all seeded source kinds.
    let (status, body) = send(&app, &board, "GET", &base, None).await;
    if status != StatusCode::OK {
        eprintln!("NON-200 BODY: {}", String::from_utf8_lossy(&body));
    }
    assert_eq!(status, StatusCode::OK, "attention feed → 200");
    let feed = parse(&body);
    let items = feed["items"].as_array().expect("feed.items is an array");
    let kinds: Vec<&str> = items
        .iter()
        .map(|i| i["sourceKind"].as_str().unwrap_or(""))
        .collect();
    for expected in [
        "approval",
        "issue_thread_interaction",
        "decision",
        "failed_run",
    ] {
        assert!(
            kinds.contains(&expected),
            "feed aggregates {expected} (got {kinds:?})"
        );
    }

    // 2. Interaction item: plan-target flag + resolver audience parity.
    let interaction = items
        .iter()
        .find(|i| i["sourceKind"] == "issue_thread_interaction")
        .expect("interaction item present");
    assert_eq!(
        interaction["subject"]["metadata"]["isPlanTarget"], true,
        "plan-target interaction is flagged"
    );
    assert_eq!(
        interaction["subject"]["metadata"]["targetDocumentKey"], "plan"
    );
    let audience = &interaction["resolverAudience"];
    assert_eq!(
        audience["requestedResolverPolicy"], "anyone",
        "board_or_agents canonicalizes to anyone"
    );
    assert_eq!(audience["effectiveResolverPolicy"], "anyone");
    assert_eq!(audience["effectiveResolverPolicySource"], "requested");
    assert_eq!(
        audience["resolverPolicyProvenance"], "legacy_inherited_restriction",
        "legacy board-prefixed policy provenance"
    );
    assert_eq!(audience["addresseeAgentId"], f.addressee_a.to_string());
    assert_eq!(audience["addresseeName"], "Addressee Agent");
    assert_eq!(audience["createdByAgentId"], f.agent_a.to_string());
    assert_eq!(audience["createdByAgentName"], "Origin Agent");

    // 3. Seeded-queue wiring: the PR-linked issue routes approval, decision
    //    and interaction items into `prs`; the plan-target interaction also
    //    lands in `plans` (materializeSeededQueues ran during the request).
    let pr_linked = items
        .iter()
        .filter(|i| i["sourceKind"] != "failed_run")
        .collect::<Vec<_>>();
    assert!(!pr_linked.is_empty());
    for item in pr_linked {
        let queue_keys: Vec<&str> = item["queues"]
            .as_array()
            .expect("queues is an array")
            .iter()
            .filter_map(|q| q["key"].as_str())
            .collect();
        assert!(
            queue_keys.contains(&"prs"),
            "{} item queues include prs (got {queue_keys:?})",
            item["sourceKind"]
        );
    }
    let plan_queues: Vec<&str> = interaction["queues"]
        .as_array()
        .expect("interaction queues")
        .iter()
        .filter_map(|q| q["key"].as_str())
        .collect();
    assert!(
        plan_queues.contains(&"plans"),
        "plan-target interaction queues include plans (got {plan_queues:?})"
    );

    // 4. queue filter narrows the feed to the seeded queue.
    let (status, body) = send(&app, &board, "GET", &format!("{base}?queue=prs"), None).await;
    assert_eq!(status, StatusCode::OK);
    let filtered = parse(&body);
    let filtered_items = filtered["items"].as_array().expect("filtered items");
    assert!(!filtered_items.is_empty(), "queue=prs filter is not empty");
    assert!(
        filtered_items.len() < items.len(),
        "queue=prs narrows the feed"
    );
    for item in filtered_items {
        let keys: Vec<&str> = item["queues"]
            .as_array()
            .expect("queues")
            .iter()
            .filter_map(|q| q["key"].as_str())
            .collect();
        assert!(keys.contains(&"prs"), "filtered item carries prs queue");
    }

    // 5. decide sort and all-mode are accepted.
    let (status, _) = send(&app, &board, "GET", &format!("{base}?sort=decide"), None).await;
    assert_eq!(status, StatusCode::OK, "sort=decide → 200");
    let (status, _) = send(&app, &board, "GET", &format!("{base}?all=true"), None).await;
    assert_eq!(status, StatusCode::OK, "all=true → 200");

    cleanup_fixture(&f).await;
}
