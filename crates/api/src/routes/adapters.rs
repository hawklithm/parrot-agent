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

/// AppState for adapter routes
#[derive(Clone)]
pub struct AdapterAppState {
    pub adapter_registry: Arc<AdapterRegistry>,
    pub environment_runtime_service: Arc<dyn EnvironmentRuntimeService>,
}

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

    // 构建测试上下文
    let test_context = services::TestEnvironmentContext {
        adapter_config: payload.adapter_config,
    };

    // 执行环境测试
    let test_result = adapter
        .test_environment(&test_context)
        .await
        .map_err(|e| {
            tracing::error!("Adapter environment test failed: {:?}", e);
            AppError::Internal
        })?;

    // 转换为响应格式
    let response = TestAdapterEnvironmentResponse {
        adapter_type: test_result.adapter_type,
        status: match test_result.status {
            models::AdapterEnvironmentTestStatus::Pass => crate::schemas::AdapterEnvironmentTestStatus::Pass,
            models::AdapterEnvironmentTestStatus::Warning => crate::schemas::AdapterEnvironmentTestStatus::Warning,
            models::AdapterEnvironmentTestStatus::Fail => crate::schemas::AdapterEnvironmentTestStatus::Fail,
        },
        tested_at: test_result.tested_at,
        checks: test_result
            .checks
            .into_iter()
            .map(|check| crate::schemas::AdapterEnvironmentCheck {
                name: check.name,
                status: match check.status {
                    models::AdapterEnvironmentTestStatus::Pass => crate::schemas::AdapterEnvironmentTestStatus::Pass,
                    models::AdapterEnvironmentTestStatus::Warning => crate::schemas::AdapterEnvironmentTestStatus::Warning,
                    models::AdapterEnvironmentTestStatus::Fail => crate::schemas::AdapterEnvironmentTestStatus::Fail,
                },
                message: check.message,
                details: check.details,
            })
            .collect(),
    };

    Ok(Json(response))
}
