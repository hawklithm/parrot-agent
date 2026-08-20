//! Asset routes — Paperclip 一比一迁移
//!
//! 对应 Paperclip: server/src/routes/assets.ts
//!
//! 与 Paperclip 对齐的行为：
//! - 上传走真实 multipart（单文件），大小上限 / 内容类型白名单 / 空文件均返回 422。
//! - SVG 走安全检查（Paperclip 用 DOMPurify sanitize，这里做保守拒绝）。
//! - 落盘与入库任一失败都要回滚，避免孤儿对象或孤儿元数据。
//! - 读取 content 时校验 company access，并输出完整响应头（含 SVG 的 CSP 沙箱）。
//! - 所有 mutation 写 activity log（`asset.created`）。

use axum::{
    extract::{DefaultBodyLimit, Extension, Multipart, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use crate::routes::attachments::{build_content_response, read_single_file_field};
use services::asset_storage::{LocalStorageService, PutFileRequest, StorageService};
use services::attachment_types::{
    is_allowed_content_type, is_safe_svg, max_attachment_bytes,
    normalize_upload_attachment_content_type, ALLOWED_COMPANY_LOGO_CONTENT_TYPES, SVG_CONTENT_TYPE,
};
use services::auth::AuthorizationActor;

pub fn asset_routes() -> Router<AppState> {
    let body_limit = (max_attachment_bytes() as usize).saturating_add(1024 * 1024);
    Router::new()
        .route(
            "/companies/:company_id/assets/images",
            post(upload_asset_image).layer(DefaultBodyLimit::max(body_limit)),
        )
        .route(
            "/companies/:company_id/logo",
            post(upload_company_logo).layer(DefaultBodyLimit::max(body_limit)),
        )
        .route("/assets/:asset_id/content", get(get_asset_content))
}

#[derive(Debug, Deserialize, Default)]
struct ImageUploadQuery {
    /// 与 Paperclip `createAssetImageMetadataSchema.namespace` 对齐
    #[serde(default)]
    namespace: Option<String>,
}

/// POST /companies/:company_id/assets/images
/// 上传资产图片。
/// 对应 Paperclip: assetRoutes -> POST /companies/:companyId/assets/images
async fn upload_asset_image(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ImageUploadQuery>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| AppError::Forbidden("No access to this company".to_string()))?;

    let file = read_single_file_field(multipart, max_attachment_bytes()).await?;
    let content_type =
        normalize_upload_attachment_content_type(Some(&file.content_type), Some(&file.filename));

    // Paperclip 对 images 端点允许 SVG + 通用附件白名单
    if content_type != SVG_CONTENT_TYPE && !is_allowed_content_type(&content_type) {
        return Err(AppError::Unprocessable(format!(
            "Unsupported file type: {}",
            content_type
        )));
    }
    let body = validate_image_body(&content_type, file.data)?;

    let namespace = format!(
        "assets/{}",
        query.namespace.as_deref().unwrap_or("general")
    );
    let asset = store_asset(
        &state,
        &actor,
        company_id,
        &namespace,
        &content_type,
        Some(&file.filename),
        body,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(asset)))
}

/// POST /companies/:company_id/logo
/// 上传公司 Logo。
/// 对应 Paperclip: assetRoutes -> POST /companies/:companyId/logo
async fn upload_company_logo(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| AppError::Forbidden("No access to this company".to_string()))?;

    let file = read_single_file_field(multipart, max_attachment_bytes()).await?;
    let content_type =
        normalize_upload_attachment_content_type(Some(&file.content_type), Some(&file.filename));
    if !ALLOWED_COMPANY_LOGO_CONTENT_TYPES.contains(&content_type.as_str()) {
        return Err(AppError::Unprocessable(format!(
            "Unsupported image type: {}",
            content_type
        )));
    }
    let body = validate_image_body(&content_type, file.data)?;

    let asset = store_asset(
        &state,
        &actor,
        company_id,
        "assets/companies",
        &content_type,
        Some(&file.filename),
        body,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(asset)))
}

#[derive(Debug, Deserialize, Default)]
struct ContentQuery {
    #[serde(default)]
    download: Option<String>,
}

/// GET /assets/:asset_id/content
/// 获取资产内容。
/// 对应 Paperclip: assetRoutes -> GET /assets/:assetId/content
async fn get_asset_content(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(asset_id): Path<Uuid>,
    Query(query): Query<ContentQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let row: Option<(Uuid, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT company_id, object_key, content_type, original_filename
        FROM assets
        WHERE id = $1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(&state.pool)
    .await?;

    let (company_id, object_key, content_type, original_filename) =
        row.ok_or_else(|| AppError::NotFound(format!("Asset not found: {}", asset_id)))?;

    crate::routes::assert_company_access(&actor, company_id, true)
        .map_err(|_| AppError::Forbidden("No access to this company".to_string()))?;

    let storage = LocalStorageService::from_env();
    let data = storage.get_object(company_id, &object_key).await?;

    let force_download = matches!(
        query.download.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes" | "on")
    );
    Ok(build_content_response(
        data,
        &content_type,
        original_filename.as_deref(),
        "asset",
        force_download,
        headers.get(header::RANGE).and_then(|v| v.to_str().ok()),
    ))
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// 空文件 → 422；SVG 走安全检查，不通过 → 422。
fn validate_image_body(content_type: &str, body: Vec<u8>) -> Result<Vec<u8>, AppError> {
    if body.is_empty() {
        return Err(AppError::Unprocessable("Image is empty".to_string()));
    }
    if content_type.eq_ignore_ascii_case(SVG_CONTENT_TYPE) && !is_safe_svg(&body) {
        return Err(AppError::Unprocessable(
            "SVG could not be sanitized".to_string(),
        ));
    }
    Ok(body)
}

/// 落盘 + 入库 + 审计。入库失败时删除已落盘对象（存储回滚）。
async fn store_asset(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    namespace: &str,
    content_type: &str,
    original_filename: Option<&str>,
    body: Vec<u8>,
) -> Result<serde_json::Value, AppError> {
    let storage = LocalStorageService::from_env();
    let stored = storage
        .put_file(PutFileRequest {
            company_id,
            namespace: namespace.to_string(),
            original_filename: original_filename.map(|s| s.to_string()),
            content_type: content_type.to_string(),
            body,
        })
        .await?;

    let asset_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let insert = sqlx::query(
        r#"
        INSERT INTO assets (id, company_id, provider, object_key, content_type, byte_size, sha256, original_filename, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        "#,
    )
    .bind(asset_id)
    .bind(company_id)
    .bind(&stored.provider)
    .bind(&stored.object_key)
    .bind(&stored.content_type)
    .bind(stored.byte_size)
    .bind(&stored.sha256)
    .bind(&stored.original_filename)
    .bind(now)
    .bind(now)
    .execute(&state.pool)
    .await;

    if let Err(err) = insert {
        // 存储回滚：元数据没写成功就不能留下孤儿对象
        let _ = storage.delete_object(company_id, &stored.object_key).await;
        return Err(AppError::from(err));
    }

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "asset.created",
        actor,
        "asset",
        asset_id,
        json!({
            "originalFilename": stored.original_filename,
            "contentType": stored.content_type,
            "byteSize": stored.byte_size,
            "namespace": namespace,
        }),
    )
    .await;

    Ok(json!({
        "assetId": asset_id,
        "companyId": company_id,
        "provider": stored.provider,
        "objectKey": stored.object_key,
        "contentType": stored.content_type,
        "byteSize": stored.byte_size,
        "sha256": stored.sha256,
        "originalFilename": stored.original_filename,
        "createdAt": now,
        "updatedAt": now,
        "contentPath": format!("/api/assets/{}/content", asset_id),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    #[test]
    fn empty_image_is_rejected() {
        let err = validate_image_body("image/png", Vec::new()).unwrap_err();
        assert!(matches!(err, AppError::Unprocessable(_)));
        assert_eq!(err.into_response().status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn unsafe_svg_is_rejected() {
        let err = validate_image_body(SVG_CONTENT_TYPE, br#"<svg onload="x()"/>"#.to_vec()).unwrap_err();
        assert!(matches!(err, AppError::Unprocessable(_)));
    }

    #[test]
    fn safe_svg_passes_through_unchanged() {
        let body = br#"<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>"#.to_vec();
        let out = validate_image_body(SVG_CONTENT_TYPE, body.clone()).unwrap();
        assert_eq!(out, body);
    }

    #[test]
    fn png_body_passes_through() {
        let out = validate_image_body("image/png", vec![1, 2, 3]).unwrap();
        assert_eq!(out, vec![1, 2, 3]);
    }
}
