//! Instance Settings routes — 实例级设置管理 (IS1-IS9)

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use crate::app_state::AppState;
use crate::routes::{assert_board, assert_instance_admin, log_activity};
use services::auth::AuthorizationActor;

pub fn instance_settings_routes() -> Router<AppState> {
    Router::new()
        .route("/instance/settings", get(get_instance_settings).patch(update_instance_settings))
        .route("/instance/settings/general", get(get_general_settings).patch(update_general_settings))
        .route("/instance/settings/experimental", get(get_experimental_settings).patch(update_experimental_settings))
        .route("/instance/settings/experimental/issue-graph-liveness-auto-recovery/preview", post(preview_auto_recovery))
        .route("/instance/settings/experimental/issue-graph-liveness-auto-recovery/run", post(run_auto_recovery))
        .route("/instance/database-backups", post(create_database_backup))
}

/// IS1: GET /instance/settings
async fn get_instance_settings(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // 对齐 Paperclip：读实例设置需要 board 访问。
    assert_board(&actor).map_err(|_| StatusCode::FORBIDDEN)?;
    let settings = state.instance_settings_service.get_settings()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS2: PATCH /instance/settings
async fn update_instance_settings(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // 对齐 Paperclip：写实例设置仅限实例管理员（或 local_implicit）。
    assert_instance_admin(&actor).map_err(|_| StatusCode::FORBIDDEN)?;
    let settings = state.instance_settings_service.update_settings(body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS3: GET /instance/settings/general
async fn get_general_settings(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    assert_board(&actor).map_err(|_| StatusCode::FORBIDDEN)?;
    let settings = state.instance_settings_service.get_general_settings()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get general settings: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS4: PATCH /instance/settings/general
async fn update_general_settings(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    assert_instance_admin(&actor).map_err(|_| StatusCode::FORBIDDEN)?;
    let settings = state.instance_settings_service.update_general_settings(body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS5: GET /instance/settings/experimental
async fn get_experimental_settings(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    assert_board(&actor).map_err(|_| StatusCode::FORBIDDEN)?;
    let settings = state.instance_settings_service.get_experimental_settings()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get experimental settings: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS6: PATCH /instance/settings/experimental
async fn update_experimental_settings(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    assert_instance_admin(&actor).map_err(|_| StatusCode::FORBIDDEN)?;
    let settings = state.instance_settings_service.update_experimental_settings(body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS7: POST /instance/settings/experimental/issue-graph-liveness-auto-recovery/preview
async fn preview_auto_recovery(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    assert_instance_admin(&actor).map_err(|_| StatusCode::FORBIDDEN)?;
    let result = state.instance_settings_service.preview_auto_recovery()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

/// IS8: POST /instance/settings/experimental/issue-graph-liveness-auto-recovery/run
async fn run_auto_recovery(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    assert_instance_admin(&actor).map_err(|_| StatusCode::FORBIDDEN)?;
    let result = state.instance_settings_service.run_auto_recovery()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

/// 将 backup service 的错误归类为合适的 HTTP 状态。
///
/// 对齐 Paperclip：`not configured` 表示能力未启用，返回 501；
/// 其余（IO/调度失败）归为 500。
fn database_backup_error_status(err: &str) -> StatusCode {
    if err.contains("not configured") {
        StatusCode::NOT_IMPLEMENTED
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// IS9: POST /instance/database-backups
async fn create_database_backup(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // 对齐 Paperclip：仅实例管理员可触发手动备份。
    assert_instance_admin(&actor).map_err(|_| StatusCode::FORBIDDEN)?;

    let result = state
        .instance_settings_service
        .create_database_backup()
        .await
        .map_err(|e| {
            let status = database_backup_error_status(&e);
            if status == StatusCode::INTERNAL_SERVER_ERROR {
                tracing::error!("Failed to create database backup: {}", e);
            }
            status
        })?;

    // 所有 mutation 写 activity log（对齐 handoff 审计约束）。
    log_activity(
        &state.pool,
        uuid::Uuid::nil(),
        "instance.database_backup_created",
        &actor,
        "instance",
        uuid::Uuid::nil(),
        serde_json::json!({ "backupId": result.backup_id }),
    )
    .await;

    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_configured_backup_maps_to_501() {
        assert_eq!(
            database_backup_error_status(
                "database backup is not configured; configure the deployment backup worker before requesting a backup"
            ),
            StatusCode::NOT_IMPLEMENTED
        );
    }

    #[test]
    fn unexpected_backup_error_maps_to_500() {
        assert_eq!(
            database_backup_error_status("pg_dump exited with code 1"),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
