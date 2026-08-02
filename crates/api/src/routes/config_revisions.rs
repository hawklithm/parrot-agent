use axum::{
    extract::{Extension, Path, Query, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::AppError;
use crate::routes::agents::AppState;
use services::auth::{AuthorizationAction, AuthorizationActor};

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
        .route("/agents/:id/config-revisions", get(list_config_revisions))
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
    Extension(actor): Extension<AuthorizationActor>,
    Path(agent_id): Path<Uuid>,
    Query(params): Query<RevisionListQuery>,
) -> Result<impl IntoResponse, AppError> {
    assert_agent_config_read(&state, &actor, agent_id).await?;

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
    Extension(actor): Extension<AuthorizationActor>,
    Path((agent_id, revision_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    assert_agent_config_read(&state, &actor, agent_id).await?;

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
    Extension(actor): Extension<AuthorizationActor>,
    Path((agent_id, revision_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<CompareDiffQuery>,
) -> Result<impl IntoResponse, AppError> {
    assert_agent_config_read(&state, &actor, agent_id).await?;

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

async fn assert_agent_config_read(
    state: &AppState,
    actor: &AuthorizationActor,
    agent_id: Uuid,
) -> Result<(), AppError> {
    let company_id = sqlx::query_scalar::<_, Uuid>("SELECT company_id FROM agents WHERE id = $1")
        .bind(agent_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|error| {
            AppError::InternalServerError(format!("Failed to resolve agent: {error}"))
        })?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        actor,
        &AuthorizationAction::AgentRead { agent_id },
        Some(company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agent:read permission".to_string(),
        ));
    }
    Ok(())
}
