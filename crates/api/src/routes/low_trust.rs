use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;
use services::low_trust_service::{PromoteLowTrustInput, PromoteLowTrustResult};
use models::Issue;
use services::auth::AuthorizationActor;

/// POST /issues/:id/low-trust/promotions - Promote a low-trust issue
async fn promote_low_trust(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<PromoteLowTrustInput>,
) -> Result<Json<PromoteLowTrustResult>, StatusCode> {
    let service = state.low_trust_service.clone();
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM issues WHERE id=$1")
        .bind(id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::NOT_FOUND)?
        .ok_or(StatusCode::NOT_FOUND)?;

    service
        .promote_low_trust(company_id, id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /issues/low-trust - List low-trust issues
async fn list_low_trust_issues(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    let service = state.low_trust_service.clone();
    let company_id = actor.company_id().ok_or(StatusCode::FORBIDDEN)?;

    service
        .list_low_trust_issues(company_id, 100)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Create low trust routes
pub fn low_trust_routes() -> Router<AppState> {
    Router::new()
        .route("/issues/:id/low-trust/promotions", post(promote_low_trust))
        .route("/issues/low-trust", get(list_low_trust_issues))
}
