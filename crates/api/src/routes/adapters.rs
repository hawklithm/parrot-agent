use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use uuid::Uuid;

use crate::errors::AppError;
use crate::extractors::CompanyIdOrShortname;
use crate::schemas::{
    AdapterInfoResponse, AdapterModelResponse, DetectModelRequest, DetectModelResponse,
    ListAdaptersResponse, ModelDetectionStatus, TestAdapterEnvironmentRequest,
    TestAdapterEnvironmentResponse,
};

/// AppState for adapter routes - 别名到统一的 `crate::app_state::AppState`
pub use crate::app_state::AppState as AdapterAppState;

/// 创建 Adapter 信息路由
pub fn adapter_routes() -> Router<AdapterAppState> {
    Router::new()
        .route("/companies/:company_id/adapters", get(list_adapters))
        .route(
            "/companies/:company_id/adapters/:adapter_type",
            get(get_adapter_info),
        )
        .route(
            "/companies/:company_id/adapters/:adapter_type/models",
            get(list_models),
        )
        .route(
            "/companies/:company_id/adapters/:adapter_type/detect-model",
            get(detect_model_get).post(detect_model),
        )
        .route(
            "/companies/:company_id/adapters/:adapter_type/model-profiles",
            get(list_model_profiles),
        )
        .route(
            "/companies/:company_id/adapters/:adapter_type/test-environment",
            post(test_environment),
        )
        // --- P1: Adapter 补齐 (E1-E10) ---
        .route("/adapters", get(list_global_adapters))
        .route("/adapters/install", post(install_adapter))
        .route(
            "/adapters/:adapter_type",
            get(get_global_adapter_info).patch(update_adapter_config),
        )
        .route(
            "/adapters/:adapter_type/override",
            patch(override_adapter_config),
        )
        .route("/adapters/:adapter_type", delete(delete_adapter))
        .route("/adapters/:adapter_type/reload", post(reload_adapter))
        .route("/adapters/:adapter_type/reinstall", post(reinstall_adapter))
        .route(
            "/adapters/:adapter_type/config-schema",
            get(get_adapter_config_schema),
        )
        .route(
            "/adapters/:adapter_type/ui-parser.js",
            get(get_adapter_ui_parser),
        )
}

/// GET /companies/:company_id/adapters - 列出所有可用适配器
async fn list_adapters(
    State(state): State<AdapterAppState>,
    Path(_company_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let all_adapters = state.adapter_registry.adapters();

    let adapters: Vec<AdapterInfoResponse> = all_adapters
        .into_iter()
        .map(|adapter| {
            let models: Vec<AdapterModelResponse> = adapter
                .models()
                .iter()
                .map(|m| AdapterModelResponse {
                    id: m.id.clone(),
                    label: m.label.clone(),
                })
                .collect();

            AdapterInfoResponse {
                adapter_type: adapter.adapter_type().to_string(),
                label: adapter.label().to_string(),
                models,
                config_schema: adapter.get_config_schema(),
                supports_instructions_bundle: adapter.supports_instructions_bundle().supported,
                instructions_path_key: Some(adapter.instructions_path_key().to_string()),
                agent_configuration_doc: adapter.agent_configuration_doc().map(String::from),
            }
        })
        .collect();

    Ok(Json(ListAdaptersResponse { adapters }))
}

/// GET /companies/:company_id/adapters/:adapter_type - 获取指定适配器详细信息
async fn get_adapter_info(
    State(state): State<AdapterAppState>,
    Path((_company_id, adapter_type_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let adapter_type = match adapter_type_str.parse::<services::server_adapter::AdapterType>() {
        Ok(t) => t,
        Err(_) => return Err(AppError::NotFound("Adapter not found".to_string())),
    };
    let adapter = state
        .adapter_registry
        .find_adapter(adapter_type)
        .map_err(|_| AppError::NotFound("Adapter not found".to_string()))?;

    let models: Vec<AdapterModelResponse> = adapter
        .models()
        .iter()
        .map(|m| AdapterModelResponse {
            id: m.id.clone(),
            label: m.label.clone(),
        })
        .collect();

    let response = AdapterInfoResponse {
        adapter_type: adapter.adapter_type().to_string(),
        label: adapter.label().to_string(),
        models,
        config_schema: adapter.get_config_schema(),
        supports_instructions_bundle: adapter.supports_instructions_bundle().supported,
        instructions_path_key: Some(adapter.instructions_path_key().to_string()),
        agent_configuration_doc: adapter.agent_configuration_doc().map(String::from),
    };

    Ok(Json(response))
}

/// GET /companies/:company_id/adapters/:adapter_type/models - 获取适配器支持的模型列表
async fn list_models(
    State(state): State<AdapterAppState>,
    Path((_company_id, adapter_type_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let adapter_type = match adapter_type_str.parse::<services::server_adapter::AdapterType>() {
        Ok(t) => t,
        Err(_) => return Err(AppError::NotFound("Adapter not found".to_string())),
    };
    let adapter = state
        .adapter_registry
        .find_adapter(adapter_type)
        .map_err(|_| AppError::NotFound("Adapter not found".to_string()))?;

    let models: Vec<AdapterModelResponse> = adapter
        .list_models(&serde_json::json!({}))
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| AdapterModelResponse {
            id: m.id,
            label: m.label,
        })
        .collect();

    // Keep parity with Paperclip: this endpoint returns the model array
    // directly, rather than wrapping it with adapter metadata.
    Ok(Json(models))
}

/// POST /companies/:company_id/adapters/:adapter_type/detect-model - 检测可用模型
async fn detect_model(
    State(state): State<AdapterAppState>,
    Path((_company_id, adapter_type_str)): Path<(String, String)>,
    Json(payload): Json<DetectModelRequest>,
) -> Result<impl IntoResponse, AppError> {
    let adapter_type = match adapter_type_str.parse::<services::server_adapter::AdapterType>() {
        Ok(t) => t,
        Err(_) => return Err(AppError::NotFound("Adapter not found".to_string())),
    };
    let _adapter = state
        .adapter_registry
        .find_adapter(adapter_type)
        .map_err(|_| AppError::NotFound("Adapter not found".to_string()))?;

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

/// GET /companies/:company_id/adapters/:adapter_type/detect-model
/// Paperclip performs adapter detection without a request body.
async fn detect_model_get(
    State(state): State<AdapterAppState>,
    Path((_company_id, adapter_type_str)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let adapter_type = match adapter_type_str.parse::<services::server_adapter::AdapterType>() {
        Ok(t) => t,
        Err(_) => return Err(AppError::NotFound("Adapter not found".to_string())),
    };
    state
        .adapter_registry
        .find_adapter(adapter_type)
        .map_err(|_| AppError::NotFound("Adapter not found".to_string()))?;

    // A local adapter can only report a detected runtime model when its
    // executable is available. Returning null matches Paperclip's contract
    // when detection is unavailable; configured model selection remains in
    // the agent adapter configuration.
    Ok(Json(serde_json::Value::Null))
}

/// GET /companies/:company_id/adapters/:adapter_type/model-profiles
async fn list_model_profiles(
    State(state): State<AdapterAppState>,
    Path((_company_id, adapter_type_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let adapter_type = match adapter_type_str.parse::<services::server_adapter::AdapterType>() {
        Ok(t) => t,
        Err(_) => return Err(AppError::NotFound("Adapter not found".to_string())),
    };
    let adapter = state
        .adapter_registry
        .find_adapter(adapter_type)
        .map_err(|_| AppError::NotFound("Adapter not found".to_string()))?;
    
    let profiles = adapter.get_model_profiles(&serde_json::json!({})).await
        .unwrap_or_default();
    Ok(Json(profiles))
}

/// POST /companies/:company_id/adapters/:adapter_type/test-environment - 测试适配器环境
async fn test_environment(
    State(state): State<AdapterAppState>,
    Path((company_id_str, adapter_type_str)): Path<(String, String)>,
    Json(payload): Json<TestAdapterEnvironmentRequest>,
) -> Result<impl IntoResponse, AppError> {
    let company_id = Uuid::parse_str(&company_id_str)
        .map_err(|_| AppError::BadRequest("Invalid company ID parameter".to_string()))?;
    let adapter_type = match adapter_type_str.parse::<services::server_adapter::AdapterType>() {
        Ok(t) => t,
        Err(_) => return Err(AppError::NotFound("Adapter not found".to_string())),
    };
    let adapter = state
        .adapter_registry
        .find_adapter(adapter_type)
        .map_err(|_| AppError::NotFound("Adapter not found".to_string()))?;

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

    let test_context = services::server_adapter::AdapterEnvironmentTestContext {
        company_id: company_id.to_string(),
        adapter_type: adapter_type_str,
        config: serde_json::to_value(adapter_config_map).unwrap_or_default(),
        execution_target: None,
        environment_name: None,
        deployment: None,
    };

    // 执行环境测试
    let test_result = adapter.test_environment(&test_context).await.map_err(|e| {
        tracing::error!("Adapter environment test failed: {:?}", e);
        AppError::Internal
    })?;

    // 租约会在 _lease_guard drop 时自动释放
    // 这确保即使测试失败，租约也会被正确释放

    // 转换为响应格式
    // 解析 status 字符串为 enum
    let status = match test_result.status.as_str() {
        "pass" => crate::schemas::AdapterEnvironmentTestStatus::Pass,
        "fail" => crate::schemas::AdapterEnvironmentTestStatus::Fail,
        "warn" | "warning" => crate::schemas::AdapterEnvironmentTestStatus::Warning,
        _ => crate::schemas::AdapterEnvironmentTestStatus::Pass,
    };

    let response = TestAdapterEnvironmentResponse {
        adapter_type: test_result.adapter_type,
        status,
        tested_at: test_result.tested_at,
        checks: test_result
            .checks
            .into_iter()
            .map(|check| {
                let check_status = match check.status.as_str() {
                    "pass" => crate::schemas::AdapterEnvironmentTestStatus::Pass,
                    "fail" => crate::schemas::AdapterEnvironmentTestStatus::Fail,
                    "warn" | "warning" => crate::schemas::AdapterEnvironmentTestStatus::Warning,
                    _ => crate::schemas::AdapterEnvironmentTestStatus::Pass,
                };
                crate::schemas::AdapterEnvironmentCheck {
                    name: check.name,
                    status: check_status,
                    message: check.message.unwrap_or_default(),
                    details: None,
                }
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
    use crate::schemas::{AdapterCapabilities, GlobalAdapterInfo};
    use services::builtin_adapter_types::is_builtin_adapter_type;

    let all_adapters = state.adapter_registry.adapters();
    let registry_state = &state.adapter_registry_state;
    
    // 构建外部插件记录映射
    let external_records: std::collections::HashMap<String, _> = registry_state
        .list_external_plugins()
        .into_iter()
        .map(|r| (r.adapter_type.clone(), r))
        .collect();
    
    // 构建适配器信息列表
    let mut adapters: Vec<GlobalAdapterInfo> = all_adapters
        .iter()
        .map(|&adapter| {
            let adapter_type_str = adapter.adapter_type().to_string();
            let external_record = external_records.get(&adapter_type_str);
            let is_builtin = is_builtin_adapter_type(&adapter_type_str);
            let is_disabled = registry_state.is_disabled(&adapter_type_str);
            let is_override_paused = registry_state.is_override_paused(&adapter_type_str);
            
            // 构建能力集合
            let capabilities = AdapterCapabilities {
                supports_instructions_bundle: adapter.supports_instructions_bundle().supported,
                supports_skills: false, // TODO: 需要 adapter 实现 listSkills/syncSkills
                supports_local_agent_jwt: false, // TODO: 需要 adapter 实现
                requires_materialized_runtime_skills: false, // TODO: 需要 adapter 实现
                supports_model_profiles: false, // TODO: 需要 adapter 实现
                supports_acp: false, // TODO: 需要 adapter 实现 ACP
            };
            
            GlobalAdapterInfo {
                adapter_type: adapter_type_str.clone(),
                label: adapter.label().to_string(),
                source: if external_record.is_some() { "external" } else { "builtin" }.to_string(),
                models_count: adapter.models().len(),
                loaded: true, // 如果在注册表中，就是已加载
                disabled: is_disabled,
                capabilities,
                overridden_builtin: if external_record.is_some() && is_builtin {
                    Some(true)
                } else {
                    None
                },
                override_paused: if is_builtin {
                    Some(is_override_paused)
                } else {
                    None
                },
                version: external_record.and_then(|r| r.version.clone()),
                package_name: external_record.map(|r| r.package_name.clone()),
                is_local_path: external_record.and_then(|r| {
                    r.local_path.as_ref().map(|_| true)
                }),
            }
        })
        .collect();
    
    // 按适配器类型排序（对齐 Paperclip）
    adapters.sort_by(|a, b| a.adapter_type.cmp(&b.adapter_type));
    
    Ok(Json(adapters))
}
/// E2: POST /adapters/install - 安装适配器
///
/// 请求体:
/// - packageName: npm 包名或本地路径
/// - isLocalPath: 是否为本地路径（默认 false）
/// - version: 目标版本（可选）
async fn install_adapter(
    State(state): State<AdapterAppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    use services::adapter_package_loader::AdapterPackageLoader;
    use services::npm_manager::NpmManager;
    
    // 1. 解析请求参数
    let package_name = payload
        .get("packageName")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("packageName is required".to_string()))?;
    
    let is_local_path = payload
        .get("isLocalPath")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    
    let version = payload
        .get("version")
        .and_then(|v| v.as_str());
    
    // 2. 处理版本后缀（如 "pkg@1.2.3"）
    let (canonical_name, explicit_version) = if !is_local_path && package_name.contains('@') {
        let (name, ver) = NpmManager::parse_package_spec(package_name);
        (name, ver.or(version))
    } else {
        (package_name, version)
    };
    
    tracing::info!(
        package_name = canonical_name,
        is_local_path = is_local_path,
        version = ?explicit_version,
        "Installing adapter package"
    );
    
    // 3. 安装或加载适配器包
    let loader = AdapterPackageLoader::with_default_config();
    
    let plugin_record = if is_local_path {
        // 本地路径安装
        loader.load_local_adapter(canonical_name)
            .map_err(|e| AppError::BadRequest(format!("Failed to load local adapter: {}", e)))?
    } else {
        // npm 安装
        loader.install_npm_adapter(canonical_name, explicit_version)
            .map_err(|e| AppError::BadRequest(format!("Failed to install npm adapter: {}", e)))?
    };
    
    // 4. 注册到适配器状态管理
    state.adapter_registry_state.add_external_plugin(plugin_record.clone());
    
    // 5. TODO: 动态加载并注册到 AdapterRegistry
    // 这需要实现动态加载 .so/.dylib/.dll 的机制
    tracing::warn!(
        adapter_type = %plugin_record.adapter_type,
        "Adapter installed to registry state, but dynamic loading not yet implemented"
    );
    
    // 6. 返回成功响应
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "type": plugin_record.adapter_type,
            "packageName": plugin_record.package_name,
            "version": plugin_record.version,
            "isLocalPath": plugin_record.local_path.is_some(),
            "installed": true,
            "message": format!("Adapter '{}' installed successfully", plugin_record.adapter_type),
        })),
    ))
}

/// E3: GET /adapters/:adapter_type - 获取全局适配器详情
async fn get_global_adapter_info(
    State(state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let adapter_type = match adapter_type_str.parse::<services::server_adapter::AdapterType>() {
        Ok(t) => t,
        Err(_) => return Err(AppError::NotFound("Adapter not found".to_string())),
    };
    let adapter = state
        .adapter_registry
        .find_adapter(adapter_type)
        .map_err(|_| AppError::NotFound("Adapter not found".to_string()))?;

    Ok(Json(serde_json::json!({
        "adapterType": adapter.adapter_type().to_string(),
        "label": adapter.label(),
        "supportsInstructionsBundle": adapter.supports_instructions_bundle().supported,
        "configSchema": null,
    })))
}

/// E4: PATCH /adapters/:adapter_type - 更新适配器配置（启用/禁用）
///
/// 请求体:
/// - disabled: boolean (是否禁用)
async fn update_adapter_config(
    State(state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    // 1. 解析请求参数
    let disabled = payload
        .get("disabled")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| AppError::BadRequest("disabled (boolean) is required".to_string()))?;
    
    // 2. 检查适配器是否存在
    let adapter_type = adapter_type_str.parse::<services::server_adapter::AdapterType>()
        .map_err(|_| AppError::NotFound(format!("Adapter \"{}\" not found", adapter_type_str)))?;
    
    if !state.adapter_registry.has_adapter(adapter_type) {
        return Err(AppError::NotFound(format!(
            "Adapter \"{}\" is not registered",
            adapter_type_str
        )));
    }
    
    // 3. 更新禁用状态
    let was_disabled = state.adapter_registry_state.is_disabled(&adapter_type_str);
    state.adapter_registry_state.set_disabled(&adapter_type_str, disabled);
    let changed = was_disabled != disabled;
    
    if changed {
        tracing::info!(
            adapter_type = %adapter_type_str,
            disabled = disabled,
            "Adapter enabled/disabled"
        );
    }
    
    // 4. 返回结果
    Ok(Json(serde_json::json!({
        "type": adapter_type_str,
        "disabled": disabled,
        "changed": changed,
    })))
}

/// E6: DELETE /adapters/:adapter_type - 删除适配器
async fn delete_adapter(
    State(_state): State<AdapterAppState>,
    Path(_adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // TODO: 实现完整的删除逻辑
    Ok(StatusCode::NO_CONTENT)
}

/// E7: POST /adapters/:adapter_type/reload - 重载外部适配器
async fn reload_adapter(
    State(state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    use services::builtin_adapter_types::is_builtin_adapter_type;
    
    // 1. 内置适配器不能重载（除非被外部覆盖）
    let is_builtin = is_builtin_adapter_type(&adapter_type_str);
    let external_record = state.adapter_registry_state.get_external_plugin(&adapter_type_str);
    
    if is_builtin && external_record.is_none() {
        return Err(AppError::BadRequest("Cannot reload built-in adapter".to_string()));
    }
    
    // 2. 检查是否为外部安装的适配器
    let record = external_record.ok_or_else(|| {
        AppError::NotFound(format!(
            "Adapter \"{}\" is not an externally installed adapter",
            adapter_type_str
        ))
    })?;
    
    // 3. TODO: 实现动态重载逻辑
    tracing::warn!(
        adapter_type = %adapter_type_str,
        "Adapter reload requested, but dynamic reloading not yet implemented"
    );
    
    // 4. 返回结果（目前只是模拟）
    Ok(Json(serde_json::json!({
        "type": adapter_type_str,
        "version": record.version,
        "reloaded": false,
        "message": "Dynamic reloading not yet implemented",
    })))
}

/// E8: POST /adapters/:adapter_type/reinstall - 重新安装适配器
/// E5: PATCH /adapters/:adapter_type/override - 暂停/恢复外部适配器覆盖
///
/// 请求体:
/// - paused: boolean (是否暂停外部覆盖)
async fn override_adapter_config(
    State(state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    use services::builtin_adapter_types::is_builtin_adapter_type;
    
    // 1. 解析请求参数
    let paused = payload
        .get("paused")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| AppError::BadRequest("paused (boolean) is required".to_string()))?;
    
    // 2. 检查是否为内置适配器类型
    if !is_builtin_adapter_type(&adapter_type_str) {
        return Err(AppError::BadRequest(format!(
            "Type \"{}\" is not a builtin adapter",
            adapter_type_str
        )));
    }
    
    // 3. 更新覆盖暂停状态
    let was_paused = state.adapter_registry_state.is_override_paused(&adapter_type_str);
    state.adapter_registry_state.set_override_paused(&adapter_type_str, paused);
    let changed = was_paused != paused;
    
    if changed {
        tracing::info!(
            adapter_type = %adapter_type_str,
            paused = paused,
            "Adapter override toggle"
        );
    }
    
    
    // 4. 返回结果
    Ok(Json(serde_json::json!({
        "type": adapter_type_str,
        "paused": paused,
        "changed": changed,
    })))
}

/// E9: GET /adapters/:adapter_type/config-schema - 获取配置 schema
async fn get_adapter_config_schema(
    State(state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let adapter_type = adapter_type_str.parse::<services::server_adapter::AdapterType>()
        .map_err(|_| AppError::NotFound(format!("Adapter \"{}\" not found", adapter_type_str)))?;
    
    let adapter = state.adapter_registry
        .find_adapter(adapter_type)
        .map_err(|_| AppError::NotFound(format!("Adapter \"{}\" is not registered", adapter_type_str)))?;
    
    Ok(Json(serde_json::json!({
        "adapterType": adapter_type_str,
        "schema": adapter.get_config_schema(),
    })))
}

/// E10: GET /adapters/:adapter_type/ui-parser.js - 获取 UI 解析器
async fn get_adapter_ui_parser(
    State(_state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
) -> Result<axum::response::Response, AppError> {
    Err(AppError::NotFound(format!("UI parser not found for adapter {}", adapter_type_str)))
}

/// E8: POST /adapters/:adapter_type/reinstall - 重新安装适配器
async fn reinstall_adapter(
    State(_state): State<AdapterAppState>,
    Path(adapter_type_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // TODO: 实现重新安装逻辑
    Ok(Json(serde_json::json!({
        "type": adapter_type_str,
        "reinstalled": false,
        "message": "Reinstall not yet implemented",
    })))
}
