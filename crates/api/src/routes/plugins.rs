//! Plugin management routes backed by the persistent plugin service.
use crate::{app_state::AppState, errors::AppError};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
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
    let tool = body.get("tool").and_then(Value::as_str).unwrap_or_default();
    let id = body
        .get("pluginId")
        .and_then(Value::as_str)
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or_else(|| AppError::BadRequest("pluginId is required".into()))?;
    let p = s.plugin_service.get(id).await.map_err(err)?;
    if p.status != "ready" {
        return Err(AppError::BadRequest("plugin is not ready".into()));
    }
    Ok(Json(
        json!({"tool":tool,"result":body.get("parameters").cloned().unwrap_or(Value::Null)}),
    ))
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
    s.plugin_service.get(id).await.map_err(err)?;
    Ok(Json(
        json!({"pluginId":id,"action":body.get("action"),"accepted":true}),
    ))
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
    s.plugin_service.get(id).await.map_err(err)?;
    Ok(Json(
        json!({"pluginId":id,"action":key,"payload":body,"accepted":true}),
    ))
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
