use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;
use services::low_trust_service::{LowTrustService, PromoteLowTrustInput, PromoteLowTrustResult};
use models::Issue;

/// POST /issues/:id/low-trust/promotions - Promote a low-trust issue
async fn promote_low_trust(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<PromoteLowTrustInput>,
) -> Result<Json<PromoteLowTrustResult>, StatusCode> {
    let service = state.low_trust_service.clone();
    let company_id = Uuid::nil();

    service
        .promote_low_trust(company_id, id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /issues/low-trust - List low-trust issues
async fn list_low_trust_issues(
    State(state): State<AppState>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    let service = state.low_trust_service.clone();
    let company_id = Uuid::nil();

    service
        .list_low_trust_issues(company_id, 100)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Create low trust routes
pub fn low_trust_routes() -> Router<AppState> {
    Router::new()
        .route("/api/issues/:id/low-trust/promotions", post(promote_low_trust))
        .route("/api/issues/low-trust", get(list_low_trust_issues))
}
