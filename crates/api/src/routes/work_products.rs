use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use std::sync::Arc;
use uuid::Uuid;

use models::{CreateWorkProductInput, UpdateWorkProductInput, WorkProduct};
use services::WorkProductService;

/// GET /issues/:id/work-products - List work products
async fn list_work_products(
    State(service): State<Arc<dyn WorkProductService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<WorkProduct>>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .list_work_products(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/work-products - Create work product
async fn create_work_product(
    State(service): State<Arc<dyn WorkProductService>>,
    Path(id): Path<Uuid>,
    Json(input): Json<CreateWorkProductInput>,
) -> Result<Json<WorkProduct>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .create_work_product(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// PATCH /work-products/:id - Update work product
async fn update_work_product(
    State(service): State<Arc<dyn WorkProductService>>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateWorkProductInput>,
) -> Result<Json<WorkProduct>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .update_work_product(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// DELETE /work-products/:id - Delete work product
async fn delete_work_product(
    State(service): State<Arc<dyn WorkProductService>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .delete_work_product(id, company_id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Create work product routes
pub fn work_product_routes(service: Arc<dyn WorkProductService>) -> Router {
    Router::new()
        .route("/api/issues/:id/work-products", get(list_work_products).post(create_work_product))
        .route("/api/work-products/:id", patch(update_work_product).delete(delete_work_product))
        .with_state(service)
}
