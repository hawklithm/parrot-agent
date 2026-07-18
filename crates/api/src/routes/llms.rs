//! LLMs/OpenAPI routes — P4 收尾域 (LM1-LM5)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use crate::app_state::AppState;

pub fn llm_routes() -> Router<AppState> {
    Router::new()
        .route("/llms/agent-configuration.txt", get(get_agent_config_txt))
        .route("/llms/agent-icons.txt", get(get_agent_icons_txt))
        .route("/llms/agent-configuration/:adapter_type.txt", get(get_adapter_config_txt))
        .route("/openapi.json", get(get_openapi_spec))
        .route("/stats", get(get_stats))
}

async fn get_agent_config_txt(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    (StatusCode::OK, "# Agent Configuration\n\nThis file describes available agents and their configurations.\n")
}

async fn get_agent_icons_txt(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    (StatusCode::OK, "# Agent Icons\n\nagent-default: 🤖\nagent-researcher: 🔬\n")
}

async fn get_adapter_config_txt(
    State(_state): State<AppState>,
    Path(adapter_type): Path<String>,
) -> impl IntoResponse {
    (StatusCode::OK, format!("# {} Adapter Configuration\n\nAdapter type: {}\n", adapter_type, adapter_type))
}

async fn get_openapi_spec(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "openapi": "3.0.0",
        "info": {"title": "Parrot Agent API", "version": "0.1.0"},
        "paths": {},
    })))
}

async fn get_stats(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "agents": 0,
        "issues": 0,
        "runs": 0,
        "companies": 0,
    })))
}
