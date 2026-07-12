use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;

use crate::errors::AppError;
use crate::schemas::{
    AdapterInfoResponse, AdapterModelResponse, ListAdaptersResponse,
    TestAdapterEnvironmentRequest, TestAdapterEnvironmentResponse,
    ListAdapterModelsResponse, DetectModelRequest, DetectModelResponse,
    ModelDetectionStatus,
};
use crate::extractors::CompanyIdOrShortname;
use services::{AdapterRegistry, EnvironmentRuntimeService};
use models::AdapterType;

/// AppState for adapter routes - 别名到统一的 `crate::app_state::AppState`
pub use crate::app_state::AppState as AdapterAppState;

/// 创建 Adapter 信息路由
pub fn adapter_routes() -> Router<AdapterAppState> {
    Router::new()
        .route("/companies/:company_id/adapters", get(list_adapters))
        .route("/companies/:company_id/adapters/:adapter_type", get(get_adapter_info))
        .route("/companies/:company_id/adapters/:adapter_type/models", get(list_models))
        .route("/companies/:company_id/adapters/:adapter_type/detect-model", post(detect_model))
        .route("/companies/:company_id/adapters/:adapter_type/test-environment", post(test_environment))
}

/// GET /companies/:company_id/adapters - 列出所有可用适配器
async fn list_adapters(
    State(state): State<AdapterAppState>,
    CompanyIdOrShortname(_company_id): CompanyIdOrShortname,
) -> Result<impl IntoResponse, AppError> {
    let all_adapters = state.adapter_registry.list_all();

    let adapters: Vec<AdapterInfoResponse> = all_adapters
        .iter()
        .map(|adapter| {
            let models: Vec<AdapterModelResponse> = adapter
                .models()
                .into_iter()
                .map(|m| AdapterModelResponse {
                    id: m.id,
                    label: m.label,
                })
                .collect();

            AdapterInfoResponse {
                adapter_type: adapter.adapter_type().as_str().to_string(),
                label: adapter.label().to_string(),
                models,
                config_schema: None, // TODO: 从 adapter.get_config_schema() 获取
                supports_instructions_bundle: adapter.supports_instructions_bundle(),
                instructions_path_key: adapter.instructions_path_key().map(String::from),
                agent_configuration_doc: Some(adapter.agent_configuration_doc().to_string()),
            }
        })
        .collect();

    Ok(Json(ListAdaptersResponse { adapters }))
}

/// GET /companies/:company_id/adapters/:adapter_type - 获取指定适配器详细信息
async fn get_adapter_info(
    State(state): State<AdapterAppState>,
    CompanyIdOrShortname(_company_id): CompanyIdOrShortname,
    Path(adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let adapter = state
        .adapter_registry
        .find_server_adapter(&adapter_type_str)
        .ok_or_else(|| AppError::NotFound("Adapter not found".to_string()))?;

    let models: Vec<AdapterModelResponse> = adapter
        .models()
        .into_iter()
        .map(|m| AdapterModelResponse {
            id: m.id,
            label: m.label,
        })
        .collect();

    let response = AdapterInfoResponse {
        adapter_type: adapter.adapter_type().as_str().to_string(),
        label: adapter.label().to_string(),
        models,
        config_schema: None, // TODO: 从 adapter.get_config_schema() 获取
        supports_instructions_bundle: adapter.supports_instructions_bundle(),
        instructions_path_key: adapter.instructions_path_key().map(String::from),
        agent_configuration_doc: Some(adapter.agent_configuration_doc().to_string()),
    };

    Ok(Json(response))
}

/// GET /companies/:company_id/adapters/:adapter_type/models - 获取适配器支持的模型列表
async fn list_models(
    State(state): State<AdapterAppState>,
    CompanyIdOrShortname(_company_id): CompanyIdOrShortname,
    Path(adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let adapter = state
        .adapter_registry
        .find_server_adapter(&adapter_type_str)
        .ok_or_else(|| AppError::NotFound("Adapter not found".to_string()))?;

    let models: Vec<AdapterModelResponse> = adapter
        .list_models()
        .await
        .into_iter()
        .map(|m| AdapterModelResponse {
            id: m.id,
            label: m.label,
        })
        .collect();

    let response = ListAdapterModelsResponse {
        adapter_type: adapter_type_str,
        models,
    };

    Ok(Json(response))
}

/// POST /companies/:company_id/adapters/:adapter_type/detect-model - 检测可用模型
async fn detect_model(
    State(state): State<AdapterAppState>,
    CompanyIdOrShortname(_company_id): CompanyIdOrShortname,
    Path(adapter_type_str): Path<String>,
    Json(payload): Json<DetectModelRequest>,
) -> Result<impl IntoResponse, AppError> {
    let adapter = state
        .adapter_registry
        .find_server_adapter(&adapter_type_str)
        .ok_or_else(|| AppError::NotFound("Adapter not found".to_string()))?;

    // 尝试从配置中检测模型
    // 注意：这里需要 ServerAdapterModule trait 支持 detect_model 方法
    // 暂时返回配置中的 model 字段（如果存在）
    let model = payload
        .adapter_config
        .get("model")
        .and_then(|v| v.as_str())
        .map(String::from);

    let response = if model.is_some() {
        DetectModelResponse {
            model,
            status: ModelDetectionStatus::Success,
            message: None,
        }
    } else {
        DetectModelResponse {
            model: None,
            status: ModelDetectionStatus::NotFound,
            message: Some("No model specified in configuration".to_string()),
        }
    };

    Ok(Json(response))
}

/// POST /companies/:company_id/adapters/:adapter_type/test-environment - 测试适配器环境
async fn test_environment(
    State(state): State<AdapterAppState>,
    CompanyIdOrShortname(company_id): CompanyIdOrShortname,
    Path(adapter_type_str): Path<String>,
    Json(payload): Json<TestAdapterEnvironmentRequest>,
) -> Result<impl IntoResponse, AppError> {
    let adapter = state
        .adapter_registry
        .find_server_adapter(&adapter_type_str)
        .ok_or_else(|| AppError::NotFound("Adapter not found".to_string()))?;

    // 如果需要租约，先获取租约
    let _lease_guard = if payload.with_lease {
        if let Some(env_id) = payload.environment_id {
            match state
                .environment_runtime_service
                .acquire_run_lease(
                    &env_id.to_string(),
                    None,
                    serde_json::json!({"purpose": "adapter_test"}),
                )
                .await
            {
                Ok(lease) => {
                    tracing::info!("Acquired lease {} for adapter test", lease.id);
                    Some(lease)
                }
                Err(e) => {
                    tracing::error!("Failed to acquire lease: {:?}", e);
                    return Err(AppError::BadRequest(format!(
                        "Failed to acquire environment lease: {}",
                        e
                    )));
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

    // 如果需要工作空间实现，也需要租约
    if payload.with_workspace && !payload.with_lease {
        return Err(AppError::BadRequest(
            "with_lease must be true when with_workspace=true".to_string(),
        ));
    }

    // 构建测试上下文
    let adapter_config_map: std::collections::HashMap<String, serde_json::Value> = payload
        .adapter_config
        .as_object()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    let test_context = models::TestEnvironmentContext {
        company_id,
        agent_id: None,
        adapter_config: adapter_config_map,
        runtime_config: std::collections::HashMap::new(),
    };

    // 执行环境测试
    let test_result = adapter
        .test_environment(&test_context)
        .await
        .map_err(|e| {
            tracing::error!("Adapter environment test failed: {:?}", e);
            AppError::Internal
        })?;

    // 租约会在 _lease_guard drop 时自动释放
    // 这确保即使测试失败，租约也会被正确释放

    // 转换为响应格式
    let response = TestAdapterEnvironmentResponse {
        adapter_type: test_result.adapter_type,
        status: map_adapter_test_status(test_result.status),
        tested_at: test_result.tested_at,
        checks: test_result
            .checks
            .into_iter()
            .map(|check| crate::schemas::AdapterEnvironmentCheck {
                name: check.name.unwrap_or_default(),
                status: check
                    .status
                    .map(map_adapter_test_status)
                    .unwrap_or(crate::schemas::AdapterEnvironmentTestStatus::Pass),
                message: check.message,
                details: check.details.map(serde_json::Value::String),
            })
            .collect(),
    };

    Ok(Json(response))
}

/// 将模型层的适配器环境测试状态映射到 API 响应枚举
fn map_adapter_test_status(
    status: models::AdapterEnvironmentTestStatus,
) -> crate::schemas::AdapterEnvironmentTestStatus {
    match status {
        models::AdapterEnvironmentTestStatus::Pass => crate::schemas::AdapterEnvironmentTestStatus::Pass,
        models::AdapterEnvironmentTestStatus::Fail => crate::schemas::AdapterEnvironmentTestStatus::Fail,
        models::AdapterEnvironmentTestStatus::Warn
        | models::AdapterEnvironmentTestStatus::Warning => crate::schemas::AdapterEnvironmentTestStatus::Warning,
    }
}
