use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use models::user_secret::{UserSecretDefinition, UserSecret, UserSecretScope, UserSecretCoverage, SecretBinding};
use services::user_secret_service::UserSecretService;

pub struct UserSecretRoutes {
    service: Arc<dyn UserSecretService>,
}

impl UserSecretRoutes {
    pub fn new(service: Arc<dyn UserSecretService>) -> Self {
        Self { service }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateDefinitionRequest {
    pub key: String,
    pub description: Option<String>,
    pub required: bool,
    pub scope: UserSecretScope,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDefinitionRequest {
    pub key: Option<String>,
    pub description: Option<String>,
    pub required: Option<bool>,
    pub scope: Option<UserSecretScope>,
}

#[derive(Debug, Deserialize)]
pub struct SetUserSecretRequest {
    pub definition_id: Uuid,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct UserSecretResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub definition_id: Uuid,
    pub created_at: String,
    pub updated_at: String,
    pub last_rotated_at: Option<String>,
}

impl From<UserSecret> for UserSecretResponse {
    fn from(secret: UserSecret) -> Self {
        Self {
            id: secret.id,
            user_id: secret.user_id,
            definition_id: secret.definition_id,
            created_at: secret.created_at.to_rfc3339(),
            updated_at: secret.updated_at.to_rfc3339(),
            last_rotated_at: secret.last_rotated_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

// GET /companies/:companyId/user-secret-definitions
pub async fn list_definitions(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn UserSecretService>>,
) -> Result<Json<Vec<UserSecretDefinition>>, StatusCode> {
    let definitions = service
        .list_definitions(company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(definitions))
}

// POST /companies/:companyId/user-secret-definitions
pub async fn create_definition(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn UserSecretService>>,
    Json(req): Json<CreateDefinitionRequest>,
) -> Result<Json<UserSecretDefinition>, StatusCode> {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::nil();

    let definition = service
        .create_definition(company_id, req.key, req.description, req.required, req.scope, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(definition))
}

// GET /companies/:companyId/user-secret-definitions/:definitionId
pub async fn get_definition(
    Path((company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(service): State<Arc<dyn UserSecretService>>,
) -> Result<Json<UserSecretDefinition>, StatusCode> {
    let definition = service
        .get_definition(definition_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(definition))
}

// PATCH /companies/:companyId/user-secret-definitions/:definitionId
pub async fn update_definition(
    Path((company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(service): State<Arc<dyn UserSecretService>>,
    Json(req): Json<UpdateDefinitionRequest>,
) -> Result<Json<UserSecretDefinition>, StatusCode> {
    let definition = service
        .update_definition(definition_id, req.key, req.description, req.required, req.scope)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(definition))
}

// DELETE /companies/:companyId/user-secret-definitions/:definitionId
pub async fn delete_definition(
    Path((company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(service): State<Arc<dyn UserSecretService>>,
) -> Result<StatusCode, StatusCode> {
    service
        .delete_definition(definition_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

// GET /companies/:companyId/user-secret-definitions/:definitionId/coverage
pub async fn get_coverage_stats(
    Path((company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(service): State<Arc<dyn UserSecretService>>,
) -> Result<Json<UserSecretCoverage>, StatusCode> {
    let coverage = service
        .get_coverage_stats(company_id, definition_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(coverage))
}

// GET /companies/:companyId/me/user-secrets
pub async fn list_user_secrets(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn UserSecretService>>,
) -> Result<Json<Vec<UserSecretResponse>>, StatusCode> {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::nil();

    let secrets = service
        .list_user_secrets(user_id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let responses: Vec<UserSecretResponse> = secrets.into_iter().map(Into::into).collect();
    Ok(Json(responses))
}

// POST /companies/:companyId/me/user-secrets
pub async fn set_user_secret(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn UserSecretService>>,
    Json(req): Json<SetUserSecretRequest>,
) -> Result<Json<UserSecretResponse>, StatusCode> {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::nil();

    let secret = service
        .set_user_secret(user_id, req.definition_id, req.value)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(secret.into()))
}

// GET /companies/:companyId/me/user-secrets/:definitionId
pub async fn get_user_secret(
    Path((company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(service): State<Arc<dyn UserSecretService>>,
) -> Result<Json<UserSecretResponse>, StatusCode> {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::nil();

    let secret = service
        .get_user_secret(user_id, definition_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(secret.into()))
}

// POST /companies/:companyId/me/user-secrets/:secretId/rotate
pub async fn rotate_user_secret(
    Path((company_id, secret_id)): Path<(Uuid, Uuid)>,
    State(service): State<Arc<dyn UserSecretService>>,
    Json(new_value): Json<String>,
) -> Result<Json<UserSecretResponse>, StatusCode> {
    let secret = service
        .rotate_user_secret(secret_id, new_value)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(secret.into()))
}

// DELETE /companies/:companyId/me/user-secrets/:secretId
pub async fn delete_user_secret(
    Path((company_id, secret_id)): Path<(Uuid, Uuid)>,
    State(service): State<Arc<dyn UserSecretService>>,
) -> Result<StatusCode, StatusCode> {
    service
        .delete_user_secret(secret_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

// GET /companies/:companyId/me/user-secrets/:secretId/bindings
pub async fn get_secret_bindings(
    Path((company_id, secret_id)): Path<(Uuid, Uuid)>,
    State(service): State<Arc<dyn UserSecretService>>,
) -> Result<Json<Vec<SecretBinding>>, StatusCode> {
    let bindings = service
        .get_secret_bindings(secret_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(bindings))
}

pub fn user_secret_routes() -> axum::Router<Arc<dyn UserSecretService>> {
    use axum::routing::{delete, get, patch, post};

    axum::Router::new()
        .route("/companies/:company_id/user-secret-definitions", get(list_definitions).post(create_definition))
        .route("/companies/:company_id/user-secret-definitions/:definition_id",
            get(get_definition).patch(update_definition).delete(delete_definition))
        .route("/companies/:company_id/user-secret-definitions/:definition_id/coverage", get(get_coverage_stats))
        .route("/companies/:company_id/me/user-secrets", get(list_user_secrets).post(set_user_secret))
        .route("/companies/:company_id/me/user-secrets/:definition_id", get(get_user_secret))
        .route("/companies/:company_id/me/user-secrets/:secret_id/rotate", post(rotate_user_secret))
        .route("/companies/:company_id/me/user-secrets/:secret_id", delete(delete_user_secret))
        .route("/companies/:company_id/me/user-secrets/:secret_id/bindings", get(get_secret_bindings))
}
