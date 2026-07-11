use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use services::InviteResourceService;
use std::sync::Arc;

/// GET /api/invites/:token/logo - 返回公司Logo
pub async fn get_company_logo(
    Path(token): Path<String>,
    State(service): State<Arc<dyn InviteResourceService>>,
) -> Response {
    match service.get_company_logo(&token).await {
        Ok(logo) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, logo.content_type)],
            logo.data,
        )
            .into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e }))).into_response(),
    }
}

/// GET /api/invites/:token/onboarding - 返回onboarding文档（Markdown）
pub async fn get_onboarding(
    Path(token): Path<String>,
    State(service): State<Arc<dyn InviteResourceService>>,
) -> Response {
    match service.get_onboarding(&token).await {
        Ok(manifest) => (StatusCode::OK, Json(manifest)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e }))).into_response(),
    }
}

/// GET /api/invites/:token/onboarding.txt - 返回纯文本版本
pub async fn get_onboarding_text(
    Path(token): Path<String>,
    State(service): State<Arc<dyn InviteResourceService>>,
) -> Response {
    match service.get_onboarding_text(&token).await {
        Ok(text) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/plain".to_string())],
            text,
        )
            .into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e }))).into_response(),
    }
}

/// GET /api/invites/:token/skills/index - 邀请范围内的技能索引
pub async fn get_skills_index(
    Path(token): Path<String>,
    State(service): State<Arc<dyn InviteResourceService>>,
) -> Response {
    match service.get_skills_index(&token).await {
        Ok(index) => (StatusCode::OK, Json(index)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e }))).into_response(),
    }
}

/// GET /api/invites/:token/skills/:skillName - 技能详情
pub async fn get_skill_details(
    Path((token, skill_name)): Path<(String, String)>,
    State(service): State<Arc<dyn InviteResourceService>>,
) -> Response {
    match service.get_skill_details(&token, &skill_name).await {
        Ok(details) => (StatusCode::OK, Json(details)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e }))).into_response(),
    }
}

/// 创建Invite资源路由器
pub fn invite_resource_routes(service: Arc<dyn InviteResourceService>) -> axum::Router {
    axum::Router::new()
        .route("/api/invites/:token/logo", axum::routing::get(get_company_logo))
        .route("/api/invites/:token/onboarding", axum::routing::get(get_onboarding))
        .route("/api/invites/:token/onboarding.txt", axum::routing::get(get_onboarding_text))
        .route("/api/invites/:token/skills/index", axum::routing::get(get_skills_index))
        .route("/api/invites/:token/skills/:skillName", axum::routing::get(get_skill_details))
        .with_state(service)
}
