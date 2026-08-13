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
    let mut adapters: Vec<_> = state.adapter_registry.adapters();
    adapters.sort_by(|a, b| a.adapter_type().to_string().cmp(&b.adapter_type().to_string()));


    let mut lines = vec![
        "# Paperclip Agent Configuration Index".to_string(),
        String::new(),
        "Installed adapters:".to_string(),
    ];

    for adapter in &adapters {
        lines.push(format!(
            "- {}: /llms/agent-configuration/{}.txt",
            adapter.adapter_type().to_string(),
            adapter.adapter_type().to_string()
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
async fn get_adapter_config_txt(
    State(state): State<AppState>,
    Path(adapter_type_str): Path<String>,
) -> impl IntoResponse {
    // Parse adapter type string
    let adapter_type = match adapter_type_str.parse::<services::server_adapter::AdapterType>() {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                [("content-type", "text/plain; charset=utf-8")],
                format!("Unknown adapter type: {}", adapter_type_str),
            )
        }
    };

    match state.adapter_registry.find_adapter(adapter_type) {
        Ok(adapter) => {
            let doc = adapter.agent_configuration_doc()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!(
                    "# {} agent configuration\n\nNo adapter-specific documentation registered.", 
                    adapter_type_str
                ));
            (StatusCode::OK, [("content-type", "text/plain; charset=utf-8")], doc)
        }
        Err(_) => (
            StatusCode::NOT_FOUND,
            [("content-type", "text/plain; charset=utf-8")],
            format!("Unknown adapter type: {}", adapter_type_str),
        ),
    }
}

/// GET /openapi.json — OpenAPI 规范
///
/// 返回一份真实的 OpenAPI 3.1 文档，覆盖平台主要资源（agents、issues、
/// companies、adapters、decisions、watchdog、skills、secrets、work products、
/// stats）。路径以 `/api` 前缀给出，与实际 production router 的挂载一致。
/// 这是一份“核心清单”而非逐路由自动生成——新增资源组时应在此同步补充。
async fn get_openapi_spec(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(build_openapi_spec()))
}

/// 构造 OpenAPI 3.1 文档（纯函数，便于测试）。
fn build_openapi_spec() -> serde_json::Value {
    let errs = serde_json::json!({
        "400": {"description": "Bad Request"},
        "401": {"description": "Unauthorized"},
        "403": {"description": "Forbidden"},
        "404": {"description": "Not Found"},
        "409": {"description": "Conflict"},
        "422": {"description": "Unprocessable Entity"},
        "500": {"description": "Internal Server Error"},
    });

    let bearer = serde_json::json!([{"bearerAuth": []}]);

    let paths = serde_json::json!({
        "/api/companies": {
            "get": op("List companies", "companies", &bearer, &errs)
        },
        "/api/companies/{companyId}": {
            "get": op("Get company", "companies", &bearer, &errs),
            "patch": op("Update company", "companies", &bearer, &errs)
        },
        "/api/companies/{companyId}/agents": {
            "get": op("List company agents", "agents", &bearer, &errs),
            "post": op("Create agent in company", "agents", &bearer, &errs)
        },
        "/api/agents": {
            "get": op("List agents", "agents", &bearer, &errs),
            "post": op("Create agent", "agents", &bearer, &errs)
        },
        "/api/agents/{id}": {
            "get": op("Get agent", "agents", &bearer, &errs),
            "patch": op("Update agent", "agents", &bearer, &errs),
            "delete": op("Delete/terminate agent", "agents", &bearer, &errs)
        },
        "/api/companies/{companyId}/issues": {
            "get": op("List company issues", "issues", &bearer, &errs),
            "post": op("Create issue", "issues", &bearer, &errs)
        },
        "/api/issues/{id}": {
            "get": op("Get issue", "issues", &bearer, &errs),
            "patch": op("Update issue", "issues", &bearer, &errs)
        },
        "/api/companies/{companyId}/adapters": {
            "get": op("List adapters for company", "adapters", &bearer, &errs)
        },
        "/api/adapters": {
            "get": op("List all adapters (instance)", "adapters", &bearer, &errs),
            "post": op("Install adapter (instance admin)", "adapters", &bearer, &errs)
        },
        "/api/adapters/{adapterType}": {
            "get": op("Get adapter info", "adapters", &bearer, &errs),
            "patch": op("Update adapter config", "adapters", &bearer, &errs),
            "delete": op("Delete adapter (instance admin)", "adapters", &bearer, &errs)
        },
        "/api/adapters/{adapterType}/reload": {
            "post": op("Reload adapter (instance admin)", "adapters", &bearer, &errs)
        },
        "/api/adapters/{adapterType}/reinstall": {
            "post": op("Reinstall adapter (instance admin)", "adapters", &bearer, &errs)
        },
        "/api/adapters/{adapterType}/override": {
            "patch": op("Toggle external override (builtin only)", "adapters", &bearer, &errs)
        },
        "/api/adapters/{adapterType}/config-schema": {
            "get": op("Get adapter config schema", "adapters", &bearer, &errs)
        },
        "/api/adapters/{adapterType}/ui-parser.js": {
            "get": op("Get adapter UI parser", "adapters", &bearer, &errs)
        },
        "/api/decisions": {
            "get": op("List decisions (company scoped)", "decisions", &bearer, &errs),
            "post": op("Create decision", "decisions", &bearer, &errs)
        },
        "/api/decisions/{id}": {
            "get": op("Get decision", "decisions", &bearer, &errs),
            "patch": op("Update/act on decision", "decisions", &bearer, &errs),
            "delete": op("Delete decision", "decisions", &bearer, &errs)
        },
        "/api/companies/{companyId}/attention": {
            "get": op("Attention aggregate", "decisions", &bearer, &errs)
        },
        "/api/decision-queues": {
            "get": op("List decision queues", "decisions", &bearer, &errs),
            "post": op("Create decision queue", "decisions", &bearer, &errs)
        },
        "/api/decision-training": {
            "get": op("List decision training snapshots", "decisions", &bearer, &errs),
            "post": op("Export decision training", "decisions", &bearer, &errs)
        },
        "/api/heartbeat-runs/{runId}/watchdog-decisions": {
            "get": op("List watchdog decisions", "watchdog", &bearer, &errs),
            "post": op("Submit watchdog decision", "watchdog", &bearer, &errs)
        },
        "/api/skills": {
            "get": op("List skills", "skills", &bearer, &errs),
            "post": op("Create skill", "skills", &bearer, &errs)
        },
        "/api/companies/{companyId}/skill-policy": {
            "get": op("Get company skill policy", "skills", &bearer, &errs),
            "delete": op("Reset company skill policy", "skills", &bearer, &errs)
        },
        "/api/secrets": {
            "get": op("List company secrets", "secrets", &bearer, &errs),
            "post": op("Create secret", "secrets", &bearer, &errs)
        },
        "/api/user-secrets": {
            "get": op("List user secrets", "secrets", &bearer, &errs),
            "post": op("Create user secret", "secrets", &bearer, &errs)
        },
        "/api/companies/{companyId}/work-products": {
            "get": op("List work products", "work-products", &bearer, &errs)
        },
        "/api/attachments/{id}": {
            "get": op("Get attachment content", "attachments", &bearer, &errs)
        },
        "/api/stats": {
            "get": op("Instance statistics", "ops", &bearer, &errs)
        },
        "/api/openapi.json": {
            "get": op("This OpenAPI document", "ops", &bearer, &errs)
        }
    });

    let spec = serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Parrot Agent API",
            "version": "0.1.0",
            "description": "Parrot Agent control-plane API. All routes are mounted under /api and require bearer authentication."
        },
        "servers": [{"url": "/api"}],
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT"
                }
            }
        },
        "security": bearer,
        "paths": paths
    });

    spec
}

/// 构造一个最小但完整的 OpenAPI operation 对象。
fn op(summary: &str, tag: &str, security: &serde_json::Value, errs: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "summary": summary,
        "tags": [tag],
        "security": security,
        "responses": {
            "200": {"description": "OK"},
            "400": errs["400"],
            "401": errs["401"],
            "403": errs["403"],
            "404": errs["404"],
            "409": errs["409"],
            "422": errs["422"],
            "500": errs["500"]
        }
    })
}

/// GET /stats — 系统统计
///
/// 对应 Paperclip: GET /stats。返回实例级聚合计数，均从数据库实时查询，
/// 不再返回硬编码的零值。数据库连接失败或表缺失时对应字段回退为 0，
/// 避免监控端点本身触发 500。
async fn get_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let agents: i64 = sqlx::query_scalar("SELECT count(*) FROM agents")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);
    let issues: i64 = sqlx::query_scalar("SELECT count(*) FROM issues")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);
    let companies: i64 = sqlx::query_scalar("SELECT count(*) FROM companies")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM heartbeat_runs")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

    Ok(Json(serde_json::json!({
        "agents": agents,
        "issues": issues,
        "runs": runs,
        "companies": companies,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openapi_spec_is_valid_and_non_empty() {
        let spec = build_openapi_spec();
        assert_eq!(spec["openapi"], "3.1.0");

        let paths = spec["paths"].as_object().expect("paths must be an object");
        assert!(!paths.is_empty(), "openapi paths must not be empty");

        // bearerAuth security scheme declared
        let scheme = &spec["components"]["securitySchemes"]["bearerAuth"];
        assert_eq!(scheme["type"], "http");
        assert_eq!(scheme["scheme"], "bearer");

        // a few known resources present
        assert!(paths.contains_key("/api/agents"));
        assert!(paths.contains_key("/api/adapters/{adapterType}/reload"));
        assert!(paths.contains_key("/api/decisions"));
        assert!(paths.contains_key("/api/heartbeat-runs/{runId}/watchdog-decisions"));

        // every operation declares security
        for (_path, methods) in paths {
            for (_method, op) in methods.as_object().unwrap() {
                assert!(
                    op["security"].as_array().map(|s| !s.is_empty()).unwrap_or(false),
                    "operation must declare security"
                );
                assert!(op["responses"].get("200").is_some(), "operation must define 200");
            }
        }
    }
}
