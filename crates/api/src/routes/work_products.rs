use crate::app_state::AppState;
use crate::errors::AppError;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, patch},
    Json, Router,
};
use uuid::Uuid;

use models::issue_auxiliary::{CreateWorkProductInput, UpdateWorkProductInput, WorkProduct};
use services::auth::AuthorizationActor;

/// Helper: 通过 issue_id 查询 company_id
async fn get_company_id_for_issue(state: &AppState, issue_id: Uuid) -> Result<Uuid, AppError> {
    sqlx::query_scalar("SELECT company_id FROM issues WHERE id=$1")
        .bind(issue_id).fetch_optional(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or(AppError::NotFound("Issue not found".to_string()))
}

/// GET /issues/:id/work-products - List work products
async fn list_work_products(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<WorkProduct>>, AppError> {
    let company_id = get_company_id_for_issue(&state, id).await?;
    crate::routes::assert_company_access(&actor, company_id, true)
        .map_err(|e| AppError::Forbidden(e.to_string()))?;

    state.work_product_service
        .list_work_products(id, company_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// POST /issues/:id/work-products - Create work product
async fn create_work_product(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(input): Json<CreateWorkProductInput>,
) -> Result<Json<WorkProduct>, AppError> {
    let company_id = get_company_id_for_issue(&state, id).await?;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|e| AppError::Forbidden(e.to_string()))?;

    state.work_product_service
        .create_work_product(id, company_id, input)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// PATCH /work-products/:id - Update work product
async fn update_work_product(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateWorkProductInput>,
) -> Result<Json<WorkProduct>, AppError> {
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM issue_work_products WHERE id = $1")
        .bind(id).fetch_optional(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or(AppError::NotFound("Work product not found".to_string()))?;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|e| AppError::Forbidden(e.to_string()))?;

    state.work_product_service
        .update_work_product(id, company_id, input)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// DELETE /work-products/:id - Delete work product
async fn delete_work_product(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM issue_work_products WHERE id = $1")
        .bind(id).fetch_optional(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or(AppError::NotFound("Work product not found".to_string()))?;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|e| AppError::Forbidden(e.to_string()))?;

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
