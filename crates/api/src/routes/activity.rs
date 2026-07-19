//! Activity routes — Paperclip 一比一迁移
//!
//! 对应 Paperclip: server/src/routes/activity.ts
//! 提供活动日志的查询和创建端点。

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;

/// 活动查询参数
#[derive(Debug, Deserialize)]
pub struct ActivityQueryParams {
    #[allow(dead_code)]
    actor_id: Option<Uuid>,
    #[allow(dead_code)]
    entity_type: Option<String>,
    #[allow(dead_code)]
    entity_id: Option<Uuid>,
    limit: Option<i64>,
}

/// 创建活动请求体
#[derive(Debug, Deserialize)]
pub struct CreateActivityRequest {
    actor_type: String,
    actor_id: Uuid,
    action: String,
    entity_type: String,
    entity_id: Uuid,
    #[allow(dead_code)]
    agent_id: Option<Uuid>,
    details: Option<serde_json::Value>,
}

/// 活动日志数据库行
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
struct ActivityRow {
    id: Uuid,
    company_id: Uuid,
    actor_type: String,
    actor_id: Uuid,
    action: String,
    resource_type: String,
    resource_id: Uuid,
    metadata: Option<serde_json::Value>,
    created_at: DateTime<Utc>,
}

pub fn activity_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/activity", get(list_company_activity).post(create_activity))
        .route("/issues/:id/activity", get(get_issue_activity))
}

/// GET /companies/:company_id/activity
/// 列出公司活动日志。
/// 对应 Paperclip: activityRoutes -> GET /companies/:companyId/activity
async fn list_company_activity(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<ActivityQueryParams>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let limit = params.limit.unwrap_or(50);

    let rows = sqlx::query_as::<_, ActivityRow>(
        r#"
SELECT id, company_id, actor_type, actor_id, event_type AS action, resource_type, resource_id, metadata, created_at
        FROM activity_logs
        WHERE company_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(company_id)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to query activity: {}", e)))?;

    let result: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "companyId": r.company_id,
                "actorType": r.actor_type,
                "actorId": r.actor_id,
                "action": r.action,
                "resourceType": r.resource_type,
                "resourceId": r.resource_id,
                "metadata": r.metadata,
                "createdAt": r.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}

/// POST /companies/:company_id/activity
/// 创建活动日志。
/// 对应 Paperclip: activityRoutes -> POST /companies/:companyId/activity
async fn create_activity(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<CreateActivityRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        r#"
INSERT INTO activity_logs (id, company_id, actor_type, actor_id, event_type, resource_type, resource_id, metadata, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(id)
    .bind(company_id)
    .bind(&body.actor_type)
    .bind(body.actor_id)
    .bind(&body.action)
    .bind(&body.entity_type)
    .bind(body.entity_id)
    .bind(&body.details)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to create activity: {}", e)))?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "id": id,
        "companyId": company_id,
        "created": true,
    }))))
}

/// GET /issues/:id/activity
/// 获取议题活动日志。
/// 对应 Paperclip: activityRoutes -> GET /issues/:id/activity
async fn get_issue_activity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let rows = sqlx::query_as::<_, ActivityRow>(
        r#"
SELECT id, company_id, actor_type, actor_id, event_type AS action, resource_type, resource_id, metadata, created_at
        FROM activity_logs
        WHERE resource_type = 'issue' AND resource_id = $1
        ORDER BY created_at DESC
        LIMIT 50
        "#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to query issue activity: {}", e)))?;

    let result: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "companyId": r.company_id,
                "actorType": r.actor_type,
                "actorId": r.actor_id,
                "action": r.action,
                "resourceType": r.resource_type,
                "resourceId": r.resource_id,
                "metadata": r.metadata,
                "createdAt": r.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}
