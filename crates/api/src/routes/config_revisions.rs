use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::AppError;
use crate::routes::agents::AppState;

/// 配置版本查询参数
#[derive(Debug, Deserialize)]
pub struct RevisionListQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// 配置版本差异查询参数
#[derive(Debug, Deserialize)]
pub struct CompareDiffQuery {
    pub compare_with: Uuid,
}

/// 配置版本响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionResponse {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub snapshot: serde_json::Value,
    pub created_at: String,
}

/// 配置版本列表响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionListResponse {
    pub revisions: Vec<RevisionResponse>,
    pub total: i64,
}

/// 创建配置版本路由
pub fn config_revision_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/agents/:id/config-revisions",
            get(list_config_revisions),
        )
        .route(
            "/agents/:id/config-revisions/:revision_id",
            get(get_config_revision),
        )
        .route(
            "/agents/:id/config-revisions/:revision_id/diff",
            get(compare_config_revisions),
        )
}

/// GET /agents/:id/config-revisions - 查询Agent的配置版本列表
async fn list_config_revisions(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(params): Query<RevisionListQuery>,
) -> Result<impl IntoResponse, AppError> {
    // TODO: 验证权限（agent_config:read）

    let revisions = state
        .config_revision_service
        .list_revisions(agent_id, params.limit, params.offset)
        .await
        .map_err(|_e| AppError::Internal)?;

    let total = state
        .config_revision_service
        .count_revisions(agent_id)
        .await
        .map_err(|_e| AppError::Internal)?;

    let response = RevisionListResponse {
        revisions: revisions
            .into_iter()
            .map(|rev| RevisionResponse {
                id: rev.id,
                agent_id: rev.agent_id,
                snapshot: rev.snapshot.0,
                created_at: rev.created_at.to_rfc3339(),
            })
            .collect(),
        total,
    };

    Ok(Json(response))
}

/// GET /agents/:id/config-revisions/:revision_id - 获取特定配置版本
async fn get_config_revision(
    State(state): State<AppState>,
    Path((agent_id, revision_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    // TODO: 验证权限（agent_config:read）

    let revision = state
        .config_revision_service
        .get_revision(revision_id)
        .await
        .map_err(|_e| AppError::NotFound(format!("Config revision {} not found", revision_id)))?;

    // 验证revision属于指定的agent
    if revision.agent_id != agent_id {
        return Err(AppError::NotFound(format!(
            "Config revision {} not found for agent {}",
            revision_id, agent_id
        )));
    }

    let response = RevisionResponse {
        id: revision.id,
        agent_id: revision.agent_id,
        snapshot: revision.snapshot.0,
        created_at: revision.created_at.to_rfc3339(),
    };

    Ok(Json(response))
}

/// GET /agents/:id/config-revisions/:revision_id/diff - 比较配置版本差异
async fn compare_config_revisions(
    State(state): State<AppState>,
    Path((agent_id, revision_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<CompareDiffQuery>,
) -> Result<impl IntoResponse, AppError> {
    // TODO: 验证权限（agent_config:read）

    let diff = state
        .config_revision_service
        .compare_revisions(revision_id, params.compare_with)
        .await
        .map_err(|_e| AppError::Internal)?;

    // 验证两个revision都属于指定的agent
    let rev1 = state
        .config_revision_service
        .get_revision(revision_id)
        .await
        .map_err(|_| AppError::NotFound(format!("Config revision {} not found", revision_id)))?;
    let rev2 = state
        .config_revision_service
        .get_revision(params.compare_with)
        .await
        .map_err(|_| {
            AppError::NotFound(format!("Config revision {} not found", params.compare_with))
        })?;

    if rev1.agent_id != agent_id || rev2.agent_id != agent_id {
        return Err(AppError::BadRequest(
            "Both revisions must belong to the same agent".to_string(),
        ));
    }

    Ok(Json(diff))
}
