//! Plugin management routes backed by the persistent plugin service.
use crate::{app_state::AppState, errors::AppError};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn plugin_routes() -> Router<AppState> {
    Router::new()
        .route("/plugins", get(list_plugins))
        .route("/plugins/examples", get(list_plugin_examples))
        .route("/plugins/ui-contributions", get(list_ui_contributions))
        .route("/plugins/tools", get(list_plugin_tools))
        .route("/plugins/tools/execute", post(execute_plugin_tool))
        .route("/plugins/install", post(install_plugin))
        .route("/plugins/:plugin_id", get(get_plugin).delete(delete_plugin))
        .route("/plugins/:plugin_id/enable", post(enable_plugin))
        .route("/plugins/:plugin_id/disable", post(disable_plugin))
        .route("/plugins/:plugin_id/upgrade", post(upgrade_plugin))
        .route("/plugins/:plugin_id/health", get(get_plugin_health))
        .route("/plugins/:plugin_id/logs", get(get_plugin_logs))
        .route("/plugins/:plugin_id/dashboard", get(get_plugin_dashboard))
        .route(
            "/plugins/:plugin_id/config",
            get(get_plugin_config).post(update_plugin_config),
        )
        .route("/plugins/:plugin_id/config/test", post(test_plugin_config))
        .route("/plugins/:plugin_id/bridge/data", post(bridge_plugin_data))
        .route(
            "/plugins/:plugin_id/bridge/action",
            post(bridge_plugin_action),
        )
        .route(
            "/plugins/:plugin_id/bridge/stream/:channel",
            get(bridge_plugin_stream),
        )
        .route("/plugins/:plugin_id/data/:key", post(store_plugin_data))
        .route(
            "/plugins/:plugin_id/actions/:key",
            post(trigger_plugin_action),
        )
        .route("/plugins/:plugin_id/jobs", get(list_plugin_jobs))
        .route(
            "/plugins/:plugin_id/jobs/:job_id/runs",
            get(list_plugin_job_runs),
        )
        .route(
            "/plugins/:plugin_id/jobs/:job_id/trigger",
            post(trigger_plugin_job),
        )
        .route(
            "/plugins/:plugin_id/jobs/:job_id/runs/:run_id/cancel",
            post(cancel_plugin_job_run),
        )
        .route(
            "/plugins/:plugin_id/jobs/:job_id/runs/:run_id/retry",
            post(retry_plugin_job_run),
        )
        .route(
            "/plugins/:plugin_id/webhooks/:endpoint_key",
            post(plugin_webhook),
        )
        .route(
            "/plugins/:plugin_id/companies/:company_id/local-folders",
            get(list_local_folders),
        )
        .route(
            "/plugins/:plugin_id/companies/:company_id/local-folders/:folder_key/status",
            get(get_local_folder_status),
        )
        .route(
            "/plugins/:plugin_id/companies/:company_id/local-folders/:folder_key/validate",
            post(validate_local_folder),
        )
        .route(
            "/plugins/:plugin_id/companies/:company_id/local-folders/:folder_key",
            put(update_local_folder),
        )
        .route("/plugins/:plugin_id/ui/*file_path", get(serve_plugin_ui_asset))
        .layer(axum::middleware::from_fn(crate::routes::require_plugin_access))
}

#[derive(Deserialize)]
struct PluginFilter {
    status: Option<String>,
}
fn err(e: impl std::fmt::Display) -> AppError {
    AppError::InternalServerError(e.to_string())
}
async fn list_plugins(
    State(s): State<AppState>,
    Query(q): Query<PluginFilter>,
) -> Result<Json<Vec<models::Plugin>>, AppError> {
    Ok(Json(s.plugin_service.list(q.status).await.map_err(err)?))
}
async fn list_plugin_examples(State(s): State<AppState>) -> Result<Json<Vec<Value>>, AppError> {
    Ok(Json(
        s.plugin_service
            .list(None)
            .await
            .map_err(err)?
            .into_iter()
            .filter_map(|p| {
                p.manifest
                    .get("example")
                    .and_then(Value::as_bool)
                    .filter(|v| *v)
                    .map(|_| json!(p))
            })
            .collect(),
    ))
}
async fn list_ui_contributions(State(s): State<AppState>) -> Result<Json<Vec<Value>>, AppError> {
    Ok(Json(
        s.plugin_service
            .list(Some("ready".into()))
            .await
            .map_err(err)?
            .into_iter()
            .filter_map(|p| {
                p.manifest
                    .get("ui")
                    .map(|ui| json!({"pluginId":p.id,"pluginKey":p.plugin_key,"ui":ui}))
            })
            .collect(),
    ))
}
async fn list_plugin_tools(State(s): State<AppState>) -> Result<Json<Vec<Value>>, AppError> {
    Ok(Json(
        s.plugin_service
            .list(Some("ready".into()))
            .await
            .map_err(err)?
            .into_iter()
            .flat_map(|p| {
                p.manifest
                    .get("tools")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |t| json!({"pluginId":p.id,"pluginKey":p.plugin_key,"tool":t}))
            })
            .collect(),
    ))
}
async fn execute_plugin_tool(
    State(s): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let tool = body.get("tool").and_then(Value::as_str).ok_or_else(|| AppError::BadRequest("tool is required".into()))?;
    let id = body
        .get("pluginId")
        .and_then(Value::as_str)
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or_else(|| AppError::BadRequest("pluginId is required".into()))?;
    Ok(Json(s.plugin_service.dispatch_tool(id, tool, body.get("parameters").cloned().unwrap_or(Value::Null)).await.map_err(err)?))
}
async fn install_plugin(
    State(s): State<AppState>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<models::Plugin>), AppError> {
    Ok((
        StatusCode::CREATED,
        Json(s.plugin_service.install(body).await.map_err(err)?),
    ))
}
async fn get_plugin(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<models::Plugin>, AppError> {
    Ok(Json(s.plugin_service.get(id).await.map_err(err)?))
}
async fn delete_plugin(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    s.plugin_service.remove(id).await.map_err(err)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn enable_plugin(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<models::Plugin>, AppError> {
    Ok(Json(
        s.plugin_service
            .transition(id, "ready")
            .await
            .map_err(err)?,
    ))
}
async fn disable_plugin(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<models::Plugin>, AppError> {
    Ok(Json(
        s.plugin_service
            .transition(id, "disabled")
            .await
            .map_err(err)?,
    ))
}
async fn upgrade_plugin(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<models::Plugin>, AppError> {
    let _ = body;
    Ok(Json(
        s.plugin_service
            .transition(id, "upgrade_pending")
            .await
            .map_err(err)?,
    ))
}
async fn get_plugin_health(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let p = s.plugin_service.get(id).await.map_err(err)?;
    Ok(Json(
        json!({"pluginId":id,"status":p.status,"healthy":p.status=="ready","checks":[{"name":"manifest","passed":p.manifest.is_object()}]}),
    ))
}
async fn get_plugin_logs(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Value>>, AppError> {
    Ok(Json(s.plugin_service.logs(id).await.map_err(err)?))
}
async fn get_plugin_dashboard(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let p = s.plugin_service.get(id).await.map_err(err)?;
    let jobs = s.plugin_service.jobs(id).await.map_err(err)?;
    Ok(Json(
        json!({"pluginId":id,"status":p.status,"version":p.version,"jobCount":jobs.len()}),
    ))
}
async fn get_plugin_config(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(s.plugin_service.get(id).await.map_err(err)?.config))
}
async fn update_plugin_config(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<models::Plugin>, AppError> {
    Ok(Json(
        s.plugin_service
            .update_config(id, body)
            .await
            .map_err(err)?,
    ))
}
async fn test_plugin_config(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    s.plugin_service.get(id).await.map_err(err)?;
    Ok(Json(
        json!({"pluginId":id,"valid":body.is_object(),"testPassed":body.is_object()}),
    ))
}
async fn bridge_plugin_data(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let key = body.get("key").and_then(Value::as_str).unwrap_or("default");
    Ok(Json(s.plugin_service.get_data(id, key).await.map_err(err)?))
}
async fn bridge_plugin_action(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let action = body.get("action").and_then(Value::as_str).ok_or_else(|| AppError::BadRequest("action is required".into()))?.to_owned();
    let payload = body.get("payload").cloned().unwrap_or_else(|| body.clone());
    Ok(Json(s.plugin_service.dispatch_action(id, &action, payload).await.map_err(err)?))
}
async fn store_plugin_data(
    State(s): State<AppState>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        s.plugin_service
            .set_data(id, &key, body)
            .await
            .map_err(err)?,
    ))
}
async fn trigger_plugin_action(
    State(s): State<AppState>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(s.plugin_service.dispatch_action(id, &key, body).await.map_err(err)?))
}
async fn list_plugin_jobs(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Value>>, AppError> {
    Ok(Json(s.plugin_service.jobs(id).await.map_err(err)?))
}
async fn list_plugin_job_runs(
    State(s): State<AppState>,
    Path((id, jid)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<Value>>, AppError> {
    Ok(Json(s.plugin_service.job_runs(id, jid).await.map_err(err)?))
}
async fn trigger_plugin_job(
    State(s): State<AppState>,
    Path((id, jid)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        s.plugin_service.trigger_job(id, jid).await.map_err(err)?,
    ))
}

// ============================================================================
// P1.2: Plugin 扩展面 handlers
// ============================================================================

/// GET /plugins/:plugin_id/bridge/stream/:channel
/// Plugin bridge SSE 流。仅当 manifest 声明 bridge.stream 时可用；否则返回 501。
async fn bridge_plugin_stream(
    State(s): State<AppState>,
    Path((plugin_id, channel)): Path<(Uuid, String)>,
) -> Result<Response, AppError> {
    let supported = s
        .plugin_service
        .bridge_stream_supported(plugin_id)
        .await
        .map_err(AppError::from)?;
    if !supported {
        return Err(AppError::NotImplemented(
            "plugin bridge stream is not enabled for this plugin".into(),
        ));
    }
    // 已声明支持：返回最小 keepalive SSE 流（事件源由 plugin bridge runtime 提供）。
    let body = format!("event: ready\ndata: {{\"channel\":\"{}\"}}\n\n", channel);
    let resp = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(body))
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(resp)
}

/// POST /plugins/:plugin_id/webhooks/:endpoint_key
/// Plugin webhook ingress（company-scoped）。
async fn plugin_webhook(
    State(s): State<AppState>,
    Path((plugin_id, endpoint_key)): Path<(Uuid, String)>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let result = s
        .plugin_service
        .ingest_webhook(plugin_id, &endpoint_key, Uuid::nil(), payload)
        .await
        .map_err(AppError::from)?;
    Ok(Json(result))
}

/// GET /plugins/:plugin_id/companies/:company_id/local-folders
async fn list_local_folders(
    State(s): State<AppState>,
    Path((plugin_id, company_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<Value>>, AppError> {
    Ok(Json(
        s.plugin_service
            .list_local_folders(plugin_id, company_id)
            .await
            .map_err(AppError::from)?,
    ))
}

/// GET /plugins/:plugin_id/companies/:company_id/local-folders/:folder_key/status
async fn get_local_folder_status(
    State(s): State<AppState>,
    Path((plugin_id, company_id, folder_key)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        s.plugin_service
            .get_local_folder_status(plugin_id, company_id, &folder_key)
            .await
            .map_err(AppError::from)?,
    ))
}

/// POST /plugins/:plugin_id/companies/:company_id/local-folders/:folder_key/validate
async fn validate_local_folder(
    State(s): State<AppState>,
    Path((plugin_id, company_id, folder_key)): Path<(Uuid, Uuid, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let path = body
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("missing 'path'".into()))?;
    s.plugin_service
        .validate_local_folder_path(path)
        .await
        .map_err(AppError::from)?;
    // 校验通过后再更新状态
    let updated = s
        .plugin_service
        .update_local_folder(plugin_id, company_id, &folder_key, json!({ "path": path, "status": "validated" }))
        .await
        .map_err(AppError::from)?;
    Ok(Json(updated))
}

/// PUT /plugins/:plugin_id/companies/:company_id/local-folders/:folder_key
async fn update_local_folder(
    State(s): State<AppState>,
    Path((plugin_id, company_id, folder_key)): Path<(Uuid, Uuid, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        s.plugin_service
            .update_local_folder(plugin_id, company_id, &folder_key, body)
            .await
            .map_err(AppError::from)?,
    ))
}

/// GET /plugins/:plugin_id/ui/*file_path
/// 安全提供 plugin UI 静态资源（防路径穿越）。
async fn serve_plugin_ui_asset(
    State(s): State<AppState>,
    Path((plugin_id, file_path)): Path<(Uuid, String)>,
) -> Result<Response, AppError> {
    let bytes = s
        .plugin_service
        .serve_ui_asset(plugin_id, &file_path)
        .await
        .map_err(AppError::from)?;
    let content_type = if file_path.ends_with(".js") {
        "application/javascript"
    } else if file_path.ends_with(".css") {
        "text/css"
    } else if file_path.ends_with(".html") {
        "text/html"
    } else {
        "application/octet-stream"
    };
    let resp = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(resp)
}

/// POST /plugins/:plugin_id/jobs/:job_id/runs/:run_id/cancel
async fn cancel_plugin_job_run(
    State(s): State<AppState>,
    Path((plugin_id, job_id, run_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        s.plugin_service
            .cancel_job_run(plugin_id, job_id, run_id)
            .await
            .map_err(AppError::from)?,
    ))
}

/// POST /plugins/:plugin_id/jobs/:job_id/runs/:run_id/retry
async fn retry_plugin_job_run(
    State(s): State<AppState>,
    Path((plugin_id, job_id, run_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        s.plugin_service
            .retry_job_run(plugin_id, job_id, run_id)
            .await
            .map_err(AppError::from)?,
    ))
}
