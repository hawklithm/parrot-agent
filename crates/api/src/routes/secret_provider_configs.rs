//! Provider-vault routes (`/secret-provider-configs`, `/secret-providers`).
//!
//! Contract source: Paperclip `server/src/routes/secrets.ts:378-590`.
//!
//! Two authorization shapes coexist here and they are NOT interchangeable:
//!   * `/companies/:companyId/...` — a company is the address, so a caller
//!     without access gets `403` (`assertCompanyAccess`).
//!   * `/secret-provider-configs/:id` — the resource is the address, so a
//!     missing row and a row owned by another company both get an identical
//!     `404` (`getAccessibleResource`). Collapsing the latter into `403` hands
//!     an authenticated caller an existence oracle over every other tenant's
//!     config ids.
//!
//! Error split, matching Paperclip's middleware layering:
//!   * request-shape failures are rejected by serde before the handler runs and
//!     surface as `400` with Paperclip's `{"error":"Validation error"}` body;
//!   * semantic failures decided by the service surface as `422`.

use axum::{
    extract::{rejection::JsonRejection, Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use models::{
    CreateSecretProviderConfigRequest, SecretProviderConfigDiscoveryPreviewRequest,
    SecretProviderHealthResponse, UpdateSecretProviderConfigRequest,
};
use serde_json::json;
use services::auth::AuthorizationActor;
use services::errors::ServiceError;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{assert_board, has_company_access, require_company_access, AccessMode};

/// Paperclip `validate(...)` failures are reported by the global error handler
/// as `400 {"error":"Validation error"}` before any service call happens.
fn validation_error(message: impl std::fmt::Display) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "Validation error", "message": message.to_string() })),
    )
        .into_response()
}

/// Service errors follow Paperclip's `HttpError` status conventions. The
/// messages are already operator-facing, so they pass through verbatim.
fn service_error_response(error: ServiceError) -> Response {
    let status = match &error {
        ServiceError::NotFound(_) => StatusCode::NOT_FOUND,
        ServiceError::Forbidden(_) => StatusCode::FORBIDDEN,
        ServiceError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
        ServiceError::Conflict(_) => StatusCode::CONFLICT,
        // Paperclip `unprocessable(...)`: semantically understood, refused.
        ServiceError::Unprocessable(_) => StatusCode::UNPROCESSABLE_ENTITY,
        ServiceError::Validation(_) | ServiceError::InvalidInput(_) | ServiceError::BadRequest(_) => {
            StatusCode::BAD_REQUEST
        }
        ServiceError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    if status.is_server_error() {
        tracing::error!(error = %error, "secret provider config request failed");
    }
    (status, Json(json!({ "error": error.to_string() }))).into_response()
}

/// Paperclip `assertBoard` + `assertCompanyAccess` on a company-scoped path.
fn guard_company(
    actor: &AuthorizationActor,
    company_id: Uuid,
    mode: AccessMode,
) -> Result<(), Response> {
    assert_board(actor).map_err(|status| status.into_response())?;
    require_company_access(actor, company_id, mode).map_err(|status| status.into_response())
}

/// Paperclip `getAccessibleResource`: `404` for both a missing row and a row
/// outside the actor's companies, then the write-mode membership check.
///
/// Returns the config's company id when the actor may touch it.
async fn guard_config(
    state: &AppState,
    actor: &AuthorizationActor,
    config_id: Uuid,
    mode: AccessMode,
) -> Result<Uuid, Response> {
    assert_board(actor).map_err(|status| status.into_response())?;

    // `sqlx::Error` here means the lookup itself failed, which is a genuine 500
    // and must not be flattened into the not-found branch by `.ok()`.
    let company_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT company_id FROM company_secret_provider_configs WHERE id = $1",
    )
    .bind(config_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|err| {
        tracing::error!(error = %err, %config_id, "provider vault lookup failed");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })?;

    match company_id {
        Some(company_id) if has_company_access(actor, company_id) => {
            require_company_access(actor, company_id, mode).map_err(|status| status.into_response())?;
            Ok(company_id)
        }
        _ => Err((StatusCode::NOT_FOUND, Json(json!({ "error": "Provider vault not found" })))
            .into_response()),
    }
}

// ---------------------------------------------------------------------------
// Company-scoped routes
// ---------------------------------------------------------------------------

/// `GET /companies/:companyId/secret-provider-configs`.
pub async fn list_configs(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if let Err(response) = guard_company(&actor, company_id, AccessMode::Read) {
        return response;
    }

    match state
        .secret_provider_config_service
        .list_configs(company_id)
        .await
    {
        Ok(configs) => (StatusCode::OK, Json(configs)).into_response(),
        Err(error) => service_error_response(error),
    }
}

/// `POST /companies/:companyId/secret-provider-configs/discovery/preview`.
pub async fn discovery_preview(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    payload: Result<Json<SecretProviderConfigDiscoveryPreviewRequest>, JsonRejection>,
) -> Response {
    if let Err(response) = guard_company(&actor, company_id, AccessMode::Write) {
        return response;
    }
    let Json(request) = match payload {
        Ok(request) => request,
        Err(rejection) => return validation_error(rejection.body_text()),
    };

    match state
        .secret_provider_config_service
        .discovery_preview(company_id, request)
        .await
    {
        Ok(result) => {
            crate::routes::log_activity(
                &state.pool,
                company_id,
                "secret_provider_config.discovery_previewed",
                &actor,
                "secret_provider_config_discovery",
                company_id,
                json!({
                    "provider": result.provider,
                    "candidateCount": result.candidates.len(),
                    "sampledSecretCount": result.sampled_secret_count,
                    "warningCount": result.warnings.len(),
                }),
            )
            .await;
            (StatusCode::OK, Json(result)).into_response()
        }
        Err(error) => service_error_response(error),
    }
}

/// `POST /companies/:companyId/secret-provider-configs`.
pub async fn create_config(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    payload: Result<Json<CreateSecretProviderConfigRequest>, JsonRejection>,
) -> Response {
    if let Err(response) = guard_company(&actor, company_id, AccessMode::Write) {
        return response;
    }
    let Json(request) = match payload {
        Ok(request) => request,
        Err(rejection) => return validation_error(rejection.body_text()),
    };

    let actor_user_id = actor.principal_id().map(|id| id.to_string());
    match state
        .secret_provider_config_service
        .create_config(company_id, request, actor_user_id)
        .await
    {
        Ok(config) => {
            crate::routes::log_activity(
                &state.pool,
                company_id,
                "secret_provider_config.created",
                &actor,
                "secret_provider_config",
                config.id,
                json!({
                    "provider": config.provider,
                    "displayName": config.display_name,
                    "status": config.status,
                    "isDefault": config.is_default,
                }),
            )
            .await;
            (StatusCode::CREATED, Json(config)).into_response()
        }
        Err(error) => service_error_response(error),
    }
}

/// `GET /companies/:companyId/secret-providers/health`.
///
/// The body is deployment-level (`checkSecretProviders`), not per-company; the
/// company only gates access.
pub async fn company_health(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if let Err(response) = guard_company(&actor, company_id, AccessMode::Read) {
        return response;
    }

    match state.secret_provider_config_service.company_health().await {
        Ok(SecretProviderHealthResponse { providers }) => {
            (StatusCode::OK, Json(json!({ "providers": providers }))).into_response()
        }
        Err(error) => service_error_response(error),
    }
}

// ---------------------------------------------------------------------------
// Config-scoped routes
// ---------------------------------------------------------------------------

/// `GET /secret-provider-configs/:id`.
pub async fn get_config(
    Path(config_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if let Err(response) = guard_config(&state, &actor, config_id, AccessMode::Read).await {
        return response;
    }

    match state
        .secret_provider_config_service
        .get_config(config_id)
        .await
    {
        Ok(config) => (StatusCode::OK, Json(config)).into_response(),
        Err(error) => service_error_response(error),
    }
}

/// `PATCH /secret-provider-configs/:id`.
pub async fn update_config(
    Path(config_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    payload: Result<Json<UpdateSecretProviderConfigRequest>, JsonRejection>,
) -> Response {
    if let Err(response) = guard_config(&state, &actor, config_id, AccessMode::Write).await {
        return response;
    }
    let Json(request) = match payload {
        Ok(request) => request,
        Err(rejection) => return validation_error(rejection.body_text()),
    };

    match state
        .secret_provider_config_service
        .update_config(config_id, request)
        .await
    {
        Ok(config) => {
            crate::routes::log_activity(
                &state.pool,
                config.company_id,
                "secret_provider_config.updated",
                &actor,
                "secret_provider_config",
                config.id,
                json!({
                    "provider": config.provider,
                    "displayName": config.display_name,
                    "status": config.status,
                    "isDefault": config.is_default,
                }),
            )
            .await;
            (StatusCode::OK, Json(config)).into_response()
        }
        Err(error) => service_error_response(error),
    }
}

/// `DELETE /secret-provider-configs/:id` — responds `200` with the removed row.
pub async fn delete_config(
    Path(config_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if let Err(response) = guard_config(&state, &actor, config_id, AccessMode::Write).await {
        return response;
    }

    match state
        .secret_provider_config_service
        .delete_config(config_id)
        .await
    {
        Ok(removed) => {
            crate::routes::log_activity(
                &state.pool,
                removed.company_id,
                "secret_provider_config.removed",
                &actor,
                "secret_provider_config",
                removed.id,
                json!({
                    "provider": removed.provider,
                    "displayName": removed.display_name,
                    "remoteDeleted": false,
                }),
            )
            .await;
            (StatusCode::OK, Json(removed)).into_response()
        }
        Err(error) => service_error_response(error),
    }
}

/// `POST /secret-provider-configs/:id/default`.
pub async fn set_default(
    Path(config_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if let Err(response) = guard_config(&state, &actor, config_id, AccessMode::Write).await {
        return response;
    }

    match state
        .secret_provider_config_service
        .set_default(config_id)
        .await
    {
        Ok(config) => {
            crate::routes::log_activity(
                &state.pool,
                config.company_id,
                "secret_provider_config.default_set",
                &actor,
                "secret_provider_config",
                config.id,
                json!({
                    "provider": config.provider,
                    "displayName": config.display_name,
                    "isDefault": config.is_default,
                }),
            )
            .await;
            (StatusCode::OK, Json(config)).into_response()
        }
        Err(error) => service_error_response(error),
    }
}

/// `POST /secret-provider-configs/:id/health`.
pub async fn health_check(
    Path(config_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if let Err(response) = guard_config(&state, &actor, config_id, AccessMode::Write).await {
        return response;
    }

    match state
        .secret_provider_config_service
        .health_check(config_id)
        .await
    {
        Ok(health) => {
            crate::routes::log_activity(
                &state.pool,
                health.config_id,
                "secret_provider_config.health_checked",
                &actor,
                "secret_provider_config",
                health.config_id,
                json!({
                    "provider": health.provider,
                    "status": health.status,
                    "code": health.details.code,
                }),
            )
            .await;
            (StatusCode::OK, Json(health)).into_response()
        }
        Err(error) => service_error_response(error),
    }
}

/// Router for the provider-vault endpoints.
pub fn secret_provider_config_routes() -> Router<AppState> {
    axum::Router::new()
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
}
