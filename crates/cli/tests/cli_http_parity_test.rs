//! CLI ↔ HTTP API parity tests.
//!
//! These exercise the real `parrot` binary (`CARGO_BIN_EXE_parrot`) against a
//! local `axum` mock server that mirrors the Parrot HTTP contract for the
//! resources the CLI drives. The goal is to prove the CLI commands map to the
//! correct routes, verbs, auth header and JSON shape — not to re-test server
//! behavior (that is covered by the Axum/PostgreSQL suites).

use std::process::Command;

use axum::{
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

fn parrot() -> String {
    env!("CARGO_BIN_EXE_parrot").to_string()
}

fn require_auth(headers: &HeaderMap) -> Result<(), StatusCode> {
    if headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.starts_with("Bearer "))
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn companies_list(headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([{"id": "c1", "name": "Acme"}])))
}

async fn companies_create(
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": "c1", "name": body.get("name").cloned().unwrap_or(Value::Null)})))
}

async fn company_get(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "name": "Acme"})))
}

async fn company_delete(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "deleted": true})))
}

async fn company_export(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "exported": true})))
}

async fn approvals_list(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([{"id": "a1", "status": "pending"}])))
}

async fn approval_get(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "status": "pending"})))
}

async fn approval_approve(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "status": "approved"})))
}

async fn approval_reject(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "status": "rejected"})))
}

async fn pipeline_get(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "stages": []})))
}

async fn pipelines_list(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([{"id": "p1"}])))
}

async fn skills_list(headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([{"name": "skill-a"}])))
}

async fn teams_catalog(headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([{"id": "t1"}])))
}

async fn plugins_list(headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([{"id": "plugin-a", "enabled": true}])))
}

async fn plugins_install(
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": "plugin-new", "source": body})))
}

async fn plugin_enable(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "enabled": true})))
}

async fn plugin_disable(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "enabled": false})))
}

async fn feedback_get(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "votes": []})))
}

async fn issue_runs(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([{"id": format!("{id}-run1")}])))
}

async fn run_get(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "status": "succeeded"})))
}

async fn workspace_get(
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"id": id, "status": "running"})))
}

async fn dashboard_get(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"cards": []})))
}

async fn cost_summary(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"totalSpend": 0})))
}

async fn activity_get(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"events": []})))
}

async fn org_chart(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!({"nodes": []})))
}

async fn channels_list(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([])))
}

async fn agents_list(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([])))
}

async fn issues_list(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([])))
}

async fn goals_list(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([])))
}

async fn projects_list(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([])))
}

async fn secrets_list(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([])))
}

async fn routines_list(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([])))
}

async fn workspaces_list(
    headers: HeaderMap,
    _p: axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers)?;
    Ok(Json(json!([])))
}

fn build_app() -> Router {
    Router::new()
        .route("/api/companies", get(companies_list).post(companies_create))
        .route("/api/companies/:id", get(company_get).delete(company_delete))
        .route("/api/companies/:id/export", get(company_export))
        .route("/api/companies/:id/approvals", get(approvals_list))
        .route("/api/companies/:id/dashboard", get(dashboard_get))
        .route("/api/companies/:id/costs/summary", get(cost_summary))
        .route("/api/companies/:id/execution-workspaces", get(workspaces_list))
        .route("/api/companies/:id/activity", get(activity_get))
        .route("/api/companies/:id/org-chart", get(org_chart))
        .route("/api/companies/:id/channels", get(channels_list))
        .route("/api/companies/:id/agents", get(agents_list))
        .route("/api/companies/:id/issues", get(issues_list))
        .route("/api/companies/:id/goals", get(goals_list))
        .route("/api/companies/:id/projects", get(projects_list))
        .route("/api/companies/:id/secrets", get(secrets_list))
        .route("/api/companies/:id/routines", get(routines_list))
        .route("/api/companies/:id/pipelines", get(pipelines_list))
        .route("/api/approvals/:id", get(approval_get))
        .route("/api/approvals/:id/approve", post(approval_approve))
        .route("/api/approvals/:id/reject", post(approval_reject))
        .route("/api/pipelines/:id", get(pipeline_get))
        .route("/api/skills/index", get(skills_list))
        .route("/api/teams/catalog", get(teams_catalog))
        .route("/api/plugins", get(plugins_list))
        .route("/api/plugins/install", post(plugins_install))
        .route("/api/plugins/:id/enable", post(plugin_enable))
        .route("/api/plugins/:id/disable", post(plugin_disable))
        .route("/api/feedback-traces/:id", get(feedback_get))
        .route("/api/issues/:id/runs", get(issue_runs))
        .route("/api/heartbeat-runs/:id", get(run_get))
        .route("/api/execution-workspaces/:id", get(workspace_get))
}

/// Start a mock server on its own OS thread with a dedicated tokio runtime so
/// its lifetime is independent of the test runtime and the spawned `parrot`
/// subprocess can reach it over loopback.
fn start_server() -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let app = build_app();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tx.send(format!("http://{addr}")).unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });
    rx.recv().unwrap()
}

fn run_cli(server_url: &str, token: &str, args: &[&str]) -> (bool, String) {
    let output = Command::new(parrot())
        .args(args)
        .env("PARROT_SERVER_URL", server_url)
        .env("PARROT_API_TOKEN", token)
        .output()
        .expect("spawn parrot");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), format!("{stdout}{stderr}"))
}

#[tokio::test]
async fn cli_maps_resource_commands_to_correct_routes() {
    let url = start_server();
    let (ok, out) = run_cli(&url, "tok", &["company", "list"]);
    assert!(ok, "company list failed: {out}");
    assert!(out.contains("\"id\": \"c1\""), "output: {out}");

    let (ok, out) = run_cli(&url, "tok", &["company", "get", "c1"]);
    assert!(ok && out.contains("\"id\": \"c1\""));

    assert!(run_cli(&url, "tok", &["agent", "list", "c1"]).0);
    assert!(run_cli(&url, "tok", &["issue", "list", "c1"]).0);

    let (ok, out) = run_cli(&url, "tok", &["approval", "list", "c1"]);
    assert!(ok && out.contains("\"id\": \"a1\""), "output: {out}");

    let (ok, out) = run_cli(&url, "tok", &["approval", "get", "a1"]);
    assert!(ok && out.contains("\"id\": \"a1\""));

    let (ok, out) = run_cli(&url, "tok", &["pipeline", "list", "c1"]);
    assert!(ok && out.contains("\"id\": \"p1\""), "output: {out}");

    let (ok, out) = run_cli(&url, "tok", &["pipeline", "get", "p1"]);
    assert!(ok && out.contains("\"id\": \"p1\""));

    let (ok, out) = run_cli(&url, "tok", &["skill", "list"]);
    assert!(ok && out.contains("skill-a"), "output: {out}");

    let (ok, out) = run_cli(&url, "tok", &["team", "catalog"]);
    assert!(ok && out.contains("\"id\": \"t1\""), "output: {out}");

    let (ok, out) = run_cli(&url, "tok", &["plugin", "list"]);
    assert!(ok && out.contains("plugin-a"), "output: {out}");

    assert!(run_cli(&url, "tok", &["dashboard", "get", "c1"]).0);
    assert!(run_cli(&url, "tok", &["cost", "summary", "c1"]).0);
    assert!(run_cli(&url, "tok", &["access", "org-chart", "c1"]).0);
    assert!(run_cli(&url, "tok", &["channel", "list", "c1"]).0);
    assert!(run_cli(&url, "tok", &["workspace", "list", "c1"]).0);

    let (ok, out) = run_cli(&url, "tok", &["run", "list", "i1"]);
    assert!(ok && out.contains("i1-run1"), "output: {out}");

    let (ok, out) = run_cli(&url, "tok", &["run", "get", "r1"]);
    assert!(ok && out.contains("\"id\": \"r1\""), "output: {out}");

    let (ok, out) = run_cli(&url, "tok", &["feedback", "get", "f1"]);
    assert!(ok, "output: {out}");
}

#[tokio::test]
async fn cli_mutation_commands_use_correct_verbs_and_bodies() {
    let url = start_server();

    let (ok, out) = run_cli(
        &url,
        "tok",
        &["company", "create", "--json", "{\"name\":\"New Co\"}"],
    );
    assert!(ok, "company create failed: {out}");
    assert!(out.contains("New Co"));

    let (ok, out) = run_cli(&url, "tok", &["company", "delete", "c1"]);
    assert!(ok && out.contains("\"deleted\": true"));

    let (ok, out) = run_cli(&url, "tok", &["company", "export", "c1"]);
    assert!(ok && out.contains("\"exported\": true"));

    let (ok, out) = run_cli(&url, "tok", &["approval", "approve", "a1", "--json", "{}"]);
    assert!(ok && out.contains("approved"), "output: {out}");

    let (ok, out) = run_cli(&url, "tok", &["approval", "reject", "a1", "--json", "{}"]);
    assert!(ok && out.contains("rejected"));

    let (ok, out) = run_cli(
        &url,
        "tok",
        &["plugin", "install", "--json", "{\"source\":\"git\"}"],
    );
    assert!(ok && out.contains("plugin-new"), "output: {out}");

    assert!(run_cli(&url, "tok", &["plugin", "enable", "plugin-a"]).0);

    let (ok, out) = run_cli(&url, "tok", &["plugin", "disable", "plugin-a"]);
    assert!(ok && out.contains("\"enabled\": false"));
}

#[tokio::test]
async fn cli_requires_auth_header_and_fails_without_token() {
    let url = start_server();
    let output = Command::new(parrot())
        .args(["company", "list"])
        .env("PARROT_SERVER_URL", &url)
        .env_remove("PARROT_API_TOKEN")
        .output()
        .expect("spawn parrot");
    assert!(!output.status.success(), "CLI must fail without auth");
}
