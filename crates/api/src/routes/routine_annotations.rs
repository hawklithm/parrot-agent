use crate::app_state::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use models::{
    CreateRoutineAnnotationCommentRequest, CreateRoutineAnnotationThreadRequest,
    UpdateRoutineAnnotationThreadRequest,
};
use services::auth::AuthorizationActor;
use uuid::Uuid;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAnnotationsQuery {
    #[serde(default)]
    include_comments: bool,
}

/// GET /routines/:id/description/annotations - 获取routine的所有annotations
pub async fn list_annotations(
    Path(routine_id): Path<Uuid>,
    Query(query): Query<ListAnnotationsQuery>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if !routine_access(&state, &actor, routine_id, true).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state
        .routine_annotation_service
        .list_annotations(routine_id, query.include_comments)
        .await
    {
        Ok(threads) => (StatusCode::OK, Json(threads)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// POST /routines/:id/description/annotations - 创建新annotation thread
pub async fn create_annotation_thread(
    Path(routine_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(request): Json<CreateRoutineAnnotationThreadRequest>,
) -> Response {
    if !routine_access(&state, &actor, routine_id, false).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state
        .routine_annotation_service
        .create_annotation_thread(routine_id, request)
        .await
    {
        Ok(thread) => (StatusCode::CREATED, Json(thread)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// POST /routines/:id/description/annotations/:threadId/comments - 添加评论到thread
pub async fn add_annotation_comment(
    Path((routine_id, thread_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(request): Json<CreateRoutineAnnotationCommentRequest>,
) -> Response {
    if !routine_access(&state, &actor, routine_id, false).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state
        .routine_annotation_service
        .add_comment(routine_id, thread_id, request)
        .await
    {
        Ok(comment) => (StatusCode::CREATED, Json(comment)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// GET /routines/:id/description/annotations/:threadId - 获取单个 annotation thread
/// 对齐 Paperclip `GET /routines/:id/description/annotations/:threadId`。
pub async fn get_annotation_thread(
    Path((routine_id, thread_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if !routine_access(&state, &actor, routine_id, true).await {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .routine_annotation_service
        .get_thread(routine_id, thread_id)
        .await
    {
        Ok(Some(thread)) => (StatusCode::OK, Json(thread)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Annotation thread not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// PATCH /routines/:id/description/annotations/:threadId - 更新thread状态
pub async fn update_annotation_thread(
    Path((routine_id, thread_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(request): Json<UpdateRoutineAnnotationThreadRequest>,
) -> Response {
    if !routine_access(&state, &actor, routine_id, false).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state
        .routine_annotation_service
        .update_thread(routine_id, thread_id, request)
        .await
    {
        Ok(thread) => (StatusCode::OK, Json(thread)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn routine_access(
    state: &AppState,
    actor: &AuthorizationActor,
    routine_id: Uuid,
    read_only: bool,
) -> bool {
    let company_id = sqlx::query_scalar::<_, Uuid>("SELECT company_id FROM routines WHERE id = $1")
        .bind(routine_id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();
    company_id
        .map(|company_id| {
            crate::routes::assert_company_access(actor, company_id, !read_only).is_ok()
        })
        .unwrap_or(false)
}

/// 创建Routine Annotation路由器
pub fn routine_annotation_routes() -> Router<AppState> {
    axum::Router::new()
        .route(
            "/routines/:id/description/annotations",
            axum::routing::get(list_annotations).post(create_annotation_thread),
        )
        .route(
            "/routines/:id/description/annotations/:threadId/comments",
            axum::routing::post(add_annotation_comment),
        )
        .route(
            "/routines/:id/description/annotations/:threadId",
            // 对齐 Paperclip：GET 获取单线程 + PATCH 更新状态
            axum::routing::get(get_annotation_thread).patch(update_annotation_thread),
        )
}
