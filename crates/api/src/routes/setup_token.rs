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
use uuid::Uuid;

use crate::app_state::AppState;
use services::auth::AuthorizationActor;

const CLAUDE_ADAPTER: &str = "claude_local";
const SETUP_TOKEN_NOT_FOUND: &str = "SETUP_TOKEN_SESSION_NOT_FOUND";
const SETUP_TOKEN_START_FAILED: &str = "SETUP_TOKEN_START_FAILED";

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
        "SELECT s.id, s.latest_version FROM company_secrets s JOIN user_secret_definitions d ON d.id = s.user_secret_definition_id WHERE s.company_id = $1 AND s.scope = 'user' AND s.owner_user_id = $2 AND d.key = 'claude_code_oauth_token' AND s.deleted_at IS NULL AND d.deleted_at IS NULL LIMIT 1",
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

    // Starting without a real transport would create a misleading session.
    // The transport adapter will opt in through this explicit URL while the
    // process/PTY binding is completed in the next migration slice.
    let Some(authorization_url) = std::env::var_os("PARROT_CLAUDE_SETUP_TOKEN_AUTHORIZATION_URL") else {
        return error(StatusCode::SERVICE_UNAVAILABLE, SETUP_TOKEN_START_FAILED);
    };
    let authorization_url = authorization_url.to_string_lossy().trim().to_string();
    if authorization_url.is_empty() {
        return error(StatusCode::SERVICE_UNAVAILABLE, SETUP_TOKEN_START_FAILED);
    }

    let session_id = format!("parrot_{}", Uuid::new_v4().simple());
    let deadline = Utc::now() + Duration::minutes(15);
    let result = sqlx::query(
        "INSERT INTO claude_setup_token_sessions (session_id, company_id, owner_user_id, adapter_type, environment_id, state, deadline_at) VALUES ($1,$2,$3,$4,$5,'awaiting_code',$6)",
    )
    .bind(&session_id)
    .bind(company_id)
    .bind(&owner)
    .bind(CLAUDE_ADAPTER)
    .bind(request.environment_id)
    .bind(deadline)
    .execute(&state.pool)
    .await;
    if let Err(err) = result {
        tracing::error!(%err, "failed to create Claude setup-token session");
        return error(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR");
    }
    let body = json!({
        "sessionId": session_id,
        "environmentId": request.environment_id,
        "status": "waiting_for_user",
        "expiresAt": deadline,
        "failure": null,
        "panelMode": "submitted_browser_code",
        "prompt": null,
    });
    let _ = authorization_url; // prompt is returned only by the guarded read.
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
    let Some(url) = std::env::var_os("PARROT_CLAUDE_SETUP_TOKEN_AUTHORIZATION_URL") else {
        return error(StatusCode::NOT_FOUND, SETUP_TOKEN_NOT_FOUND);
    };
    no_store(Json(json!({
        "authorizationUrl": url.to_string_lossy(),
        "transportAdvisory": { "code": "setup_token_transport_unverified" }
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
    let _ = row;
    // Do not persist or echo the browser code. Until the PTY/credential writer
    // is configured, fail closed rather than accepting a code without storing
    // the resulting credential.
    error(StatusCode::SERVICE_UNAVAILABLE, SETUP_TOKEN_START_FAILED)
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
    error(StatusCode::BAD_REQUEST, "The Claude login session is not complete.")
}

async fn cancel(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, session_id)): Path<(Uuid, String)>,
) -> Response {
    let result = sqlx::query("UPDATE claude_setup_token_sessions SET state = 'cancelled', updated_at = NOW() WHERE session_id = $1 AND company_id = $2 AND owner_user_id = $3")
        .bind(&session_id)
        .bind(company_id)
        .bind(match owner(&actor, company_id) { Ok(owner) => owner, Err(response) => return response })
        .execute(&state.pool)
        .await;
    match result {
        Ok(_) => no_store(Json(json!({})).into_response()),
        Err(err) => { tracing::error!(%err, "failed to cancel Claude setup-token session"); error(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR") }
    }
}
