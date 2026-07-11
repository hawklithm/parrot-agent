use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::{
    CreateRoutineAnnotationCommentRequest, CreateRoutineAnnotationThreadRequest,
    UpdateRoutineAnnotationThreadRequest,
};
use services::RoutineAnnotationService;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListAnnotationsQuery {
    #[serde(default)]
    include_comments: bool,
}

/// GET /routines/:id/description/annotations - 获取routine的所有annotations
pub async fn list_annotations(
    Path(routine_id): Path<Uuid>,
    Query(query): Query<ListAnnotationsQuery>,
    State(service): State<Arc<dyn RoutineAnnotationService>>,
) -> Response {
    // TODO: Add permission check - assertCanReadRoutine(routine_id, auth)

    match service
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
    State(service): State<Arc<dyn RoutineAnnotationService>>,
    Json(request): Json<CreateRoutineAnnotationThreadRequest>,
) -> Response {
    // TODO: Add permission check - assertCanWriteRoutine(routine_id, auth)

    match service.create_annotation_thread(routine_id, request).await {
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
    State(service): State<Arc<dyn RoutineAnnotationService>>,
    Json(request): Json<CreateRoutineAnnotationCommentRequest>,
) -> Response {
    // TODO: Add permission check - assertCanWriteRoutine(routine_id, auth)

    match service.add_comment(routine_id, thread_id, request).await {
        Ok(comment) => (StatusCode::CREATED, Json(comment)).into_response(),
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
    State(service): State<Arc<dyn RoutineAnnotationService>>,
    Json(request): Json<UpdateRoutineAnnotationThreadRequest>,
) -> Response {
    // TODO: Add permission check - assertCanWriteRoutine(routine_id, auth)

    match service.update_thread(routine_id, thread_id, request).await {
        Ok(thread) => (StatusCode::OK, Json(thread)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// 创建Routine Annotation路由器
pub fn routine_annotation_routes(service: Arc<dyn RoutineAnnotationService>) -> axum::Router {
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
            axum::routing::patch(update_annotation_thread),
        )
        .with_state(service)
}
