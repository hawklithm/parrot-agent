use crate::app_state::AppState;
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use models::user_secret::{UserSecret, UserSecretScope};
use uuid::Uuid;

// ============================================================================
// Request types
// ============================================================================

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateDefinitionRequest {
    key: String,
    description: Option<String>,
    required: bool,
    scope: Option<UserSecretScope>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateDefinitionRequest {
    key: Option<String>,
    description: Option<String>,
    required: Option<bool>,
    scope: Option<UserSecretScope>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetUserSecretRequest {
    definition_id: Uuid,
    value: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserSecretRequest {
    value: String,
}

// ============================================================================
// Response types
// ============================================================================

/// JSON-friendly UserSecret response that omits sensitive material fields.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UserSecretResponse {
    id: Uuid,
    company_id: Uuid,
    user_secret_definition_id: Uuid,
    user_id: Uuid,
    env_key: String,
    value_sha256: Option<String>,
    version_selector: String,
    required: bool,
    allow_missing_override: bool,
    created_at: String,
    updated_at: String,
}

impl From<UserSecret> for UserSecretResponse {
    fn from(s: UserSecret) -> Self {
        Self {
            id: s.id,
            company_id: s.company_id,
            user_secret_definition_id: s.user_secret_definition_id,
            user_id: s.user_id,
            env_key: s.env_key,
            value_sha256: s.value_sha256,
            version_selector: s.version_selector,
            required: s.required,
            allow_missing_override: s.allow_missing_override,
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        }
    }
}

// ============================================================================
// Handler helpers
// ============================================================================

fn extract_user_id() -> Uuid {
    // TODO: Extract user_id from auth context
    Uuid::nil()
}

fn internal_error(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn not_found(msg: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, msg.into())
}

// ============================================================================
// Definition handlers
// ============================================================================

/// GET /companies/:companyId/user-secret-definitions
async fn list_definitions(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state
        .user_secret_service
        .list_definitions(company_id)
        .await
    {
        Ok(defs) => (StatusCode::OK, Json(defs)).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// POST /companies/:companyId/user-secret-definitions
async fn create_definition(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(req): Json<CreateDefinitionRequest>,
) -> impl IntoResponse {
    let user_id = extract_user_id();
    let scope = req.scope.unwrap_or(UserSecretScope {
        project_ids: None,
        agent_ids: None,
        applies_to_all: true,
    });

    match state
        .user_secret_service
        .create_definition(company_id, req.key, req.description, req.required, scope, user_id)
        .await
    {
        Ok(def) => (StatusCode::CREATED, Json(def)).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// GET /companies/:companyId/user-secret-definitions/:definitionId
async fn get_definition(
    Path((_company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state
        .user_secret_service
        .get_definition(definition_id)
        .await
    {
        Ok(Some(def)) => (StatusCode::OK, Json(def)).into_response(),
        Ok(None) => not_found(format!("Definition {} not found", definition_id)).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// PATCH /companies/:companyId/user-secret-definitions/:definitionId
async fn update_definition(
    Path((_company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(req): Json<UpdateDefinitionRequest>,
) -> impl IntoResponse {
    match state
        .user_secret_service
        .update_definition(definition_id, req.key, req.description, req.required, req.scope)
        .await
    {
        Ok(def) => (StatusCode::OK, Json(def)).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// DELETE /companies/:companyId/user-secret-definitions/:definitionId
async fn delete_definition(
    Path((_company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state
        .user_secret_service
        .delete_definition(definition_id)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// GET /companies/:companyId/user-secret-definitions/:definitionId/coverage
async fn get_coverage(
    Path((company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state
        .user_secret_service
        .get_coverage_stats(company_id, definition_id)
        .await
    {
        Ok(coverage) => (StatusCode::OK, Json(coverage)).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

// ============================================================================
// User secret (value) handlers
// ============================================================================

/// GET /companies/:companyId/me/user-secrets
async fn list_user_secrets(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let user_id = extract_user_id();
    match state
        .user_secret_service
        .list_user_secrets(user_id, company_id)
        .await
    {
        Ok(secrets) => {
            let resp: Vec<UserSecretResponse> = secrets.into_iter().map(Into::into).collect();
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

/// POST /companies/:companyId/me/user-secrets
async fn upsert_user_secret(
    Path(_company_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(req): Json<SetUserSecretRequest>,
) -> impl IntoResponse {
    let user_id = extract_user_id();
    match state
        .user_secret_service
        .set_user_secret(user_id, req.definition_id, req.value)
        .await
    {
        Ok(secret) => {
            let resp: UserSecretResponse = secret.into();
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

/// GET /companies/:companyId/me/user-secrets/:definitionId
/// Get the current user's secret value for a specific definition
async fn get_user_secret(
    Path((_company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let user_id = extract_user_id();
    match state
        .user_secret_service
        .get_user_secret(user_id, definition_id)
        .await
    {
        Ok(Some(secret)) => {
            let resp: UserSecretResponse = secret.into();
            (StatusCode::OK, Json(resp)).into_response()
        }
        Ok(None) => not_found(format!("Secret for definition {} not found", definition_id))
            .into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// PATCH /companies/:companyId/me/user-secrets/:secretId
async fn update_user_secret(
    Path((_company_id, secret_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(req): Json<UpdateUserSecretRequest>,
) -> impl IntoResponse {
    // PATCH on a user secret is semantically a rotate (update the value).
    match state
        .user_secret_service
        .rotate_user_secret(secret_id, req.value)
        .await
    {
        Ok(secret) => {
            let resp: UserSecretResponse = secret.into();
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

/// DELETE /companies/:companyId/me/user-secrets/:secretId
async fn delete_user_secret(
    Path((_company_id, secret_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state
        .user_secret_service
        .delete_user_secret(secret_id)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// POST /companies/:companyId/me/user-secrets/:secretId/rotate
async fn rotate_user_secret(
    Path((_company_id, secret_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(new_value): Json<String>,
) -> impl IntoResponse {
    match state
        .user_secret_service
        .rotate_user_secret(secret_id, new_value)
        .await
    {
        Ok(secret) => {
            let resp: UserSecretResponse = secret.into();
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => internal_error(e).into_response(),
    }
}

/// GET /companies/:companyId/me/user-secrets/:secretId/bindings
async fn list_secret_bindings(
    Path((_company_id, secret_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state
        .user_secret_service
        .get_secret_bindings(secret_id)
        .await
    {
        Ok(bindings) => (StatusCode::OK, Json(bindings)).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// GET /secrets/:secretId/bindings (top-level alias)
async fn get_secret_bindings(
    Path(secret_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state
        .user_secret_service
        .get_secret_bindings(secret_id)
        .await
    {
        Ok(bindings) => (StatusCode::OK, Json(bindings)).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

// ============================================================================
// Router
// ============================================================================

/// Create the user secret routes, covering all paperclip-aligned endpoints
/// for UserSecretDefinition (SE1-SE4) and UserSecret management.
pub fn user_secret_definition_routes() -> Router<AppState> {
    use axum::routing::{get, post};

    Router::new()
        // --- Definition management (companies scope) ---
        .route(
            "/companies/:companyId/user-secret-definitions",
            get(list_definitions).post(create_definition),
        )
        .route(
            "/companies/:companyId/user-secret-definitions/:definitionId",
            get(get_definition).patch(update_definition).delete(delete_definition),
        )
        .route(
            "/companies/:companyId/user-secret-definitions/:definitionId/coverage",
            get(get_coverage),
        )
        // --- Current user's secret values (companies scope) ---
        .route(
            "/companies/:companyId/me/user-secrets",
            get(list_user_secrets).post(upsert_user_secret),
        )
        // Note: :secretId and :definitionId are the same path segment in Axum's router.
        // All HTTP methods on /me/user-secrets/:id must be registered on one route.
        .route(
            "/companies/:companyId/me/user-secrets/:secretId",
            get(get_user_secret).patch(update_user_secret).delete(delete_user_secret),
        )
        .route(
            "/companies/:companyId/me/user-secrets/:secretId/rotate",
            post(rotate_user_secret),
        )
        .route(
            "/companies/:companyId/me/user-secrets/:secretId/bindings",
            get(list_secret_bindings),
        )
        // --- Top-level secret bindings alias ---
        .route(
            "/secrets/:secretId/bindings",
            get(get_secret_bindings),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_secret_response_from_secret() {
        let now = chrono::Utc::now();
        let secret = UserSecret {
            id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            user_secret_definition_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            env_key: "TEST_KEY".to_string(),
            value_material: Some("encrypted".to_string()),
            value_sha256: Some("abc123".to_string()),
            version_selector: "latest".to_string(),
            required: true,
            allow_missing_override: false,
            created_at: now,
            updated_at: now,
        };

        let resp: UserSecretResponse = secret.into();
        assert_eq!(resp.env_key, "TEST_KEY");
        assert_eq!(resp.value_sha256.as_deref(), Some("abc123"));
        assert_eq!(resp.version_selector, "latest");
        assert!(resp.required);
    }
}
