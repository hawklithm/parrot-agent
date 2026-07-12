use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::{
    CompanySecretProviderConfig, CreateSecretProviderConfigRequest,
    SecretProviderConfigDiscoveryPreviewRequest, SecretProviderConfigDiscoveryPreviewResult,
    SecretProviderConfigHealthResponse, UpdateSecretProviderConfigRequest,
};
use services::secret_provider_config_service::SecretProviderConfigService;
use std::sync::Arc;
use uuid::Uuid;

/// GET /companies/:companyId/secret-provider-configs
/// List all provider configurations for a company
pub async fn list_configs(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretProviderConfigService>>,
) -> Response {
    // TODO: Add permission check - user must be company member

    match service.list_configs(company_id).await {
        Ok(configs) => (StatusCode::OK, Json(configs)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /companies/:companyId/secret-provider-configs/discovery/preview
/// Preview secret discovery from external provider
pub async fn discovery_preview(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretProviderConfigService>>,
    Json(request): Json<SecretProviderConfigDiscoveryPreviewRequest>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match service.discovery_preview(company_id, request).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /companies/:companyId/secret-provider-configs
/// Create a new provider configuration
pub async fn create_config(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretProviderConfigService>>,
    Json(request): Json<CreateSecretProviderConfigRequest>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match service.create_config(company_id, request).await {
        Ok(config) => (StatusCode::CREATED, Json(config)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /secret-provider-configs/:id
/// Get a single provider configuration
pub async fn get_config(
    Path(config_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretProviderConfigService>>,
) -> Response {
    match service.get_config(config_id).await {
        Ok(config) => (StatusCode::OK, Json(config)).into_response(),
        Err(e) => match e {
            crate::errors::ServiceError::NotFound(_) => {
                (StatusCode::NOT_FOUND, e.to_string()).into_response()
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}

/// PATCH /secret-provider-configs/:id
/// Update an existing provider configuration
pub async fn update_config(
    Path(config_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretProviderConfigService>>,
    Json(request): Json<UpdateSecretProviderConfigRequest>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match service.update_config(config_id, request).await {
        Ok(config) => (StatusCode::OK, Json(config)).into_response(),
        Err(e) => match e {
            crate::errors::ServiceError::NotFound(_) => {
                (StatusCode::NOT_FOUND, e.to_string()).into_response()
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}

/// DELETE /secret-provider-configs/:id
/// Delete a provider configuration
pub async fn delete_config(
    Path(config_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretProviderConfigService>>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match service.delete_config(config_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => match e {
            crate::errors::ServiceError::NotFound(_) => {
                (StatusCode::NOT_FOUND, e.to_string()).into_response()
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}

/// POST /secret-provider-configs/:id/default
/// Set a provider configuration as default
pub async fn set_default(
    Path(config_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretProviderConfigService>>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match service.set_default(config_id).await {
        Ok(config) => (StatusCode::OK, Json(config)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /secret-provider-configs/:id/health
/// Perform health check on a specific configuration
pub async fn health_check(
    Path(config_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretProviderConfigService>>,
) -> Response {
    match service.health_check(config_id).await {
        Ok(health) => (StatusCode::OK, Json(health)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /companies/:companyId/secret-providers/health
/// Get aggregated health status for all providers in a company
pub async fn company_health(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretProviderConfigService>>,
) -> Response {
    match service.company_health(company_id).await {
        Ok(health_list) => (StatusCode::OK, Json(health_list)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Router setup for secret provider configuration endpoints
pub fn secret_provider_config_routes(
    service: Arc<dyn SecretProviderConfigService>,
) -> axum::Router {
    axum::Router::new()
        // Company-scoped endpoints
        .route(
            "/companies/:companyId/secret-provider-configs",
            axum::routing::get(list_configs).post(create_config),
        )
        .route(
            "/companies/:companyId/secret-provider-configs/discovery/preview",
            axum::routing::post(discovery_preview),
        )
        .route(
            "/companies/:companyId/secret-providers/health",
            axum::routing::get(company_health),
        )
        // Config-scoped endpoints
        .route(
            "/secret-provider-configs/:id",
            axum::routing::get(get_config)
                .patch(update_config)
                .delete(delete_config),
        )
        .route(
            "/secret-provider-configs/:id/default",
            axum::routing::post(set_default),
        )
        .route(
            "/secret-provider-configs/:id/health",
            axum::routing::post(health_check),
        )
        .with_state(service)
}
