//! Company-owned Claude setup-token session contract.
//!
//! The durable session boundary is implemented here even when the live Claude
//! transport is unavailable. Secrets and browser codes never enter Postgres;
//! the transport must be explicitly configured before a session can start.

use axum::{
    extract::{Extension, Json, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::{collections::HashMap, sync::{Arc, OnceLock}};
use tokio::{io::{AsyncBufReadExt, AsyncWriteExt, BufReader}, process::Command, sync::{oneshot, Mutex}};
use uuid::Uuid;

use crate::app_state::AppState;
use services::auth::AuthorizationActor;

const CLAUDE_ADAPTER: &str = "claude_local";
const SETUP_TOKEN_NOT_FOUND: &str = "SETUP_TOKEN_SESSION_NOT_FOUND";
const SETUP_TOKEN_START_FAILED: &str = "SETUP_TOKEN_START_FAILED";
const SETUP_TOKEN_TRANSPORT_ADVISORY: &str = "setup_token_transport_unverified";

struct RuntimeSession {
    stdin: Option<tokio::process::ChildStdin>,
    prompt: Option<String>,
    token: Option<String>,
    done: bool,
    kill: Option<oneshot::Sender<()>>,
}

type RuntimeStore = Arc<Mutex<HashMap<String, Arc<Mutex<RuntimeSession>>>>>;

fn runtimes() -> &'static RuntimeStore {
    static STORE: OnceLock<RuntimeStore> = OnceLock::new();
    STORE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartRequest {
    environment_id: Uuid,
    adapter_type: String,
    overwrite: Option<OverwriteRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OverwriteRequest {
    expected_secret_id: Uuid,
    expected_latest_version: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CodeRequest {
    browser_code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicSession {
    session_id: String,
    environment_id: Uuid,
    status: String,
    expires_at: Option<chrono::DateTime<Utc>>,
    failure: Option<Failure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transport_advisory: Option<TransportAdvisory>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Failure {
    reason: String,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct TransportAdvisory {
    code: &'static str,
}

pub fn setup_token_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:company_id/claude-oauth-token-status",
            get(claude_oauth_token_status),
        )
        .route(
            "/companies/:company_id/setup-token-login-sessions",
            post(start_session),
        )
        .route(
            "/companies/:company_id/setup-token-login-sessions/:session_id",
            get(get_session),
        )
        .route(
            "/companies/:company_id/setup-token-login-sessions/:session_id/prompt",
            get(get_prompt),
        )
        .route(
            "/companies/:company_id/setup-token-login-sessions/:session_id/code",
            post(submit_code),
        )
        .route(
            "/companies/:company_id/setup-token-login-sessions/:session_id/completion",
            post(completion),
        )
        .route(
            "/companies/:company_id/setup-token-login-sessions/:session_id/cancel",
            post(cancel),
        )
}

/// Crash-recovery backstop. The in-memory process map cannot survive a server
/// restart, so stale durable active rows must be closed before a new login can
/// claim the same owner/environment slot.
pub async fn reap_stale_sessions(pool: sqlx::PgPool) {
    if let Err(err) = sqlx::query("UPDATE claude_setup_token_sessions SET state = 'timed_out', failure_reason = 'server_restarted', failure_message = 'The Claude login session ended when the server restarted.', updated_at = NOW() WHERE state IN ('starting','awaiting_code','submitting','persisting')")
        .execute(&pool)
        .await
    {
        tracing::warn!(%err, "failed to reap stale Claude setup-token sessions");
    }
}

fn owner(actor: &AuthorizationActor, company_id: Uuid) -> Result<String, Response> {
    crate::routes::assert_company_access(actor, company_id, true)
        .map_err(|status| status.into_response())?;
    if !actor.is_board() {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    actor
        .principal_id()
        .map(|id| id.to_string())
        .ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())
}

fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

fn error(status: StatusCode, code: &str) -> Response {
    no_store((status, Json(json!({ "error": code }))).into_response())
}

fn session_status(state: &str, deadline: chrono::DateTime<Utc>) -> (String, Option<chrono::DateTime<Utc>>) {
    if deadline <= Utc::now() && matches!(state, "starting" | "awaiting_code" | "submitting" | "persisting") {
        ("timed_out".to_string(), Some(deadline))
    } else {
        (match state {
            "awaiting_code" | "submitting" | "persisting" => "waiting_for_user".to_string(),
            "stored" | "completed" => "authenticated".to_string(),
            other => other.to_string(),
        }, Some(deadline))
    }
}

fn extract_prompt(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|part| part.starts_with("https://") && part.contains("claude"))
        .map(|part| part.trim_matches(|c: char| matches!(c, ')' | ']' | '>' | '`' | '"' | '\'' )).to_string())
}

fn extract_token(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|part| part.contains("sk-ant-oat"))
        .map(|part| part.trim_matches(|c: char| matches!(c, ')' | ']' | '>' | '`' | '"' | '\'' | ',' )).to_string())
}

async fn spawn_runtime(session_id: &str) -> Result<(), ()> {
    let executable = std::env::var_os("PARROT_CLAUDE_SETUP_TOKEN_EXECUTABLE")
        .unwrap_or_else(|| "claude".into());
    let mut child = Command::new(executable)
        .arg("setup-token")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|err| {
            tracing::warn!(%err, "Claude setup-token executable could not start");
        })?;
    let Some(stdout) = child.stdout.take() else { return Err(()); };
    let stdin = child.stdin.take();
    let (kill_tx, mut kill_rx) = oneshot::channel();
    let runtime = Arc::new(Mutex::new(RuntimeSession {
        stdin,
        prompt: None,
        token: None,
        done: false,
        kill: Some(kill_tx),
    }));
    runtimes().lock().await.insert(session_id.to_string(), runtime.clone());

    let reader_runtime = runtime.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut current = reader_runtime.lock().await;
            if current.prompt.is_none() {
                current.prompt = extract_prompt(&line);
            }
            if current.token.is_none() {
                current.token = extract_token(&line);
            }
        }
    });

    let waiter_runtime = runtime;
    tokio::spawn(async move {
        tokio::select! {
            _ = &mut kill_rx => { let _ = child.kill().await; }
            _ = child.wait() => {}
        }
        waiter_runtime.lock().await.done = true;
    });
    Ok(())
}

async fn claude_oauth_token_status(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Response {
    let owner = match owner(&actor, company_id) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let row = sqlx::query(
        "SELECT declaration.id, declaration.latest_version FROM user_secret_declarations declaration JOIN user_secret_definitions definition ON definition.id = declaration.user_secret_definition_id WHERE declaration.company_id = $1 AND declaration.target_type = 'user' AND declaration.target_id = $2 AND declaration.value_material IS NOT NULL AND definition.key = 'claude_code_oauth_token' AND definition.deleted_at IS NULL LIMIT 1",
    )
    .bind(company_id)
    .bind(&owner)
    .fetch_optional(&state.pool)
    .await;
    let row = match row {
        Ok(Some(row)) => row,
        Ok(None) => return error(StatusCode::NOT_FOUND, SETUP_TOKEN_NOT_FOUND),
        Err(err) => {
            tracing::error!(%err, "failed to read Claude OAuth token metadata");
            return error(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR");
        }
    };
    no_store(Json(json!({
        "secretId": row.get::<Uuid, _>("id"),
        "latestVersion": row.get::<i32, _>("latest_version")
    })).into_response())
}

async fn start_session(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<StartRequest>,
) -> Response {
    let owner = match owner(&actor, company_id) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    if request.adapter_type != CLAUDE_ADAPTER {
        return error(StatusCode::BAD_REQUEST, "Only claude_local is supported.");
    }
    let environment_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM environments WHERE id = $1 AND company_id = $2 AND status = 'active')",
    )
    .bind(request.environment_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await;
    if !matches!(environment_exists, Ok(true)) {
        return error(StatusCode::NOT_FOUND, "Environment not found.");
    }

    if let Some(expected) = request.overwrite.as_ref() {
        let matches_capture = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM user_secret_declarations declaration JOIN user_secret_definitions definition ON definition.id = declaration.user_secret_definition_id WHERE declaration.id = $1 AND declaration.company_id = $2 AND declaration.target_type = 'user' AND declaration.target_id = $3 AND declaration.value_material IS NOT NULL AND declaration.latest_version = $4 AND definition.key = 'claude_code_oauth_token' AND definition.deleted_at IS NULL)",
        )
        .bind(expected.expected_secret_id)
        .bind(company_id)
        .bind(&owner)
        .bind(expected.expected_latest_version)
        .fetch_one(&state.pool)
        .await;
        if !matches!(matches_capture, Ok(true)) {
            return error(StatusCode::CONFLICT, "The Claude OAuth confirmation is stale.");
        }
    }

    let session_id = format!("parrot_{}", Uuid::new_v4().simple());
    let deadline = Utc::now() + Duration::minutes(15);
    let result = sqlx::query(
        "INSERT INTO claude_setup_token_sessions (session_id, company_id, owner_user_id, adapter_type, environment_id, state, deadline_at, expected_secret_id, expected_latest_version) VALUES ($1,$2,$3,$4,$5,'awaiting_code',$6,$7,$8)",
    )
    .bind(&session_id)
    .bind(company_id)
    .bind(&owner)
    .bind(CLAUDE_ADAPTER)
    .bind(request.environment_id)
    .bind(deadline)
    .bind(request.overwrite.as_ref().map(|value| value.expected_secret_id))
    .bind(request.overwrite.as_ref().map(|value| value.expected_latest_version))
    .execute(&state.pool)
    .await;
    if let Err(err) = result {
        tracing::error!(%err, "failed to create Claude setup-token session");
        return error(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR");
    }
    if spawn_runtime(&session_id).await.is_err() {
        let _ = sqlx::query("DELETE FROM claude_setup_token_sessions WHERE session_id = $1")
            .bind(&session_id)
            .execute(&state.pool)
            .await;
        return error(StatusCode::SERVICE_UNAVAILABLE, SETUP_TOKEN_START_FAILED);
    }
    let cleanup_state = state.clone();
    let cleanup_session_id = session_id.clone();
    tokio::spawn(async move {
        let wait_for = (deadline - Utc::now()).to_std().unwrap_or_default();
        tokio::time::sleep(wait_for).await;
        let expired = sqlx::query("UPDATE claude_setup_token_sessions SET state = 'timed_out', failure_reason = 'timed_out', failure_message = 'The Claude login session expired.', updated_at = NOW() WHERE session_id = $1 AND state IN ('starting','awaiting_code','submitting','persisting')")
            .bind(&cleanup_session_id)
            .execute(&cleanup_state.pool)
            .await
            .map(|result| result.rows_affected() == 1)
            .unwrap_or(false);
        if expired {
            if let Some(runtime) = runtimes().lock().await.remove(&cleanup_session_id) {
                if let Some(kill) = runtime.lock().await.kill.take() {
                    let _ = kill.send(());
                }
            }
        }
    });
    let body = json!({
        "sessionId": session_id,
        "environmentId": request.environment_id,
        "status": "waiting_for_user",
        "expiresAt": deadline,
        "failure": null,
        "panelMode": "submitted_browser_code",
        "prompt": null,
    });
    no_store((StatusCode::CREATED, Json(body)).into_response())
}

async fn owned_session(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    session_id: &str,
) -> Result<sqlx::postgres::PgRow, Response> {
    let owner = owner(actor, company_id)?;
    sqlx::query("SELECT * FROM claude_setup_token_sessions WHERE session_id = $1 AND company_id = $2 AND owner_user_id = $3")
        .bind(session_id)
        .bind(company_id)
        .bind(owner)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, SETUP_TOKEN_NOT_FOUND))
}

async fn get_session(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, session_id)): Path<(Uuid, String)>,
) -> Response {
    get_session_by_string(state, actor, company_id, &session_id).await
}

async fn get_session_by_string(
    state: AppState,
    actor: AuthorizationActor,
    company_id: Uuid,
    session_id: &str,
) -> Response {
    let row = match owned_session(&state, &actor, company_id, &session_id).await {
        Ok(row) => row,
        Err(response) => return response,
    };
    let (status, expires_at) = session_status(row.get("state"), row.get("deadline_at"));
    no_store(Json(PublicSession {
        session_id: row.get("session_id"),
        environment_id: row.get("environment_id"),
        status,
        expires_at,
        failure: row.get::<Option<String>, _>("failure_reason").map(|reason| Failure { reason, message: row.get("failure_message") }),
        transport_advisory: None,
    }).into_response())
}

async fn get_prompt(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, session_id)): Path<(Uuid, String)>,
) -> Response {
    let row = match owned_session(&state, &actor, company_id, &session_id).await {
        Ok(row) => row,
        Err(response) => return response,
    };
    if row.get::<chrono::DateTime<Utc>, _>("deadline_at") <= Utc::now() {
        return error(StatusCode::NOT_FOUND, SETUP_TOKEN_NOT_FOUND);
    }
    let runtime = runtimes().lock().await.get(&session_id).cloned();
    let Some(runtime) = runtime else {
        return error(StatusCode::NOT_FOUND, SETUP_TOKEN_NOT_FOUND);
    };
    let url = runtime.lock().await.prompt.clone();
    let Some(url) = url else {
        return error(StatusCode::NOT_FOUND, SETUP_TOKEN_NOT_FOUND);
    };
    no_store(Json(json!({
        "authorizationUrl": url,
        "transportAdvisory": { "code": SETUP_TOKEN_TRANSPORT_ADVISORY }
    })).into_response())
}

async fn submit_code(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, session_id)): Path<(Uuid, String)>,
    Json(request): Json<CodeRequest>,
) -> Response {
    if request.browser_code.is_empty() || request.browser_code.len() > 512 || !request.browser_code.is_ascii() || request.browser_code.chars().any(char::is_whitespace) {
        return error(StatusCode::BAD_REQUEST, "A valid browser code is required.");
    }
    let row = match owned_session(&state, &actor, company_id, &session_id).await {
        Ok(row) => row,
        Err(response) => return response,
    };
    let deadline = row.get::<chrono::DateTime<Utc>, _>("deadline_at");
    if deadline <= Utc::now() {
        return error(StatusCode::NOT_FOUND, SETUP_TOKEN_NOT_FOUND);
    }
    let runtime = runtimes().lock().await.get(&session_id).cloned();
    let Some(runtime) = runtime else {
        return error(StatusCode::SERVICE_UNAVAILABLE, SETUP_TOKEN_START_FAILED);
    };
    let mut stdin = {
        let mut current = runtime.lock().await;
        current.stdin.take()
    };
    let Some(mut stdin_handle) = stdin.take() else {
        return error(StatusCode::CONFLICT, "The Claude login session cannot accept this code.");
    };
    if stdin_handle.write_all(request.browser_code.as_bytes()).await.is_err()
        || stdin_handle.write_all(b"\n").await.is_err()
        || stdin_handle.flush().await.is_err()
    {
        return error(StatusCode::SERVICE_UNAVAILABLE, SETUP_TOKEN_START_FAILED);
    }
    runtime.lock().await.stdin = Some(stdin_handle);
    let updated = sqlx::query("UPDATE claude_setup_token_sessions SET state = 'submitting', updated_at = NOW() WHERE session_id = $1 AND company_id = $2 AND state = 'awaiting_code' AND deadline_at > NOW()")
        .bind(&session_id)
        .bind(company_id)
        .execute(&state.pool)
        .await;
    match updated {
        Ok(result) if result.rows_affected() == 1 => {
            get_session_by_string(state, actor, company_id, &session_id).await
        }
        Ok(_) => error(StatusCode::CONFLICT, "The Claude login session cannot accept this code."),
        Err(err) => { tracing::error!(%err, "failed to mark Claude setup-token session submitting"); error(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR") }
    }
}

async fn completion(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, session_id)): Path<(Uuid, String)>,
) -> Response {
    let row = match owned_session(&state, &actor, company_id, &session_id).await {
        Ok(row) => row,
        Err(response) => return response,
    };
    if row.get::<String, _>("state") == "stored" {
        return no_store(Json(json!({ "storedSessionId": session_id })).into_response());
    }
    let runtime = runtimes().lock().await.get(&session_id).cloned();
    let Some(runtime) = runtime else {
        return error(StatusCode::SERVICE_UNAVAILABLE, SETUP_TOKEN_START_FAILED);
    };
    let token = runtime.lock().await.token.clone();
    let Some(token) = token else {
        if runtime.lock().await.done {
            let _ = sqlx::query("UPDATE claude_setup_token_sessions SET state = 'failed', failure_reason = 'login_failed', failure_message = 'Claude login did not produce a credential.', updated_at = NOW() WHERE session_id = $1 AND company_id = $2 AND state NOT IN ('stored','completed','cancelled')")
                .bind(&session_id)
                .bind(company_id)
                .execute(&state.pool)
                .await;
            return error(StatusCode::BAD_REQUEST, "The Claude login session failed.");
        }
        return error(StatusCode::CONFLICT, "The Claude login session is not complete.");
    };
    let owner_id = match owner(&actor, company_id).ok().and_then(|value| Uuid::parse_str(&value).ok()) {
        Some(id) => id,
        None => return error(StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
    };
    let expected = (
        row.get::<Option<Uuid>, _>("expected_secret_id"),
        row.get::<Option<i32>, _>("expected_latest_version"),
    );
    let expected = match expected {
        (Some(secret_id), Some(version)) => Some((secret_id, version)),
        (None, None) => None,
        _ => return error(StatusCode::CONFLICT, "The Claude OAuth confirmation is invalid."),
    };
    if store_claude_token(&state, company_id, owner_id, &token, expected).await.is_err() {
        let _ = sqlx::query("UPDATE claude_setup_token_sessions SET state = 'failed', failure_reason = 'storage_failed', failure_message = 'The Claude credential could not be stored.', updated_at = NOW() WHERE session_id = $1 AND company_id = $2")
            .bind(&session_id)
            .bind(company_id)
            .execute(&state.pool)
            .await;
        return error(StatusCode::INTERNAL_SERVER_ERROR, "The Claude credential could not be stored.");
    }
    let stored = sqlx::query("UPDATE claude_setup_token_sessions SET state = 'stored', updated_at = NOW() WHERE session_id = $1 AND company_id = $2 AND state IN ('submitting','awaiting_code')")
        .bind(&session_id)
        .bind(company_id)
        .execute(&state.pool)
        .await;
    match stored {
        Ok(result) if result.rows_affected() == 1 => {
            let mut current = runtime.lock().await;
            current.token = None;
            no_store(Json(json!({ "storedSessionId": session_id })).into_response())
        }
        Ok(_) => error(StatusCode::CONFLICT, "The Claude login session is not complete."),
        Err(err) => { tracing::error!(%err, "failed to mark Claude setup-token session stored"); error(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR") }
    }
}

async fn store_claude_token(
    state: &AppState,
    company_id: Uuid,
    owner_id: Uuid,
    token: &str,
    expected: Option<(Uuid, i32)>,
) -> Result<(), ()> {
    let definitions = state.user_secret_definition_service
        .list_definitions(company_id)
        .await
        .map_err(|_| ())?;
    let definition_id = if let Some(definition) = definitions.into_iter().find(|definition| definition.key == "claude_code_oauth_token") {
        definition.id
    } else {
        let request = models::CreateUserSecretDefinitionRequest {
            key: "claude_code_oauth_token".to_string(),
            name: "Claude Code OAuth Token".to_string(),
            description: Some("Owner-bound Claude Code OAuth credential".to_string()),
            provider: "local_encrypted".to_string(),
            managed_mode: "managed".to_string(),
            usage_guidance: Some("Managed by the Claude setup-token login flow".to_string()),
        };
        state.user_secret_definition_service
            .create_definition(company_id, request)
            .await
            .map_err(|_| ())?
            .id
    };
    if let Some((secret_id, expected_version)) = expected {
        state.user_secret_service
            .rotate_user_secret_if_version(secret_id, token.to_string(), expected_version)
            .await
            .map_err(|_| ())?
            .ok_or(())?;
    } else {
        state.user_secret_service
            .set_user_secret(owner_id, definition_id, token.to_string())
            .await
            .map_err(|_| ())?;
    }
    Ok(())
}

async fn cancel(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, session_id)): Path<(Uuid, String)>,
) -> Response {
    let owner_id = match owner(&actor, company_id) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let result = sqlx::query("UPDATE claude_setup_token_sessions SET state = 'cancelled', updated_at = NOW() WHERE session_id = $1 AND company_id = $2 AND owner_user_id = $3")
        .bind(&session_id)
        .bind(company_id)
        .bind(owner_id)
        .execute(&state.pool)
        .await;
    match result {
        Ok(_) => {
            if let Some(runtime) = runtimes().lock().await.remove(&session_id) {
                if let Some(kill) = runtime.lock().await.kill.take() {
                    let _ = kill.send(());
                }
            }
            no_store(Json(json!({})).into_response())
        },
        Err(err) => { tracing::error!(%err, "failed to cancel Claude setup-token session"); error(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_parser_keeps_url_but_strips_terminal_punctuation() {
        assert_eq!(
            extract_prompt("Open https://claude.com/oauth/authorize?state=abc)"),
            Some("https://claude.com/oauth/authorize?state=abc".to_string())
        );
    }

    #[test]
    fn token_parser_only_accepts_claude_oauth_token_marker() {
        assert_eq!(extract_token("token sk-ant-oat01-example,"), Some("sk-ant-oat01-example".to_string()));
        assert_eq!(extract_token("ordinary output"), None);
    }

    #[test]
    fn active_session_deadline_projects_to_timeout() {
        let deadline = Utc::now() - Duration::seconds(1);
        assert_eq!(session_status("awaiting_code", deadline).0, "timed_out");
        assert_eq!(session_status("stored", deadline).0, "authenticated");
    }
}
