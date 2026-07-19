//! Asset routes — Paperclip 一比一迁移
//!
//! 对应 Paperclip: server/src/routes/assets.ts
//! 提供文件上传和内容获取端点。

use axum::{
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;

pub fn asset_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/assets/images", post(upload_asset_image))
        .route("/companies/:company_id/logo", post(upload_company_logo))
        .route("/assets/:asset_id/content", get(get_asset_content))
}

/// POST /companies/:company_id/assets/images
/// 上传资产图片。
/// 对应 Paperclip: assetRoutes -> POST /companies/:companyId/assets/images
async fn upload_asset_image(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut asset_id = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart: {}", e)))?
    {
        let filename = field.file_name().map(|s| s.to_string());
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to read field data: {}", e)))?;

        // Store in assets table
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        // Generate a simple content hash from the data bytes
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let sha256 = format!("{:x}", hasher.finalize());

        // Store file on filesystem
        let upload_dir = std::path::Path::new("uploads").join("images");
        tokio::fs::create_dir_all(&upload_dir)
            .await
            .map_err(|e| AppError::InternalServerError(format!("Failed to create upload dir: {}", e)))?;

        let object_key = format!("images/{}/{}", company_id, id);
        let file_path = upload_dir.join(&object_key);
        tokio::fs::write(&file_path, &data)
            .await
            .map_err(|e| AppError::InternalServerError(format!("Failed to write file: {}", e)))?;

        // Insert asset record
        sqlx::query(
            r#"
            INSERT INTO assets (id, company_id, provider, object_key, content_type, byte_size, sha256, original_filename, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(id)
        .bind(company_id)
        .bind("local_fs")
        .bind(&object_key)
        .bind(&content_type)
        .bind(data.len() as i64)
        .bind(&sha256)
        .bind(&filename)
        .bind(now)
        .bind(now)
        .execute(&state.pool)
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to insert asset: {}", e)))?;

        asset_id = Some(id);
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "assetId": asset_id.unwrap_or_else(Uuid::new_v4),
            "companyId": company_id,
            "uploaded": true,
        })),
    ))
}

/// POST /companies/:company_id/logo
/// 上传公司 Logo。
/// 对应 Paperclip: assetRoutes -> POST /companies/:companyId/logo
async fn upload_company_logo(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut asset_id = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart: {}", e)))?
    {
        let filename = field.file_name().map(|s| s.to_string());
        let content_type = field
            .content_type()
            .unwrap_or("image/png")
            .to_string();
        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to read field data: {}", e)))?;

        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let sha256 = format!("{:x}", hasher.finalize());

        let upload_dir = std::path::Path::new("uploads").join("logos");
        tokio::fs::create_dir_all(&upload_dir)
            .await
            .map_err(|e| AppError::InternalServerError(format!("Failed to create upload dir: {}", e)))?;

        let object_key = format!("logos/{}/{}", company_id, id);
        let file_path = upload_dir.join(&object_key);
        tokio::fs::write(&file_path, &data)
            .await
            .map_err(|e| AppError::InternalServerError(format!("Failed to write file: {}", e)))?;

        sqlx::query(
            r#"
            INSERT INTO assets (id, company_id, provider, object_key, content_type, byte_size, sha256, original_filename, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(id)
        .bind(company_id)
        .bind("local_fs")
        .bind(&object_key)
        .bind(&content_type)
        .bind(data.len() as i64)
        .bind(&sha256)
        .bind(&filename)
        .bind(now)
        .bind(now)
        .execute(&state.pool)
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to insert asset: {}", e)))?;

        asset_id = Some(id);
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "assetId": asset_id.unwrap_or_else(Uuid::new_v4),
            "companyId": company_id,
            "uploaded": true,
        })),
    ))
}

/// GET /assets/:asset_id/content
/// 获取资产内容。
/// 对应 Paperclip: assetRoutes -> GET /assets/:assetId/content
async fn get_asset_content(
    State(state): State<AppState>,
    Path(asset_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        r#"
        SELECT provider, object_key, content_type, original_filename
        FROM assets
        WHERE id = $1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to query asset: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("Asset not found: {}", asset_id)))?;

    let (_provider, object_key, content_type, _original_filename) = row;

    let upload_dir = std::path::Path::new("uploads");
    let file_path = upload_dir.join(&object_key);
    let data = tokio::fs::read(&file_path)
        .await
        .map_err(|e| AppError::NotFound(format!("Asset file not found: {}", e)))?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, content_type)],
        data,
    ))
}
