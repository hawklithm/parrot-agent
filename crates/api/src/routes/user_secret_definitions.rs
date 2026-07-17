use crate::app_state::AppState;
use axum::{Router, 
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::{
    CreateUserSecretDefinitionRequest,
    UpdateUserSecretDefinitionRequest, UpsertUserSecretRequest,
};
use uuid::Uuid;

/// GET /companies/:companyId/user-secret-definitions
/// List all user secret definitions for a company
pub async fn list_definitions(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    // TODO: Add permission check - user must be company member

    match state.user_secret_definition_service.list_definitions(company_id).await {
        Ok(definitions) => (StatusCode::OK, Json(definitions)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /companies/:companyId/user-secret-definitions
/// Create a new user secret definition
pub async fn create_definition(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<CreateUserSecretDefinitionRequest>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match state.user_secret_definition_service.create_definition(company_id, request).await {
        Ok(definition) => (StatusCode::CREATED, Json(definition)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// PATCH /companies/:companyId/user-secret-definitions/:definitionId
/// Update a user secret definition
pub async fn update_definition(
    Path((_company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(request): Json<UpdateUserSecretDefinitionRequest>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match state.user_secret_definition_service.update_definition(definition_id, request).await {
        Ok(definition) => (StatusCode::OK, Json(definition)).into_response(),
        Err(e) => match e {
            services::errors::ServiceError::NotFound(_) => {
                (StatusCode::NOT_FOUND, e.to_string()).into_response()
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}

/// DELETE /companies/:companyId/user-secret-definitions/:definitionId
/// Delete a user secret definition
pub async fn delete_definition(
    Path((_company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match state.user_secret_definition_service.delete_definition(definition_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => match e {
            services::errors::ServiceError::NotFound(_) => {
                (StatusCode::NOT_FOUND, e.to_string()).into_response()
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}

/// GET /companies/:companyId/user-secret-definitions/:definitionId/coverage
/// Get coverage statistics for a user secret definition
pub async fn get_coverage(
    Path((_company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Response {
    match state.user_secret_definition_service.get_coverage(definition_id).await {
        Ok(coverage) => (StatusCode::OK, Json(coverage)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /companies/:companyId/me/user-secrets
/// List current user's secrets
pub async fn list_my_secrets(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::new_v4(); // Mock for now

    match state.user_secret_definition_service.list_my_secrets(company_id, user_id).await {
        Ok(entries) => (StatusCode::OK, Json(entries)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /companies/:companyId/me/user-secrets
/// Create or update current user's secret value
pub async fn upsert_my_secret(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<UpsertUserSecretRequest>,
) -> Response {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::new_v4(); // Mock for now

    match state.user_secret_definition_service.upsert_my_secret(company_id, user_id, request).await {
        Ok(secret) => (StatusCode::OK, Json(secret)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// PATCH /companies/:companyId/me/user-secrets/:secretId
/// Update current user's secret value (alias to upsert)
pub async fn update_my_secret(
    Path((company_id, _secret_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(request): Json<UpsertUserSecretRequest>,
) -> Response {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::new_v4();

    match state.user_secret_definition_service.upsert_my_secret(company_id, user_id, request).await {
        Ok(secret) => (StatusCode::OK, Json(secret)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// DELETE /companies/:companyId/me/user-secrets/:secretId
/// Delete current user's secret value
pub async fn delete_my_secret(
    Path((_company_id, secret_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Response {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::new_v4();

    match state.user_secret_definition_service.delete_my_secret(secret_id, user_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /companies/:companyId/me/user-secrets/:secretId/rotate
/// Rotate current user's secret (generate new version)
pub async fn rotate_my_secret(
    Path((_company_id, secret_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Response {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::new_v4();

    match state.user_secret_definition_service.rotate_my_secret(secret_id, user_id).await {
        Ok(secret) => (StatusCode::OK, Json(secret)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /secrets/:secretId/bindings
/// Get bindings (usage locations) for a secret
pub async fn get_secret_bindings(
    Path(secret_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    match state.user_secret_definition_service.get_secret_bindings(secret_id).await {
        Ok(bindings) => (StatusCode::OK, Json(bindings)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Router setup for user secret definition endpoints
pub fn user_secret_definition_routes() -> Router<AppState> {
    axum::Router::new()
        // Definition management
        .route(
            "/companies/:companyId/user-secret-definitions",
            axum::routing::get(list_definitions).post(create_definition),
        )
        .route(
            "/companies/:companyId/user-secret-definitions/:definitionId",
            axum::routing::patch(update_definition).delete(delete_definition),
        )
        .route(
            "/companies/:companyId/user-secret-definitions/:definitionId/coverage",
            axum::routing::get(get_coverage),
        )
        // User secret values (current user's secrets)
        .route(
            "/companies/:companyId/me/user-secrets",
            axum::routing::get(list_my_secrets).post(upsert_my_secret),
        )
        .route(
            "/companies/:companyId/me/user-secrets/:secretId",
            axum::routing::patch(update_my_secret).delete(delete_my_secret),
        )
        .route(
            "/companies/:companyId/me/user-secrets/:secretId/rotate",
            axum::routing::post(rotate_my_secret),
        )
        // Secret bindings (usage查询)
        .route(
            "/secrets/:secretId/bindings",
            axum::routing::get(get_secret_bindings),
        )
}
