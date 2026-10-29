//! Exercise task-orchestration tools through the MCP gateway and the existing
//! authenticated REST routes. The fake heartbeat runs provide agent-bound
//! short-lived gateway tokens; no model or adapter process is involved.

use api::routes::{issues::issue_routes, tools::tool_routes};
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::AuthorizationActor;
use sqlx::PgPool;
use tokio::net::TcpListener;
use tower::util::ServiceExt;
use uuid::Uuid;

mod common;
use common::migrate;

struct PaperclipApiUrlGuard(Option<std::ffi::OsString>);

impl PaperclipApiUrlGuard {
    fn set(url: &str) -> Self {
        let previous = std::env::var_os("PAPERCLIP_API_URL");
        std::env::set_var("PAPERCLIP_API_URL", url);
        Self(previous)
    }
}

impl Drop for PaperclipApiUrlGuard {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("PAPERCLIP_API_URL", value),
            None => std::env::remove_var("PAPERCLIP_API_URL"),
        }
    }
}

async fn send_json(
    app: &Router,
    method: Method,
    uri: impl Into<String>,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri.into());
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    if let Some(token) = token {
        builder = builder.header("x-paperclip-tool-gateway-token", token);
    }
    let body = body
        .map(|value| Body::from(serde_json::to_vec(&value).expect("serialize request")))
        .unwrap_or_else(Body::empty);
    let request = builder.body(body).expect("build request");
    let response = app.clone().oneshot(request).await.expect("dispatch request");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .expect("read response");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn send_with_actor(
    app: &Router,
    method: Method,
    uri: impl Into<String>,
    actor: &AuthorizationActor,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri.into())
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).expect("serialize request")))
        .expect("build request");
    request.extensions_mut().insert(actor.clone());
    let response = app.clone().oneshot(request).await.expect("dispatch request");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .expect("read response");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn create_gateway_session(
    app: &Router,
    company_id: Uuid,
    agent_id: Uuid,
    run_id: Uuid,
) -> String {
    let (status, body) = send_json(
        app,
        Method::POST,
        "/tool-gateway/sessions",
        None,
        Some(json!({"companyId": company_id, "agentId": agent_id, "runId": run_id})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create session failed: {body}");
    body["token"]
        .as_str()
        .expect("gateway token")
        .to_string()
}

async fn call_tool(
    app: &Router,
    token: &str,
    tool: &str,
    parameters: Value,
) -> (StatusCode, Value) {
    send_json(
        app,
        Method::POST,
        "/tool-gateway/tools/call",
        Some(token),
        Some(json!({"tool": tool, "parameters": parameters})),
    )
    .await
}

async fn seed_agent(pool: &PgPool, company_id: Uuid, name: &str, role: &str) -> Uuid {
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents
            (id, company_id, name, role, status, adapter_type, adapter_config)
         VALUES ($1, $2, $3, $4, 'idle', 'process', '{}'::jsonb)",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind(name)
    .bind(role)
    .execute(pool)
    .await
    .expect("insert agent");
    agent_id
}

async fn seed_running_gateway_run(
    pool: &PgPool,
    company_id: Uuid,
    agent_id: Uuid,
    issue_id: Uuid,
) -> Uuid {
    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs
            (id, company_id, agent_id, status, responsible_user_id, context_snapshot)
         VALUES ($1, $2, $3, 'running', 'task-orchestration-mcp-test', $4)",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .bind(json!({"issueId": issue_id}))
    .execute(pool)
    .await
    .expect("insert running gateway test run");
    run_id
}

#[sqlx::test]
async fn task_orchestration_tools_dispatch_through_rest_and_keep_role_checks(pool: PgPool) {
    migrate(&pool).await;
    let company_id = Uuid::new_v4();
    let board_user_id = Uuid::new_v4();
    let issue_prefix = format!("OM{}", &company_id.simple().to_string()[..8]);
    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix, require_board_approval_for_new_agents)
         VALUES ($1, 'Orchestration MCP tools', $2, FALSE)",
    )
    .bind(company_id)
    .bind(issue_prefix)
    .execute(&pool)
    .await
    .expect("insert company");

    let ceo_id = seed_agent(&pool, company_id, "CEO", "ceo").await;
    let lead_id = seed_agent(&pool, company_id, "Lead", "manager").await;
    let worker_id = seed_agent(&pool, company_id, "Worker", "general").await;
    sqlx::query("UPDATE agents SET reports_to = $1 WHERE id = $2")
        .bind(ceo_id)
        .bind(lead_id)
        .execute(&pool)
        .await
        .expect("link lead to CEO");
    sqlx::query("UPDATE agents SET reports_to = $1 WHERE id = $2")
        .bind(lead_id)
        .bind(worker_id)
        .execute(&pool)
        .await
        .expect("link worker to lead");

    let state = build_app_state(pool.clone())
        .await
        .expect("build app state");
    let issue_app = Router::new()
        .merge(issue_routes())
        .with_state(state.clone());
    let board_actor = AuthorizationActor::board(board_user_id, company_id);
    // The real create route assigns and wakes the CEO. Seed the run for a
    // caller-supplied root id first so that wake coalesces with this stable
    // gateway fixture instead of inserting a second active run for the same
    // agent and issue.
    let root_issue_id = Uuid::new_v4();
    let ceo_run = seed_running_gateway_run(&pool, company_id, ceo_id, root_issue_id).await;
    let (issue_status, issue_body) = send_with_actor(
        &issue_app,
        Method::POST,
        format!("/companies/{company_id}/issues"),
        &board_actor,
        json!({"id": root_issue_id, "title": "Orchestration MCP coverage", "status": "todo"}),
    )
    .await;
    assert!(issue_status.is_success(), "create issue failed: {issue_body}");
    let issue_id = issue_body["id"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .or_else(|| {
            issue_body["issue"]["id"]
                .as_str()
                .and_then(|value| Uuid::parse_str(value).ok())
        })
        .expect("root issue id");
    assert_eq!(issue_id, root_issue_id);

    let gateway_app = tool_routes().with_state(state.clone());
    let ceo_token = create_gateway_session(&gateway_app, company_id, ceo_id, ceo_run).await;

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind API listener");
    let address = listener.local_addr().expect("API listener address");
    let server = tokio::spawn(async move {
        axum::serve(listener, api::create_router(state))
            .await
            .expect("serve API router");
    });
    let _api_url = PaperclipApiUrlGuard::set(&format!("http://{address}/api"));

    let (status, listed) = send_json(
        &gateway_app,
        Method::GET,
        "/tool-gateway/tools",
        Some(&ceo_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "tool list failed: {listed}");
    let tools = listed.as_array().expect("tool definitions");
    for name in [
        "paperclipGetTaskOrchestration",
        "paperclipSubmitTaskDecision",
        "paperclipAdjustTaskTeam",
        "paperclipSubmitTaskDeliverable",
        "paperclipReviewTaskSubtask",
        "paperclipSubmitTaskFinalReport",
    ] {
        assert!(tools.iter().any(|tool| tool["name"] == name), "missing {name}");
    }
    assert!(!tools
        .iter()
        .any(|tool| tool["name"] == "paperclipDecideTaskTeamPlan"));
    let decision_schema = tools
        .iter()
        .find(|tool| tool["name"] == "paperclipSubmitTaskDecision")
        .expect("decision tool")["inputSchema"]
        .clone();
    assert_eq!(
        decision_schema["properties"]["contract"]["properties"]["contractVersion"]["const"],
        1
    );
    assert_eq!(
        decision_schema["properties"]["contract"]["properties"]["subtasks"]["items"]["properties"]["ownerAgentId"]["format"],
        "uuid"
    );

    let (status, read) = call_tool(
        &gateway_app,
        &ceo_token,
        "paperclipGetTaskOrchestration",
        json!({"issueId": issue_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "read dispatch failed: {read}");
    assert_eq!(read["result"]["stage"], "assessing");
    assert_eq!(read["result"]["rootIssueId"], issue_id.to_string());

    let contract = json!({
        "contractVersion": 1,
        "assessment": "A worker can complete and a lead can review the deliverable.",
        "complexity": "complex",
        "rationale": "Split the work for direct review.",
        "ceoExecutes": false,
        "members": [
            {"agentId": lead_id, "action": "reuse", "role": "reviewer",
             "responsibilities": "Review the deliverable", "reason": "Direct supervisor",
             "reportsToAgentId": ceo_id},
            {"agentId": worker_id, "action": "reuse", "role": "worker",
             "responsibilities": "Produce the deliverable", "reason": "Owns the work",
             "reportsToAgentId": lead_id}
        ],
        "subtasks": [{
            "key": "deliverable",
            "title": "Prepare the deliverable",
            "description": "Prepare a concise result for review.",
            "ownerAgentId": worker_id,
            "reviewerAgentId": lead_id,
            "acceptanceCriteria": "The result is complete and clearly explained.",
            "blockedBy": [],
            "parentKey": null
        }],
        "questions": []
    });

    let (status, decision) = call_tool(
        &gateway_app,
        &ceo_token,
        "paperclipSubmitTaskDecision",
        json!({"issueId": issue_id, "contract": contract}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "decision dispatch failed: {decision}");
    let subtask_issue_id = decision["result"]["subtaskIssueIds"][0]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .expect("created subtask issue id");

    let (status, adjusted) = call_tool(
        &gateway_app,
        &ceo_token,
        "paperclipAdjustTaskTeam",
        json!({
            "issueId": issue_id,
            "members": [{
                "agentId": worker_id,
                "action": "reuse",
                "role": "worker",
                "responsibilities": "Produce the deliverable and report evidence",
                "reason": "Clarify task-scope responsibility",
                "reportsToAgentId": lead_id
            }]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "team adjustment dispatch failed: {adjusted}");

    let lead_run = seed_running_gateway_run(&pool, company_id, lead_id, issue_id).await;
    let worker_run = seed_running_gateway_run(&pool, company_id, worker_id, issue_id).await;
    let lead_token = create_gateway_session(&gateway_app, company_id, lead_id, lead_run).await;
    let worker_token = create_gateway_session(&gateway_app, company_id, worker_id, worker_run).await;

    let (status, denied_decision) = call_tool(
        &gateway_app,
        &worker_token,
        "paperclipSubmitTaskDecision",
        json!({"issueId": issue_id, "contract": contract}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "non-CEO decision: {denied_decision}");

    let (status, submission) = call_tool(
        &gateway_app,
        &worker_token,
        "paperclipSubmitTaskDeliverable",
        json!({
            "issueId": issue_id,
            "subtaskIssueId": subtask_issue_id,
            "body": "The deliverable is ready for review.",
            "evidence": {"source": "mcp integration test"}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "submission dispatch failed: {submission}");

    let (status, self_review) = call_tool(
        &gateway_app,
        &worker_token,
        "paperclipReviewTaskSubtask",
        json!({"issueId": issue_id, "subtaskIssueId": subtask_issue_id, "accepted": true}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "owner self-review: {self_review}");

    let (status, reviewed) = call_tool(
        &gateway_app,
        &lead_token,
        "paperclipReviewTaskSubtask",
        json!({
            "issueId": issue_id,
            "subtaskIssueId": subtask_issue_id,
            "accepted": true,
            "feedback": "The acceptance criteria are met."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "review dispatch failed: {reviewed}");

    let (status, denied_report) = call_tool(
        &gateway_app,
        &worker_token,
        "paperclipSubmitTaskFinalReport",
        json!({"issueId": issue_id, "body": "Unauthorized report"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "non-CEO final report: {denied_report}");

    let (status, report) = call_tool(
        &gateway_app,
        &ceo_token,
        "paperclipSubmitTaskFinalReport",
        json!({
            "issueId": issue_id,
            "body": "The required work was completed and accepted.",
            "evidence": {"deliverableId": submission["result"]["deliverableId"]}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "final report dispatch failed: {report}");
    assert_eq!(report["result"]["stage"], "completed");

    let (status, board_action) = call_tool(
        &gateway_app,
        &ceo_token,
        "paperclipApiRequest",
        json!({
            "method": "POST",
            "path": format!("/companies/{company_id}/issues/{issue_id}/orchestration/team-plan"),
            "jsonBody": "{\"approved\":true}"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "agent reached Board-only operation: {board_action}");

    let other_company_id = Uuid::new_v4();
    let (status, cross_company_read) = call_tool(
        &gateway_app,
        &ceo_token,
        "paperclipApiRequest",
        json!({
            "method": "GET",
            "path": format!("/companies/{other_company_id}/issues/{issue_id}/orchestration")
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "agent crossed company scope: {cross_company_read}"
    );

    server.abort();
}
