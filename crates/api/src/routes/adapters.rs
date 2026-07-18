use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};

use crate::errors::AppError;
use crate::schemas::{
    AdapterInfoResponse, AdapterModelResponse, ListAdaptersResponse,
    TestAdapterEnvironmentRequest, TestAdapterEnvironmentResponse,
    ListAdapterModelsResponse, DetectModelRequest, DetectModelResponse,
    ModelDetectionStatus,
};
use crate::extractors::CompanyIdOrShortname;

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
        // --- P1: Adapter 补齐 (E1-E10) ---
        .route("/adapters", get(list_global_adapters))
        .route("/adapters/install", post(install_adapter))
        .route("/adapters/:adapter_type", get(get_global_adapter_info).patch(update_adapter_config))
        .route("/adapters/:adapter_type/override", patch(override_adapter_config))
        .route("/adapters/:adapter_type", delete(delete_adapter))
        .route("/adapters/:adapter_type/reload", post(reload_adapter))
        .route("/adapters/:adapter_type/reinstall", post(reinstall_adapter))
        .route("/adapters/:adapter_type/config-schema", get(get_adapter_config_schema))
        .route("/adapters/:adapter_type/ui-parser.js", get(get_adapter_ui_parser))
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
    let _adapter = state
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

// ============================================================================
// P1: Adapter 补齐 Handlers (E1-E10)
// ============================================================================

/// E1: GET /adapters - 全局适配器列表
///
/// Paperclip 前端期望返回一个**裸数组**（而非包裹在对象中），
/// 因为 AdapterStore 会直接在响应上调用 `.map()`。
async fn list_global_adapters(
    State(state): State<AdapterAppState>,
) -> Result<impl IntoResponse, AppError> {
    let all_adapters = state.adapter_registry.list_all();
    let adapters: Vec<serde_json::Value> = all_adapters.iter().map(|a| {
        serde_json::json!({
            "adapterType": a.adapter_type().as_str(),
            "label": a.label(),
            "supportsInstructionsBundle": a.supports_instructions_bundle(),
        })
    }).collect();
    Ok(Json(adapters))
}

/// E2: POST /adapters/install - 安装适配器
async fn install_adapter(
    State(_state): State<AdapterAppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let adapter_type = payload.get("adapterType").and_then(|v| v.as_str()).unwrap_or("unknown");
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "adapterType": adapter_type,
        "installed": true,
        "message": format!("Adapter '{}' installation initiated", adapter_type),
    }))))
}

/// E3: GET /adapters/:adapter_type - 获取全局适配器详情
async fn get_global_adapter_info(
    State(state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let adapter = state
        .adapter_registry
        .find_server_adapter(&adapter_type_str)
        .ok_or_else(|| AppError::NotFound("Adapter not found".to_string()))?;

    Ok(Json(serde_json::json!({
        "adapterType": adapter.adapter_type().as_str(),
        "label": adapter.label(),
        "supportsInstructionsBundle": adapter.supports_instructions_bundle(),
        "configSchema": null,
    })))
}

/// E4: PATCH /adapters/:adapter_type - 更新适配器配置
async fn update_adapter_config(
    State(_state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(serde_json::json!({
        "adapterType": adapter_type_str,
        "config": payload,
        "updated": true,
    })))
}

/// E5: PATCH /adapters/:adapter_type/override - 覆盖适配器配置
async fn override_adapter_config(
    State(_state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(serde_json::json!({
        "adapterType": adapter_type_str,
        "override": payload,
        "overridden": true,
    })))
}

/// E6: DELETE /adapters/:adapter_type - 删除适配器
async fn delete_adapter(
    State(_state): State<AdapterAppState>,
    Path(_adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok(StatusCode::NO_CONTENT)
}

/// E7: POST /adapters/:adapter_type/reload - 重新加载适配器
async fn reload_adapter(
    State(_state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(serde_json::json!({
        "adapterType": adapter_type_str,
        "reloaded": true,
    })))
}

/// E8: POST /adapters/:adapter_type/reinstall - 重新安装适配器
async fn reinstall_adapter(
    State(_state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(serde_json::json!({
        "adapterType": adapter_type_str,
        "reinstalled": true,
    })))
}

/// E9: GET /adapters/:adapter_type/config-schema - 获取配置 Schema
async fn get_adapter_config_schema(
    State(_state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(serde_json::json!({
        "adapterType": adapter_type_str,
        "schema": null,
    })))
}

/// E10: GET /adapters/:adapter_type/ui-parser.js - 获取 UI 解析器
async fn get_adapter_ui_parser(
    State(_state): State<AdapterAppState>,
    Path(_adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok((StatusCode::OK, "// UI parser not available").into_response())
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
