//! Asset routes — P4 收尾域 (AS1-AS3)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;

pub fn asset_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/assets/images", post(upload_asset_image))
        .route("/companies/:company_id/logo", post(upload_company_logo))
        .route("/assets/:asset_id/content", get(get_asset_content))
}

async fn upload_asset_image(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "assetId": Uuid::new_v4(),
        "companyId": company_id,
        "uploaded": true,
    }))))
}

async fn upload_company_logo(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "assetId": Uuid::new_v4(),
        "companyId": company_id,
        "uploaded": true,
    }))))
}

async fn get_asset_content(
    State(_state): State<AppState>,
    Path(_asset_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    Ok((StatusCode::OK, "Binary content placeholder"))
}
