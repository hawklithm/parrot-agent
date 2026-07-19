use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use models::user_secret::{UserSecretDefinition, UserSecret, UserSecretScope, UserSecretCoverage, SecretBinding};
use services::user_secret_service::UserSecretService;

pub struct UserSecretRoutes {
    #[allow(dead_code)]
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
    pub company_id: Uuid,
    pub user_secret_definition_id: Uuid,
    pub user_id: Uuid,
    pub env_key: String,
    pub value_sha256: Option<String>,
    pub version_selector: String,
    pub required: bool,
    pub allow_missing_override: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<UserSecret> for UserSecretResponse {
    fn from(secret: UserSecret) -> Self {
        Self {
            id: secret.id,
            company_id: secret.company_id,
            user_secret_definition_id: secret.user_secret_definition_id,
            user_id: secret.user_id,
            env_key: secret.env_key,
            value_sha256: secret.value_sha256,
            version_selector: secret.version_selector,
            required: secret.required,
            allow_missing_override: secret.allow_missing_override,
            created_at: secret.created_at.to_rfc3339(),
            updated_at: secret.updated_at.to_rfc3339(),
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
    // TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）
    let current_user_id = Uuid::nil();

    let definition = service
        .create_definition(company_id, req.key, req.description, req.required, req.scope, current_user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(definition))
}

// GET /companies/:companyId/user-secret-definitions/:definitionId
pub async fn get_definition(
    Path((_company_id, definition_id)): Path<(Uuid, Uuid)>,
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
    Path((_company_id, definition_id)): Path<(Uuid, Uuid)>,
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
    Path((_company_id, definition_id)): Path<(Uuid, Uuid)>,
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
    // TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）
    let current_user_id = Uuid::nil();

    let secrets = service
        .list_user_secrets(current_user_id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let responses: Vec<UserSecretResponse> = secrets.into_iter().map(Into::into).collect();
    Ok(Json(responses))
}

// POST /companies/:companyId/me/user-secrets
pub async fn set_user_secret(
    Path(_company_id): Path<Uuid>,
    State(service): State<Arc<dyn UserSecretService>>,
    Json(req): Json<SetUserSecretRequest>,
) -> Result<Json<UserSecretResponse>, StatusCode> {
    // TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）
    let current_user_id = Uuid::nil();

    let secret = service
        .set_user_secret(current_user_id, req.definition_id, req.value)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(secret.into()))
}

// GET /companies/:companyId/me/user-secrets/:definitionId
pub async fn get_user_secret(
    Path((_company_id, definition_id)): Path<(Uuid, Uuid)>,
    State(service): State<Arc<dyn UserSecretService>>,
) -> Result<Json<UserSecretResponse>, StatusCode> {
    // TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）
    let current_user_id = Uuid::nil();

    let secret = service
        .get_user_secret(current_user_id, definition_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(secret.into()))
}

// POST /companies/:companyId/me/user-secrets/:secretId/rotate
pub async fn rotate_user_secret(
    Path((_company_id, secret_id)): Path<(Uuid, Uuid)>,
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
    Path((_company_id, secret_id)): Path<(Uuid, Uuid)>,
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
    Path((_company_id, secret_id)): Path<(Uuid, Uuid)>,
    State(service): State<Arc<dyn UserSecretService>>,
) -> Result<Json<Vec<SecretBinding>>, StatusCode> {
    let bindings = service
        .get_secret_bindings(secret_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(bindings))
}

/// All user-secret routes have been migrated to `user_secret_definitions::user_secret_definition_routes()`.
/// This module is kept only to avoid breaking the `with_state` call in `app_state.rs`.
/// All handler functions remain here for reference but are no longer registered as routes.
pub fn user_secret_routes() -> axum::Router<Arc<dyn UserSecretService>> {
    axum::Router::new()
}
