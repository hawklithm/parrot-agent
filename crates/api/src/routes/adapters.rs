use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;

use crate::errors::AppError;
use crate::schemas::TestAdapterEnvironmentSchema;
use adapters::{AdapterRegistry, AdapterType, TestEnvironmentInput};
use services::EnvironmentRuntimeService;

/// AppState for adapter routes
#[derive(Clone)]
pub struct AdapterAppState {
    pub adapter_registry: Arc<AdapterRegistry>,
    pub environment_runtime_service: Arc<dyn EnvironmentRuntimeService>,
}

/// 创建Adapter信息路由
pub fn adapter_routes() -> Router<AdapterAppState> {
    Router::new()
        .route("/companies/:company_id/adapters/:adapter_type/models", get(list_models))
        .route("/companies/:company_id/adapters/:adapter_type/detect-model", get(detect_model))
        .route("/companies/:company_id/adapters/:adapter_type/test-environment", post(test_environment))
}

/// GET /companies/:company_id/adapters/:adapter_type/models - 获取适配器支持的模型列表
async fn list_models(
    State(state): State<AdapterAppState>,
    Path((_company_id, adapter_type_str)): Path<(uuid::Uuid, String)>,
) -> Result<impl IntoResponse, AppError>
{
    let adapter_type: AdapterType = adapter_type_str
        .parse()
        .map_err(|e| AppError::BadRequest(format!("Invalid adapter type: {}", e)))?;

    let adapter = state
        .adapter_registry
        .find_server_adapter(adapter_type)
        .ok_or_else(|| AppError::NotFound("Adapter not found".to_string()))?;

    let models = adapter
        .list_models()
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(Json(models))
}

/// GET /companies/:company_id/adapters/:adapter_type/detect-model - 检测可用模型
async fn detect_model(
    State(state): State<AdapterAppState>,
    Path((_company_id, adapter_type_str)): Path<(uuid::Uuid, String)>,
) -> Result<impl IntoResponse, AppError>
{
    let adapter_type: AdapterType = adapter_type_str
        .parse()
        .map_err(|e| AppError::BadRequest(format!("Invalid adapter type: {}", e)))?;

    let adapter = state
        .adapter_registry
        .find_server_adapter(adapter_type)
        .ok_or_else(|| AppError::NotFound("Adapter not found".to_string()))?;

    // TODO: 从请求中提取配置
    let config = serde_json::json!({});

    let model = adapter
        .detect_model(&config)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(Json(serde_json::json!({
        "model": model,
    })))
}

/// POST /companies/:company_id/adapters/:adapter_type/test-environment - 测试适配器环境
async fn test_environment(
    State(state): State<AdapterAppState>,
    Path((_company_id, adapter_type_str)): Path<(uuid::Uuid, String)>,
    Json(payload): Json<TestAdapterEnvironmentSchema>,
) -> Result<impl IntoResponse, AppError> {
    let adapter_type: AdapterType = adapter_type_str
        .parse()
        .map_err(|e| AppError::BadRequest(format!("Invalid adapter type: {}", e)))?;

    let adapter = state
        .adapter_registry
        .find_server_adapter(adapter_type)
        .ok_or_else(|| AppError::NotFound("Adapter not found".to_string()))?;

    // 构建测试输入
    let input = TestEnvironmentInput {
        adapter_config: payload.adapter_config,
        environment_id: payload.environment_id,
        with_lease: payload.with_lease.unwrap_or(false),
        with_workspace: payload.with_workspace.unwrap_or(false),
    };

    // 如果需要租约，则获取租约并在测试后释放
    let lease_guard = if input.with_lease {
        if let Some(ref env_id) = input.environment_id {
            match state
                .environment_runtime_service
                .acquire_run_lease(env_id, None, serde_json::json!({"test": true}))
                .await
            {
                Ok(lease) => Some(lease),
                Err(e) => {
                    return Ok(Json(serde_json::json!({
                        "success": false,
                        "message": format!("Failed to acquire lease: {}", e),
                        "details": null,
                    })));
                }
            }
        } else {
            return Err(AppError::BadRequest(
                "environment_id is required when with_lease=true".to_string(),
            ));
        }
    } else {
        None
    };

    // 执行环境测试
    let test_result = adapter
        .test_environment_enhanced(input)
        .await
        .map_err(|_| AppError::Internal)?;

    // 释放租约
    if let Some(lease) = lease_guard {
        let _ = state
            .environment_runtime_service
            .release_run_lease(lease.id, services::LeaseStatus::Released)
            .await;
    }

    // 序列化结果为 JSON
    let response = serde_json::json!({
        "success": test_result.success,
        "message": test_result.message,
        "details": test_result.details,
    });

    Ok(Json(response))
}
