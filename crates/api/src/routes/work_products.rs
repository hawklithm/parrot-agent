use crate::app_state::AppState;
use crate::errors::AppError;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use uuid::Uuid;

use models::issue_auxiliary::{CreateWorkProductInput, UpdateWorkProductInput, WorkProduct};

/// GET /issues/:id/work-products - List work products
async fn list_work_products(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<WorkProduct>>, AppError> {
    let company_id = Uuid::nil();
    
    state.work_product_service
        .list_work_products(id, company_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// POST /issues/:id/work-products - Create work product
async fn create_work_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<CreateWorkProductInput>,
) -> Result<Json<WorkProduct>, AppError> {
    let company_id = Uuid::nil();
    
    state.work_product_service
        .create_work_product(id, company_id, input)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// PATCH /work-products/:id - Update work product
async fn update_work_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateWorkProductInput>,
) -> Result<Json<WorkProduct>, AppError> {
    let company_id = Uuid::nil();
    
    state.work_product_service
        .update_work_product(id, company_id, input)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// DELETE /work-products/:id - Delete work product
async fn delete_work_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let company_id = Uuid::nil();
    
    state.work_product_service
        .delete_work_product(id, company_id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// Create work product routes
pub fn work_product_routes() -> Router<AppState> {

    Router::new()
        .route("/api/issues/:id/work-products", get(list_work_products).post(create_work_product))
        .route("/api/work-products/:id", patch(update_work_product).delete(delete_work_product))
}
