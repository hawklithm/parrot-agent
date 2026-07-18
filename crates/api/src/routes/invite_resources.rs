use crate::app_state::AppState;
use axum::{Router,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use models::skill::SkillDetail;

/// GET /api/invites/:token/logo - 返回公司Logo
pub async fn get_company_logo(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Response {
    match state.invite_resource_service.get_company_logo(&token).await {
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
    State(state): State<AppState>,
) -> Response {
    match state.invite_resource_service.get_onboarding(&token).await {
        Ok(manifest) => (StatusCode::OK, Json(manifest)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e }))).into_response(),
    }
}

/// GET /api/invites/:token/onboarding.txt - 返回纯文本版本
pub async fn get_onboarding_text(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Response {
    match state.invite_resource_service.get_onboarding_text(&token).await {
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
    State(state): State<AppState>,
) -> Response {
    match state.invite_resource_service.get_skills_index(&token).await {
        Ok(index) => (StatusCode::OK, Json(index)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e }))).into_response(),
    }
}

/// GET /api/invites/:token/skills/:skillName - 技能详情
/// Uses InviteService (real implementation with token verification).
pub async fn get_skill_details(
    Path((token, skill_name)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<SkillDetail>, StatusCode> {
    state
        .invite_service
        .get_invite_skill_detail(&token, &skill_name)
        .await
        .map(Json)
        .map_err(|e| {
            if matches!(e, services::ServiceError::Unauthorized(_)) {
                StatusCode::UNAUTHORIZED
            } else if matches!(e, services::ServiceError::NotFound(_)) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })
}

/// 创建Invite资源路由器
pub fn invite_resource_routes() -> Router<AppState> {
    axum::Router::new()
        .route("/invites/:token/logo", axum::routing::get(get_company_logo))
        .route("/invites/:token/onboarding", axum::routing::get(get_onboarding))
        .route("/invites/:token/onboarding.txt", axum::routing::get(get_onboarding_text))
        .route("/invites/:token/skills/index", axum::routing::get(get_skills_index))
        .route("/invites/:token/skills/:skillName", axum::routing::get(get_skill_details))
}
