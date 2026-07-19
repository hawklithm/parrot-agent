use crate::app_state::AppState;
use crate::errors::AppError;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
    Json, Router,
};
use uuid::Uuid;

use models::issue_auxiliary::{CreateWorkProductInput, UpdateWorkProductInput, WorkProduct};

/// Helper: 通过 issue_id 查询 company_id
async fn get_company_id_for_issue(state: &AppState, issue_id: Uuid) -> Result<Uuid, AppError> {
    let issue = state
        .issue_service
        .get(issue_id, Uuid::nil())
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or(AppError::NotFound("Issue not found".to_string()))?;
    Ok(issue.company_id)
}

/// GET /issues/:id/work-products - List work products
async fn list_work_products(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<WorkProduct>>, AppError> {
    let company_id = get_company_id_for_issue(&state, id).await?;

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
    let company_id = get_company_id_for_issue(&state, id).await?;

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
    // Note: work_product 没有 issue_id 路径参数，无法直接获取 company_id。
    // 当前 work_product_service 实现中 company_id 参数被忽略（_company_id）。
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
    // Note: work_product 没有 issue_id 路径参数，无法直接获取 company_id。
    // 当前 work_product_service 实现中 company_id 参数被忽略（_company_id）。
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
        .route("/issues/:id/work-products", get(list_work_products).post(create_work_product))
        .route("/work-products/:id", patch(update_work_product).delete(delete_work_product))
}
