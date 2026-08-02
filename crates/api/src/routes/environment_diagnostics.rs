use crate::app_state::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use models::AcquireEnvironmentLeaseRequest;
use services::auth::AuthorizationActor;
use uuid::Uuid;

/// POST /environments/:id/probe
/// Probe environment connectivity and health
pub async fn probe(
    Path(environment_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if let Err(response) = assert_environment_access(&state, &actor, environment_id, true).await {
        return response;
    }

    match state
        .environment_diagnostics_service
        .probe(environment_id)
        .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /environments/:id/acquire
/// Acquire exclusive lease for environment access
pub async fn acquire_lease(
    Path(environment_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(request): Json<AcquireEnvironmentLeaseRequest>,
) -> Response {
    if let Err(response) = assert_environment_access(&state, &actor, environment_id, false).await {
        return response;
    }

    match state
        .environment_diagnostics_service
        .acquire_lease(environment_id, request)
        .await
    {
        Ok(lease) => (StatusCode::CREATED, Json(lease)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /environments/:id/delete-blast-radius
/// Analyze impact of deleting an environment
pub async fn delete_blast_radius(
    Path(environment_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if let Err(response) = assert_environment_access(&state, &actor, environment_id, false).await {
        return response;
    }

    match state
        .environment_diagnostics_service
        .delete_blast_radius(environment_id)
        .await
    {
        Ok(analysis) => (StatusCode::OK, Json(analysis)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn assert_environment_access(
    state: &AppState,
    actor: &AuthorizationActor,
    environment_id: Uuid,
    read_only: bool,
) -> Result<(), Response> {
    let company_id =
        sqlx::query_scalar::<_, Uuid>("SELECT company_id FROM environments WHERE id = $1")
            .bind(environment_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to resolve environment",
                )
                    .into_response()
            })?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "environment not found").into_response())?;
    crate::routes::assert_company_access(actor, company_id, !read_only)
        .map_err(|_| StatusCode::FORBIDDEN.into_response())
}

/// Router setup for environment diagnostic endpoints
pub fn environment_diagnostics_routes() -> Router<AppState> {
    axum::Router::new()
        .route(
            "/environments/:id/acquire",
            axum::routing::post(acquire_lease),
        )
        .route(
            "/environments/:id/delete-blast-radius",
            axum::routing::get(delete_blast_radius),
        )
}
