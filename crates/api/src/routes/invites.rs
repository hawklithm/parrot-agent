use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use axum::response::Json;
use models::skill::{SkillDetail, SkillIndexEntry};
use services::invite_service::InviteService;
use std::sync::Arc;

/// GET /invites/:token/logo
/// Get company logo for invite
pub async fn get_invite_logo(
    Path(token): Path<String>,
    State(service): State<Arc<dyn InviteService>>,
) -> Result<Response, StatusCode> {
    let logo_bytes = service
        .get_invite_logo(&token)
        .await
        .map_err(|e| {
            if matches!(e, services::ServiceError::Unauthorized(_)) {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(logo_bytes))
        .unwrap())
}

/// GET /invites/:token/onboarding
/// Get onboarding documentation (Markdown)
pub async fn get_invite_onboarding(
    Path(token): Path<String>,
    State(service): State<Arc<dyn InviteService>>,
) -> Result<Response, StatusCode> {
    let markdown = service
        .get_invite_onboarding(&token)
        .await
        .map_err(|e| {
            if matches!(e, services::ServiceError::Unauthorized(_)) {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/markdown; charset=utf-8")
        .body(Body::from(markdown))
        .unwrap())
}

/// GET /invites/:token/onboarding.txt
/// Get onboarding documentation (plain text)
pub async fn get_invite_onboarding_text(
    Path(token): Path<String>,
    State(service): State<Arc<dyn InviteService>>,
) -> Result<Response, StatusCode> {
    let text = service
        .get_invite_onboarding_text(&token)
        .await
        .map_err(|e| {
            if matches!(e, services::ServiceError::Unauthorized(_)) {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(text))
        .unwrap())
}

/// GET /invites/:token/skills/index
/// Get skills index for invite scope
pub async fn get_invite_skills_index(
    Path(token): Path<String>,
    State(service): State<Arc<dyn InviteService>>,
) -> Result<Json<Vec<SkillIndexEntry>>, StatusCode> {
    service
        .get_invite_skills_index(&token)
        .await
        .map(Json)
        .map_err(|e| {
            if matches!(e, services::ServiceError::Unauthorized(_)) {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })
}

/// GET /invites/:token/skills/:skillName
/// Get specific skill details for invite scope
pub async fn get_invite_skill_detail(
    Path((token, skill_name)): Path<(String, String)>,
    State(service): State<Arc<dyn InviteService>>,
) -> Result<Json<SkillDetail>, StatusCode> {
    service
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

/// Register invite sub-resource routes
pub fn invite_subresource_routes() -> Router<Arc<dyn InviteService>> {
    Router::new()
        .route("/invites/:token/skills/:skill_name", get(get_invite_skill_detail))
}
