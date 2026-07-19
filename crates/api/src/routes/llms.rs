//! LLMs/OpenAPI routes — Paperclip 一比一迁移
//!
//! 对应 Paperclip: server/src/routes/llms.ts
//! 提供 LLM/agent 配置文档和图标列表的纯文本端点。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use crate::app_state::AppState;

/// Paperclip AGENT_ICON_NAMES 常量
const AGENT_ICON_NAMES: &[&str] = &[
    "agent",
    "assistant",
    "search",
    "analytics",
    "developer",
    "writer",
    "designer",
    "researcher",
    "operator",
    "coordinator",
    "reviewer",
    "tester",
    "devops",
    "support",
    "admin",
    "custom",
];

pub fn llm_routes() -> Router<AppState> {
    Router::new()
        .route("/llms/agent-configuration.txt", get(get_agent_config_txt))
        .route("/llms/agent-icons.txt", get(get_agent_icons_txt))
        .route("/llms/agent-configuration/:adapter_type.txt", get(get_adapter_config_txt))
        .route("/openapi.json", get(get_openapi_spec))
        .route("/stats", get(get_stats))
}

/// GET /llms/agent-configuration.txt
/// 列出所有已安装 adapter 及对应的配置文档路径。
/// 对应 Paperclip: llmRoutes -> GET /llms/agent-configuration.txt
async fn get_agent_config_txt(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mut adapters: Vec<_> = state.adapter_registry.list_all();
    adapters.sort_by(|a, b| a.adapter_type().as_str().cmp(b.adapter_type().as_str()));

    let mut lines = vec![
        "# Paperclip Agent Configuration Index".to_string(),
        String::new(),
        "Installed adapters:".to_string(),
    ];

    for adapter in &adapters {
        lines.push(format!(
            "- {}: /llms/agent-configuration/{}.txt",
            adapter.adapter_type().as_str(),
            adapter.adapter_type().as_str()
        ));
    }

    lines.push(String::new());
    lines.push("Related API endpoints:".to_string());
    lines.push("- GET /api/companies/:companyId/agent-configurations".to_string());
    lines.push("- GET /api/agents/:id/configuration".to_string());
    lines.push(String::new());
    lines.push("Agent identity references:".to_string());
    lines.push("- GET /llms/agent-icons.txt".to_string());
    lines.push(String::new());
    lines.push("Notes:".to_string());
    lines.push("- Sensitive values are redacted in configuration read APIs.".to_string());
    lines.push("- New hires may be created in pending_approval state depending on company settings.".to_string());
    lines.push("- Timer heartbeats are opt-in for new hires.".to_string());

    (StatusCode::OK, [("content-type", "text/plain; charset=utf-8")], lines.join("\n"))
}

/// GET /llms/agent-icons.txt
/// 返回可用 agent icon 列表。
/// 对应 Paperclip: llmRoutes -> GET /llms/agent-icons.txt
async fn get_agent_icons_txt() -> impl IntoResponse {
    let mut lines = vec![
        "# Paperclip Agent Icon Names".to_string(),
        String::new(),
        "Set the `icon` field on hire/create payloads to one of:".to_string(),
    ];

    for name in AGENT_ICON_NAMES {
        lines.push(format!("- {}", name));
    }

    lines.push(String::new());
    lines.push("Example:".to_string());
    lines.push(r#"{ "name": "SearchOps", "role": "researcher", "icon": "search" }"#.to_string());

    (StatusCode::OK, [("content-type", "text/plain; charset=utf-8")], lines.join("\n"))
}

/// GET /llms/agent-configuration/:adapter_type.txt
/// 返回对应 adapter 的配置文档。
/// 对应 Paperclip: llmRoutes -> GET /llms/agent-configuration/:adapterType.txt
async fn get_adapter_config_txt(
    State(state): State<AppState>,
    Path(adapter_type): Path<String>,
) -> impl IntoResponse {
    let adapter = state.adapter_registry.find_server_adapter(&adapter_type);

    match adapter {
        Some(adapter) => {
            let doc = adapter.agent_configuration_doc();
            (StatusCode::OK, [("content-type", "text/plain; charset=utf-8")], doc.to_string())
        }
        None => (
            StatusCode::NOT_FOUND,
            [("content-type", "text/plain; charset=utf-8")],
            format!("Unknown adapter type: {}", adapter_type),
        ),
    }
}

/// GET /openapi.json — OpenAPI 规范
async fn get_openapi_spec(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "openapi": "3.0.0",
        "info": {"title": "Parrot Agent API", "version": "0.1.0"},
        "paths": {},
    })))
}

/// GET /stats — 系统统计
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
