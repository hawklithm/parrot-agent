//! Attachment routes —— 对齐 Paperclip `server/src/routes/issues.ts` 的附件段落。
//!
//! 关键点：
//! - 上传走真实 multipart，单文件、大小上限、内容类型白名单，落盘失败/入库失败均回滚。
//! - content 响应对齐 Paperclip：Content-Type / Content-Length / Content-Disposition /
//!   Cache-Control / X-Content-Type-Options，SVG 额外挂 CSP 沙箱，支持 Range(206/416)。
//! - 所有 mutation 写 activity log。

use axum::{
    extract::{DefaultBodyLimit, Extension, Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use models::issue_auxiliary::{Attachment, UploadAttachmentInput};
use services::attachment_types::{
    content_disposition, max_attachment_bytes, parse_range_header, RangeSpec,
    SVG_CONTENT_SECURITY_POLICY, SVG_CONTENT_TYPE,
};
use services::auth::AuthorizationActor;

use crate::routes::{require_company_access, AccessMode};

/// Attachment 路由的访问语义表。handler 直接消费本表，测试也基于本表断言，
/// 避免「代码改了但权限测试还在测旧语义」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachmentOp {
    List,
    UploadJson,
    UploadMultipart,
    GetContent,
    Delete,
}

impl AttachmentOp {
    pub(crate) const fn access(self) -> AccessMode {
        match self {
            Self::List | Self::GetContent => AccessMode::Read,
            Self::UploadJson | Self::UploadMultipart | Self::Delete => AccessMode::Write,
        }
    }
}

fn forbidden(_: StatusCode) -> AppError {
    AppError::Forbidden("No access to this company".to_string())
}

/// 追加 `contentPath` 字段，与 Paperclip `withContentPath` 一致。
fn with_content_path(attachment: &Attachment) -> serde_json::Value {
    let mut value = serde_json::to_value(attachment).unwrap_or_else(|_| json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "contentPath".to_string(),
            json!(format!("/api/attachments/{}/content", attachment.id)),
        );
    }
    value
}

async fn issue_company_id(state: &AppState, issue_id: Uuid) -> Result<Uuid, AppError> {
    sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))
}

/// GET /issues/:id/attachments - List issue attachments
async fn list_issue_attachments(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let company_id = issue_company_id(&state, id).await?;
    require_company_access(&actor, company_id, AttachmentOp::List.access()).map_err(forbidden)?;

    let attachments = state
        .attachment_service
        .list_attachments("issue", id, company_id)
        .await?;
    Ok(Json(attachments.iter().map(with_content_path).collect()))
}

/// POST /issues/:id/attachments - JSON 形式上传（保留既有契约，供内部/Agent 调用）
async fn upload_issue_attachment_json(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(input): Json<UploadAttachmentInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let company_id = issue_company_id(&state, id).await?;
    require_company_access(&actor, company_id, AttachmentOp::UploadJson.access()).map_err(forbidden)?;

    let attachment = state
        .attachment_service
        .upload_attachment("issue", id, company_id, input)
        .await?;

    log_attachment_added(&state, company_id, &actor, id, &attachment).await;
    Ok((StatusCode::CREATED, Json(with_content_path(&attachment))))
}

/// POST /companies/:company_id/issues/:issue_id/attachments
/// 真实 multipart 上传，对应 Paperclip 的同名端点。
async fn upload_issue_attachment_multipart(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_company_access(&actor, company_id, AttachmentOp::UploadMultipart.access()).map_err(forbidden)?;

    let issue_company_id = issue_company_id(&state, issue_id).await?;
    if issue_company_id != company_id {
        return Err(AppError::Unprocessable(
            "Issue does not belong to company".to_string(),
        ));
    }

    let file = read_single_file_field(multipart, max_attachment_bytes()).await?;
    let attachment = state
        .attachment_service
        .upload_attachment(
            "issue",
            issue_id,
            company_id,
            UploadAttachmentInput {
                filename: file.filename,
                content_type: file.content_type,
                size: file.data.len() as i64,
                content: file.data,
            },
        )
        .await?;

    log_attachment_added(&state, company_id, &actor, issue_id, &attachment).await;
    Ok((StatusCode::CREATED, Json(with_content_path(&attachment))))
}

#[derive(Debug, Deserialize, Default)]
struct ContentQuery {
    #[serde(default)]
    download: Option<String>,
}

fn is_truthy(value: Option<&String>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

/// GET /attachments/:id/content - Get attachment content
async fn get_attachment_content(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Query(query): Query<ContentQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT company_id, filename, content_type FROM attachments WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let (company_id, filename, stored_content_type) =
        row        .ok_or_else(|| AppError::NotFound("Attachment not found".to_string()))?;
    require_company_access(&actor, company_id, AttachmentOp::GetContent.access()).map_err(forbidden)?;

    let data = state
        .attachment_service
        .get_attachment_content(id, company_id)
        .await?;

    Ok(build_content_response(
        data,
        &stored_content_type,
        Some(&filename),
        "attachment",
        is_truthy(query.download.as_ref()),
        headers.get(header::RANGE).and_then(|v| v.to_str().ok()),
    ))
}

/// DELETE /attachments/:id - Delete attachment
async fn delete_attachment(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT company_id, parent_id, filename FROM attachments WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let (company_id, parent_id, filename) =
        row        .ok_or_else(|| AppError::NotFound("Attachment not found".to_string()))?;
    require_company_access(&actor, company_id, AttachmentOp::Delete.access()).map_err(forbidden)?;

    state.attachment_service.delete_attachment(id, company_id).await?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "issue.attachment_removed",
        &actor,
        "issue",
        parent_id,
        json!({ "attachmentId": id, "filename": filename }),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn log_attachment_added(
    state: &AppState,
    company_id: Uuid,
    actor: &AuthorizationActor,
    issue_id: Uuid,
    attachment: &Attachment,
) {
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "issue.attachment_added",
        actor,
        "issue",
        issue_id,
        json!({
            "attachmentId": attachment.id,
            "originalFilename": attachment.filename,
            "contentType": attachment.content_type,
            "byteSize": attachment.size,
        }),
    )
    .await;
}

// ---------------------------------------------------------------------------
// 共享工具：multipart 单文件读取 + content 响应构造
// ---------------------------------------------------------------------------

pub(crate) struct UploadedFile {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

/// 从 multipart 中读取唯一的文件字段。
///
/// 与 Paperclip `multer({ limits: { files: 1 } })` 对齐：
/// - 缺少文件字段 → 400
/// - 出现第二个文件字段 → 400
/// - 单个文件超过上限 → 422
pub(crate) async fn read_single_file_field(
    mut multipart: Multipart,
    max_bytes: i64,
) -> Result<UploadedFile, AppError> {
    let mut found: Option<UploadedFile> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart: {}", e)))?
    {
        // 非文件字段（元数据）直接跳过，保持与 Paperclip `req.body` 的宽松行为一致。
        let Some(filename) = field.file_name().map(|s| s.to_string()) else {
            let _ = field.bytes().await;
            continue;
        };
        if found.is_some() {
            return Err(AppError::BadRequest(
                "Only a single file field is supported".to_string(),
            ));
        }
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to read field data: {}", e)))?;
        if data.len() as i64 > max_bytes {
            return Err(AppError::Unprocessable(format!(
                "File exceeds {} bytes",
                max_bytes
            )));
        }
        found = Some(UploadedFile {
            filename,
            content_type,
            data: data.to_vec(),
        });
    }

    found.ok_or_else(|| AppError::BadRequest("Missing file field 'file'".to_string()))
}

/// 构造带完整语义响应头的二进制响应（支持 Range → 206 / 416）。
pub(crate) fn build_content_response(
    data: Vec<u8>,
    content_type: &str,
    filename: Option<&str>,
    fallback_filename: &str,
    force_download: bool,
    range_header: Option<&str>,
) -> Response {
    let total = data.len() as i64;
    let mut headers = HeaderMap::new();
    insert_header(&mut headers, header::CONTENT_TYPE, content_type);
    insert_header(&mut headers, header::ACCEPT_RANGES, "bytes");
    insert_header(&mut headers, header::CACHE_CONTROL, "private, max-age=60");
    insert_header(&mut headers, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    insert_header(
        &mut headers,
        header::CONTENT_DISPOSITION,
        &content_disposition(content_type, filename, force_download, fallback_filename),
    );
    if content_type.eq_ignore_ascii_case(SVG_CONTENT_TYPE) {
        insert_header(
            &mut headers,
            header::CONTENT_SECURITY_POLICY,
            SVG_CONTENT_SECURITY_POLICY,
        );
    }

    match parse_range_header(range_header, total) {
        RangeSpec::Invalid => {
            insert_header(
                &mut headers,
                header::CONTENT_RANGE,
                &format!("bytes */{}", total),
            );
            (StatusCode::RANGE_NOT_SATISFIABLE, headers).into_response()
        }
        RangeSpec::Range { start, end } => {
            let slice = data[start as usize..=(end as usize)].to_vec();
            insert_header(
                &mut headers,
                header::CONTENT_RANGE,
                &format!("bytes {}-{}/{}", start, end, total),
            );
            insert_header(
                &mut headers,
                header::CONTENT_LENGTH,
                &(end - start + 1).to_string(),
            );
            (StatusCode::PARTIAL_CONTENT, headers, slice).into_response()
        }
        RangeSpec::Full => {
            insert_header(&mut headers, header::CONTENT_LENGTH, &total.to_string());
            (StatusCode::OK, headers, data).into_response()
        }
    }
}

fn insert_header(headers: &mut HeaderMap, name: header::HeaderName, value: &str) {
    if let Ok(v) = HeaderValue::from_str(value) {
        headers.insert(name, v);
    }
}

/// Create attachment routes (AppState compatible)
pub fn attachment_routes() -> Router<AppState> {
    // multipart body 需要放开 axum 默认 2MB 限制；留 1MiB 余量给 multipart 边界与元数据字段。
    let body_limit = (max_attachment_bytes() as usize).saturating_add(1024 * 1024);
    Router::new()
        .route(
            "/issues/:id/attachments",
            get(list_issue_attachments).post(upload_issue_attachment_json),
        )
        .route(
            "/companies/:company_id/issues/:issue_id/attachments",
            post(upload_issue_attachment_multipart).layer(DefaultBodyLimit::max(body_limit)),
        )
        .route("/attachments/:id/content", get(get_attachment_content))
        .route("/attachments/:id", delete(delete_attachment))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::access_test_support::{agent_of, anonymous, board_with_role};
    use services::auth::MembershipRole;

    #[test]
    fn truthy_query_values() {
        assert!(is_truthy(Some(&"1".to_string())));
        assert!(is_truthy(Some(&"TRUE".to_string())));
        assert!(is_truthy(Some(&"yes".to_string())));
        assert!(!is_truthy(Some(&"0".to_string())));
        assert!(!is_truthy(Some(&"".to_string())));
        assert!(!is_truthy(None));
    }

    #[test]
    fn full_response_sets_semantic_headers() {
        let resp = build_content_response(b"hello".to_vec(), "image/png", Some("a.png"), "attachment", false, None);
        assert_eq!(resp.status(), StatusCode::OK);
        let h = resp.headers();
        assert_eq!(h.get(header::CONTENT_TYPE).unwrap(), "image/png");
        assert_eq!(h.get(header::CONTENT_LENGTH).unwrap(), "5");
        assert_eq!(h.get(header::ACCEPT_RANGES).unwrap(), "bytes");
        assert_eq!(h.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(), "nosniff");
        assert_eq!(h.get(header::CONTENT_DISPOSITION).unwrap(), "inline; filename=\"a.png\"");
        assert!(h.get(header::CONTENT_SECURITY_POLICY).is_none());
    }

    #[test]
    fn non_inline_type_forces_attachment_disposition() {
        let resp = build_content_response(b"zip".to_vec(), "application/zip", Some("x.zip"), "attachment", false, None);
        assert_eq!(
            resp.headers().get(header::CONTENT_DISPOSITION).unwrap(),
            "attachment; filename=\"x.zip\""
        );
    }

    #[test]
    fn download_flag_forces_attachment_disposition() {
        let resp = build_content_response(b"png".to_vec(), "image/png", Some("a.png"), "attachment", true, None);
        assert_eq!(
            resp.headers().get(header::CONTENT_DISPOSITION).unwrap(),
            "attachment; filename=\"a.png\""
        );
    }

    #[test]
    fn svg_gets_csp_sandbox_header() {
        let resp = build_content_response(b"<svg/>".to_vec(), SVG_CONTENT_TYPE, Some("i.svg"), "asset", false, None);
        assert_eq!(
            resp.headers().get(header::CONTENT_SECURITY_POLICY).unwrap(),
            SVG_CONTENT_SECURITY_POLICY
        );
    }

    #[test]
    fn range_request_returns_206_with_content_range() {
        let resp = build_content_response(
            b"0123456789".to_vec(),
            "video/mp4",
            Some("v.mp4"),
            "attachment",
            false,
            Some("bytes=2-5"),
        );
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        let h = resp.headers();
        assert_eq!(h.get(header::CONTENT_RANGE).unwrap(), "bytes 2-5/10");
        assert_eq!(h.get(header::CONTENT_LENGTH).unwrap(), "4");
    }

    #[test]
    fn unsatisfiable_range_returns_416() {
        let resp = build_content_response(
            b"0123456789".to_vec(),
            "video/mp4",
            Some("v.mp4"),
            "attachment",
            false,
            Some("bytes=50-60"),
        );
        assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(resp.headers().get(header::CONTENT_RANGE).unwrap(), "bytes */10");
    }

    #[test]
    fn content_path_is_appended_to_attachment_json() {
        let attachment = Attachment {
            id: Uuid::nil(),
            parent_type: "issue".into(),
            parent_id: Uuid::nil(),
            company_id: Uuid::nil(),
            asset_id: None,
            filename: "a.png".into(),
            content_type: "image/png".into(),
            size: 3,
            created_at: chrono::Utc::now(),
        };
        let value = with_content_path(&attachment);
        assert_eq!(
            value["contentPath"],
            json!(format!("/api/attachments/{}/content", Uuid::nil()))
        );
        assert_eq!(value["filename"], json!("a.png"));
    }

    // ---- 权限矩阵测试（与 work_products 同构，共用 access_test_support） ----

    const ALL_OPS: [AttachmentOp; 5] = [
        AttachmentOp::List,
        AttachmentOp::UploadJson,
        AttachmentOp::UploadMultipart,
        AttachmentOp::GetContent,
        AttachmentOp::Delete,
    ];

    #[test]
    fn reads_are_list_and_content_only() {
        assert_eq!(AttachmentOp::List.access(), AccessMode::Read);
        assert_eq!(AttachmentOp::GetContent.access(), AccessMode::Read);
        for op in [
            AttachmentOp::UploadJson,
            AttachmentOp::UploadMultipart,
            AttachmentOp::Delete,
        ] {
            assert_eq!(op.access(), AccessMode::Write, "{op:?} must be a write op");
        }
    }

    #[test]
    fn viewer_can_only_read_attachments() {
        let company = Uuid::new_v4();
        let viewer = board_with_role(company, MembershipRole::Viewer);
        for op in ALL_OPS {
            let allowed = require_company_access(&viewer, company, op.access()).is_ok();
            assert_eq!(
                allowed,
                matches!(op, AttachmentOp::List | AttachmentOp::GetContent),
                "viewer access mismatch for {op:?}"
            );
        }
    }

    #[test]
    fn owner_admin_operator_and_agent_can_mutate_attachments() {
        let company = Uuid::new_v4();
        for actor in [
            board_with_role(company, MembershipRole::Owner),
            board_with_role(company, MembershipRole::Admin),
            board_with_role(company, MembershipRole::Operator),
            agent_of(company),
        ] {
            for op in ALL_OPS {
                assert!(
                    require_company_access(&actor, company, op.access()).is_ok(),
                    "{op:?} should be allowed"
                );
            }
        }
    }

    #[test]
    fn cross_company_and_anonymous_are_rejected_for_all_ops() {
        let company = Uuid::new_v4();
        let other = Uuid::new_v4();
        for actor in [
            board_with_role(other, MembershipRole::Owner),
            agent_of(other),
            anonymous(),
        ] {
            for op in ALL_OPS {
                assert_eq!(
                    require_company_access(&actor, company, op.access()),
                    Err(StatusCode::FORBIDDEN),
                    "{op:?} must be forbidden"
                );
            }
        }
    }
}
