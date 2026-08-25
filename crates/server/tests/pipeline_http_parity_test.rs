//! Pipeline runtime parity test (PAPERCLIP_MIGRATION_PLAN §4B.3 line 342).
//!
//! Drives the real pipeline run / trigger / metrics / logs / outputs surface
//! over HTTP against the live compile DB, replacing the former hardcoded stubs:
//!   - run lifecycle: create -> list -> get -> retry (retryOfRunId, attempt+1)
//!     -> cancel -> delete
//!   - triggers: create -> list -> delete
//!   - metrics: totalRuns / successRate / avgDuration computed from pipeline_runs
//!   - logs: run creation writes pipeline_logs rows, readable via GET /logs
//!   - outputs: GET /cases/:id/outputs returns the Paperclip aggregated shape
//!     (items: document + attachment kinds, sources, counts)

use api::routes::pipelines::pipeline_routes;
use api::routes::cases::case_routes;
use parrot_server::build_app_state;
use services::auth::{ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::util::ServiceExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
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
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.expect("read body");
    let parsed = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, parsed)
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
    actor: AuthorizationActor,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let prefix = format!("PL{}", &company_a.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Pipeline Parity Co")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");
    Fixture {
        pool: pool.clone(),
        company_a,
        actor: board_actor(Uuid::new_v4(), company_a),
    }
}

async fn cleanup(f: &Fixture) {
    sqlx::query("DELETE FROM pipeline_logs WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM pipeline_runs WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM pipeline_triggers WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM attachments WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM case_attachments WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM case_documents WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM cases WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM pipeline_case_outputs WHERE case_id IN (SELECT id FROM pipeline_cases WHERE company_id = $1)")
        .bind(f.company_a)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM pipeline_cases WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM pipelines WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await
        .ok();
}

#[tokio::test]
async fn pipeline_run_trigger_metrics_logs_lifecycle() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.expect("connect");
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = pipeline_routes().merge(case_routes()).with_state(state);

    // Create a pipeline with one stage.
    let (status, created) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/companies/{}/pipelines", f.company_a),
        Some(json!({
            "companyId": f.company_a,
            "key": "pl-main",
            "name": "Main Pipeline",
            "enforceTransitions": true,
            "stages": [{
                "key": "stage-1",
                "name": "Stage 1",
                "kind": "Open",
                "position": 1,
                "config": {}
            }]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create pipeline: {created}");
    let pipeline_id = created["id"].as_str().expect("pipeline id").to_string();

    // Create a run.
    let (status, run) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/pipelines/{pipeline_id}/runs"),
        Some(json!({ "triggerType": "manual" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create run: {run}");
    let run_id = run["runId"].as_str().expect("run id").to_string();
    assert_eq!(run["status"], "queued");

    // List runs returns the queued run.
    let (status, runs) = send(&app, &f.actor, "GET", &format!("/pipelines/{pipeline_id}/runs"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(runs.as_array().map(|a| a.len()), Some(1), "one run listed");

    // Get the run by id.
    let (status, got) = send(&app, &f.actor, "GET", &format!("/pipelines/{pipeline_id}/runs/{run_id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["status"], "queued");
    assert_eq!(got["id"], run_id);

    // Automation retry: creates a fresh run linked to the original (attempt 2).
    let (status, retried) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/pipelines/{pipeline_id}/runs/{run_id}/retry"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "retry: {retried}");
    let retry_run_id = retried["runId"].as_str().expect("retry run id").to_string();
    assert_eq!(retried["retryOfRunId"], run_id);
    assert_eq!(retried["attempt"], 2);
    let (_, retry_run) = send(&app, &f.actor, "GET", &format!("/pipelines/{pipeline_id}/runs/{retry_run_id}"), None).await;
    assert_eq!(retry_run["retryOfRunId"], run_id, "retry run must link to original");
    assert_eq!(retry_run["attempt"], 2);

    // Cancel the original run.
    let (status, cancelled) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/pipelines/{pipeline_id}/runs/{run_id}/cancel"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cancelled["cancelled"], true);

    // Metrics reflect two runs (one queued retry, one cancelled) — no success yet.
    let (status, metrics) = send(&app, &f.actor, "GET", &format!("/pipelines/{pipeline_id}/metrics"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(metrics["totalRuns"], 2);

    // Logs: run creation + retry write log lines.
    let (status, logs) = send(&app, &f.actor, "GET", &format!("/pipelines/{pipeline_id}/logs"), None).await;
    assert_eq!(status, StatusCode::OK);
    let log_messages: Vec<String> = logs
        .as_array()
        .map(|a| a.iter().filter_map(|l| l["message"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    assert!(
        log_messages.iter().any(|m| m.contains("queued")) && log_messages.iter().any(|m| m.contains("retried")),
        "logs must contain queued + retried messages, got {log_messages:?}"
    );

    // Triggers: create -> list -> delete.
    let (status, trig) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/pipelines/{pipeline_id}/triggers"),
        Some(json!({ "triggerType": "schedule", "config": { "cron": "0 * * * *" } })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create trigger: {trig}");
    let trigger_id = trig["triggerId"].as_str().expect("trigger id").to_string();
    let (status, triggers) = send(&app, &f.actor, "GET", &format!("/pipelines/{pipeline_id}/triggers"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(triggers.as_array().map(|a| a.len()), Some(1), "one trigger listed");
    assert_eq!(triggers[0]["triggerType"], "schedule");
    let (status, _) = send(&app, &f.actor, "DELETE", &format!("/pipelines/{pipeline_id}/triggers/{trigger_id}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, triggers_after) = send(&app, &f.actor, "GET", &format!("/pipelines/{pipeline_id}/triggers"), None).await;
    assert_eq!(triggers_after.as_array().map(|a| a.len()), Some(0), "trigger deleted");

    // Delete the retry run (cleanup path).
    let (status, _) = send(&app, &f.actor, "DELETE", &format!("/pipelines/{pipeline_id}/runs/{retry_run_id}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    cleanup(&f).await;
}

#[tokio::test]
async fn case_outputs_aggregates_documents_and_attachments() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.expect("connect");
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = pipeline_routes().merge(case_routes()).with_state(state);

    // Create a pipeline + a legacy `cases` row whose key matches a pipeline_cases
    // mapping, then attach a document + attachment via DB. GET /cases/:id/outputs
    // serves legacy cases (case_documents/case_attachments FK to cases(id)); the
    // pipelineId is resolved through pipeline_cases (company_id, case_key).
    let (_, created) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/companies/{}/pipelines", f.company_a),
        Some(json!({
            "companyId": f.company_a,
            "key": "pl-out",
            "name": "Output Pipeline",
            "enforceTransitions": false,
            "stages": [{
                "key": "out-stage",
                "name": "Out Stage",
                "kind": "Open",
                "position": 1,
                "config": {}
            }]
        })),
    )
    .await;
    let pipeline_id = created["id"].as_str().expect("pipeline id").to_string();
    let (status, stages) = send(&app, &f.actor, "GET", &format!("/pipelines/{pipeline_id}/stages"), None).await;
    assert_eq!(status, StatusCode::OK, "list stages: {stages}");
    let stage_id = stages[0]["id"].as_str().expect("stage id").to_string();

    // Legacy case row (cases table) carrying the same key as the pipeline mapping.
    let case_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO cases (id, company_id, case_number, identifier, case_type, key, title, status) \
         VALUES ($1, $2, 1, 'C-1', 'pipeline', 'out-case-1', 'Output Case', 'draft')",
    )
    .bind(case_id)
    .bind(f.company_a)
    .execute(&pool)
    .await
    .expect("insert case");
    sqlx::query(
        "INSERT INTO pipeline_cases (id, company_id, pipeline_id, stage_id, case_key, title) VALUES ($1, $2, $3, $4, 'out-case-1', 'Output Case')",
    )
    .bind(Uuid::new_v4())
    .bind(f.company_a)
    .bind(Uuid::parse_str(&pipeline_id).unwrap())
    .bind(Uuid::parse_str(&stage_id).unwrap())
    .execute(&pool)
    .await
    .expect("insert pipeline_cases mapping");

    // Seed a document and an attachment for the case.
    let document_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO documents (id, company_id, title, content, content_type) VALUES ($1, $2, 'Doc', 'hello world body', 'markdown')",
    )
    .bind(document_id)
    .bind(f.company_a)
    .execute(&pool)
    .await
    .expect("insert document");
    sqlx::query("INSERT INTO case_documents (id, company_id, case_id, document_id, key) VALUES ($1, $2, $3, $4, 'doc-1')")
        .bind(Uuid::new_v4())
        .bind(f.company_a)
        .bind(case_id)
        .bind(document_id)
        .execute(&pool)
        .await
        .expect("insert case_documents");

    let asset_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO attachments (id, company_id, parent_type, parent_id, asset_id, filename, content_type, size_bytes) \
         VALUES ($1, $2, 'case', $3, $4, 'report.pdf', 'application/pdf', 2048)",
    )
    .bind(asset_id)
    .bind(f.company_a)
    .bind(case_id)
    .bind(asset_id)
    .execute(&pool)
    .await
    .expect("insert attachment");
    sqlx::query("INSERT INTO case_attachments (id, company_id, case_id, asset_id) VALUES ($1, $2, $3, $4)")
        .bind(Uuid::new_v4())
        .bind(f.company_a)
        .bind(case_id)
        .bind(asset_id)
        .execute(&pool)
        .await
        .expect("insert case_attachments");

    let (status, outputs) = send(&app, &f.actor, "GET", &format!("/cases/{case_id}/outputs"), None).await;
    assert_eq!(status, StatusCode::OK, "outputs must not 500 (42P01 regression): {outputs}");
    assert_eq!(outputs["caseId"], case_id.to_string());
    assert_eq!(outputs["pipelineId"], pipeline_id, "pipelineId resolved via pipeline_cases, got: {outputs}");
    let items = outputs["items"].as_array().expect("items array");
    let kinds: Vec<&str> = items.iter().filter_map(|i| i["kind"].as_str()).collect();
    assert!(kinds.contains(&"document"), "document item present, got {kinds:?}");
    assert!(kinds.contains(&"attachment"), "attachment item present, got {kinds:?}");
    assert_eq!(outputs["counts"]["documents"], 1);
    assert_eq!(outputs["counts"]["attachments"], 1);
    assert_eq!(outputs["counts"]["workProducts"], 0);

    cleanup(&f).await;
}
