//! Tool Access —— 对齐 Paperclip `routes/tool-access.ts` 的 MCP 连接、目录、
//! profile、policy、OAuth、runtime 和治理工作流；持久化数据通过本模块的
//! company-scoped queries 访问，MCP transport/secret projection 由 `routes::tools`
//! 共享。

use axum::{
    extract::{Extension, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::time::Duration as StdDuration;
use sqlx::Row;
use url::Url;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{
    assert_board, has_company_access, require_company_access, AccessMode,
};
use services::auth::{AuthorizationAction, AuthorizationActor, AuthorizationService, PermissionKey};
use services::secret_provider::{decrypt_secret_material, encrypt_secret_material, sha256_hex};

// ---------- companies/:cid/tools/* 只读聚合 ----------

fn require_board_company_access(
    actor: &AuthorizationActor,
    company_id: Uuid,
    mode: AccessMode,
) -> Result<(), StatusCode> {
    assert_board(actor)?;
    require_company_access(actor, company_id, mode)
}

fn parrot_mcp_gallery_apps() -> Vec<Value> {
    let ownership = json!({
        "platformShared": false,
        "platformProvisioned": false,
        "customer": true,
        "dcr": true,
    });
    let remote_method = |key: &str,
                         auth: &str,
                         ownership_modes: Value,
                         server_url: &str,
                         authorization_endpoint: Option<&str>,
                         token_endpoint: Option<&str>,
                         scopes_hint: Value,
                         guidance: &str,
                         risk_tier: &str,
                         credential_fields: Value,
                         key_placement: Value,
                         required_resource_filters: Value| {
        let mut defaults = json!({"serverUrl": server_url});
        if let Some(endpoint) = authorization_endpoint {
            defaults["authorizationEndpoint"] = json!(endpoint);
        }
        if let Some(endpoint) = token_endpoint {
            defaults["tokenEndpoint"] = json!(endpoint);
        }
        if !scopes_hint.is_null() {
            defaults["scopesHint"] = scopes_hint;
        }
        json!({
            "key": key,
            "transport": "mcp_remote",
            "auth": auth,
            "ownershipModes": ownership_modes,
            "whenToUse": "Use the provider-hosted connection for the quickest setup.",
            "defaults": defaults,
            "guidanceMd": guidance,
            "riskTier": risk_tier,
            "credentialFields": credential_fields,
            "keyPlacement": key_placement,
            "requiredResourceFilters": required_resource_filters,
        })
    };
    let local_method = |key: &str,
                        template_key: &str,
                        guidance: &str,
                        risk_tier: &str,
                        required_resource_filters: Value| {
        json!({
            "key": key,
            "transport": "local_stdio",
            "auth": "none",
            "ownershipModes": ["customer"],
            "whenToUse": "Use credentials from your provider account.",
            "defaults": {"templateKey": template_key},
            "guidanceMd": guidance,
            "riskTier": risk_tier,
            "requiredResourceFilters": required_resource_filters,
        })
    };
    let api_key = |label: &str, placeholder: &str| {
        json!([{
            "key": "authorization",
            "label": label,
            "type": "password",
            "required": true,
            "placeholder": placeholder,
            "secret": true,
        }])
    };
    let bearer_placement = json!({
        "location": "header",
        "name": "Authorization",
        "prefix": "Bearer ",
    });
    vec![
        json!({
            "schemaVersion": 1,
            "slug": "zapier",
            "name": "Zapier",
            "description": "Reach thousands of apps through your Zapier account.",
            "categories": ["productivity"],
            "featured": true,
            "branding": {"logoUrl": "https://www.google.com/s2/favicons?domain=zapier.com&sz=128"},
            "urlPatterns": ["https://mcp.zapier.com/*"],
            "methods": [remote_method(
                "mcp-key", "api_key", json!(["customer"]), "https://mcp.zapier.com/api/mcp",
                None, None, Value::Null,
                "Create a Zapier MCP connection, then paste its token here.", "S3",
                api_key("Zapier MCP token", "Paste your Zapier token"), bearer_placement.clone(), Value::Null,
            )],
            "ownershipAvailability": ownership.clone(),
            "availability": {"available": true},
        }),
        json!({
            "schemaVersion": 1,
            "slug": "github",
            "name": "GitHub",
            "description": "Read code and pull requests, and coordinate repository work.",
            "categories": ["developer"],
            "featured": true,
            "branding": {"logoUrl": "https://www.google.com/s2/favicons?domain=github.com&sz=128"},
            "urlPatterns": ["https://api.githubcopilot.com/mcp/*"],
            "methods": [remote_method(
                "mcp-key", "api_key", json!(["customer"]), "https://api.githubcopilot.com/mcp/",
                None, None, Value::Null,
                "Create a fine-grained token limited to the repositories agents should use.", "S3",
                api_key("GitHub token", "github_pat_..."), bearer_placement.clone(),
                json!(["organization", "repository"]),
            )],
            "ownershipAvailability": ownership.clone(),
            "availability": {"available": true},
        }),
        json!({
            "schemaVersion": 1,
            "slug": "slack",
            "name": "Slack",
            "description": "Search channels and coordinate team communication.",
            "categories": ["communication"],
            "featured": true,
            "branding": {"logoUrl": "https://www.google.com/s2/favicons?domain=slack.com&sz=128"},
            "urlPatterns": ["https://mcp.slack.com/*"],
            "methods": [remote_method(
                "mcp-oauth", "oauth", json!(["customer", "dcr"]), "https://mcp.slack.com/mcp",
                Some("https://slack.com/oauth/v2/authorize"), Some("https://slack.com/api/oauth.v2.access"),
                json!(["channels:read", "chat:write", "search:read"]),
                "Connect a Slack workspace and limit access to the channels agents need.", "S3",
                Value::Null, Value::Null, json!(["workspace", "channel"]),
            )],
            "ownershipAvailability": ownership.clone(),
            "availability": {"available": true},
        }),
        json!({
            "schemaVersion": 1,
            "slug": "notion",
            "name": "Notion",
            "description": "Read and update pages in your Notion workspace.",
            "categories": ["content"],
            "featured": true,
            "branding": {"logoUrl": "https://www.google.com/s2/favicons?domain=notion.so&sz=128"},
            "urlPatterns": ["https://mcp.notion.com/*"],
            "methods": [remote_method(
                "mcp-oauth", "oauth", json!(["customer", "dcr"]), "https://mcp.notion.com/mcp",
                None, None, Value::Null,
                "Connect Notion for workspace content. Share only the pages and databases agents should use.", "S3",
                Value::Null, Value::Null, json!(["workspace", "page", "database"]),
            )],
            "redirectConstraints": "https-or-loopback-http",
            "ownershipAvailability": ownership.clone(),
            "availability": {"available": true},
        }),
        json!({
            "schemaVersion": 1,
            "slug": "linear",
            "name": "Linear",
            "description": "Create, update, and read Linear issues.",
            "categories": ["productivity"],
            "featured": true,
            "branding": {"logoUrl": "https://www.google.com/s2/favicons?domain=linear.app&sz=128"},
            "urlPatterns": ["https://mcp.linear.app/*"],
            "methods": [remote_method(
                "mcp-oauth", "oauth", json!(["customer", "dcr"]), "https://mcp.linear.app/mcp",
                Some("https://linear.app/oauth/authorize"), Some("https://api.linear.app/oauth/token"),
                json!(["read", "write"]),
                "Register a Linear OAuth app and add the Parrot redirect URI before connecting.", "S2",
                Value::Null, Value::Null, json!(["workspace", "team", "project"]),
            )],
            "ownershipAvailability": ownership.clone(),
            "availability": {"available": true},
        }),
        json!({
            "schemaVersion": 1,
            "slug": "google-sheets",
            "name": "Google Sheets",
            "description": "Read and update selected spreadsheets.",
            "categories": ["data"],
            "featured": false,
            "branding": {"logoUrl": "https://www.google.com/s2/favicons?domain=sheets.google.com&sz=128"},
            "urlPatterns": ["https://docs.google.com/spreadsheets/*", "https://sheets.google.com/*"],
            "methods": [local_method(
                "local", "paperclip.google-sheets",
                "Share each spreadsheet with the configured service account, then paste the sheet links.", "S3",
                json!(["spreadsheet"]),
            )],
            "ownershipAvailability": ownership.clone(),
            "availability": {"available": false, "reason": "An active paperclip.google-sheets stdio template is required on this Parrot instance."},
        }),
        json!({
            "schemaVersion": 1,
            "slug": "context7",
            "name": "Context7",
            "description": "Look up current documentation for software libraries.",
            "categories": ["developer"],
            "featured": false,
            "branding": {"logoUrl": "https://www.google.com/s2/favicons?domain=context7.com&sz=128"},
            "urlPatterns": ["https://mcp.context7.com/*"],
            "methods": [remote_method(
                "mcp", "none", json!(["customer"]), "https://mcp.context7.com/mcp",
                None, None, Value::Null,
                "Connect Context7 to give agents current library documentation.", "S1",
                Value::Null, Value::Null, Value::Null,
            )],
            "ownershipAvailability": ownership.clone(),
            "availability": {"available": true},
        }),
        // Parrot-specific generic entries remain useful for arbitrary MCP
        // servers, while the seven entries above mirror Paperclip's gallery.
        json!({
            "slug": "custom-mcp-http",
            "name": "Custom MCP (HTTP)",
            "description": "Connect a Streamable HTTP or SSE MCP server and review its discovered tools.",
            "category": "mcp",
            "transport": "mcp_remote",
            "connectionMethods": [{"transport": "mcp_remote", "auth": "none", "requiresLink": true}],
            "parrotExtension": true,
            "ownershipAvailability": {"platformShared": true, "platformProvisioned": true, "customer": true, "dcr": true},
            "availability": {"available": true}
        }),
        json!({
            "slug": "custom-mcp-stdio",
            "name": "Custom MCP (approved stdio)",
            "description": "Use a reviewed stdio command template; arbitrary commands are never accepted from the client.",
            "category": "mcp",
            "transport": "local_stdio",
            "connectionMethods": [{"transport": "local_stdio", "auth": "none", "requiresApprovedTemplate": true}],
            "parrotExtension": true,
            "ownershipAvailability": {"platformShared": true, "platformProvisioned": true, "customer": true, "dcr": true},
            "availability": {"available": true}
        }),
    ]
}

/// GET /companies/:cid/tools/gallery —— 可连接的 MCP provider 目录。
async fn tools_gallery(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let google_sheets_ready = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM tool_stdio_command_templates
              WHERE company_id = $1
                AND template_key = 'paperclip.google-sheets'
                AND status = 'active'
         )",
    )
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to inspect Google Sheets MCP template");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut apps = parrot_mcp_gallery_apps();
    if let Some(google_sheets) = apps
        .iter_mut()
        .find(|app| app.get("slug").and_then(Value::as_str) == Some("google-sheets"))
    {
        google_sheets["availability"] = if google_sheets_ready {
            json!({"available": true})
        } else {
            json!({"available": false, "reason": "An active paperclip.google-sheets stdio template is required on this Parrot instance."})
        };
    }
    Ok(Json(json!({ "apps": apps })))
}

/// GET /companies/:cid/tools/examples —— 可重复安装的内置 MCP 治理示例。
async fn tools_examples(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let application_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM tool_applications
          WHERE company_id = $1 AND application_key = $2",
    )
    .bind(company_id)
    .bind("paperclip.examples.safe-read-only-todo-kv")
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to inspect MCP example application");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let connection_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM tool_connections
          WHERE company_id = $1 AND name = $2
          ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind("Paperclip example: Safe read-only Todo / KV")
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to inspect MCP example connection");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let profile_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM tool_profiles
          WHERE company_id = $1 AND profile_key = $2",
    )
    .bind(company_id)
    .bind("paperclip.examples.safe-read-only-todo-kv.profile")
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to inspect MCP example profile");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let profile_binding_id = if let Some(profile_id) = profile_id {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM tool_profile_bindings
              WHERE company_id = $1 AND profile_id = $2
                AND target_type = 'company' AND target_id = $1::text
              ORDER BY created_at DESC LIMIT 1",
        )
        .bind(company_id)
        .bind(profile_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %company_id, "Failed to inspect MCP example profile binding");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else {
        None
    };
    let catalog = if let Some(connection_id) = connection_id {
        sqlx::query(
            "SELECT id, tool_name, description, risk_level, status
               FROM tool_catalog_entries
              WHERE company_id = $1 AND connection_id = $2
              ORDER BY tool_name ASC",
        )
        .bind(company_id)
        .bind(connection_id)
        .fetch_all(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %company_id, "Failed to inspect MCP example catalog");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else {
        Vec::new()
    };
    let fixture_tools = crate::routes::tools::builtin_mcp_template_tools(
        "paperclip.synthetic-todo-kv",
    )
    .and_then(|tools| tools.as_array().cloned())
    .unwrap_or_default()
    .into_iter()
    .map(|tool| {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let risk_level = tool
            .get("annotations")
            .map(|annotations| services::tool_access_contract::classify_risk(name, annotations))
            .unwrap_or("read");
        json!({
            "name": name,
            "description": tool.get("description"),
            "riskLevel": risk_level,
            "readOnly": risk_level == "read",
        })
    })
    .collect::<Vec<_>>();
    let installed = application_id.is_some()
        && connection_id.is_some()
        && profile_id.is_some()
        && profile_binding_id.is_some();
    let catalog_tools = catalog
        .iter()
        .map(|row| {
            use sqlx::Row;
            json!({
                "id": row.get::<Uuid, _>("id"),
                "name": row.get::<String, _>("tool_name"),
                "description": row.get::<Option<String>, _>("description"),
                "riskLevel": row.get::<String, _>("risk_level"),
                "status": row.get::<String, _>("status"),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "examples": [{
            "id": "safe-read-only-todo-kv",
            "title": "Safe read-only Todo / KV fixture",
            "description": "Installs a deterministic built-in MCP fixture and grants only its read-only catalog entries.",
            "fixture": {
                "transport": "local_stdio",
                "templateId": "paperclip.synthetic-todo-kv",
                "available": true,
                "tools": fixture_tools,
            },
            "safeDefaultProfile": {
                "profileKey": "paperclip.examples.safe-read-only-todo-kv.profile",
                "name": "Example safe read-only tools",
                "defaultAction": "deny",
                "allowedToolNames": ["list_items", "get_value"],
            },
            "catalog": catalog_tools,
            "install": {
                "installed": installed,
                "canInstall": true,
                "applicationId": application_id,
                "connectionId": connection_id,
                "profileId": profile_id,
                "profileBindingId": profile_binding_id,
            },
        }]
    })))
}

/// GET /companies/:cid/tools/apps/attention —— 返回需要操作员介入的 MCP 连接。
async fn tools_attention_apps(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;

    let rows = sqlx::query(
        "SELECT c.id, c.name, c.uid, c.status, c.health_status, c.health_message,
                c.enabled, c.application_id,
                COALESCE(q.quarantined_count, 0) AS quarantined_count,
                COALESCE(a.pending_count, 0) AS pending_count,
                COALESCE(n.pending_count, 0) AS new_tools_pending_count
           FROM tool_connections c
           LEFT JOIN (
                SELECT connection_id, COUNT(*)::bigint AS quarantined_count
                  FROM tool_catalog_entries
                 WHERE company_id = $1 AND status = 'quarantined'
                 GROUP BY connection_id
           ) q ON q.connection_id = c.id
           LEFT JOIN (
                SELECT i.connection_id, COUNT(*)::bigint AS pending_count
                  FROM tool_action_requests r
                  JOIN tool_invocations i ON i.id = r.invocation_id
                 WHERE r.company_id = $1 AND r.status = 'pending'
                 GROUP BY i.connection_id
           ) a ON a.connection_id = c.id
           LEFT JOIN (
                SELECT connection_id, COUNT(*)::bigint AS pending_count
                  FROM tool_catalog_entries
                 WHERE company_id = $1 AND status = 'active' AND reviewed_at IS NULL
                 GROUP BY connection_id
           ) n ON n.connection_id = c.id
          WHERE c.company_id = $1 AND c.status <> 'archived'
          ORDER BY c.name ASC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to list MCP connections needing attention");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let attention = rows
        .iter()
        .filter_map(|row| {
            let health_status: String = row.get("health_status");
            let health_needs_attention = services::tool_access_contract::is_tool_connection_attention_health(
                &health_status,
            ) || matches!(health_status.as_str(), "unhealthy" | "missing_secret");
            let quarantined_count: i64 = row.get("quarantined_count");
            let pending_count: i64 = row.get("pending_count");
            let new_tools_pending_count: i64 = row.get("new_tools_pending_count");
            if !health_needs_attention
                && quarantined_count == 0
                && pending_count == 0
                && new_tools_pending_count == 0
            {
                return None;
            }
            let mut reasons = Vec::new();
            if health_needs_attention {
                reasons.push("health");
            }
            if quarantined_count > 0 {
                reasons.push("quarantined_catalog_entries");
            }
            if pending_count > 0 {
                reasons.push("pending_action_requests");
            }
            if new_tools_pending_count > 0 {
                reasons.push("profile_new_tools");
            }
            Some(json!({
                "connection": {
                    "id": row.get::<Uuid, _>("id"),
                    "name": row.get::<String, _>("name"),
                    "uid": row.get::<String, _>("uid"),
                    "status": row.get::<String, _>("status"),
                    "healthStatus": health_status,
                    "healthMessage": row.get::<Option<String>, _>("health_message"),
                    "enabled": row.get::<bool, _>("enabled"),
                    "applicationId": row.get::<Uuid, _>("application_id"),
                },
                "healthNeedsAttention": health_needs_attention,
                "quarantinedCatalogEntryCount": quarantined_count,
                "pendingActionRequestCount": pending_count,
                "newToolsPendingReviewCount": new_tools_pending_count,
                "newToolsPendingProfiles": [],
                "reasons": reasons,
            }))
        })
        .collect::<Vec<Value>>();
    let totals = json!({
        "connections": attention.len(),
        "health": attention.iter().filter(|app| app.get("healthNeedsAttention").and_then(Value::as_bool) == Some(true)).count(),
        "quarantinedCatalogEntries": attention.iter().filter_map(|app| app.get("quarantinedCatalogEntryCount").and_then(Value::as_i64)).sum::<i64>(),
        "pendingActionRequests": attention.iter().filter_map(|app| app.get("pendingActionRequestCount").and_then(Value::as_i64)).sum::<i64>(),
        "newToolsPendingReview": attention.iter().filter_map(|app| app.get("newToolsPendingReviewCount").and_then(Value::as_i64)).sum::<i64>(),
        "newToolsPendingProfiles": 0,
    });
    Ok(Json(json!({
        "generatedAt": Utc::now(),
        "apps": attention,
        "totals": totals,
    })))
}

#[derive(Debug, Deserialize, Default)]
struct ToolActionRequestListQuery {
    status: Option<String>,
}

/// GET /companies/:cid/tools/action-requests —— 从 tool_action_requests 列表。
async fn tools_action_requests(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ToolActionRequestListQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let status = query
        .status
        .filter(|status| {
            matches!(
                status.as_str(),
                "pending" | "approved" | "executing" | "rejected" | "declined"
                    | "expired" | "cancelled" | "executed" | "failed"
            )
        })
        .unwrap_or_else(|| "pending".to_string());
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT r.id, i.agent_id, i.tool_name, i.policy_decision,
                r.issue_id, r.invocation_id, r.status, r.created_at,
                r.expires_at
           FROM tool_action_requests r
           JOIN tool_invocations i ON i.id = r.invocation_id
          WHERE r.company_id = $1
            AND ($2::text IS NULL OR r.status = $2)
          ORDER BY r.created_at DESC LIMIT 100",
    )
    .bind(company_id)
    .bind(status)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list tool action requests: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(json!({
        "actionRequests": rows.iter().map(|r| json!({
            "id": r.get::<Uuid, _>("id"),
            "agentId": r.get::<Option<Uuid>, _>("agent_id"),
            "toolName": r.get::<String, _>("tool_name"),
            "action": "tool_call",
            "policyDecision": r.get::<Option<String>, _>("policy_decision"),
            "issueId": r.get::<Option<Uuid>, _>("issue_id"),
            "invocationId": r.get::<Uuid, _>("invocation_id"),
            "status": r.get::<String, _>("status"),
            "expiresAt": r.get::<Option<DateTime<Utc>>, _>("expires_at"),
            "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
        })).collect::<Vec<_>>()
    })))
}

/// GET /companies/:cid/tools/applications —— tool_applications 列表。
async fn tools_applications(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let rows = sqlx::query(
        "SELECT ta.id, ta.company_id, ta.application_key,
                COALESCE(NULLIF(ta.name, ''), NULLIF(tc.name, ''), ta.application_key,
                         'legacy-tool-application') AS name,
                ta.description,
                COALESCE(NULLIF(ta.type, ''), 'custom') AS application_type,
                CASE WHEN ta.status IN ('draft', 'active', 'disabled', 'archived')
                     THEN ta.status ELSE 'active' END AS status,
                ta.plugin_id, ta.owner_agent_id, ta.owner_user_id,
                COALESCE(ta.metadata, '{}'::jsonb) AS metadata,
                ta.archived_at, ta.created_at, ta.updated_at,
                ta.agent_id AS legacy_agent_id,
                ta.connection_id AS legacy_connection_id
           FROM tool_applications ta
           LEFT JOIN tool_connections tc
             ON tc.id = ta.connection_id AND tc.company_id = ta.company_id
          WHERE ta.company_id = $1
          ORDER BY ta.updated_at DESC
          LIMIT 100",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list tool applications: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let applications = rows.iter().map(tool_application_json).collect::<Vec<_>>();
    Ok(Json(json!({ "applications": applications })))
}

fn tool_application_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;
    let mut application = json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "applicationKey": row.get::<Option<String>, _>("application_key"),
        "name": row.get::<String, _>("name"),
        "description": row.get::<Option<String>, _>("description"),
        "type": row.get::<String, _>("application_type"),
        "status": row.get::<String, _>("status"),
        "pluginId": row.get::<Option<Uuid>, _>("plugin_id"),
        "ownerAgentId": row.get::<Option<Uuid>, _>("owner_agent_id"),
        "ownerUserId": row.get::<Option<String>, _>("owner_user_id"),
        "metadata": row.get::<Value, _>("metadata"),
        "archivedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("archived_at"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    });
    if let Some(object) = application.as_object_mut() {
        if let Some(agent_id) = row.get::<Option<Uuid>, _>("legacy_agent_id") {
            object.insert("agentId".to_string(), json!(agent_id));
        }
        if let Some(connection_id) = row.get::<Option<Uuid>, _>("legacy_connection_id") {
            object.insert("connectionId".to_string(), json!(connection_id));
        }
    }
    application
}

fn is_safe_tool_application_key(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase() || ch.is_ascii_digit())
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
}

fn normalize_tool_application_key(value: &str) -> String {
    let mut normalized = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
        } else if !normalized.ends_with('-') {
            normalized.push('-');
        }
    }
    let normalized = normalized.trim_matches('-');
    if normalized.is_empty() {
        "tool-application".to_string()
    } else {
        normalized.to_string()
    }
}

fn validate_tool_application_type(value: &str) -> bool {
    matches!(value, "mcp_http" | "mcp_stdio" | "paperclip_plugin" | "a2a")
}

fn validate_tool_application_status(value: &str) -> bool {
    matches!(value, "draft" | "active" | "disabled" | "archived")
}

async fn validate_tool_application_references(
    state: &AppState,
    company_id: Uuid,
    plugin_id: Option<Uuid>,
    owner_agent_id: Option<Uuid>,
) -> Result<(), StatusCode> {
    if let Some(plugin_id) = plugin_id {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM plugins WHERE id = $1)",
        )
        .bind(plugin_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to validate tool application plugin: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if !exists {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
    if let Some(owner_agent_id) = owner_agent_id {
        let belongs_to_company = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM agents WHERE id = $1 AND company_id = $2
             )",
        )
        .bind(owner_agent_id)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to validate tool application owner agent: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if !belongs_to_company {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
    Ok(())
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .as_deref()
        == Some("23505")
}

/// POST /companies/:cid/tools/applications —— 创建应用定义。
#[derive(Debug, Deserialize)]
struct CreateToolApplicationRequest {
    #[serde(rename = "applicationKey")]
    application_key: Option<String>,
    name: String,
    description: Option<String>,
    #[serde(rename = "type")]
    application_type: String,
    status: Option<String>,
    #[serde(rename = "pluginId")]
    plugin_id: Option<Uuid>,
    #[serde(rename = "ownerAgentId")]
    owner_agent_id: Option<Uuid>,
    #[serde(rename = "ownerUserId")]
    owner_user_id: Option<String>,
    metadata: Option<Value>,
}
async fn create_tool_application(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateToolApplicationRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let name = request.name.trim();
    let application_type = request.application_type.trim();
    let status = request.status.as_deref().unwrap_or("active");
    if name.is_empty()
        || name.chars().count() > 160
        || !validate_tool_application_type(application_type)
        || !validate_tool_application_status(status)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let application_key = request
        .application_key
        .as_deref()
        .map(str::trim)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| normalize_tool_application_key(name));
    if application_key.chars().count() > 160 || !is_safe_tool_application_key(&application_key) {
        return Err(StatusCode::BAD_REQUEST);
    }
    validate_tool_application_references(
        &state,
        company_id,
        request.plugin_id,
        request.owner_agent_id,
    )
    .await?;
    let metadata = request
        .metadata
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!({}));
    let id = Uuid::new_v4();
    let row = sqlx::query(
        "INSERT INTO tool_applications
            (id, company_id, application_key, name, description, type, status,
             plugin_id, owner_agent_id, owner_user_id, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING id, company_id, application_key, name, description,
                   type AS application_type, status, plugin_id, owner_agent_id,
                   owner_user_id, metadata, archived_at, created_at, updated_at,
                   NULL::uuid AS legacy_agent_id,
                   NULL::uuid AS legacy_connection_id",
    )
    .bind(id)
    .bind(company_id)
    .bind(application_key)
    .bind(name)
    .bind(request.description)
    .bind(application_type)
    .bind(status)
    .bind(request.plugin_id)
    .bind(request.owner_agent_id)
    .bind(request.owner_user_id)
    .bind(metadata)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            return StatusCode::CONFLICT;
        }
        tracing::error!("Failed to create tool application: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_application.created",
        &actor,
        "tool_application",
        id,
        json!({ "name": name, "type": application_type }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(tool_application_json(&row))))
}

/// GET /companies/:cid/tools/profiles —— tool_profiles 列表。
async fn tools_profiles(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows =
        sqlx::query("SELECT * FROM tool_profiles WHERE company_id = $1 ORDER BY created_at ASC")
            .bind(company_id)
            .fetch_all(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list tool profiles: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let entry_rows = sqlx::query(
        "SELECT * FROM tool_profile_entries
          WHERE company_id = $1
          ORDER BY created_at ASC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to list tool profile entries");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let binding_rows = sqlx::query(
        "SELECT * FROM tool_profile_bindings
          WHERE company_id = $1
          ORDER BY priority ASC, created_at ASC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to list tool profile bindings");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut entries_by_profile: HashMap<Uuid, Vec<Value>> = HashMap::new();
    for row in &entry_rows {
        entries_by_profile
            .entry(row.get::<Uuid, _>("profile_id"))
            .or_default()
            .push(json!({
                "id": row.get::<Uuid, _>("id"),
                "companyId": row.get::<Uuid, _>("company_id"),
                "profileId": row.get::<Uuid, _>("profile_id"),
                "selectorType": row.get::<String, _>("selector_type"),
                "effect": row.get::<String, _>("effect"),
                "applicationId": row.get::<Option<Uuid>, _>("application_id"),
                "connectionId": row.get::<Option<Uuid>, _>("connection_id"),
                "catalogEntryId": row.get::<Option<Uuid>, _>("catalog_entry_id"),
                "toolName": row.get::<Option<String>, _>("tool_name"),
                "riskLevel": row.get::<Option<String>, _>("risk_level"),
                "conditions": row.get::<Option<Value>, _>("conditions"),
                "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
                "updatedAt": row.get::<DateTime<Utc>, _>("updated_at"),
            }));
    }
    let mut bindings_by_profile: HashMap<Uuid, Vec<Value>> = HashMap::new();
    for row in &binding_rows {
        bindings_by_profile
            .entry(row.get::<Uuid, _>("profile_id"))
            .or_default()
            .push(json!({
                "id": row.get::<Uuid, _>("id"),
                "companyId": row.get::<Uuid, _>("company_id"),
                "profileId": row.get::<Uuid, _>("profile_id"),
                "targetType": row.get::<String, _>("target_type"),
                "targetId": row.get::<String, _>("target_id"),
                "priority": row.get::<i32, _>("priority"),
                "metadata": row.get::<Value, _>("metadata"),
                "createdByAgentId": row.get::<Option<Uuid>, _>("created_by_agent_id"),
                "createdByUserId": row.get::<Option<String>, _>("created_by_user_id"),
                "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
                "updatedAt": row.get::<DateTime<Utc>, _>("updated_at"),
            }));
    }
    Ok(Json(json!({
        "profiles": rows.iter().map(|r| json!({
            "id": r.get::<Uuid, _>("id"),
            "companyId": r.get::<Uuid, _>("company_id"),
            "profileKey": r.get::<String, _>("profile_key"),
            "name": r.get::<String, _>("name"),
            "description": r.get::<Option<String>, _>("description"),
            "status": r.get::<String, _>("status"),
            "defaultAction": r.get::<String, _>("default_action"),
            "newToolsReviewedAt": r.get::<Option<DateTime<Utc>>, _>("new_tools_reviewed_at"),
            "metadata": r.get::<Value, _>("metadata"),
            "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
            "updatedAt": r.get::<DateTime<Utc>, _>("updated_at"),
            "entries": entries_by_profile.get(&r.get::<Uuid, _>("id")).cloned().unwrap_or_default(),
            "bindings": bindings_by_profile.get(&r.get::<Uuid, _>("id")).cloned().unwrap_or_default(),
        })).collect::<Vec<_>>()
    })))
}

/// GET /companies/:cid/tools/runtime-health —— 连接健康聚合。
async fn tools_runtime_health(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT status, COUNT(*) AS cnt FROM tool_connections WHERE company_id = $1 GROUP BY status",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to aggregate tool health: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut by_status = serde_json::Map::new();
    for r in &rows {
        by_status.insert(r.get::<String, _>("status"), json!(r.get::<i64, _>("cnt")));
    }
    Ok(Json(
        json!({ "connectionsByStatus": by_status, "healthy": true }),
    ))
}

/// GET /companies/:cid/tools/runtime-slots —— 当前工具运行时槽位。
async fn tools_runtime_slots(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, connection_id, slot_key, runtime_kind, status, health_status, process_id, last_error, last_used_at, updated_at FROM tool_runtime_slots WHERE company_id = $1 ORDER BY updated_at DESC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list tool runtime slots: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let runtime_slots = rows.iter().map(|row| json!({
        "id": row.get::<Uuid, _>("id"),
        "connectionId": row.get::<Option<Uuid>, _>("connection_id"),
        "slotKey": row.get::<String, _>("slot_key"),
        "runtimeKind": row.get::<String, _>("runtime_kind"),
        "status": row.get::<String, _>("status"),
        "healthStatus": row.get::<String, _>("health_status"),
        "processId": row.get::<Option<i32>, _>("process_id"),
        "lastError": row.get::<Option<String>, _>("last_error"),
        "lastUsedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_used_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })).collect::<Vec<_>>();
    Ok(Json(json!({ "runtimeSlots": runtime_slots })))
}

/// GET /companies/:cid/tools/trust-rules —— 已持久化的工具策略。
async fn tools_trust_rules(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, name, description, policy_type, priority, enabled, selectors, conditions, config, created_at, updated_at
         FROM tool_policies
        WHERE company_id = $1 AND policy_type = 'trust_rule'
        ORDER BY priority DESC, created_at ASC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list tool policies: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let trust_rules = rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "name": row.get::<String, _>("name"),
                    "description": row.get::<Option<String>, _>("description"),
                    "scope": row.get::<String, _>("policy_type"),
                    "priority": row.get::<i32, _>("priority"),
                    "enabled": row.get::<bool, _>("enabled"),
                    "selectors": row.get::<Value, _>("selectors"),
                    "conditions": row.get::<Option<Value>, _>("conditions"),
                    "config": row.get::<Option<Value>, _>("config"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            })
            .collect::<Vec<_>>();
    Ok(Json(json!({ "trustRules": trust_rules })))
}

/// GET /companies/:cid/tools/stdio-templates —— 已登记的 stdio 模板。
async fn tools_stdio_templates(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_gateway_permission(&state, &actor, company_id, PermissionKey::TOOLS_ADMIN).await?;
    let rows = sqlx::query(
        "SELECT id, company_id, template_key, name, description, status, command, args, env_keys, tools,
                created_by_agent_id, created_by_user_id, disabled_at, created_at, updated_at
         FROM tool_stdio_command_templates WHERE company_id = $1 ORDER BY name ASC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list stdio templates: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut templates = rows.iter().map(stdio_template_json).collect::<Vec<_>>();
    if let Some(tools) = crate::routes::tools::builtin_mcp_template_tools("paperclip.synthetic-todo-kv") {
        templates.push(json!({
            "id": Value::Null,
            "companyId": company_id,
            "templateId": "paperclip.synthetic-todo-kv",
            "templateKey": "paperclip.synthetic-todo-kv",
            "name": "Paperclip synthetic Todo / KV fixture",
            "title": "Safe read-only Todo / KV fixture",
            "description": "Deterministic local MCP fixture used for Tool Access smoke tests.",
            "status": "active",
            "source": "builtin",
            "command": Value::Null,
            "args": [],
            "envKeys": [],
            "tools": tools,
            "createdByAgentId": Value::Null,
            "createdByUserId": Value::Null,
            "disabledAt": Value::Null,
            "createdAt": Value::Null,
            "updatedAt": Value::Null,
        }));
    }
    Ok(Json(json!({
        "templates": templates
    })))
}

fn stdio_template_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "templateId": row.get::<String, _>("template_key"),
        "templateKey": row.get::<String, _>("template_key"),
        "name": row.get::<String, _>("name"),
        "title": Value::Null,
        "description": row.get::<Option<String>, _>("description"),
        "status": row.get::<String, _>("status"),
        "source": "admin",
        "command": row.get::<String, _>("command"),
        "args": row.get::<Value, _>("args"),
        "envKeys": row.get::<Value, _>("env_keys"),
        "tools": row.get::<Value, _>("tools"),
        "createdByAgentId": row.get::<Option<Uuid>, _>("created_by_agent_id"),
        "createdByUserId": row.get::<Option<String>, _>("created_by_user_id"),
        "disabledAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("disabled_at"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

// ---------- tool-applications ----------

/// PATCH /api/tool-applications/:id —— 更新应用定义。
#[derive(Debug, Deserialize)]
struct UpdateToolApplicationRequest {
    name: Option<String>,
    description: Option<String>,
    status: Option<String>,
    #[serde(rename = "pluginId")]
    plugin_id: Option<Uuid>,
    #[serde(rename = "ownerAgentId")]
    owner_agent_id: Option<Uuid>,
    #[serde(rename = "ownerUserId")]
    owner_user_id: Option<String>,
    metadata: Option<Value>,
}
async fn update_tool_application(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(application_id): Path<Uuid>,
    Json(request): Json<UpdateToolApplicationRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    assert_board(&actor)?;
    let company_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT company_id FROM tool_applications WHERE id = $1",
    )
    .bind(application_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load tool application: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if request
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty() || name.trim().chars().count() > 160)
        || request
            .status
            .as_deref()
            .is_some_and(|status| !validate_tool_application_status(status))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    validate_tool_application_references(
        &state,
        company_id,
        request.plugin_id,
        request.owner_agent_id,
    )
    .await?;
    let metadata = request
        .metadata
        .as_ref()
        .filter(|value| !value.is_null());
    let row = sqlx::query(
        "UPDATE tool_applications
            SET name = COALESCE($2, name),
                description = COALESCE($3, description),
                status = COALESCE($4, status),
                plugin_id = COALESCE($5, plugin_id),
                owner_agent_id = COALESCE($6, owner_agent_id),
                owner_user_id = COALESCE($7, owner_user_id),
                metadata = COALESCE($8, metadata),
                updated_at = NOW()
          WHERE id = $1 AND company_id = $9
         RETURNING id, company_id, application_key, name, description,
                   type AS application_type, status, plugin_id, owner_agent_id,
                   owner_user_id, metadata, archived_at, created_at, updated_at,
                   agent_id AS legacy_agent_id,
                   connection_id AS legacy_connection_id",
    )
    .bind(application_id)
    .bind(request.name.as_deref().map(str::trim))
    .bind(request.description.as_deref())
    .bind(request.status.as_deref())
    .bind(request.plugin_id)
    .bind(request.owner_agent_id)
    .bind(request.owner_user_id.as_deref())
    .bind(metadata)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            return StatusCode::CONFLICT;
        }
        tracing::error!("Failed to update tool application: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_application.updated",
        &actor,
        "tool_application",
        application_id,
        json!({
            "name": request.name,
            "status": request.status,
        }),
    )
    .await;
    Ok(Json(tool_application_json(&row)))
}

/// DELETE /api/tool-applications/:id
async fn delete_tool_application(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(application_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    assert_board(&actor)?;
    use sqlx::Row;
    let company_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT company_id FROM tool_applications WHERE id = $1",
    )
    .bind(application_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load tool application: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let company_id = company_id.ok_or(StatusCode::NOT_FOUND)?;
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;

    let mut transaction = state.pool.begin().await.map_err(|e| {
        tracing::error!("Failed to start tool application delete transaction: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let has_connections = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM tool_connections
              WHERE application_id = $1 AND company_id = $2
         )",
    )
    .bind(application_id)
    .bind(company_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to inspect tool application connections: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if has_connections {
        return Err(StatusCode::CONFLICT);
    }
    let row = sqlx::query(
        "DELETE FROM tool_applications
          WHERE id = $1 AND company_id = $2
         RETURNING id, company_id, application_key, name, description,
                   type AS application_type, status, plugin_id, owner_agent_id,
                   owner_user_id, metadata, archived_at, created_at, updated_at,
                   agent_id AS legacy_agent_id,
                   connection_id AS legacy_connection_id",
    )
        .bind(application_id)
        .bind(company_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool application: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    transaction.commit().await.map_err(|e| {
        tracing::error!("Failed to commit tool application delete: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_application.deleted",
        &actor,
        "tool_application",
        application_id,
        json!({
            "name": row.get::<String, _>("name"),
            "type": row.get::<String, _>("application_type"),
        }),
    )
    .await;
    Ok(Json(tool_application_json(&row)))
}

// ---------- tool-connections ----------

fn connection_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;
    let transport = row
        .get::<Option<String>, _>("transport")
        .or_else(|| row.get::<Option<String>, _>("tool_type"))
        .unwrap_or_else(|| "mcp_remote".to_string());
    let status = match row.get::<String, _>("status").as_str() {
        "draft" | "active" | "disabled" | "archived" => row.get::<String, _>("status"),
        _ => "draft".to_string(),
    };
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "applicationId": row.get::<Option<Uuid>, _>("application_id"),
        "name": row.get::<String, _>("name"),
        "uid": row.get::<String, _>("uid"),
        "connectionKind": row.get::<String, _>("connection_kind"),
        "ownership": row.get::<String, _>("ownership"),
        "transport": transport.clone(),
        "toolType": transport,
        "authKind": row.get::<String, _>("auth_kind"),
        "status": status,
        "enabled": row.get::<bool, _>("enabled"),
        "config": row.get::<Option<Value>, _>("config").unwrap_or_else(|| json!({})),
        "transportConfig": row.get::<Option<Value>, _>("transport_config").unwrap_or_else(|| json!({})),
        "credentialRefs": row.get::<Option<Value>, _>("credential_refs").unwrap_or_else(|| json!([])),
        "credentialSecretRefs": row.get::<Option<Value>, _>("credential_secret_refs").unwrap_or_else(|| json!([])),
        "healthStatus": row.get::<String, _>("health_status"),
        "healthMessage": row.get::<Option<String>, _>("health_message"),
        "healthCheckedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("health_checked_at"),
        "lastHealthAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_healthy_at"),
        "lastCatalogRefreshAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_catalog_refresh_at"),
        "lastError": row.get::<Option<String>, _>("last_error"),
        "createdByAgentId": row.get::<Option<Uuid>, _>("created_by_agent_id"),
        "createdByUserId": row.get::<Option<String>, _>("created_by_user_id"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

async fn get_connection_by_id(
    state: &AppState,
    company_id: Uuid,
    connection_id: Uuid,
) -> Result<Option<sqlx::postgres::PgRow>, StatusCode> {
    sqlx::query("SELECT * FROM tool_connections WHERE id = $1 AND company_id = $2")
        .bind(connection_id)
        .bind(company_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load tool connection: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /api/tool-connections/:id
async fn get_tool_connection(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    let row = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(connection_json(&row)))
}

/// Resolve the owning company of a tool connection from the resource row and
/// authorize against it — the `getAccessibleResource` contract
/// (`server/src/routes/authz.ts:182-195` in Paperclip).
///
/// These routes carry no company segment, so the actor's own company cannot
/// stand in for the resource's: the row is the only source of truth. A row
/// that does not exist and a row belonging to another company both return
/// `NotFound` so neither can be used to probe for ids.
async fn accessible_connection_company(
    state: &AppState,
    actor: &AuthorizationActor,
    connection_id: Uuid,
    mode: AccessMode,
) -> Result<Uuid, StatusCode> {
    accessible_resource_company(
        state,
        actor,
        "SELECT company_id FROM tool_connections WHERE id = $1",
        connection_id,
        mode,
    )
    .await
}

async fn accessible_profile_company(
    state: &AppState,
    actor: &AuthorizationActor,
    profile_id: Uuid,
    mode: AccessMode,
) -> Result<Uuid, StatusCode> {
    accessible_resource_company(
        state,
        actor,
        "SELECT company_id FROM tool_profiles WHERE id = $1",
        profile_id,
        mode,
    )
    .await
}

async fn accessible_profile_entry_company(
    state: &AppState,
    actor: &AuthorizationActor,
    entry_id: Uuid,
    mode: AccessMode,
) -> Result<Uuid, StatusCode> {
    accessible_resource_company(
        state,
        actor,
        "SELECT company_id FROM tool_profile_entries WHERE id = $1",
        entry_id,
        mode,
    )
    .await
}

async fn accessible_resource_company(
    state: &AppState,
    actor: &AuthorizationActor,
    query: &str,
    resource_id: Uuid,
    mode: AccessMode,
) -> Result<Uuid, StatusCode> {
    assert_board(actor)?;
    // A lookup failure is a genuine 500 and must stay distinct from the
    // not-found branch, so it is mapped before the `Option` is matched on.
    let company_id = sqlx::query_scalar::<_, Uuid>(query)
        .bind(resource_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %resource_id, "tool resource lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    match company_id {
        Some(company_id) if has_company_access(actor, company_id) => {
            require_company_access(actor, company_id, mode)?;
            Ok(company_id)
        }
        _ => Err(StatusCode::NOT_FOUND),
    }
}

/// PATCH /api/tool-connections/:id —— 更新配置。
#[derive(Debug, Deserialize)]
struct UpdateToolConnectionRequest {
    name: Option<String>,
    status: Option<String>,
    enabled: Option<bool>,
    config: Option<Value>,
    #[serde(rename = "transportConfig")]
    transport_config: Option<Value>,
    #[serde(rename = "credentialRefs")]
    credential_refs: Option<Value>,
    #[serde(rename = "credentialSecretRefs")]
    credential_secret_refs: Option<Value>,
}
async fn update_tool_connection(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<UpdateToolConnectionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Write).await?;
    if request
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty() || name.trim().chars().count() > 160)
        || request.status.as_deref().is_some_and(|status| {
            !matches!(status, "draft" | "active" | "disabled" | "archived")
        })
        || request
            .credential_refs
            .as_ref()
            .is_some_and(|refs| !refs.is_array())
        || request
            .credential_secret_refs
            .as_ref()
            .is_some_and(|refs| !refs.is_array())
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let row = sqlx::query(
        "UPDATE tool_connections
            SET name = COALESCE($3, name),
                status = COALESCE($4, status),
                enabled = COALESCE($5, enabled),
                config = COALESCE($6, config),
                transport_config = COALESCE($7, transport_config),
                credential_refs = COALESCE($8, credential_refs),
                credential_secret_refs = COALESCE($9, credential_secret_refs),
                updated_at = NOW()
          WHERE id = $1 AND company_id = $2
         RETURNING *",
    )
    .bind(connection_id)
    .bind(company_id)
    .bind(request.name.as_deref().map(str::trim))
    .bind(request.status.as_deref())
    .bind(request.enabled)
    .bind(request.config.as_ref())
    .bind(request.transport_config.as_ref())
    .bind(request.credential_refs.as_ref())
    .bind(request.credential_secret_refs.as_ref())
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update tool connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_connection.updated",
        &actor,
        "tool_connection",
        connection_id,
        json!({}),
    )
    .await;
    Ok(Json(connection_json(&row)))
}

/// DELETE /api/tool-connections/:id
async fn delete_tool_connection(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Write).await?;
    let row = sqlx::query(
        "UPDATE tool_connections
            SET status = 'archived', enabled = false, updated_at = NOW()
          WHERE id = $1 AND company_id = $2
         RETURNING *",
    )
        .bind(connection_id)
        .bind(company_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool connection: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_connection.deleted",
        &actor,
        "tool_connection",
        connection_id,
        json!({}),
    )
    .await;
    Ok(Json(connection_json(&row)))
}

/// GET /api/tool-connections/:id/grants
async fn list_connection_grants(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT * FROM tool_connection_grants
          WHERE company_id = $1 AND connection_id = $2
          ORDER BY created_at DESC",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list grants: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|r| {
                json!({
                    "id": r.get::<Uuid, _>("id"),
                    "agentId": r.get::<Uuid, _>("agent_id"),
                    "grantType": r.get::<String, _>("grant_type"),
                    "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect(),
    ))
}

/// DELETE /api/tool-connections/:id/grants/:grant_id
async fn delete_connection_grant(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((connection_id, grant_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    require_gateway_permission(
        &state,
        &actor,
        company_id,
        PermissionKey::TOOLS_MANAGE_CONNECTIONS,
    )
    .await?;
    sqlx::query(
        "DELETE FROM tool_connection_grants
          WHERE id = $1 AND connection_id = $2 AND company_id = $3",
    )
        .bind(grant_id)
        .bind(connection_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete grant: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/tool-connections/:id/usage —— 聚合 tool_invocations。
async fn connection_usage(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    let total: i64 =
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM tool_invocations
              WHERE company_id = $1 AND connection_id = $2",
        )
            .bind(company_id)
            .bind(connection_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);
    Ok(Json(json!({ "totalInvocations": total })))
}

async fn load_connection_installs(
    pool: &sqlx::PgPool,
    company_id: Uuid,
    connection_id: Uuid,
) -> Result<Vec<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, target_type, target_id, created_by_agent_id, created_by_user_id, created_at
           FROM tool_connection_installs
          WHERE company_id = $1 AND connection_id = $2
          ORDER BY created_at DESC",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, %connection_id, "Failed to list connection installs");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(rows
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"),
                "targetType": row.get::<String, _>("target_type"),
                "targetId": row.get::<String, _>("target_id"),
                "createdByAgentId": row.get::<Option<Uuid>, _>("created_by_agent_id"),
                "createdByUserId": row.get::<Option<String>, _>("created_by_user_id"),
                "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect())
}

/// GET /api/tool-connections/:id/installs —— 连接安装记录。
async fn connection_installs(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    let installs = load_connection_installs(&state.pool, company_id, connection_id).await?;
    Ok(Json(json!({
        "connectionId": connection_id,
        "installs": installs,
    })))
}

#[derive(Debug, Deserialize)]
struct ConnectionInstallRequest {
    #[serde(rename = "targetType", alias = "target_type")]
    target_type: String,
    #[serde(rename = "targetId", alias = "target_id")]
    target_id: String,
}

#[derive(Debug, Deserialize)]
struct PutConnectionInstallsRequest {
    #[serde(default)]
    installs: Vec<ConnectionInstallRequest>,
}

/// PUT /api/tool-connections/:id/installs —— 以声明式快照同步安装目标。
async fn put_connection_installs(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<PutConnectionInstallsRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    require_gateway_permission(
        &state,
        &actor,
        company_id,
        PermissionKey::TOOLS_MANAGE_CONNECTIONS,
    )
    .await?;

    if request.installs.len() > 1000 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut requested = Vec::with_capacity(request.installs.len());
    let mut requested_keys = HashSet::with_capacity(request.installs.len());
    for install in request.installs {
        let target_type = install.target_type.trim().to_ascii_lowercase();
        let target_id = install.target_id.trim().to_string();
        if target_id.is_empty() || !matches!(target_type.as_str(), "company" | "agent") {
            return Err(StatusCode::BAD_REQUEST);
        }
        let target_uuid = target_id.parse::<Uuid>().map_err(|_| StatusCode::BAD_REQUEST)?;
        if target_type == "company" && target_uuid != company_id {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        let key = (target_type.clone(), target_uuid.to_string());
        if requested_keys.insert(key) {
            requested.push((target_type, target_uuid));
        }
    }

    let created_by_agent_id = match &actor {
        AuthorizationActor::Agent { agent_id, .. } => Some(*agent_id),
        _ => None,
    };
    let created_by_user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => Some(user_id.to_string()),
        _ => None,
    };

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(%e, %company_id, %connection_id, "Failed to start connection install sync");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    for (target_type, target_id) in &requested {
        if target_type == "agent" {
            let valid_agent = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(
                     SELECT 1 FROM agents
                      WHERE id = $1 AND company_id = $2 AND status <> 'terminated'
                 )",
            )
            .bind(target_id)
            .bind(company_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(%e, %company_id, %target_id, "Failed to validate connection install agent");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            if !valid_agent {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
        }
    }

    use sqlx::Row;
    let existing = sqlx::query(
        "SELECT id, target_type, target_id
           FROM tool_connection_installs
          WHERE company_id = $1 AND connection_id = $2",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, %connection_id, "Failed to load connection installs for sync");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let existing_keys: HashSet<(String, String)> = existing
        .iter()
        .map(|row| {
            (
                row.get::<String, _>("target_type"),
                row.get::<String, _>("target_id"),
            )
        })
        .collect();

    for row in &existing {
        let key = (
            row.get::<String, _>("target_type"),
            row.get::<String, _>("target_id"),
        );
        if !requested_keys.contains(&key) {
            sqlx::query(
                "DELETE FROM tool_connection_installs
                  WHERE id = $1 AND company_id = $2 AND connection_id = $3",
            )
            .bind(row.get::<Uuid, _>("id"))
            .bind(company_id)
            .bind(connection_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(%e, %company_id, %connection_id, "Failed to remove connection install");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
    }

    for (target_type, target_id) in &requested {
        if !existing_keys.contains(&(target_type.clone(), target_id.to_string())) {
            sqlx::query(
                "INSERT INTO tool_connection_installs
                    (company_id, connection_id, target_type, target_id, created_by_agent_id, created_by_user_id)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (company_id, connection_id, target_type, target_id) DO NOTHING",
            )
            .bind(company_id)
            .bind(connection_id)
            .bind(target_type)
            .bind(target_id.to_string())
            .bind(created_by_agent_id)
            .bind(created_by_user_id.as_deref())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(%e, %company_id, %connection_id, "Failed to add connection install");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
    }

    tx.commit().await.map_err(|e| {
        tracing::error!(%e, %company_id, %connection_id, "Failed to commit connection install sync");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let installs = load_connection_installs(&state.pool, company_id, connection_id).await?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_connection.installs_synced",
        &actor,
        "tool_connection",
        connection_id,
        json!({
            "installs": installs.iter().map(|install| json!({
                "targetType": install.get("targetType").and_then(Value::as_str),
                "targetId": install.get("targetId").and_then(Value::as_str),
            })).collect::<Vec<_>>(),
        }),
    )
    .await;
    Ok(Json(json!({
        "connectionId": connection_id,
        "installs": installs,
    })))
}

/// GET /api/tool-connections/:id/catalog —— MCP 发现得到的工具目录。
async fn connection_catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, name, tool_name, title, description, input_schema, output_schema, risk_level, status, last_seen_at
           FROM tool_catalog_entries
          WHERE company_id = $1 AND connection_id = $2
          ORDER BY name ASC",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list connection catalog: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "name": row.get::<String, _>("name"),
                    "toolName": row.get::<String, _>("tool_name"),
                    "title": row.get::<Option<String>, _>("title"),
                    "description": row.get::<Option<String>, _>("description"),
                    "inputSchema": row.get::<Value, _>("input_schema"),
                    "outputSchema": row.get::<Option<Value>, _>("output_schema"),
                    "riskLevel": row.get::<String, _>("risk_level"),
                    "status": row.get::<String, _>("status"),
                    "lastSeenAt": row.get::<chrono::DateTime<chrono::Utc>, _>("last_seen_at"),
                })
            })
            .collect(),
    ))
}

/// GET /api/tool-connections/:id/activity —— 最近调用事件。
async fn connection_activity(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, tool_name, event_type, outcome, reason_code, created_at
           FROM tool_call_events
          WHERE company_id = $1 AND connection_id = $2
          ORDER BY created_at DESC LIMIT 50",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list connection activity: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let events: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "toolName": r.get::<Option<String>, _>("tool_name"),
                "eventType": r.get::<String, _>("event_type"),
                "status": r.get::<String, _>("outcome"),
                "reasonCode": r.get::<Option<String>, _>("reason_code"),
                "occurredAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();

    // Derive lifecycle events from the connection-scoped activity log, the
    // same way Paperclip's listConnectionLifecycleEvents does (no separate
    // table): map each log row through the canonical lifecycle mapper. The
    // live table is `activity_logs`, whose columns are renamed relative to
    // Paperclip's (`event_type`/`resource_type`/`resource_id`/`metadata`), so
    // alias them back to the names the row mapping below reads.
    let log_rows = sqlx::query(
        "SELECT id, event_type AS action, resource_type, resource_id, metadata AS details, \
                actor_type, agent_id, \
                CASE WHEN actor_type = 'user' THEN actor_id END AS user_id, created_at \
           FROM activity_logs
          WHERE company_id = $1
            AND resource_type = 'tool_connection'
            AND resource_id = $2
          ORDER BY created_at DESC LIMIT 50",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list connection lifecycle log: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let lifecycle_events: Vec<serde_json::Value> = log_rows
        .iter()
        .filter_map(|r| {
            let action: String = r.get("action");
            let details: serde_json::Value = r.get("details");
            let lifecycle_type =
                services::tool_access_contract::activity_log_action_to_lifecycle_type(
                    &action,
                    Some(&details),
                )?;
            let actor_type: Option<String> = r.try_get("actor_type").unwrap_or(None);
            let agent_id: Option<Uuid> = r.try_get("agent_id").unwrap_or(None);
            let user_id: Option<Uuid> = r.try_get("user_id").unwrap_or(None);
            let actor_display = user_id
                .map(|id| id.to_string())
                .or_else(|| agent_id.map(|id| id.to_string()));
            Some(json!({
                "id": r.get::<Uuid, _>("id"),
                "connectionId": connection_id,
                "type": lifecycle_type,
                "actorType": actor_type,
                "actorId": user_id,
                "agentId": agent_id,
                "actorDisplayName": actor_display,
                "details": details,
                "occurredAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            }))
        })
        .collect();

    Ok(Json(json!({
        "connectionId": connection_id,
        "events": events,
        "lifecycleEvents": lifecycle_events,
    })))
}

// ---------- tool-profiles ----------

async fn ensure_tool_profile_scope(
    state: &AppState,
    profile_id: Uuid,
    company_id: Uuid,
) -> Result<(), StatusCode> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM tool_profiles WHERE id = $1 AND company_id = $2)",
    )
    .bind(profile_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check tool profile scope: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    exists.then_some(()).ok_or(StatusCode::NOT_FOUND)
}

/// GET /api/tool-profiles/:id/new-tools —— profile 尚未审核的目录工具。
async fn profile_new_tools(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id =
        accessible_profile_company(&state, &actor, profile_id, AccessMode::Read).await?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT c.id, c.connection_id, c.name, c.tool_name, c.title, c.description, c.input_schema,
                c.output_schema, c.risk_level, c.status, c.first_seen_at, c.last_seen_at
           FROM tool_catalog_entries c
           JOIN tool_profiles p ON p.id = $1 AND p.company_id = c.company_id
          WHERE c.company_id = $2 AND c.reviewed_at IS NULL AND c.status = 'active'
          ORDER BY c.first_seen_at DESC",
    )
    .bind(profile_id)
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list profile new tools: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "connectionId": row.get::<Uuid, _>("connection_id"),
                    "name": row.get::<String, _>("name"),
                    "toolName": row.get::<String, _>("tool_name"),
                    "title": row.get::<Option<String>, _>("title"),
                    "description": row.get::<Option<String>, _>("description"),
                    "inputSchema": row.get::<Value, _>("input_schema"),
                    "outputSchema": row.get::<Option<Value>, _>("output_schema"),
                    "riskLevel": row.get::<String, _>("risk_level"),
                    "status": row.get::<String, _>("status"),
                    "firstSeenAt": row.get::<chrono::DateTime<chrono::Utc>, _>("first_seen_at"),
                    "lastSeenAt": row.get::<chrono::DateTime<chrono::Utc>, _>("last_seen_at"),
                })
            })
            .collect(),
    ))
}

/// DELETE /api/tool-profiles/:id
async fn delete_tool_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id =
        accessible_profile_company(&state, &actor, profile_id, AccessMode::Write).await?;
    let result = sqlx::query("DELETE FROM tool_profiles WHERE id = $1 AND company_id = $2")
        .bind(profile_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool profile: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/tool-profiles/:id
#[derive(Debug, Deserialize)]
struct UpdateToolProfileRequest {
    #[serde(rename = "profileKey", alias = "profile_key")]
    profile_key: Option<String>,
    name: Option<String>,
    description: Option<String>,
    status: Option<String>,
    #[serde(rename = "defaultAction", alias = "default_action")]
    default_action: Option<String>,
    metadata: Option<Value>,
    entries: Option<Vec<CreateProfileEntryRequest>>,
}
async fn update_tool_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
    Json(request): Json<UpdateToolProfileRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_profile_company(&state, &actor, profile_id, AccessMode::Write).await?;
    if request
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty() || name.len() > 160)
        || request.profile_key.as_deref().is_some_and(|key| {
            key.trim().is_empty() || key.len() > 160 || !is_safe_tool_application_key(key.trim())
        })
        || request
            .status
            .as_deref()
            .is_some_and(|status| !matches!(status, "draft" | "active" | "disabled" | "archived"))
        || request
            .default_action
            .as_deref()
            .is_some_and(|action| !matches!(action, "deny" | "allow"))
        || request.metadata.as_ref().is_some_and(|metadata| !metadata.is_object())
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let normalized_entries = if let Some(entries) = request.entries.as_ref() {
        if entries.len() > 250 {
            return Err(StatusCode::BAD_REQUEST);
        }
        let mut normalized_entries = Vec::with_capacity(entries.len());
        for entry in entries {
            let tool_name = entry.tool_name.clone().or_else(|| entry.tool.clone());
            let selector_type = entry.selector_type.as_deref().or_else(|| {
                if tool_name.is_some() {
                    Some("tool_name")
                } else {
                    None
                }
            });
            let effect = entry
                .effect
                .as_deref()
                .or_else(|| entry.enabled.map(|enabled| if enabled { "include" } else { "exclude" }))
                .or(Some("include"));
            let normalized = normalize_profile_entry_values(
                selector_type,
                effect,
                entry.application_id,
                entry.connection_id,
                entry.catalog_entry_id,
                tool_name,
                entry.risk_level.clone(),
                entry.conditions.clone(),
            )?;
            validate_profile_entry_references(&state, company_id, &normalized).await?;
            normalized_entries.push(normalized);
        }
        normalized_entries
    } else {
        Vec::new()
    };
    use sqlx::Row;
    let mut transaction = state.pool.begin().await.map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to begin tool profile update");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let row = sqlx::query(
        "UPDATE tool_profiles SET
             profile_key = COALESCE($3, profile_key),
             name = COALESCE($4, name),
             description = COALESCE($5, description),
             status = COALESCE($6, status),
             default_action = COALESCE($7, default_action),
             metadata = COALESCE($8, metadata),
             updated_at = NOW()
          WHERE id = $1 AND company_id = $2 RETURNING *",
    )
    .bind(profile_id)
    .bind(company_id)
    .bind(request.profile_key.as_deref().map(str::trim))
    .bind(request.name.as_deref())
    .bind(request.description.as_deref())
    .bind(request.status.as_deref())
    .bind(request.default_action.as_deref())
    .bind(request.metadata.as_ref())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update tool profile: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    if request.entries.is_some() {
        sqlx::query(
            "DELETE FROM tool_profile_entries
              WHERE company_id = $1 AND profile_id = $2",
        )
        .bind(company_id)
        .bind(profile_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            tracing::error!(%error, %profile_id, "Failed to replace tool profile entries");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        for entry in &normalized_entries {
            sqlx::query(
                "INSERT INTO tool_profile_entries
                    (id, company_id, profile_id, selector_type, effect,
                     application_id, connection_id, catalog_entry_id, tool_name,
                     risk_level, conditions)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            )
            .bind(Uuid::new_v4())
            .bind(company_id)
            .bind(profile_id)
            .bind(&entry.selector_type)
            .bind(&entry.effect)
            .bind(entry.application_id)
            .bind(entry.connection_id)
            .bind(entry.catalog_entry_id)
            .bind(&entry.tool_name)
            .bind(&entry.risk_level)
            .bind(&entry.conditions)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                tracing::error!(%error, %profile_id, "Failed to insert replacement tool profile entry");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
    }
    transaction.commit().await.map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to commit tool profile update");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let entry_rows = sqlx::query(
        "SELECT * FROM tool_profile_entries
          WHERE company_id = $1 AND profile_id = $2
          ORDER BY created_at ASC",
    )
    .bind(company_id)
    .bind(profile_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to load updated tool profile entries");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(json!({
        "id": profile_id,
        "companyId": company_id,
        "profileKey": row.get::<String, _>("profile_key"),
        "name": row.get::<String, _>("name"),
        "description": row.get::<Option<String>, _>("description"),
        "status": row.get::<String, _>("status"),
        "defaultAction": row.get::<String, _>("default_action"),
        "metadata": row.get::<Value, _>("metadata"),
        "newToolsReviewedAt": row.get::<Option<DateTime<Utc>>, _>("new_tools_reviewed_at"),
        "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
        "updatedAt": row.get::<DateTime<Utc>, _>("updated_at"),
        "entries": entry_rows.iter().map(profile_entry_json).collect::<Vec<_>>(),
    })))
}

// ---------- tool-gateway ----------

#[derive(Debug, Deserialize)]
struct GatewayAuditQuery {
    #[serde(rename = "companyId")]
    company_id: Option<String>,
    app: Option<String>,
    agent: Option<String>,
    outcome: Option<String>,
    window: Option<String>,
    search: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
}

/// `/tool-gateway/runtime-slots*` carry no resource row to read a company from,
/// so the caller supplies it explicitly — Paperclip
/// `server/src/routes/tool-gateway.ts:567-580`.
#[derive(Debug, Deserialize)]
struct GatewayRuntimeSlotsQuery {
    #[serde(rename = "companyId")]
    company_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct GatewayAuditCursor {
    #[serde(rename = "createdAt")]
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn gateway_audit_cursor_encode(cursor: &GatewayAuditCursor) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(cursor).expect("audit cursor is serializable"))
}

fn gateway_audit_cursor_decode(value: &str) -> Option<GatewayAuditCursor> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn gateway_audit_window(window: &str) -> Option<Duration> {
    match window {
        "1h" => Some(Duration::hours(1)),
        "24h" => Some(Duration::hours(24)),
        "7d" => Some(Duration::days(7)),
        "30d" => Some(Duration::days(30)),
        _ => None,
    }
}

#[derive(Debug)]
struct ActiveAgentRunContext {
    agent_id: Uuid,
    company_id: Uuid,
    run_id: Uuid,
    responsible_user_id: Option<String>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
}

async fn load_active_agent_run_context(
    state: &AppState,
    actor: &AuthorizationActor,
) -> Result<ActiveAgentRunContext, StatusCode> {
    let (agent_id, company_id, run_id) = match actor {
        AuthorizationActor::Agent {
            agent_id,
            company_id,
            run_id: Some(run_id),
            ..
        } => (*agent_id, *company_id, *run_id),
        _ => return Err(StatusCode::UNAUTHORIZED),
    };
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT responsible_user_id, context_snapshot
           FROM heartbeat_runs
          WHERE id = $1 AND company_id = $2 AND agent_id = $3 AND status = 'running'",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, %agent_id, %company_id, %run_id, "Failed to validate agent run context");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::FORBIDDEN);
    };
    let context_snapshot = row.get::<Option<Value>, _>("context_snapshot");
    let issue_id = context_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.get("issueId").or_else(|| snapshot.get("taskId")))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let project_id = context_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.get("projectId"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    Ok(ActiveAgentRunContext {
        agent_id,
        company_id,
        run_id,
        responsible_user_id: row.get("responsible_user_id"),
        issue_id,
        project_id,
    })
}

fn gateway_audit_like_pattern(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn gateway_audit_outcome(event_type: &str, decision: Option<&str>, outcome: &str) -> &'static str {
    if matches!(event_type, "call_completed" | "tool_gateway.call_completed")
        || matches!(decision, Some("allow" | "approved"))
        || matches!(outcome, "success" | "allowed")
    {
        return "allowed";
    }
    if matches!(event_type, "approval_requested" | "tool_gateway.approval_requested")
        || decision == Some("require_approval")
    {
        return "asked_first";
    }
    if matches!(event_type, "call_deferred" | "tool_gateway.call_deferred")
        || decision == Some("defer_runtime")
    {
        return "waiting";
    }
    if matches!(event_type, "call_failed" | "tool_gateway.call_failed")
        || matches!(outcome, "failure" | "failed")
    {
        return "failed";
    }
    if matches!(event_type, "call_denied" | "tool_gateway.call_denied")
        || matches!(decision, Some("deny" | "rate_limited"))
        || matches!(outcome, "denied" | "blocked")
    {
        return "blocked";
    }
    "unknown"
}

/// GET /api/tool-gateway/audit —— Paperclip-compatible filtered activity feed.
async fn gateway_audit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Query(query): Query<GatewayAuditQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    crate::routes::assert_board(&actor).map_err(|_| StatusCode::FORBIDDEN)?;
    let company_id = query
        .company_id
        .as_deref()
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let permission = AuthorizationService::decide(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission {
            key: PermissionKey::from_const(PermissionKey::TOOLS_VIEW_AUDIT),
        },
        Some(company_id),
    )
    .await;
    if !permission.allowed {
        return Err(StatusCode::FORBIDDEN);
    }

    let application_or_connection_id = query
        .app
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let agent_id = query
        .agent
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let window = query.window.as_deref().unwrap_or("24h");
    let window_duration = gateway_audit_window(window).ok_or(StatusCode::BAD_REQUEST)?;
    let search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(gateway_audit_like_pattern);
    let outcome = query
        .outcome
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    let cursor = match query.cursor.as_deref() {
        Some(value) => Some(gateway_audit_cursor_decode(value).ok_or(StatusCode::BAD_REQUEST)?),
        None => None,
    };
    let limit = query.limit.unwrap_or(100).clamp(1, 100);
    let window_start = Utc::now() - window_duration;

    let rows = sqlx::query(
        r#"
        SELECT e.id,
               e.company_id,
               e.action AS event_type,
               e.actor_type,
               e.actor_id,
               CASE WHEN e.details->>'agentId' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
                    THEN (e.details->>'agentId')::uuid END AS agent_id,
               CASE WHEN e.details->>'runId' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
                    THEN (e.details->>'runId')::uuid END AS run_id,
               NULL::uuid AS application_id,
               e.connection_id,
               CASE WHEN e.correlation_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
                    THEN e.correlation_id::uuid END AS invocation_id,
               NULL::uuid AS action_request_id,
               e.details->>'toolName' AS tool_name,
               e.details->>'decision' AS decision,
               e.outcome,
               e.reason_code,
               e.details->'argumentsSummary' AS arguments_summary,
               e.details->'requestSummary' AS request_summary,
               e.details->'resultSummary' AS result_summary,
               e.details->>'errorCode' AS error_code,
               e.details->>'errorMessage' AS error_message,
               e.details AS metadata,
               e.created_at,
               a.name AS agent_name,
               c.name AS connection_name,
               c.application_id AS connection_application_id
          FROM tool_access_audit_events e
          LEFT JOIN agents a
            ON a.id::text = e.details->>'agentId'
           AND a.company_id = e.company_id
          LEFT JOIN tool_connections c
            ON c.id = e.connection_id
           AND c.company_id = e.company_id
         WHERE e.company_id = $1
           AND e.created_at >= $2
           AND ($3::uuid IS NULL
                OR e.connection_id = $3
                OR c.application_id = $3)
           AND ($4::uuid IS NULL OR (
                CASE WHEN e.details->>'agentId' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
                     THEN (e.details->>'agentId')::uuid END
               ) = $4)
           AND (
             $5::text IS NULL
             OR e.action ILIKE $5 ESCAPE '\'
             OR COALESCE(e.details->>'toolName', '') ILIKE $5 ESCAPE '\'
             OR COALESCE(e.reason_code, '') ILIKE $5 ESCAPE '\'
             OR COALESCE(e.details->>'errorCode', '') ILIKE $5 ESCAPE '\'
             OR COALESCE(a.name, '') ILIKE $5 ESCAPE '\'
             OR COALESCE(c.name, '') ILIKE $5 ESCAPE '\'
             OR COALESCE(e.details::text, '') ILIKE $5 ESCAPE '\'
           )
           AND (
             $6::text IS NULL
             OR CASE $6
                  WHEN 'allowed' THEN e.action IN ('call_completed', 'tool_gateway.call_completed')
                                      OR e.details->>'decision' IN ('allow', 'approved')
                                      OR e.outcome IN ('success', 'allowed')
                  WHEN 'blocked' THEN e.action IN ('call_denied', 'tool_gateway.call_denied')
                                      OR e.details->>'decision' IN ('deny', 'rate_limited')
                                      OR e.outcome IN ('denied', 'blocked')
                  WHEN 'asked_first' THEN e.action IN ('approval_requested', 'tool_gateway.approval_requested')
                                      OR e.details->>'decision' = 'require_approval'
                  WHEN 'waiting' THEN e.action IN ('call_deferred', 'tool_gateway.call_deferred')
                                      OR e.details->>'decision' = 'defer_runtime'
                  WHEN 'failed' THEN e.action IN ('call_failed', 'tool_gateway.call_failed')
                                      OR e.outcome IN ('failure', 'failed')
                  ELSE TRUE
                END
           )
           AND (
             $7::timestamptz IS NULL
             OR e.created_at < $7
             OR (e.created_at = $7 AND e.id < $8)
           )
         ORDER BY e.created_at DESC, e.id DESC
         LIMIT $9
        "#,
    )
    .bind(company_id)
    .bind(window_start)
    .bind(application_or_connection_id)
    .bind(agent_id)
    .bind(search)
    .bind(outcome)
    .bind(cursor.as_ref().map(|value| value.created_at))
    .bind(cursor.as_ref().map(|value| value.id))
    .bind(limit + 1)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list gateway audit: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    use sqlx::Row;
    let has_more = rows.len() > limit as usize;
    let visible = rows.into_iter().take(limit as usize).collect::<Vec<_>>();
    let events = visible
        .iter()
        .map(|row| {
            let event_type: String = row.get("event_type");
            let actor_type: String = row.get("actor_type");
            let actor_id: Option<String> = row.get("actor_id");
            let agent_id: Option<Uuid> = row.get("agent_id");
            let run_id: Option<Uuid> = row.get("run_id");
            let application_id: Option<Uuid> = row.get("application_id");
            let connection_id: Option<Uuid> = row.get("connection_id");
            let invocation_id: Option<Uuid> = row.get("invocation_id");
            let action_request_id: Option<Uuid> = row.get("action_request_id");
            let tool_name: Option<String> = row.get("tool_name");
            let decision: Option<String> = row.get("decision");
            let outcome: String = row.get("outcome");
            let reason_code: Option<String> = row.get("reason_code");
            let error_code: Option<String> = row.get("error_code");
            let error_message: Option<String> = row.get("error_message");
            let metadata: Option<Value> = row.get("metadata");
            let mut details = match metadata {
                Some(Value::Object(object)) => Value::Object(object),
                Some(value) => json!({"metadata": value}),
                None => json!({}),
            };
            if let Some(value) = decision.as_deref() {
                details["decision"] = Value::String(value.to_string());
            }
            if let Some(value) = reason_code.as_deref() {
                details["reasonCode"] = Value::String(value.to_string());
            }
            if let Some(value) = tool_name.as_deref() {
                details["tool"] = Value::String(value.to_string());
            }
            if let Some(value) = application_id {
                details["applicationId"] = Value::String(value.to_string());
            }
            if let Some(value) = connection_id {
                details["connectionId"] = Value::String(value.to_string());
            }
            if let Some(value) = invocation_id {
                details["invocationId"] = Value::String(value.to_string());
            }
            if let Some(value) = action_request_id {
                details["actionRequestId"] = Value::String(value.to_string());
            }
            if let Some(value) = error_code.as_deref() {
                details["errorCode"] = Value::String(value.to_string());
            }
            if let Some(value) = error_message.as_deref() {
                details["error"] = Value::String(value.to_string());
            }
            if let Some(value) = row.get::<Option<Value>, _>("arguments_summary") {
                details["argumentsSummary"] = value;
            }
            if let Some(value) = row.get::<Option<Value>, _>("request_summary") {
                details["requestSummary"] = value;
            }
            if let Some(value) = row.get::<Option<Value>, _>("result_summary") {
                details["resultSummary"] = value;
            }
            let agent_name: Option<String> = row.get("agent_name");
            let connection_name: Option<String> = row.get("connection_name");
            let effective_application_id = application_id.or_else(|| {
                row.get::<Option<Uuid>, _>("connection_application_id")
            });
            let normalized_outcome = gateway_audit_outcome(
                &event_type,
                decision.as_deref(),
                &outcome,
            );
            json!({
                "id": row.get::<Uuid, _>("id"),
                "companyId": row.get::<Uuid, _>("company_id"),
                "action": event_type,
                "actorType": actor_type,
                "actorId": actor_id,
                "entityType": if invocation_id.is_some() { "tool_invocation" } else if action_request_id.is_some() { "tool_action_request" } else { "tool_gateway" },
                "entityId": invocation_id.or(action_request_id),
                "details": details,
                "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
                "agentId": agent_id,
                "runId": run_id,
                "applicationId": effective_application_id,
                "connectionId": connection_id,
                "agentDisplayName": agent_name,
                "appDisplayName": connection_name,
                "applicationDisplayName": Value::Null,
                "connectionDisplayName": connection_name,
                "toolDisplayName": tool_name,
                "normalizedOutcome": normalized_outcome,
            })
        })
        .collect::<Vec<_>>();
    let next_cursor = if has_more {
        visible.last().map(|row| {
            gateway_audit_cursor_encode(&GatewayAuditCursor {
                created_at: row.get("created_at"),
                id: row.get("id"),
            })
        })
    } else {
        None
    };
    Ok(Json(json!({
        "events": events,
        "nextCursor": next_cursor,
    })))
}

/// GET /api/tool-gateway/runtime-slots —— 网关运行时槽位。
async fn require_gateway_permission(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    permission_key: &'static str,
) -> Result<(), StatusCode> {
    crate::routes::assert_board(actor).map_err(|_| StatusCode::FORBIDDEN)?;
    require_company_access(actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let decision = AuthorizationService::decide(
        &state.pool,
        actor,
        &AuthorizationAction::Permission {
            key: PermissionKey::from_const(permission_key),
        },
        Some(company_id),
    )
    .await;
    decision
        .allowed
        .then_some(())
        .ok_or(StatusCode::FORBIDDEN)
}

async fn require_gateway_any_permission(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    permission_keys: &[&'static str],
) -> Result<(), StatusCode> {
    crate::routes::assert_board(actor).map_err(|_| StatusCode::FORBIDDEN)?;
    require_company_access(actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    for permission_key in permission_keys {
        let decision = AuthorizationService::decide(
            &state.pool,
            actor,
            &AuthorizationAction::Permission {
                key: PermissionKey::from_const(*permission_key),
            },
            Some(company_id),
        )
        .await;
        if decision.allowed {
            return Ok(());
        }
    }
    Err(StatusCode::FORBIDDEN)
}

async fn require_gateway_runtime_permission(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> Result<(), StatusCode> {
    require_gateway_permission(state, actor, company_id, PermissionKey::TOOLS_MANAGE_RUNTIME).await
}

async fn gateway_runtime_slots(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Query(query): Query<GatewayRuntimeSlotsQuery>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = query
        .company_id
        .as_deref()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, connection_id, slot_key, runtime_kind, status, health_status, process_id, last_error, last_used_at, updated_at
           FROM tool_runtime_slots WHERE company_id = $1 ORDER BY updated_at DESC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list gateway runtime slots: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "connectionId": row.get::<Option<Uuid>, _>("connection_id"),
                    "slotKey": row.get::<String, _>("slot_key"),
                    "runtimeKind": row.get::<String, _>("runtime_kind"),
                    "status": row.get::<String, _>("status"),
                    "healthStatus": row.get::<String, _>("health_status"),
                    "processId": row.get::<Option<i32>, _>("process_id"),
                    "lastError": row.get::<Option<String>, _>("last_error"),
                    "lastUsedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_used_at"),
                    "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            })
            .collect(),
    ))
}

pub fn tool_access_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/tools/gallery", get(tools_gallery))
        .route("/companies/:company_id/tools/examples", get(tools_examples))
        .route(
            "/companies/:company_id/tools/apps/attention",
            get(tools_attention_apps),
        )
        .route(
            "/companies/:company_id/tools/action-requests",
            get(tools_action_requests),
        )
        .route(
            "/companies/:company_id/tools/action-requests/:action_request_id/trust-rule",
            post(create_trust_rule_from_action_request),
        )
        .route(
            "/companies/:company_id/tools/applications",
            get(tools_applications).post(create_tool_application),
        )
        .route("/companies/:company_id/tools/profiles", get(tools_profiles))
        .route(
            "/companies/:company_id/tools/profiles/:profile_id/bind",
            post(bind_tool_profile),
        )
        .route(
            "/companies/:company_id/tools/profiles/:profile_id/unbind",
            post(unbind_tool_profile),
        )
        .route(
            "/companies/:company_id/tools/runtime-health",
            get(tools_runtime_health),
        )
        .route(
            "/companies/:company_id/tools/runtime-slots",
            get(tools_runtime_slots),
        )
        .route(
            "/companies/:company_id/tools/trust-rules",
            get(tools_trust_rules),
        )
        .route(
            "/companies/:company_id/tools/stdio-templates",
            get(tools_stdio_templates),
        )
        .route(
            "/tool-applications/:id",
            patch(update_tool_application).delete(delete_tool_application),
        )
        .route(
            "/tool-connections/:id",
            get(get_tool_connection)
                .patch(update_tool_connection)
                .delete(delete_tool_connection),
        )
        .route("/tool-connections/:id/grants", get(list_connection_grants))
        .route(
            "/tool-connections/:id/grants/:grant_id",
            delete(delete_connection_grant),
        )
        .route("/tool-connections/:id/usage", get(connection_usage))
        .route(
            "/tool-connections/:id/installs",
            get(connection_installs).put(put_connection_installs),
        )
        .route("/tool-connections/:id/catalog", get(connection_catalog))
        .route("/tool-connections/:id/activity", get(connection_activity))
        .route("/tool-profiles/:id/new-tools", get(profile_new_tools))
        .route(
            "/tool-profiles/:id",
            patch(update_tool_profile).delete(delete_tool_profile),
        )
        .route("/tool-gateway/audit", get(gateway_audit))
        .route("/tool-gateway/runtime-slots", get(gateway_runtime_slots))
        // ---- round 2 ----
        .route(
            "/tool-profile-entries/:id",
            patch(update_tool_profile_entry).delete(delete_tool_profile_entry),
        )
        .route(
            "/tool-connections/:id/test-agents",
            get(connection_test_agents),
        )
        .route(
            "/tool-connections/:id/test-calls/:call_id",
            get(connection_test_call),
        )
        .route(
            "/tool-connections/:id/catalog/refresh",
            post(refresh_connection_catalog),
        )
        .route(
            "/tool-connections/:id/grants/installations",
            post(install_connection_grants),
        )
        .route("/tools/oauth/callback", get(tools_oauth_callback))
        .route(
            "/companies/:company_id/tools/connections",
            post(create_company_tool_connection),
        )
        .route(
            "/companies/:company_id/tools/profiles",
            post(create_company_tool_profile),
        )
        .route(
            "/companies/:company_id/tools/examples/:example_id/install",
            post(install_tool_example),
        )
        .route(
            "/companies/:company_id/tools/examples/:example_id/smoke",
            post(smoke_tool_example),
        )
        .route(
            "/companies/:company_id/tools/apps/connect",
            post(connect_tool_app),
        )
        .route(
            "/companies/:company_id/tools/apps/:connection_id/finish",
            post(finish_tool_app),
        )
        .route(
            "/companies/:company_id/tools/mcp/import-json",
            post(import_mcp_json),
        )
        .route(
            "/companies/:company_id/tools/policies/:policy_id",
            patch(update_company_tool_policy),
        )
        .route(
            "/companies/:company_id/tools/policies/:policy_id/duplicate",
            post(duplicate_tool_policy),
        )
        .route(
            "/companies/:company_id/tools/policies/reorder",
            post(reorder_tool_policies),
        )
        .route(
            "/companies/:company_id/tools/policy/test",
            post(test_tool_policy),
        )
        .route(
            "/companies/:company_id/tools/stdio-templates",
            post(create_stdio_template),
        )
        .route(
            "/companies/:company_id/tools/stdio-templates/:template_id/disable",
            post(disable_stdio_template),
        )
        .route(
            "/companies/:company_id/tools/trust-rules/:rule_id/revoke",
            post(revoke_trust_rule),
        )
        .route(
            "/companies/:company_id/tools/runtime-slots/:slot_id/stop",
            post(stop_runtime_slot),
        )
        .route(
            "/companies/:company_id/tools/runtime-slots/:slot_id/restart",
            post(restart_runtime_slot),
        )
        .route(
            "/agents/me/connections/:connection_id/start-authorization",
            post(start_agent_connection_auth),
        )
        .route(
            "/agents/me/connections/:connection_id/token",
            post(agent_connection_token),
        )
        // ---- round 3 ----
        .route(
            "/tool-connections/:id/health-check",
            post(connection_health_check),
        )
        .route(
            "/tool-connections/:id/reconnect",
            post(reconnect_tool_connection),
        )
        .route(
            "/tool-connections/:id/test-calls",
            post(create_connection_test_call),
        )
        .route("/tool-profiles/:id/duplicate", post(duplicate_tool_profile))
        .route(
            "/tool-profiles/:id/entries",
            post(create_tool_profile_entry),
        )
        .route(
            "/tool-profiles/:id/new-tools/review",
            post(review_profile_new_tools),
        )
        .route(
            "/tools/oauth/:connection_id/start",
            post(tools_oauth_start),
        )
        .route(
            "/companies/:company_id/tools/connections/:connection_id/start-authorization",
            post(company_connection_oauth_start),
        )
        .route(
            "/tool-gateway/runtime-slots/:slot_id/stop",
            post(gateway_slot_stop),
        )
        .route(
            "/tool-gateway/runtime-slots/:slot_id/restart",
            post(gateway_slot_restart),
        )
}

// ================= Round 2 =================

#[derive(Debug, Deserialize, Clone)]
struct CreateProfileEntryRequest {
    #[serde(rename = "selectorType", alias = "selector_type")]
    selector_type: Option<String>,
    effect: Option<String>,
    #[serde(rename = "applicationId", alias = "application_id")]
    application_id: Option<Uuid>,
    #[serde(rename = "connectionId", alias = "connection_id")]
    connection_id: Option<Uuid>,
    #[serde(rename = "catalogEntryId", alias = "catalog_entry_id")]
    catalog_entry_id: Option<Uuid>,
    #[serde(rename = "toolName", alias = "tool_name")]
    tool_name: Option<String>,
    #[serde(rename = "riskLevel", alias = "risk_level")]
    risk_level: Option<String>,
    conditions: Option<Value>,
    // Legacy Parrot clients used { tool, enabled }. Keep accepting that wire
    // shape while exposing Paperclip's selector/effect fields below.
    tool: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateProfileEntryRequest {
    #[serde(rename = "selectorType", alias = "selector_type")]
    selector_type: Option<String>,
    effect: Option<String>,
    #[serde(rename = "applicationId", alias = "application_id")]
    application_id: Option<Option<Uuid>>,
    #[serde(rename = "connectionId", alias = "connection_id")]
    connection_id: Option<Option<Uuid>>,
    #[serde(rename = "catalogEntryId", alias = "catalog_entry_id")]
    catalog_entry_id: Option<Option<Uuid>>,
    #[serde(rename = "toolName", alias = "tool_name")]
    tool_name: Option<Option<String>>,
    #[serde(rename = "riskLevel", alias = "risk_level")]
    risk_level: Option<Option<String>>,
    conditions: Option<Option<Value>>,
    // Legacy update shape.
    tool: Option<String>,
    enabled: Option<bool>,
    #[serde(rename = "order")]
    _order: Option<i32>,
}

#[derive(Debug, Clone)]
struct NormalizedProfileEntry {
    selector_type: String,
    effect: String,
    application_id: Option<Uuid>,
    connection_id: Option<Uuid>,
    catalog_entry_id: Option<Uuid>,
    tool_name: Option<String>,
    risk_level: Option<String>,
    conditions: Option<Value>,
}

fn normalize_profile_entry_values(
    selector_type: Option<&str>,
    effect: Option<&str>,
    application_id: Option<Uuid>,
    connection_id: Option<Uuid>,
    catalog_entry_id: Option<Uuid>,
    tool_name: Option<String>,
    risk_level: Option<String>,
    conditions: Option<Value>,
) -> Result<NormalizedProfileEntry, StatusCode> {
    let selector_type = selector_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    if !matches!(
        selector_type,
        "application" | "connection" | "catalog_entry" | "tool_name" | "risk_level"
    ) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let effect = effect.unwrap_or("include").trim();
    if !matches!(effect, "include" | "exclude") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let tool_name = tool_name.map(|value| value.trim().to_string());
    if tool_name
        .as_deref()
        .is_some_and(|value| value.is_empty() || value.len() > 240)
        || risk_level
            .as_deref()
            .is_some_and(|value| !matches!(value, "low" | "medium" | "high" | "critical" | "read" | "write" | "destructive"))
        || conditions.as_ref().is_some_and(|value| !value.is_object())
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let selector_is_valid = match selector_type {
        "application" => application_id.is_some(),
        "connection" => connection_id.is_some(),
        "catalog_entry" => catalog_entry_id.is_some(),
        "tool_name" => tool_name.is_some(),
        "risk_level" => risk_level.is_some(),
        _ => false,
    };
    if !selector_is_valid {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(NormalizedProfileEntry {
        selector_type: selector_type.to_string(),
        effect: effect.to_string(),
        application_id,
        connection_id,
        catalog_entry_id,
        tool_name,
        risk_level,
        conditions,
    })
}

async fn validate_profile_entry_references(
    state: &AppState,
    company_id: Uuid,
    entry: &NormalizedProfileEntry,
) -> Result<(), StatusCode> {
    if let Some(application_id) = entry.application_id {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM tool_applications WHERE id = $1 AND company_id = $2)",
        )
        .bind(application_id)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %application_id, "Failed to validate tool profile application selector");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if !exists {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(connection_id) = entry.connection_id {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM tool_connections WHERE id = $1 AND company_id = $2)",
        )
        .bind(connection_id)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %connection_id, "Failed to validate tool profile connection selector");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if !exists {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(catalog_entry_id) = entry.catalog_entry_id {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM tool_catalog_entries WHERE id = $1 AND company_id = $2)",
        )
        .bind(catalog_entry_id)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %catalog_entry_id, "Failed to validate tool profile catalog selector");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if !exists {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(())
}

fn profile_entry_json(row: &sqlx::postgres::PgRow) -> Value {
    let effect = row.get::<String, _>("effect");
    let tool_name = row.get::<Option<String>, _>("tool_name");
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "profileId": row.get::<Uuid, _>("profile_id"),
        "selectorType": row.get::<String, _>("selector_type"),
        "effect": effect,
        "applicationId": row.get::<Option<Uuid>, _>("application_id"),
        "connectionId": row.get::<Option<Uuid>, _>("connection_id"),
        "catalogEntryId": row.get::<Option<Uuid>, _>("catalog_entry_id"),
        "toolName": tool_name,
        "riskLevel": row.get::<Option<String>, _>("risk_level"),
        "conditions": row.get::<Option<Value>, _>("conditions"),
        "tool": tool_name,
        "enabled": effect == "include",
        "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
        "updatedAt": row.get::<DateTime<Utc>, _>("updated_at"),
    })
}

/// PATCH /api/tool-profile-entries/:id
async fn update_tool_profile_entry(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(entry_id): Path<Uuid>,
    Json(request): Json<UpdateProfileEntryRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_profile_entry_company(&state, &actor, entry_id, AccessMode::Write).await?;
    if request.selector_type.is_none()
        && request.effect.is_none()
        && request.application_id.is_none()
        && request.connection_id.is_none()
        && request.catalog_entry_id.is_none()
        && request.tool_name.is_none()
        && request.risk_level.is_none()
        && request.conditions.is_none()
        && request.tool.is_none()
        && request.enabled.is_none()
        && request._order.is_none()
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let existing = sqlx::query(
        "SELECT e.*
           FROM tool_profile_entries AS e
           JOIN tool_profiles AS p ON p.id = e.profile_id
          WHERE e.id = $1 AND e.company_id = $2 AND p.company_id = $2",
    )
    .bind(entry_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %entry_id, "Failed to load tool profile entry");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(existing) = existing else {
        return Err(StatusCode::NOT_FOUND);
    };
    let existing_selector_type = existing.get::<String, _>("selector_type");
    let existing_effect = existing.get::<String, _>("effect");
    let selector_type = request
        .selector_type
        .clone()
        .unwrap_or(existing_selector_type);
    let existing_tool_name = existing.get::<Option<String>, _>("tool_name");
    let tool_name = match request.tool_name {
        Some(value) => value,
        None => request.tool.or(existing_tool_name),
    };
    let normalized = normalize_profile_entry_values(
        Some(selector_type.as_str()),
        request
            .effect
            .as_deref()
            .or_else(|| request.enabled.map(|enabled| if enabled { "include" } else { "exclude" }))
            .or(Some(existing_effect.as_str())),
        request
            .application_id
            .unwrap_or_else(|| existing.get::<Option<Uuid>, _>("application_id")),
        request
            .connection_id
            .unwrap_or_else(|| existing.get::<Option<Uuid>, _>("connection_id")),
        request
            .catalog_entry_id
            .unwrap_or_else(|| existing.get::<Option<Uuid>, _>("catalog_entry_id")),
        tool_name,
        request
            .risk_level
            .unwrap_or_else(|| existing.get::<Option<String>, _>("risk_level")),
        request
            .conditions
            .unwrap_or_else(|| existing.get::<Option<Value>, _>("conditions")),
    )?;
    validate_profile_entry_references(&state, company_id, &normalized).await?;
    let row = sqlx::query(
        "UPDATE tool_profile_entries
            SET selector_type = $2, effect = $3, application_id = $4,
                connection_id = $5, catalog_entry_id = $6, tool_name = $7,
                risk_level = $8, conditions = $9, updated_at = NOW()
          WHERE id = $1 AND company_id = $10
          RETURNING *",
    )
    .bind(entry_id)
    .bind(&normalized.selector_type)
    .bind(&normalized.effect)
    .bind(normalized.application_id)
    .bind(normalized.connection_id)
    .bind(normalized.catalog_entry_id)
    .bind(&normalized.tool_name)
    .bind(&normalized.risk_level)
    .bind(&normalized.conditions)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %entry_id, "Failed to update tool profile entry");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    sqlx::query("UPDATE tool_profiles SET updated_at = NOW() WHERE id = $1 AND company_id = $2")
        .bind(row.get::<Uuid, _>("profile_id"))
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %entry_id, "Failed to touch tool profile after entry update");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(profile_entry_json(&row)))
}

/// DELETE /api/tool-profile-entries/:id
async fn delete_tool_profile_entry(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(entry_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id =
        accessible_profile_entry_company(&state, &actor, entry_id, AccessMode::Write).await?;
    let result = sqlx::query(
        "DELETE FROM tool_profile_entries AS e
           USING tool_profiles AS p
          WHERE e.id = $1 AND e.profile_id = p.id AND p.company_id = $2",
    )
        .bind(entry_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool profile entry: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/tool-connections/:id/test-agents —— 当前公司可用于测试的 agent。
async fn connection_test_agents(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    require_gateway_any_permission(
        &state,
        &actor,
        company_id,
        &[PermissionKey::TOOLS_USE, PermissionKey::TOOLS_MANAGE_CONNECTIONS],
    )
    .await?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, name, role, status FROM agents WHERE company_id = $1 AND status <> 'terminated' ORDER BY name ASC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list connection test agents: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "name": row.get::<String, _>("name"),
                    "role": row.get::<String, _>("role"),
                    "status": row.get::<String, _>("status"),
                })
            })
            .collect(),
    ))
}

/// GET /api/tool-connections/:id/test-calls/:call_id —— 返回测试调用的持久化状态。
async fn connection_test_call(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((connection_id, call_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    require_gateway_any_permission(
        &state,
        &actor,
        company_id,
        &[PermissionKey::TOOLS_USE, PermissionKey::TOOLS_MANAGE_CONNECTIONS],
    )
    .await?;
    let row = sqlx::query(
        "SELECT ar.id AS action_request_id, ar.status AS action_status,
                ar.invocation_id, ar.issue_id, ar.expires_at,
                i.tool_name, i.status AS invocation_status, i.approval_state,
                i.policy_decision, i.result_summary, i.error_code, i.error_message,
                i.created_at, i.completed_at
           FROM tool_action_requests ar
           JOIN tool_invocations i ON i.id = ar.invocation_id
          WHERE ar.id = $1 AND ar.company_id = $2 AND i.connection_id = $3
            AND EXISTS (
                SELECT 1 FROM tool_call_events e
                 WHERE e.invocation_id = i.id
                   AND e.metadata ->> 'source' = 'test'
            )",
    )
    .bind(call_id)
    .bind(company_id)
    .bind(connection_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %connection_id, %call_id, "Failed to load MCP test-call status");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if let Some(row) = row {
        use sqlx::Row;
        return Ok(Json(json!({
            "id": row.get::<Uuid, _>("action_request_id"),
            "connectionId": connection_id,
            "tool": row.get::<String, _>("tool_name"),
            "status": row.get::<String, _>("action_status"),
            "invocationStatus": row.get::<String, _>("invocation_status"),
            "approvalState": row.get::<String, _>("approval_state"),
            "decision": row.get::<Option<String>, _>("policy_decision"),
            "invocationId": row.get::<Uuid, _>("invocation_id"),
            "issueId": row.get::<Option<Uuid>, _>("issue_id"),
            "expiresAt": row.get::<Option<DateTime<Utc>>, _>("expires_at"),
            "result": row.get::<Option<Value>, _>("result_summary"),
            "errorCode": row.get::<Option<String>, _>("error_code"),
            "error": row.get::<Option<String>, _>("error_message"),
            "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
            "completedAt": row.get::<Option<DateTime<Utc>>, _>("completed_at"),
        })));
    }
    // A successful test call has no action request. Look up its invocation via
    // the test marker so the polling endpoint still reports the final result.
    let invocation = sqlx::query(
        "SELECT i.id, i.tool_name, i.status, i.approval_state, i.policy_decision,
                i.result_summary, i.error_code, i.error_message, i.created_at, i.completed_at
           FROM tool_invocations i
          WHERE i.company_id = $1 AND i.connection_id = $2 AND i.id = $3
            AND EXISTS (
                SELECT 1 FROM tool_call_events e
                 WHERE e.invocation_id = i.id
                   AND e.metadata ->> 'source' = 'test'
            )",
    )
    .bind(company_id)
    .bind(connection_id)
    .bind(call_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(row) = invocation {
        use sqlx::Row;
        return Ok(Json(json!({
            "id": call_id,
            "connectionId": connection_id,
            "tool": row.get::<String, _>("tool_name"),
            "status": row.get::<String, _>("status"),
            "approvalState": row.get::<String, _>("approval_state"),
            "decision": row.get::<Option<String>, _>("policy_decision"),
            "invocationId": row.get::<Uuid, _>("id"),
            "result": row.get::<Option<Value>, _>("result_summary"),
            "errorCode": row.get::<Option<String>, _>("error_code"),
            "error": row.get::<Option<String>, _>("error_message"),
            "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
            "completedAt": row.get::<Option<DateTime<Utc>>, _>("completed_at"),
        })));
    }

    // Preserve Paperclip's resource scoping: an unknown call id is not allowed
    // to reveal whether another connection has a test invocation.
    let connection_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM tool_connections WHERE id = $1 AND company_id = $2
         )",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !connection_exists {
        return Err(StatusCode::NOT_FOUND);
    }
    Err(StatusCode::NOT_FOUND)
}

/// POST /api/tool-connections/:id/catalog/refresh —— 真实调用 MCP tools/list 并落库。
async fn refresh_connection_catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Write).await?;
    use sqlx::Row;
    let connection = sqlx::query(
        "SELECT transport, transport_config, config, credential_refs, credential_secret_refs
           FROM tool_connections WHERE id = $1 AND company_id = $2",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let transport: String = connection.get("transport");
    let config: Value = connection.get("transport_config");
    let connection_config: Value = connection.get("config");
    let credential_refs: Value = connection.get("credential_refs");
    let credential_secret_refs: Value = connection.get("credential_secret_refs");
    let tools = if transport == "mcp_remote" {
        let url = config
            .get("url")
            .or_else(|| config.get("endpoint"))
            .or_else(|| config.get("remoteUrl"))
            .and_then(Value::as_str)
            .ok_or(StatusCode::BAD_REQUEST)?;
        let headers = crate::routes::tools::resolve_mcp_connection_headers(
            &state,
            company_id,
            connection_id,
            &connection_config,
            &config,
            &credential_refs,
            &credential_secret_refs,
        )
        .await
        .map_err(|error| {
            tracing::warn!(connection_id = %connection_id, %error, "MCP credential resolution failed");
            StatusCode::UNPROCESSABLE_ENTITY
        })?;
        let result = crate::routes::tools::mcp_http_request_with_headers(
            url,
            "tools/list",
            json!({}),
            Some(&config),
            Some(&headers),
        )
        .await
        .map_err(|error| {
            tracing::warn!(connection_id = %connection_id, %error, "MCP catalog refresh failed");
            StatusCode::BAD_GATEWAY
        })?;
        result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    } else {
        // Paperclip never discovers local tools by executing an arbitrary
        // command from a connection row. An approved stdio template is the
        // catalog source of truth.
        let Some(template) = crate::routes::tools::resolve_mcp_stdio_template(
            &state,
            company_id,
            &connection_config,
            &config,
        )
        .await
        .map_err(|error| {
            tracing::warn!(connection_id = %connection_id, %error, "MCP stdio template lookup failed");
            StatusCode::UNPROCESSABLE_ENTITY
        })?
        else {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        };
        let environment = crate::routes::tools::mcp_stdio_environment(
            &connection_config,
            &config,
            &template,
        );
        let result = if template.builtin {
            crate::routes::tools::builtin_mcp_request(
                &template.template_id,
                "tools/list",
                &json!({}),
            )
        } else {
            crate::routes::tools::mcp_stdio_request_with_env(
                &template.command,
                &template.args,
                "tools/list",
                json!({}),
                Some(&environment),
            )
            .await
        }
        .map_err(|error| {
            tracing::warn!(connection_id = %connection_id, %error, "MCP stdio catalog refresh failed");
            StatusCode::BAD_GATEWAY
        })?;
        result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .ok_or(StatusCode::BAD_GATEWAY)?
    };
    // Paperclip refreshCatalog gates: config.quarantineNewEntries === true,
    // connection active, safeDefault/sourceTemplateKey config reads.
    let connection_row = sqlx::query(
        "SELECT status, config, transport_config FROM tool_connections WHERE id = $1 AND company_id = $2",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let connection_status: String = connection_row.get("status");
    let conn_config: Value = connection_row
        .try_get::<Value, _>("config")
        .unwrap_or_else(|_| json!({}));
    let quarantine_on_refresh = conn_config.get("quarantineNewEntries").and_then(Value::as_bool)
        == Some(true)
        && connection_status == "active";
    let safe_default = conn_config.get("safeDefault").and_then(Value::as_bool) == Some(true);
    let _source_template_key = conn_config
        .get("sourceTemplateKey")
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut refreshed = 0usize;
    let mut quarantined_count = 0usize;
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .ok_or(StatusCode::BAD_GATEWAY)?;
        let title = tool.get("title").and_then(Value::as_str);
        let description = tool.get("description").and_then(Value::as_str);
        let input_schema = tool.get("inputSchema").cloned().unwrap_or_else(|| json!({}));
        let annotations = tool.get("annotations").cloned().unwrap_or_else(|| json!({}));
        // Paperclip refreshCatalog: content-derived snapshot hashes + risk
        // classification (the previous implementation stored a random UUID as
        // version_hash, so change detection never matched).
        let risk_level = services::tool_access_contract::classify_risk(name, &annotations);
        let version_hash = services::tool_access_contract::descriptor_hash(
            name,
            title,
            description,
            &input_schema,
            &annotations,
            risk_level,
        );
        let schema_hash = services::tool_access_contract::schema_hash(&input_schema);

        // Existing row drives Paperclip's quarantine decision.
        let existing: Option<(Uuid, Option<String>, Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>)> =
            sqlx::query_as(
                "SELECT id, version_hash, schema_hash, status, quarantined_at \
                 FROM tool_catalog_entries WHERE connection_id = $1 AND name = $2",
            )
            .bind(connection_id)
            .bind(name)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!("x: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let (entry_id, prev_status, prev_quarantined_at) = match &existing {
            Some((id, _, _, status, quarantined_at)) => {
                (*id, status.as_deref(), *quarantined_at)
            }
            None => (Uuid::new_v4(), None, None),
        };
        let changed = matches!(
            &existing,
            Some((_, prev_version, prev_schema, _, _))
                if prev_version.as_deref() != Some(version_hash.as_str())
                    || prev_schema.as_deref() != Some(schema_hash.as_str())
        );
        let should_quarantine = quarantine_on_refresh
            && (existing.is_none() || changed)
            && prev_status != Some("disabled")
            && (!safe_default || risk_level != "read");
        if should_quarantine {
            quarantined_count += 1;
        }
        let next_status = if should_quarantine {
            "quarantined"
        } else if prev_status == Some("disabled") {
            "disabled"
        } else if prev_status == Some("quarantined") {
            "quarantined"
        } else {
            "active"
        };
        let quarantine_reason = if should_quarantine {
            Some("pending_review")
        } else {
            None
        };
        let quarantined_at = if should_quarantine {
            Some(chrono::Utc::now())
        } else {
            prev_quarantined_at
        };

        match existing {
            Some(_) => {
                sqlx::query(
                    "UPDATE tool_catalog_entries SET
                        title = $2, description = $3, input_schema = $4, output_schema = $5,
                        annotations = $6, risk_level = $7, is_read_only = $8, is_write = $9,
                        is_destructive = $10, status = $11, version_hash = $12, schema_hash = $13,
                        last_seen_at = NOW(),
                        quarantined_at = $14, quarantine_reason = $15, updated_at = NOW()
                     WHERE id = $1",
                )
                .bind(entry_id)
                .bind(title)
                .bind(description)
                .bind(input_schema)
                .bind(tool.get("outputSchema").cloned())
                .bind(annotations)
                .bind(risk_level)
                .bind(risk_level == "read")
                .bind(risk_level == "write")
                .bind(risk_level == "destructive")
                .bind(next_status)
                .bind(version_hash)
                .bind(schema_hash)
                .bind(quarantined_at)
                .bind(quarantine_reason)
                .execute(&state.pool)
                .await
                .map_err(|e| {
                    tracing::error!("x: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO tool_catalog_entries
                        (id, company_id, connection_id, name, tool_name, title, description,
                         input_schema, output_schema, annotations, risk_level, is_read_only,
                         is_write, is_destructive, status, version_hash, schema_hash,
                         first_seen_at, last_seen_at, quarantined_at, quarantine_reason, updated_at)
                     VALUES ($1,$2,$3,$4,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,NOW(),NOW(),$17,$18,NOW())",
                )
                .bind(entry_id)
                .bind(company_id)
                .bind(connection_id)
                .bind(name)
                .bind(title)
                .bind(description)
                .bind(input_schema)
                .bind(tool.get("outputSchema").cloned())
                .bind(annotations)
                .bind(risk_level)
                .bind(risk_level == "read")
                .bind(risk_level == "write")
                .bind(risk_level == "destructive")
                .bind(next_status)
                .bind(version_hash)
                .bind(schema_hash)
                .bind(quarantined_at)
                .bind(quarantine_reason)
                .execute(&state.pool)
                .await
                .map_err(|e| {
                    tracing::error!("x: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            }
        }
        refreshed += 1;
    }
    if quarantined_count > 0 {
        tracing::info!(
            connection_id = %connection_id,
            quarantined = quarantined_count,
            "catalog refresh quarantined new/changed entries"
        );
    }
    sqlx::query("UPDATE tool_connections SET health_status = 'healthy', health_message = NULL, health_checked_at = NOW(), last_catalog_refresh_at = NOW(), last_healthy_at = NOW(), last_error = NULL, status = 'active', updated_at = NOW() WHERE id = $1 AND company_id = $2")
        .bind(connection_id).bind(company_id).execute(&state.pool).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        json!({ "connectionId": connection_id, "refreshed": true, "toolCount": refreshed }),
    ))
}

/// POST /api/tool-connections/:id/grants/installations —— 按安装批量授权（基础：204 语义）。
async fn install_connection_grants(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    require_gateway_permission(
        &state,
        &actor,
        company_id,
        PermissionKey::TOOLS_MANAGE_CONNECTIONS,
    )
    .await?;
    sqlx::query(
        "INSERT INTO tool_connection_grants (company_id, connection_id, agent_id) \
         SELECT company_id, $1, id FROM agents WHERE company_id = $2 AND status <> 'terminated' \
         ON CONFLICT (connection_id, agent_id) DO NOTHING",
    )
    .bind(connection_id)
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to install connection grants: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct OAuthCallbackQuery {
    state: String,
    code: Option<String>,
    error: Option<String>,
    #[serde(rename = "error_description")]
    _error_description: Option<String>,
}

#[derive(Debug)]
struct OAuthTokenExchange {
    access_token: String,
    refresh_token: Option<String>,
    token_type: Option<String>,
    scope: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
}

const MAX_OAUTH_METADATA_BYTES: usize = 1_000_000;
const MAX_OAUTH_DCR_CLIENT_ID_LENGTH: usize = 4096;
const MAX_OAUTH_DCR_CLIENT_SECRET_LENGTH: usize = 16_384;

#[derive(Debug, Clone)]
struct OAuthProviderEndpoints {
    provider: String,
    authorization_url: String,
    token_url: String,
    registration_url: Option<String>,
    metadata_url: Option<String>,
    scopes: Vec<String>,
    code_challenge_methods_supported: Vec<String>,
    token_endpoint_auth_methods_supported: Vec<String>,
}

fn oauth_provider(row: &sqlx::postgres::PgRow) -> String {
    connection_config_string(row, &["provider"])
        .or_else(|| connection_config_string(row, &["sourceTemplateKey", "source_template_key"]))
        .unwrap_or_else(|| "mcp".to_string())
}

fn oauth_config_value(row: &sqlx::postgres::PgRow, keys: &[&str]) -> Option<String> {
    connection_config_string_in_sections(row, &["oauth"], keys)
}

fn oauth_config_scopes(row: &sqlx::postgres::PgRow) -> Vec<String> {
    connection_config_scope_strings(row, &["oauth"], &["scopes", "scope"])
}

fn oauth_metadata_url_candidates(row: &sqlx::postgres::PgRow) -> Vec<String> {
    let configured = oauth_config_value(row, &["metadataUrl", "metadata_url", "issuer"]);
    let remote_url = connection_config_string_in_sections(
        row,
        &[],
        &["url", "endpoint", "remoteUrl", "remote_url", "serverUrl", "server_url"],
    );
    let mut candidates = Vec::new();
    if let Some(url) = configured {
        candidates.push(url);
    }
    if let Some(remote_url) = remote_url {
        if let Ok(endpoint) = Url::parse(&remote_url) {
            if let Some(host) = endpoint.host_str() {
                let origin = format!(
                    "{}://{}{}",
                    endpoint.scheme(),
                    host,
                    endpoint
                        .port()
                        .map(|port| format!(":{port}"))
                        .unwrap_or_default()
                );
                let path = endpoint.path();
                let protected_path = if path == "/" || path.is_empty() {
                    "/.well-known/oauth-protected-resource".to_string()
                } else {
                    format!("/.well-known/oauth-protected-resource{path}")
                };
                candidates.push(format!("{origin}{protected_path}"));
                candidates.push(format!("{origin}/.well-known/oauth-protected-resource"));
                candidates.push(format!("{origin}/.well-known/oauth-authorization-server"));
                candidates.push(format!("{origin}/.well-known/openid-configuration"));
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

async fn fetch_oauth_metadata(url: &str) -> Result<Option<Value>, StatusCode> {
    let url = validate_oauth_endpoint(url)?;
    let client = reqwest::Client::builder()
        .timeout(StdDuration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let response = client
        .get(url)
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| {
            tracing::debug!(%error, "OAuth metadata endpoint was unavailable");
            StatusCode::BAD_GATEWAY
        })?;
    if !response.status().is_success() {
        return Ok(None);
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_OAUTH_METADATA_BYTES as u64)
    {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let bytes = response.bytes().await.map_err(|error| {
        tracing::debug!(%error, "OAuth metadata response could not be read");
        StatusCode::BAD_GATEWAY
    })?;
    if bytes.len() > MAX_OAUTH_METADATA_BYTES {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let metadata = serde_json::from_slice::<Value>(&bytes).map_err(|error| {
        tracing::debug!(%error, "OAuth metadata response was not JSON");
        StatusCode::BAD_GATEWAY
    })?;
    Ok(metadata.is_object().then_some(metadata))
}

fn metadata_string(metadata: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        metadata
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn metadata_strings(metadata: &Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| metadata.get(*key).map(config_scope_values))
        .unwrap_or_default()
}

async fn resolve_oauth_provider_endpoints(
    row: &sqlx::postgres::PgRow,
) -> Result<OAuthProviderEndpoints, StatusCode> {
    let provider = oauth_provider(row);
    let mut authorization_url = oauth_config_value(
        row,
        &["authorizationUrl", "authorization_url", "authorizeUrl", "authorize_url"],
    );
    let mut token_url = oauth_config_value(
        row,
        &["tokenUrl", "token_url", "tokenUri", "token_uri", "tokenEndpoint", "token_endpoint"],
    );
    let mut registration_url = oauth_config_value(
        row,
        &["registrationUrl", "registration_url", "registrationEndpoint", "registration_endpoint"],
    );
    let mut metadata_url = oauth_config_value(row, &["metadataUrl", "metadata_url"]);
    let mut scopes = oauth_config_scopes(row);
    let mut code_challenge_methods_supported = connection_config_scope_strings(
        row,
        &["oauth"],
        &["codeChallengeMethodsSupported", "code_challenge_methods_supported"],
    );
    let mut token_endpoint_auth_methods_supported = connection_config_scope_strings(
        row,
        &["oauth"],
        &["tokenEndpointAuthMethodsSupported", "token_endpoint_auth_methods_supported"],
    );

    if authorization_url.is_none() || token_url.is_none() {
        for candidate in oauth_metadata_url_candidates(row) {
            let Some(metadata) = fetch_oauth_metadata(&candidate).await? else {
                continue;
            };
            if authorization_url.is_none() {
                authorization_url = metadata_string(
                    &metadata,
                    &["authorization_endpoint", "authorizationEndpoint"],
                );
            }
            if token_url.is_none() {
                token_url = metadata_string(&metadata, &["token_endpoint", "tokenEndpoint"]);
            }
            if registration_url.is_none() {
                registration_url = metadata_string(
                    &metadata,
                    &["registration_endpoint", "registrationEndpoint"],
                );
            }
            if metadata_url.is_none() {
                metadata_url = Some(candidate.clone());
            }
            if scopes.is_empty() {
                scopes = metadata_strings(&metadata, &["scopes_supported", "scopesSupported"]);
            }
            if code_challenge_methods_supported.is_empty() {
                code_challenge_methods_supported = metadata_strings(
                    &metadata,
                    &[
                        "code_challenge_methods_supported",
                        "codeChallengeMethodsSupported",
                    ],
                );
            }
            if token_endpoint_auth_methods_supported.is_empty() {
                token_endpoint_auth_methods_supported = metadata_strings(
                    &metadata,
                    &[
                        "token_endpoint_auth_methods_supported",
                        "tokenEndpointAuthMethodsSupported",
                    ],
                );
            }
            if authorization_url.is_some() && token_url.is_some() && registration_url.is_some() {
                break;
            }
        }
    }
    let authorization_url = authorization_url.ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let token_url = token_url.ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    validate_oauth_endpoint(&authorization_url)?;
    validate_oauth_endpoint(&token_url)?;
    if let Some(registration_url) = registration_url.as_deref() {
        validate_oauth_endpoint(registration_url)?;
    }
    if let Some(metadata_url) = metadata_url.as_deref() {
        validate_oauth_endpoint(metadata_url)?;
    }
    Ok(OAuthProviderEndpoints {
        provider,
        authorization_url,
        token_url,
        registration_url,
        metadata_url,
        scopes,
        code_challenge_methods_supported,
        token_endpoint_auth_methods_supported,
    })
}

fn parse_dcr_string(
    record: &Value,
    key: &str,
    required: bool,
    max_length: usize,
) -> Result<Option<String>, StatusCode> {
    let Some(value) = record.get(key) else {
        return if required {
            Err(StatusCode::BAD_GATEWAY)
        } else {
            Ok(None)
        };
    };
    let value = value.as_str().ok_or(StatusCode::BAD_GATEWAY)?;
    if value.is_empty() || value.len() > max_length || (key == "client_id" && value.trim() != value) {
        return Err(StatusCode::BAD_GATEWAY);
    }
    Ok(Some(value.to_string()))
}

fn validate_dcr_array(record: &Value, key: &str, expected: &[&str]) -> Result<(), StatusCode> {
    let actual = record
        .get(key)
        .and_then(Value::as_array)
        .ok_or(StatusCode::BAD_GATEWAY)?;
    if actual.len() != expected.len()
        || actual.iter().any(|value| {
            value
                .as_str()
                .is_none_or(|value| value.is_empty() || value.len() > 2048)
        })
        || expected
            .iter()
            .any(|value| !actual.iter().any(|candidate| candidate.as_str() == Some(*value)))
    {
        return Err(StatusCode::BAD_GATEWAY);
    }
    Ok(())
}

async fn ensure_oauth_client_registration(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    connection_id: Uuid,
    row: &sqlx::postgres::PgRow,
    endpoints: &OAuthProviderEndpoints,
    redirect_uri: &str,
) -> Result<sqlx::postgres::PgRow, StatusCode> {
    if oauth_config_value(row, &["clientId", "client_id"])
        .or_else(|| mcp_oauth_client_id(&endpoints.provider))
        .is_none()
    {
        let registration_url = endpoints
            .registration_url
            .as_deref()
            .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
        if !endpoints.code_challenge_methods_supported.is_empty()
            && !endpoints
                .code_challenge_methods_supported
                .iter()
                .any(|method| method.eq_ignore_ascii_case("S256"))
        {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        if !endpoints.token_endpoint_auth_methods_supported.is_empty()
            && !endpoints
                .token_endpoint_auth_methods_supported
                .iter()
                .any(|method| method == "none")
        {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        let redirect_host = Url::parse(redirect_uri)
            .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?
            .host_str()
            .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?
            .to_string();
        let registration = json!({
            "client_name": format!("Parrot ({redirect_host})"),
            "redirect_uris": [redirect_uri],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        });
        let registration_url = validate_oauth_endpoint(registration_url)?;
        let client = reqwest::Client::builder()
            .timeout(StdDuration::from_secs(15))
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        let response = client
            .post(registration_url.clone())
            .header(header::ACCEPT, "application/json")
            .json(&registration)
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(%error, %connection_id, "OAuth dynamic client registration failed");
                StatusCode::BAD_GATEWAY
            })?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
        if bytes.len() > MAX_OAUTH_METADATA_BYTES {
            return Err(StatusCode::BAD_GATEWAY);
        }
        let record = serde_json::from_slice::<Value>(&bytes).map_err(|_| StatusCode::BAD_GATEWAY)?;
        if !status.is_success() {
            tracing::warn!(%connection_id, status = %status, "OAuth provider rejected dynamic client registration");
            return Err(StatusCode::BAD_GATEWAY);
        }
        let client_id = parse_dcr_string(
            &record,
            "client_id",
            true,
            MAX_OAUTH_DCR_CLIENT_ID_LENGTH,
        )?
        .ok_or(StatusCode::BAD_GATEWAY)?;
        let client_secret = parse_dcr_string(
            &record,
            "client_secret",
            false,
            MAX_OAUTH_DCR_CLIENT_SECRET_LENGTH,
        )?;
        validate_dcr_array(&record, "redirect_uris", &[redirect_uri])?;
        validate_dcr_array(&record, "grant_types", &["authorization_code", "refresh_token"])?;
        validate_dcr_array(&record, "response_types", &["code"])?;
        if record
            .get("token_endpoint_auth_method")
            .and_then(Value::as_str)
            != Some("none")
        {
            return Err(StatusCode::BAD_GATEWAY);
        }
        let client_id_issued_at = record.get("client_id_issued_at").and_then(Value::as_i64);
        let client_secret_expires_at = record
            .get("client_secret_expires_at")
            .and_then(Value::as_i64);
        if record.get("client_id_issued_at").is_some() && client_id_issued_at.is_none() {
            return Err(StatusCode::BAD_GATEWAY);
        }
        if record.get("client_secret_expires_at").is_some()
            && client_secret_expires_at.is_none()
        {
            return Err(StatusCode::BAD_GATEWAY);
        }
        let mut config = row
            .get::<Option<Value>, _>("config")
            .unwrap_or_else(|| json!({}));
        let mut oauth = config
            .get("oauth")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        oauth.insert("provider".to_string(), json!(&endpoints.provider));
        oauth.insert("authorizationUrl".to_string(), json!(&endpoints.authorization_url));
        oauth.insert("tokenUrl".to_string(), json!(&endpoints.token_url));
        oauth.insert("registrationUrl".to_string(), json!(registration_url.as_str()));
        oauth.insert("metadataUrl".to_string(), json!(endpoints.metadata_url));
        oauth.insert("scopes".to_string(), json!(&endpoints.scopes));
        oauth.insert(
            "codeChallengeMethodsSupported".to_string(),
            json!(&endpoints.code_challenge_methods_supported),
        );
        oauth.insert(
            "tokenEndpointAuthMethodsSupported".to_string(),
            json!(&endpoints.token_endpoint_auth_methods_supported),
        );
        oauth.insert("clientId".to_string(), json!(&client_id));
        oauth.insert("clientRegistrationSource".to_string(), json!("dcr"));
        oauth.insert("clientTokenEndpointAuthMethod".to_string(), json!("none"));
        oauth.insert("clientRedirectUri".to_string(), json!(redirect_uri));
        if let Some(value) = client_id_issued_at {
            oauth.insert("clientIdIssuedAt".to_string(), json!(value));
        }
        if let Some(value) = client_secret_expires_at {
            oauth.insert("clientSecretExpiresAt".to_string(), json!(value));
        }
        config["oauth"] = Value::Object(oauth);

        let mut transport_config = row
            .get::<Option<Value>, _>("transport_config")
            .unwrap_or_else(|| json!({}));
        if let Some(object) = transport_config.as_object_mut() {
            object.insert("oauth".to_string(), config["oauth"].clone());
        }
        let mut secret_refs = row
            .get::<Option<Value>, _>("credential_secret_refs")
            .unwrap_or_else(|| json!([]));
        if let Some(client_secret) = client_secret {
            let secret_id = create_mcp_credential_secret(
                &state.pool,
                company_id,
                "OAuth client secret",
                "oauth.client_secret",
                &client_secret,
                actor,
            )
            .await?;
            let mut refs = secret_refs.as_array().cloned().unwrap_or_default();
            refs.retain(|reference| {
                credential_ref_path(reference) != Some("oauth.client_secret")
            });
            refs.push(json!({
                "secretId": secret_id,
                "versionSelector": "latest",
                "configPath": "oauth.client_secret",
                "required": false,
                "label": "OAuth client secret",
            }));
            secret_refs = Value::Array(refs);
        }
        sqlx::query(
            "UPDATE tool_connections
                SET config = $3, transport_config = $4, credential_secret_refs = $5,
                    ownership = 'dcr', updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(connection_id)
        .bind(company_id)
        .bind(config)
        .bind(transport_config)
        .bind(secret_refs)
        .execute(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %connection_id, "Failed to persist OAuth dynamic client");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }
    get_connection_by_id(state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)
}

fn validate_oauth_endpoint(value: &str) -> Result<Url, StatusCode> {
    let url = Url::parse(value).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let host = url.host_str().ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let local_host = matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1");
    if url.username() != ""
        || url.password().is_some()
        || (url.scheme() != "https" && !(url.scheme() == "http" && local_host))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok(url)
}

fn validate_oauth_redirect_constraints(
    row: &sqlx::postgres::PgRow,
    provider: &str,
    redirect_uri: &str,
) -> Result<(), StatusCode> {
    let configured_constraint = oauth_config_value(
        row,
        &["redirectConstraints", "redirect_constraints"],
    );
    let requires_secure_origin = configured_constraint
        .as_deref()
        .is_some_and(|value| value == "https-or-loopback-http")
        || provider.eq_ignore_ascii_case("notion");
    if !requires_secure_origin {
        return Ok(());
    }
    let redirect = Url::parse(redirect_uri).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let host = redirect
        .host_str()
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    let is_loopback = host == "localhost"
        || host.ends_with(".localhost")
        || host == "::1"
        || host == "127.0.0.1"
        || host.starts_with("127.");
    if redirect.scheme() == "https" || (redirect.scheme() == "http" && is_loopback) {
        Ok(())
    } else {
        Err(StatusCode::UNPROCESSABLE_ENTITY)
    }
}

fn oauth_callback_actor_matches(
    actor: &AuthorizationActor,
    state_row: &sqlx::postgres::PgRow,
) -> Result<Uuid, StatusCode> {
    use sqlx::Row;
    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    let user_id_string = user_id.to_string();
    let subject_user_id = state_row.get::<Option<String>, _>("subject_user_id");
    if let Some(subject_user_id) = subject_user_id {
        if subject_user_id != user_id_string {
            return Err(StatusCode::FORBIDDEN);
        }
    } else if state_row.get::<Option<String>, _>("created_by_actor_type").as_deref()
        != Some("user")
        || state_row.get::<Option<String>, _>("created_by_actor_id").as_deref()
            != Some(user_id_string.as_str())
        || state_row
            .get::<Option<String>, _>("created_by_session_id")
            .is_some()
    {
        // The current actor model does not carry a session id. Fail closed for
        // a state that requires an unavailable session binding; Agent-started
        // states use subject_user_id and remain fully verifiable here.
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(user_id)
}

async fn consume_oauth_state(pool: &sqlx::PgPool, state_token: &str) -> Result<(), StatusCode> {
    let mut transaction = pool.begin().await.map_err(|e| {
        tracing::error!(%e, "Failed to start OAuth state consumption transaction");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let consumed = sqlx::query(
        "DELETE FROM tool_oauth_states
          WHERE state = $1 AND expires_at > NOW()
       RETURNING state",
    )
    .bind(state_token)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, "Failed to consume OAuth state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if consumed.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }
    transaction.commit().await.map_err(|e| {
        tracing::error!(%e, "Failed to commit OAuth state consumption");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(())
}

async fn resolve_oauth_client_secret(
    pool: &sqlx::PgPool,
    company_id: Uuid,
    row: &sqlx::postgres::PgRow,
) -> Result<Option<String>, StatusCode> {
    use sqlx::Row;
    let refs = row
        .get::<Option<Value>, _>("credential_secret_refs")
        .unwrap_or_else(|| json!([]));
    let Some(reference) = refs.as_array().and_then(|values| {
        values.iter().find(|value| {
            let Some(path) = value
                .get("configPath")
                .or_else(|| value.get("config_path"))
                .or_else(|| value.get("path"))
                .and_then(Value::as_str)
            else {
                return false;
            };
            matches!(
                path.to_ascii_lowercase().as_str(),
                "oauth.clientsecret"
                    | "oauth.client_secret"
                    | "clientsecret"
                    | "client_secret"
            )
        })
    }) else {
        return Ok(None);
    };
    let secret_id = reference
        .get("secretId")
        .or_else(|| reference.get("secret_id"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let version_selector = reference
        .get("versionSelector")
        .or_else(|| reference.get("version_selector"))
        .and_then(Value::as_str)
        .unwrap_or("latest")
        .trim();
    let material: Option<Value> = if version_selector.eq_ignore_ascii_case("latest") {
        sqlx::query_scalar(
            "SELECT v.material
               FROM company_secret_versions v
               JOIN company_secrets s ON s.id = v.secret_id
              WHERE s.id = $1 AND s.company_id = $2
                AND s.status = 'active' AND s.deleted_at IS NULL
                AND v.status = 'current' AND v.revoked_at IS NULL
              ORDER BY v.version DESC
              LIMIT 1",
        )
        .bind(secret_id)
        .bind(company_id)
        .fetch_optional(pool)
        .await
    } else {
        let version = version_selector
            .parse::<i32>()
            .ok()
            .filter(|version| *version > 0)
            .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
        sqlx::query_scalar(
            "SELECT v.material
               FROM company_secret_versions v
               JOIN company_secrets s ON s.id = v.secret_id
              WHERE s.id = $1 AND s.company_id = $2
                AND s.status = 'active' AND s.deleted_at IS NULL
                AND v.version = $3 AND v.revoked_at IS NULL
              LIMIT 1",
        )
        .bind(secret_id)
        .bind(company_id)
        .bind(version)
        .fetch_optional(pool)
        .await
    }
    .map_err(|e| {
        tracing::error!(%e, %secret_id, "Failed to load OAuth client secret");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let material = material.ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    decrypt_secret_material(&material)
        .map(Some)
        .map_err(|e| {
            tracing::error!(%e, %secret_id, "Failed to decrypt OAuth client secret");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn exchange_oauth_code(
    token_uri: &str,
    client_id: &str,
    client_secret: Option<&str>,
    redirect_uri: &str,
    code: &str,
    code_verifier: &str,
    requested_scope: &[String],
) -> Result<OAuthTokenExchange, StatusCode> {
    let token_uri = validate_oauth_endpoint(token_uri)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let mut form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("code_verifier", code_verifier.to_string()),
        ("client_id", client_id.to_string()),
    ];
    if let Some(client_secret) = client_secret {
        form.push(("client_secret", client_secret.to_string()));
    }
    let response = client
        .post(token_uri)
        .form(&form)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(%e, "OAuth token endpoint request failed");
            StatusCode::BAD_GATEWAY
        })?;
    let status = response.status();
    let payload = response.json::<Value>().await.map_err(|e| {
        tracing::warn!(%e, "OAuth token endpoint returned invalid JSON");
        StatusCode::BAD_GATEWAY
    })?;
    if !status.is_success() {
        tracing::warn!(%status, "OAuth token endpoint rejected authorization code");
        return Err(StatusCode::BAD_GATEWAY);
    }
    let access_token = payload
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(StatusCode::BAD_GATEWAY)?
        .to_string();
    let refresh_token = payload
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let token_type = payload
        .get("token_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let scope = payload
        .get("scope")
        .map(config_scope_values)
        .filter(|scope| !scope.is_empty())
        .unwrap_or_else(|| requested_scope.to_vec());
    let expires_at = payload
        .get("expires_in")
        .and_then(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(value) => value.trim().parse::<i64>().ok(),
            _ => None,
        })
        .filter(|seconds| (1..=31_536_000).contains(seconds))
        .map(|seconds| Utc::now() + Duration::seconds(seconds));
    Ok(OAuthTokenExchange {
        access_token,
        refresh_token,
        token_type,
        scope,
        expires_at,
    })
}

async fn upsert_oauth_secret(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    company_id: Uuid,
    key: &str,
    name: &str,
    value: &str,
    created_by_user_id: Uuid,
) -> Result<Uuid, StatusCode> {
    use sqlx::Row;
    let (material, digest) = encrypt_secret_material(value).map_err(|e| {
        tracing::error!(%e, "Failed to encrypt OAuth credential");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let existing = sqlx::query(
        "SELECT id, latest_version
           FROM company_secrets
          WHERE company_id = $1 AND scope = 'company' AND key = $2
            AND deleted_at IS NULL
          FOR UPDATE",
    )
    .bind(company_id)
    .bind(key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, "Failed to lock OAuth secret");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if let Some(existing) = existing {
        let secret_id: Uuid = existing.get("id");
        let next_version = existing.get::<i32, _>("latest_version").max(1) + 1;
        sqlx::query(
            "UPDATE company_secret_versions
                SET status = 'superseded', revoked_at = NOW()
              WHERE secret_id = $1 AND status = 'current'",
        )
        .bind(secret_id)
        .execute(&mut **transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %secret_id, "Failed to retire OAuth secret version");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sqlx::query(
            "INSERT INTO company_secret_versions
                (secret_id, version, material, value_sha256, fingerprint_sha256, status)
             VALUES ($1, $2, $3, $4, $4, 'current')",
        )
        .bind(secret_id)
        .bind(next_version)
        .bind(material)
        .bind(&digest)
        .execute(&mut **transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %secret_id, "Failed to store OAuth secret version");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sqlx::query(
            "UPDATE company_secrets
                SET latest_version = $2, last_rotated_at = NOW(), updated_at = NOW(),
                    status = 'active'
              WHERE id = $1",
        )
        .bind(secret_id)
        .bind(next_version)
        .execute(&mut **transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %secret_id, "Failed to update OAuth secret metadata");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        return Ok(secret_id);
    }

    let secret_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO company_secrets
            (id, company_id, scope, key, name, provider, status, managed_mode,
             description, created_by_user_id)
         VALUES ($1, $2, 'company', $3, $4, 'local_encrypted', 'active',
                 'paperclip_managed', $5, $6)",
    )
    .bind(secret_id)
    .bind(company_id)
    .bind(key)
    .bind(name)
    .bind("OAuth credential managed by Tool Gateway")
    .bind(created_by_user_id.to_string())
    .execute(&mut **transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, "Failed to create OAuth secret");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "INSERT INTO company_secret_versions
            (secret_id, version, material, value_sha256, fingerprint_sha256, status)
         VALUES ($1, 1, $2, $3, $3, 'current')",
    )
    .bind(secret_id)
    .bind(material)
    .bind(&digest)
    .execute(&mut **transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, %secret_id, "Failed to store initial OAuth secret version");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(secret_id)
}

/// Persist a gallery API key in the same encrypted secret store used by the
/// OAuth flow.  Tool connections only retain references to this row; the
/// plaintext never enters `tool_connections.config` or an API response.
async fn create_mcp_credential_secret(
    pool: &sqlx::PgPool,
    company_id: Uuid,
    label: &str,
    config_path: &str,
    value: &str,
    actor: &AuthorizationActor,
) -> Result<Uuid, StatusCode> {
    let (material, digest) = encrypt_secret_material(value).map_err(|error| {
        tracing::error!(%error, "Failed to encrypt MCP credential");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let secret_id = Uuid::new_v4();
    let key = format!(
        "tool_mcp_credential_{}_{}",
        sha256_hex(config_path),
        secret_id.simple()
    );
    let actor_id = actor.principal_id();
    let created_by_agent_id = actor.is_agent().then_some(actor_id).flatten();
    let created_by_user_id = actor.is_board().then(|| actor_id.map(|id| id.to_string())).flatten();
    sqlx::query(
        "INSERT INTO company_secrets
            (id, company_id, scope, key, name, provider, status, managed_mode,
             description, created_by_agent_id, created_by_user_id)
         VALUES ($1, $2, 'company', $3, $4, 'local_encrypted', 'active',
                 'paperclip_managed', $5, $6, $7)",
    )
    .bind(secret_id)
    .bind(company_id)
    .bind(&key)
    .bind(format!("MCP credential: {label}"))
    .bind(format!("Encrypted MCP connection credential ({config_path})."))
    .bind(created_by_agent_id)
    .bind(created_by_user_id)
    .execute(pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to create MCP credential secret");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "INSERT INTO company_secret_versions
            (secret_id, version, material, value_sha256, fingerprint_sha256, status)
         VALUES ($1, 1, $2, $3, $3, 'current')",
    )
    .bind(secret_id)
    .bind(material)
    .bind(digest)
    .execute(pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %secret_id, "Failed to store MCP credential secret version");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(secret_id)
}

#[derive(Clone, Copy)]
struct McpCredentialFieldSpec {
    label: &'static str,
    config_path: &'static str,
    required: bool,
    header_name: Option<&'static str>,
    prefix: Option<&'static str>,
}

fn mcp_gallery_credential_fields(gallery_key: Option<&str>) -> Vec<McpCredentialFieldSpec> {
    match gallery_key {
        Some("zapier") => vec![McpCredentialFieldSpec {
            label: "Zapier MCP token",
            config_path: "credentials.authorization",
            required: true,
            header_name: Some("Authorization"),
            prefix: Some("Bearer "),
        }],
        Some("github") => vec![McpCredentialFieldSpec {
            label: "GitHub token",
            config_path: "credentials.authorization",
            required: true,
            header_name: Some("Authorization"),
            prefix: Some("Bearer "),
        }],
        Some("custom-mcp-http") | None => vec![McpCredentialFieldSpec {
            label: "App key",
            config_path: "credentials.authorization",
            required: false,
            header_name: Some("Authorization"),
            prefix: Some("Bearer "),
        }],
        _ => Vec::new(),
    }
}

fn mcp_oauth_env_value(provider: &str, suffix: &str) -> Option<String> {
    let normalized: String = provider
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    [
        format!("PARROT_TOOL_OAUTH_{normalized}_{suffix}"),
        format!("PARROT_TOOL_OAUTH_{suffix}"),
    ]
    .into_iter()
    .find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn mcp_oauth_client_id(provider: &str) -> Option<String> {
    mcp_oauth_env_value(provider, "CLIENT_ID")
}

fn valid_mcp_credential_header_name(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 160
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~')
        })
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "accept" | "content-type" | "content-length" | "host" | "connection"
        )
}

fn oauth_secret_key(connection_id: Uuid, kind: &str, subject_user_id: Option<&str>) -> String {
    let subject_suffix = subject_user_id
        .map(|subject| format!("_{}", &sha256_hex(subject)[..16]))
        .unwrap_or_default();
    format!("tool_connection_{connection_id}_oauth_{kind}{subject_suffix}")
}

fn oauth_secret_ref(secret_id: Uuid, config_path: &str, label: &str) -> Value {
    json!({
        "secretId": secret_id,
        "versionSelector": "latest",
        "configPath": config_path,
        "required": config_path.ends_with("access_token"),
        "label": label,
    })
}

fn credential_ref_path(value: &Value) -> Option<&str> {
    value
        .get("configPath")
        .or_else(|| value.get("config_path"))
        .or_else(|| value.get("path"))
        .and_then(Value::as_str)
}

fn merge_oauth_secret_refs(existing: Value, access_ref: Value, refresh_ref: Option<Value>) -> Value {
    let mut refs = existing.as_array().cloned().unwrap_or_default();
    refs.retain(|reference| {
        !credential_ref_path(reference).is_some_and(|path| {
            matches!(
                path.to_ascii_lowercase().as_str(),
                "oauth.access_token"
                    | "oauth.access-token"
                    | "oauth.refreshtoken"
                    | "oauth.refresh_token"
                    | "oauth.refresh-token"
            )
        })
    });
    refs.push(access_ref);
    if let Some(refresh_ref) = refresh_ref {
        refs.push(refresh_ref);
    }
    Value::Array(refs)
}

fn oauth_connection_config_with_metadata(
    existing: Value,
    token: &OAuthTokenExchange,
) -> Value {
    let mut config = existing.as_object().cloned().unwrap_or_default();
    let mut oauth = config
        .get("oauth")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    oauth.insert("connectedAt".to_string(), json!(Utc::now()));
    if let Some(token_type) = token.token_type.as_deref() {
        oauth.insert("tokenType".to_string(), json!(token_type));
    }
    if !token.scope.is_empty() {
        oauth.insert("scope".to_string(), json!(token.scope.join(" ")));
    }
    if let Some(expires_at) = token.expires_at {
        oauth.insert("expiresAt".to_string(), json!(expires_at));
    }
    config.insert("oauth".to_string(), Value::Object(oauth));
    Value::Object(config)
}

async fn persist_oauth_connection(
    state: &AppState,
    actor: &AuthorizationActor,
    connection_id: Uuid,
    company_id: Uuid,
    subject_user_id: Option<&str>,
    token: &OAuthTokenExchange,
) -> Result<Uuid, StatusCode> {
    use sqlx::Row;
    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    let mut transaction = state.pool.begin().await.map_err(|e| {
        tracing::error!(%e, %connection_id, "Failed to start OAuth credential transaction");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let connection = sqlx::query(
        "SELECT credential_secret_refs, config
           FROM tool_connections
          WHERE id = $1 AND company_id = $2
          FOR UPDATE",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, %connection_id, "Failed to lock OAuth connection");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    let subject_suffix = subject_user_id
        .map(|subject| format!(" for user {}", &sha256_hex(subject)[..16]))
        .unwrap_or_default();
    let access_id = upsert_oauth_secret(
        &mut transaction,
        company_id,
        &oauth_secret_key(connection_id, "access_token", subject_user_id),
        &format!("OAuth access token{subject_suffix}"),
        &token.access_token,
        user_id,
    )
    .await?;
    let refresh_id = if let Some(refresh_token) = token.refresh_token.as_deref() {
        Some(
            upsert_oauth_secret(
                &mut transaction,
                company_id,
                &oauth_secret_key(connection_id, "refresh_token", subject_user_id),
                &format!("OAuth refresh token{subject_suffix}"),
                refresh_token,
                user_id,
            )
            .await?,
        )
    } else {
        None
    };
    let access_ref = oauth_secret_ref(access_id, "oauth.access_token", "OAuth access token");
    let refresh_ref = refresh_id.map(|id| oauth_secret_ref(id, "oauth.refresh_token", "OAuth refresh token"));
    let secret_refs = merge_oauth_secret_refs(
        connection
            .get::<Option<Value>, _>("credential_secret_refs")
            .unwrap_or_else(|| json!([])),
        access_ref.clone(),
        refresh_ref.clone(),
    );
    let config = oauth_connection_config_with_metadata(
        connection
            .get::<Option<Value>, _>("config")
            .unwrap_or_else(|| json!({})),
        token,
    );
    let grant_id = if let Some(subject_user_id) = subject_user_id {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO connection_grants
                (id, company_id, connection_id, kind, subject_user_id,
                 credential_secret_refs, status, is_default, created_by_user_id)
             VALUES ($1, $2, $3, 'user', $4, $5, 'active', false, $6)
             ON CONFLICT (connection_id, subject_user_id)
             DO UPDATE SET company_id = EXCLUDED.company_id,
                           credential_secret_refs = EXCLUDED.credential_secret_refs,
                           status = 'active', revoked_at = NULL,
                           updated_at = NOW()
             RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(connection_id)
        .bind(subject_user_id)
        .bind(&secret_refs)
        .bind(user_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to persist user OAuth grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else {
        sqlx::query(
            "INSERT INTO connection_grants
                (id, company_id, connection_id, kind, credential_secret_refs,
                 status, is_default, created_by_user_id)
             VALUES ($1, $2, $3, 'workspace', $4, 'active', true, $5)
             ON CONFLICT DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(connection_id)
        .bind(&secret_refs)
        .bind(user_id.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to ensure workspace OAuth grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sqlx::query(
            "UPDATE connection_grants
                SET credential_secret_refs = $3, status = 'active', revoked_at = NULL,
                    updated_at = NOW()
              WHERE company_id = $1 AND connection_id = $2
                AND kind = 'workspace' AND is_default = true",
        )
        .bind(company_id)
        .bind(connection_id)
        .bind(&secret_refs)
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to update workspace OAuth grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM connection_grants
              WHERE company_id = $1 AND connection_id = $2
                AND kind = 'workspace' AND is_default = true
              LIMIT 1",
        )
        .bind(company_id)
        .bind(connection_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to load workspace OAuth grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    };
    if subject_user_id.is_none() {
        sqlx::query(
            "UPDATE tool_connections
                SET credential_secret_refs = $3, config = $4,
                    status = 'active', enabled = true, health_status = 'unchecked',
                    health_message = 'OAuth credentials stored; health refresh pending',
                    last_error = NULL, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(connection_id)
        .bind(company_id)
        .bind(&secret_refs)
        .bind(&config)
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to activate OAuth connection");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    } else {
        sqlx::query(
            "UPDATE tool_connections
                SET config = $3, status = 'active', enabled = true,
                    health_status = 'unchecked',
                    health_message = 'OAuth credentials stored; health refresh pending',
                    last_error = NULL, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(connection_id)
        .bind(company_id)
        .bind(&config)
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to activate user OAuth connection");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }
    transaction.commit().await.map_err(|e| {
        tracing::error!(%e, %connection_id, "Failed to commit OAuth credentials");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(grant_id)
}

/// GET /api/tools/oauth/callback —— validate, consume and complete a PKCE OAuth callback.
async fn tools_oauth_callback(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Query(query): Query<OAuthCallbackQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let state_token = query.state.trim();
    if state_token.is_empty() || state_token.chars().count() > 512 {
        return Err(StatusCode::BAD_REQUEST);
    }
    use sqlx::Row;
    let state_row = sqlx::query(
        "SELECT state, company_id, connection_id, code_verifier,
                created_by_actor_type, created_by_actor_id, created_by_session_id,
                subject_user_id, requested_scopes, expires_at
           FROM tool_oauth_states
          WHERE state = $1",
    )
    .bind(state_token)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, "Failed to load OAuth callback state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::BAD_REQUEST)?;
    let company_id: Uuid = state_row.get("company_id");
    let connection_id: Uuid = state_row.get("connection_id");
    let user_id = oauth_callback_actor_matches(&actor, &state_row)?;
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if state_row.get::<DateTime<Utc>, _>("expires_at") <= Utc::now() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let connection = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    if connection.get::<String, _>("status") == "archived" {
        return Err(StatusCode::CONFLICT);
    }
    if connection.get::<String, _>("auth_kind") != "oauth" {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let endpoints = resolve_oauth_provider_endpoints(&connection).await?;
    let client_id = connection_config_string(&connection, &["clientId", "client_id"])
        .or_else(|| mcp_oauth_client_id(&endpoints.provider))
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let redirect_uri = configured_oauth_redirect_uri(&connection)
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    validate_oauth_redirect_constraints(&connection, &endpoints.provider, &redirect_uri)?;
    let token_uri = endpoints.token_url.clone();
    let client_secret = resolve_oauth_client_secret(&state.pool, company_id, &connection).await?;
    let provider_error = query
        .error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if provider_error.is_some() {
        consume_oauth_state(&state.pool, state_token).await?;
        tracing::warn!(%connection_id, error = provider_error.unwrap_or("unknown"), "OAuth provider returned an authorization error");
        return Err(StatusCode::BAD_REQUEST);
    }
    let code = query
        .code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.chars().count() <= 8192)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let requested_scope = state_row
        .get::<Option<Value>, _>("requested_scopes")
        .map(|value| config_scope_values(&value))
        .unwrap_or_default();
    consume_oauth_state(&state.pool, state_token).await?;
    let token = exchange_oauth_code(
        &token_uri,
        &client_id,
        client_secret.as_deref(),
        &redirect_uri,
        code,
        &state_row.get::<String, _>("code_verifier"),
        &requested_scope,
    )
    .await?;
    let subject_user_id = state_row.get::<Option<String>, _>("subject_user_id");
    let grant_id = persist_oauth_connection(
        &state,
        &actor,
        connection_id,
        company_id,
        subject_user_id.as_deref(),
        &token,
    )
    .await?;
    let connection = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_connection.oauth_connected",
        &actor,
        "tool_connection",
        connection_id,
        json!({
            "grantId": grant_id,
            "subjectType": if subject_user_id.is_some() { "user" } else { "workspace" },
            "scopeCount": token.scope.len(),
            "hasRefreshToken": token.refresh_token.is_some(),
            "actorUserId": user_id,
        }),
    )
    .await;
    if headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|part| part.trim() == "text/html"))
    {
        let issue_prefix = sqlx::query_scalar::<_, String>(
            "SELECT issue_prefix FROM companies WHERE id = $1",
        )
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(%e, %company_id, "Failed to load company issue prefix for OAuth redirect");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        let location = format!(
            "/{}/apps/{}/setup?oauth=connected",
            issue_prefix, connection_id
        );
        let mut response = StatusCode::SEE_OTHER.into_response();
        response.headers_mut().insert(
            header::LOCATION,
            HeaderValue::from_str(&location).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        );
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        );
        return Ok(response);
    }
    Ok((
        StatusCode::OK,
        Json(json!({
            "connectionId": connection_id,
            "grantId": grant_id,
            "status": "connected",
            "connection": connection_json(&connection),
        })),
    )
        .into_response())
}

/// POST /companies/:cid/tools/connections —— 创建连接。
#[derive(Debug, Deserialize)]
struct CreateConnectionRequest {
    #[serde(rename = "applicationId")]
    application_id: Option<Uuid>,
    #[serde(rename = "applicationName")]
    application_name: Option<String>,
    name: String,
    transport: Option<String>,
    #[serde(rename = "authKind")]
    auth_kind: Option<String>,
    ownership: Option<String>,
    status: Option<String>,
    #[serde(rename = "connectionKind")]
    connection_kind: Option<String>,
    config: Option<Value>,
    #[serde(rename = "transportConfig")]
    transport_config: Option<Value>,
    #[serde(rename = "credentialRefs")]
    credential_refs: Option<Value>,
    #[serde(rename = "credentialSecretRefs")]
    credential_secret_refs: Option<Value>,
    enabled: Option<bool>,
}
async fn create_company_tool_connection(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateConnectionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let name = request.name.trim();
    let transport = request.transport.as_deref().unwrap_or("");
    let auth_kind = request.auth_kind.as_deref().unwrap_or("none");
    let ownership = request.ownership.as_deref().unwrap_or("customer");
    let status = request.status.as_deref().unwrap_or("draft");
    let connection_kind = request.connection_kind.as_deref().unwrap_or("managed");
    if name.is_empty()
        || name.chars().count() > 160
        || !matches!(transport, "mcp_remote" | "rest_api" | "local_stdio")
        || !matches!(auth_kind, "oauth" | "api_key" | "none")
        || !matches!(ownership, "platform_shared" | "platform_provisioned" | "customer" | "dcr")
        || !matches!(status, "draft" | "active" | "disabled" | "archived")
        || !matches!(connection_kind, "managed" | "delegated" | "self_hosted")
        || request
            .credential_refs
            .as_ref()
            .is_some_and(|refs| !refs.is_array())
        || request
            .credential_secret_refs
            .as_ref()
            .is_some_and(|refs| !refs.is_array())
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let config = request
        .config
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!({}));
    let transport_config = request
        .transport_config
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!({}));
    let credential_refs = request
        .credential_refs
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!([]));
    let credential_secret_refs = request
        .credential_secret_refs
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!([]));
    let mut transaction = state.pool.begin().await.map_err(|e| {
        tracing::error!("Failed to start tool connection transaction: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let application_id = if let Some(application_id) = request.application_id {
        let application_type = sqlx::query_scalar::<_, Option<String>>(
            "SELECT type FROM tool_applications WHERE id = $1 AND company_id = $2",
        )
        .bind(application_id)
        .bind(company_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load tool connection application: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .flatten()
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
        if transport == "mcp_remote" && application_type != "mcp_http"
            || transport == "local_stdio" && application_type != "mcp_stdio"
        {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        application_id
    } else {
        let application_name = request
            .application_name
            .as_deref()
            .unwrap_or(name)
            .trim();
        if application_name.is_empty() || application_name.chars().count() > 160 {
            return Err(StatusCode::BAD_REQUEST);
        }
        let application_id = Uuid::new_v4();
        let application_key = normalize_tool_application_key(application_name);
        sqlx::query(
            "INSERT INTO tool_applications
                (id, company_id, application_key, name, type, status, metadata)
             VALUES ($1, $2, $3, $4, $5, 'active', '{}')",
        )
        .bind(application_id)
        .bind(company_id)
        .bind(application_key)
        .bind(application_name)
        .bind(if transport == "local_stdio" {
            "mcp_stdio"
        } else {
            "mcp_http"
        })
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                return StatusCode::CONFLICT;
            }
            tracing::error!("Failed to create tool connection application: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        application_id
    };
    let id = Uuid::new_v4();
    let uid = format!(
        "{}-{}",
        normalize_tool_application_key(name),
        id.simple()
    );
    let row = sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, application_id, name, uid, tool_type,
             connection_kind, ownership, transport, auth_kind, status, enabled,
             config, transport_config, credential_refs, credential_secret_refs,
             created_by_user_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                 $13, $14, $15, $16, $17)
         RETURNING *",
    )
    .bind(id)
    .bind(company_id)
    .bind(application_id)
    .bind(name)
    .bind(uid)
    .bind(transport)
    .bind(connection_kind)
    .bind(ownership)
    .bind(transport)
    .bind(auth_kind)
    .bind(status)
    .bind(request.enabled.unwrap_or(false))
    .bind(config)
    .bind(transport_config)
    .bind(credential_refs)
    .bind(credential_secret_refs)
    .bind(actor.principal_id().map(|id| id.to_string()))
    .fetch_one(&mut *transaction)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            return StatusCode::CONFLICT;
        }
        tracing::error!("Failed to create tool connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    transaction.commit().await.map_err(|e| {
        tracing::error!("Failed to commit tool connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_connection.created",
        &actor,
        "tool_connection",
        id,
        json!({ "transport": transport, "applicationId": application_id }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(connection_json(&row))))
}

/// POST /companies/:cid/tools/profiles —— 创建 profile。
#[derive(Debug, Deserialize)]
struct CreateToolProfileRequest {
    name: String,
    description: Option<String>,
    #[serde(rename = "profileKey", alias = "profile_key")]
    profile_key: Option<String>,
    status: Option<String>,
    #[serde(rename = "defaultAction", alias = "default_action")]
    default_action: Option<String>,
    metadata: Option<Value>,
    entries: Option<Vec<CreateProfileEntryRequest>>,
}
async fn create_company_tool_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateToolProfileRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let name = request.name.trim();
    if name.is_empty() || name.len() > 160 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let id = Uuid::new_v4();
    let profile_key = request
        .profile_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            format!(
                "{}-{}",
                normalize_tool_application_key(name),
                &id.simple().to_string()[..8]
            )
        });
    if profile_key.len() > 160 || !is_safe_tool_application_key(&profile_key) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let status = request.status.as_deref().unwrap_or("active");
    let default_action = request.default_action.as_deref().unwrap_or("deny");
    if !matches!(status, "draft" | "active" | "disabled" | "archived")
        || !matches!(default_action, "deny" | "allow")
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let metadata = request.metadata.unwrap_or_else(|| json!({}));
    if !metadata.is_object() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let normalized_entries = if let Some(entries) = request.entries.as_ref() {
        if entries.len() > 250 {
            return Err(StatusCode::BAD_REQUEST);
        }
        let mut normalized_entries = Vec::with_capacity(entries.len());
        for entry in entries {
            let tool_name = entry.tool_name.clone().or_else(|| entry.tool.clone());
            let selector_type = entry.selector_type.as_deref().or_else(|| {
                if tool_name.is_some() {
                    Some("tool_name")
                } else {
                    None
                }
            });
            let effect = entry
                .effect
                .as_deref()
                .or_else(|| entry.enabled.map(|enabled| if enabled { "include" } else { "exclude" }))
                .or(Some("include"));
            let normalized = normalize_profile_entry_values(
                selector_type,
                effect,
                entry.application_id,
                entry.connection_id,
                entry.catalog_entry_id,
                tool_name,
                entry.risk_level.clone(),
                entry.conditions.clone(),
            )?;
            validate_profile_entry_references(&state, company_id, &normalized).await?;
            normalized_entries.push(normalized);
        }
        normalized_entries
    } else {
        Vec::new()
    };
    use sqlx::Row;
    let mut transaction = state.pool.begin().await.map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to begin tool profile creation");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let row = sqlx::query(
        "INSERT INTO tool_profiles
            (id, company_id, profile_key, name, description, status, default_action, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, profile_key, name, description, status, default_action,
                   metadata, created_at, updated_at",
    )
    .bind(id)
    .bind(company_id)
    .bind(&profile_key)
    .bind(name)
    .bind(request.description.as_deref())
    .bind(status)
    .bind(default_action)
    .bind(&metadata)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create tool profile: {}", e);
        if e.as_database_error()
            .and_then(|database_error| database_error.code())
            .as_deref()
            == Some("23505")
        {
            StatusCode::CONFLICT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    for entry in &normalized_entries {
        sqlx::query(
            "INSERT INTO tool_profile_entries
                (id, company_id, profile_id, selector_type, effect,
                 application_id, connection_id, catalog_entry_id, tool_name,
                 risk_level, conditions)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(id)
        .bind(&entry.selector_type)
        .bind(&entry.effect)
        .bind(entry.application_id)
        .bind(entry.connection_id)
        .bind(entry.catalog_entry_id)
        .bind(&entry.tool_name)
        .bind(&entry.risk_level)
        .bind(&entry.conditions)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            tracing::error!(%error, %profile_key, "Failed to create tool profile entries");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }
    transaction.commit().await.map_err(|error| {
        tracing::error!(%error, %profile_key, "Failed to commit tool profile creation");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let entry_rows = sqlx::query(
        "SELECT * FROM tool_profile_entries
          WHERE company_id = $1 AND profile_id = $2
          ORDER BY created_at ASC",
    )
    .bind(company_id)
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %profile_key, "Failed to load created tool profile entries");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": row.get::<Uuid, _>("id"),
            "companyId": company_id,
            "profileKey": row.get::<String, _>("profile_key"),
            "name": row.get::<String, _>("name"),
            "description": row.get::<Option<String>, _>("description"),
            "status": row.get::<String, _>("status"),
            "defaultAction": row.get::<String, _>("default_action"),
            "metadata": row.get::<Value, _>("metadata"),
            "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
            "updatedAt": row.get::<DateTime<Utc>, _>("updated_at"),
            "entries": entry_rows.iter().map(profile_entry_json).collect::<Vec<_>>(),
        })),
    ))
}

/// POST /companies/:cid/tools/examples/:example_id/install —— 安装内置 MCP fixture。
async fn install_tool_example(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, example_id)): Path<(Uuid, String)>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if example_id != "safe-read-only-todo-kv" {
        return Err(StatusCode::NOT_FOUND);
    }
    use sqlx::Row;
    let application_key = "paperclip.examples.safe-read-only-todo-kv";
    let application_name = "Paperclip example: Safe read-only Todo / KV";
    let profile_key = "paperclip.examples.safe-read-only-todo-kv.profile";
    let template_id = "paperclip.synthetic-todo-kv";
    let actor_user_id = actor.principal_id().map(|id| id.to_string());

    let existing_application = sqlx::query(
        "SELECT id FROM tool_applications
          WHERE company_id = $1 AND application_key = $2",
    )
    .bind(company_id)
    .bind(application_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to load MCP example application");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let application_created = existing_application.is_none();
    let application_id = if let Some(row) = existing_application {
        row.get::<Uuid, _>("id")
    } else {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tool_applications
                (id, company_id, application_key, name, description, type, status, metadata,
                 owner_user_id)
             VALUES ($1, $2, $3, $4, $5, 'mcp_stdio', 'active', $6, $7)
             RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(application_key)
        .bind(application_name)
        .bind("Deterministic MCP fixture for tool governance checks.")
        .bind(json!({"sourceTemplateKey": template_id, "exampleId": example_id}))
        .bind(actor_user_id.as_deref())
        .fetch_one(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %company_id, "Failed to create MCP example application");
            if is_unique_violation(&error) {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?
    };
    let config = json!({
        "templateId": template_id,
        "sourceTemplateKey": template_id,
        "safeDefault": true,
        "quarantineNewEntries": true,
    });
    let existing_connection = sqlx::query(
        "SELECT id FROM tool_connections
          WHERE company_id = $1 AND application_id = $2
            AND name = $3
          ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(application_id)
    .bind(application_name)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to load MCP example connection");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let (connection_id, connection_created) = if let Some(row) = existing_connection {
        let connection_id: Uuid = row.get("id");
        sqlx::query(
            "UPDATE tool_connections
                SET transport = 'local_stdio', tool_type = 'local_stdio',
                    auth_kind = 'none', status = 'active', enabled = true,
                    config = $3, transport_config = $3,
                    health_status = 'unchecked', health_message = NULL,
                    last_error = NULL, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(connection_id)
        .bind(company_id)
        .bind(&config)
        .execute(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %connection_id, "Failed to refresh MCP example connection");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        (connection_id, false)
    } else {
        let connection_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tool_connections
                (id, company_id, application_id, name, uid, tool_type,
                 connection_kind, ownership, transport, auth_kind, status, enabled,
                 config, transport_config, credential_refs, credential_secret_refs,
                 created_by_user_id)
             VALUES ($1, $2, $3, $4, $5, 'local_stdio', 'managed', 'customer',
                     'local_stdio', 'none', 'active', true, $6, $6, '[]', '[]', $7)",
        )
        .bind(connection_id)
        .bind(company_id)
        .bind(application_id)
        .bind(application_name)
        .bind(format!("paperclip-example-{}", connection_id.simple()))
        .bind(&config)
        .bind(actor_user_id.as_deref())
        .execute(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %company_id, "Failed to create MCP example connection");
            if is_unique_violation(&error) {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;
        (connection_id, true)
    };
    // Discovery runs through the same catalog refresh used by real MCP
    // connections. The built-in template is deterministic, so this does not
    // require an external process or a network dependency.
    let _ = refresh_connection_catalog(
        State(state.clone()),
        Extension(actor.clone()),
        Path(connection_id),
    )
    .await?;

    let existing_profile = sqlx::query(
        "SELECT id FROM tool_profiles WHERE company_id = $1 AND profile_key = $2",
    )
    .bind(company_id)
    .bind(profile_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to load MCP example profile");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let (profile_id, profile_created) = if let Some(row) = existing_profile {
        let profile_id: Uuid = row.get("id");
        sqlx::query(
            "UPDATE tool_profiles
                SET name = 'Example safe read-only tools', description = $3,
                    status = 'active', default_action = 'deny', metadata = $4,
                    updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(profile_id)
        .bind(company_id)
        .bind("Allows only the read-only tools from the deterministic MCP fixture.")
        .bind(json!({"source": "paperclip_example", "exampleId": example_id}))
        .execute(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %profile_id, "Failed to refresh MCP example profile");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        (profile_id, false)
    } else {
        let profile_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tool_profiles
                (id, company_id, profile_key, name, description, status,
                 default_action, metadata)
             VALUES ($1, $2, $3, 'Example safe read-only tools', $4,
                     'active', 'deny', $5)",
        )
        .bind(profile_id)
        .bind(company_id)
        .bind(profile_key)
        .bind("Allows only the read-only tools from the deterministic MCP fixture.")
        .bind(json!({"source": "paperclip_example", "exampleId": example_id}))
        .execute(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %company_id, "Failed to create MCP example profile");
            if is_unique_violation(&error) {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;
        (profile_id, true)
    };
    let read_catalog_ids = sqlx::query_scalar::<_, Uuid>(
        "UPDATE tool_catalog_entries
            SET status = 'active', reviewed_at = NOW(),
                reviewed_by_agent_id = NULL, reviewed_by_user_id = $3,
                quarantined_at = NULL, quarantine_reason = NULL, updated_at = NOW()
          WHERE company_id = $1 AND connection_id = $2 AND risk_level = 'read'
         RETURNING id",
    )
    .bind(company_id)
    .bind(connection_id)
    .bind(actor_user_id.as_deref())
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %connection_id, "Failed to approve MCP example read tools");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "DELETE FROM tool_profile_entries
          WHERE company_id = $1 AND profile_id = $2 AND connection_id = $3",
    )
    .bind(company_id)
    .bind(profile_id)
    .bind(connection_id)
    .execute(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to reset MCP example profile entries");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "INSERT INTO tool_profile_entries
            (id, company_id, profile_id, selector_type, effect,
             connection_id, catalog_entry_id, tool_name)
         SELECT gen_random_uuid(), company_id, $3, 'catalog_entry', 'include',
                connection_id, id, tool_name
           FROM tool_catalog_entries
          WHERE company_id = $1 AND connection_id = $2 AND risk_level = 'read'
         ON CONFLICT DO NOTHING",
    )
    .bind(company_id)
    .bind(connection_id)
    .bind(profile_id)
    .execute(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to create MCP example profile entries");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let binding = sqlx::query(
        "INSERT INTO tool_profile_bindings
            (company_id, profile_id, target_type, target_id, priority,
             metadata, created_by_user_id)
         VALUES ($1, $2, 'company', $1::text, 100, $3, $4)
         ON CONFLICT (company_id, target_type, target_id, profile_id)
         DO UPDATE SET priority = EXCLUDED.priority, metadata = EXCLUDED.metadata,
                       updated_at = NOW()
         RETURNING id, created_at, updated_at",
    )
    .bind(company_id)
    .bind(profile_id)
    .bind(json!({"source": "paperclip_example"}))
    .bind(actor_user_id.as_deref())
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to bind MCP example profile");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let connection = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    let profile = sqlx::query(
        "SELECT id, profile_key, name, description, status, default_action,
                new_tools_reviewed_at, metadata, created_at, updated_at
           FROM tool_profiles WHERE id = $1 AND company_id = $2",
    )
    .bind(profile_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let entries = sqlx::query(
        "SELECT id, catalog_entry_id, tool_name, effect, connection_id
           FROM tool_profile_entries
          WHERE company_id = $1 AND profile_id = $2
          ORDER BY tool_name ASC",
    )
    .bind(company_id)
    .bind(profile_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let catalog = sqlx::query(
        "SELECT id, tool_name, title, description, input_schema, risk_level, status,
                version_hash, schema_hash, reviewed_at
           FROM tool_catalog_entries
          WHERE company_id = $1 AND connection_id = $2
          ORDER BY tool_name ASC",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let application = sqlx::query(
        "SELECT id, company_id, application_key, name, description,
                type AS application_type, status, plugin_id, owner_agent_id,
                owner_user_id, metadata, archived_at, created_at, updated_at,
                NULL::uuid AS legacy_agent_id, NULL::uuid AS legacy_connection_id
           FROM tool_applications WHERE id = $1 AND company_id = $2",
    )
    .bind(application_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        if application_created || connection_created || profile_created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(json!({
            "example": {
                "id": example_id,
                "title": "Safe read-only Todo / KV fixture",
                "installed": true,
            },
            "created": application_created || connection_created || profile_created,
            "application": tool_application_json(&application),
            "connection": connection_json(&connection),
            "profile": {
                "id": profile.get::<Uuid, _>("id"),
                "companyId": company_id,
                "profileKey": profile.get::<String, _>("profile_key"),
                "name": profile.get::<String, _>("name"),
                "description": profile.get::<Option<String>, _>("description"),
                "status": profile.get::<String, _>("status"),
                "defaultAction": profile.get::<String, _>("default_action"),
                "newToolsReviewedAt": profile.get::<Option<DateTime<Utc>>, _>("new_tools_reviewed_at"),
                "metadata": profile.get::<Value, _>("metadata"),
                "createdAt": profile.get::<DateTime<Utc>, _>("created_at"),
                "updatedAt": profile.get::<DateTime<Utc>, _>("updated_at"),
            },
            "profileEntries": entries.iter().map(|row| json!({
                "id": row.get::<Uuid, _>("id"),
                "catalogEntryId": row.get::<Option<Uuid>, _>("catalog_entry_id"),
                "toolName": row.get::<Option<String>, _>("tool_name"),
                "effect": row.get::<String, _>("effect"),
                "connectionId": row.get::<Option<Uuid>, _>("connection_id"),
            })).collect::<Vec<_>>(),
            "profileBinding": {
                "id": binding.get::<Uuid, _>("id"),
                "companyId": company_id,
                "profileId": profile_id,
                "targetType": "company",
                "targetId": company_id,
                "priority": 100,
                "metadata": {"source": "paperclip_example"},
                "createdAt": binding.get::<DateTime<Utc>, _>("created_at"),
                "updatedAt": binding.get::<DateTime<Utc>, _>("updated_at"),
            },
            "catalog": catalog.iter().map(|row| json!({
                "id": row.get::<Uuid, _>("id"),
                "name": row.get::<String, _>("tool_name"),
                "toolName": row.get::<String, _>("tool_name"),
                "title": row.get::<Option<String>, _>("title"),
                "description": row.get::<Option<String>, _>("description"),
                "inputSchema": row.get::<Value, _>("input_schema"),
                "riskLevel": row.get::<String, _>("risk_level"),
                "status": row.get::<String, _>("status"),
                "versionHash": row.get::<String, _>("version_hash"),
                "schemaHash": row.get::<Option<String>, _>("schema_hash"),
                "reviewedAt": row.get::<Option<DateTime<Utc>>, _>("reviewed_at"),
            })).collect::<Vec<_>>(),
            "actions": {
                "readOnly": read_catalog_ids,
                "canMakeChanges": catalog.iter().filter(|row| row.get::<String, _>("risk_level") != "read").map(|row| row.get::<Uuid, _>("id")).collect::<Vec<_>>(),
            },
        })),
    ))
}

/// POST /companies/:cid/tools/examples/:example_id/smoke —— 验证 fixture、catalog 和 profile 约束。
async fn smoke_tool_example(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, example_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if example_id != "safe-read-only-todo-kv" {
        return Err(StatusCode::NOT_FOUND);
    }
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT c.id AS connection_id, p.id AS profile_id
           FROM tool_connections c
           JOIN tool_applications a ON a.id = c.application_id
          CROSS JOIN tool_profiles p
          JOIN tool_profile_bindings b
            ON b.profile_id = p.id AND b.company_id = p.company_id
           AND b.target_type = 'company' AND b.target_id = $1::text
          WHERE c.company_id = $1
            AND a.application_key = 'paperclip.examples.safe-read-only-todo-kv'
            AND p.company_id = $1
            AND p.profile_key = 'paperclip.examples.safe-read-only-todo-kv.profile'
          ORDER BY c.updated_at DESC LIMIT 1",
    )
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %company_id, "Failed to load MCP example smoke fixture");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::CONFLICT)?;
    let connection_id: Uuid = row.get("connection_id");
    let profile_id: Uuid = row.get("profile_id");
    let checks = sqlx::query(
        "SELECT
            COUNT(*) FILTER (WHERE risk_level = 'read' AND status = 'active' AND reviewed_at IS NOT NULL) AS readable,
            COUNT(*) FILTER (WHERE risk_level IN ('write', 'destructive') AND status = 'quarantined') AS guarded,
            COUNT(*) FILTER (WHERE risk_level = 'read' AND EXISTS (
                SELECT 1 FROM tool_profile_entries e
                 WHERE e.profile_id = $3 AND e.catalog_entry_id = tool_catalog_entries.id
            )) AS profiled_reads
          FROM tool_catalog_entries
         WHERE company_id = $1 AND connection_id = $2",
    )
    .bind(company_id)
    .bind(connection_id)
    .bind(profile_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %connection_id, "Failed to run MCP example smoke checks");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let readable: i64 = checks.get("readable");
    let guarded: i64 = checks.get("guarded");
    let profiled_reads: i64 = checks.get("profiled_reads");
    let check_values = vec![
        json!({"name": "allow_read_tool", "ok": readable > 0 && profiled_reads == readable}),
        json!({"name": "deny_write_tool", "ok": guarded > 0}),
        json!({"name": "fixture_catalog_discovered", "ok": readable + guarded >= 6}),
    ];
    Ok(Json(json!({
        "exampleId": example_id,
        "ok": check_values.iter().all(|check| check.get("ok").and_then(Value::as_bool).unwrap_or(false)),
        "connectionId": connection_id,
        "profileId": profile_id,
        "checks": check_values,
    })))
}

#[derive(Debug, Deserialize, Default)]
struct ConnectToolAppRequest {
    #[serde(rename = "galleryKey")]
    gallery_key: Option<String>,
    link: Option<String>,
    name: Option<String>,
    #[serde(rename = "applicationId")]
    application_id: Option<Uuid>,
    #[serde(rename = "credentialValues")]
    credential_values: Option<Value>,
    #[serde(rename = "configValues")]
    config_values: Option<Value>,
}

/// POST /companies/:cid/tools/apps/connect —— 创建、探测并返回真实 MCP 连接。
async fn connect_tool_app(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<ConnectToolAppRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let gallery_key = request
        .gallery_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let link = request
        .link
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if gallery_key.is_some() == link.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let credential_values = request
        .credential_values
        .clone()
        .unwrap_or_else(|| json!({}));
    if !credential_values.is_object() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let config_values = request.config_values.unwrap_or_else(|| json!({}));
    if !config_values.is_object() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (transport, auth_kind, ownership, mut config, default_name) = if let Some(gallery_key) = gallery_key {
        match gallery_key {
            "zapier" => (
                "mcp_remote",
                "api_key",
                "customer",
                json!({
                    "url": "https://mcp.zapier.com/api/mcp",
                    "sourceTemplateKey": "zapier",
                    "quarantineNewEntries": true,
                }),
                "Zapier".to_string(),
            ),
            "github" => (
                "mcp_remote",
                "api_key",
                "customer",
                json!({
                    "url": "https://api.githubcopilot.com/mcp/",
                    "sourceTemplateKey": "github",
                    "quarantineNewEntries": true,
                }),
                "GitHub".to_string(),
            ),
            "slack" => {
                let mut config = json!({
                    "url": "https://mcp.slack.com/mcp",
                    "sourceTemplateKey": "slack",
                    "quarantineNewEntries": true,
                    "oauth": {
                        "provider": "slack",
                        "authorizationUrl": "https://slack.com/oauth/v2/authorize",
                        "tokenUrl": "https://slack.com/api/oauth.v2.access",
                        "scopes": ["channels:read", "chat:write", "search:read"],
                    },
                });
                if let Some(client_id) = mcp_oauth_client_id("slack") {
                    config["oauth"]["clientId"] = json!(client_id);
                }
                (
                    "mcp_remote",
                    "oauth",
                    if config["oauth"].get("clientId").is_some() { "customer" } else { "dcr" },
                    config,
                    "Slack".to_string(),
                )
            }
            "notion" => {
                let mut config = json!({
                    "url": "https://mcp.notion.com/mcp",
                    "sourceTemplateKey": "notion",
                    "quarantineNewEntries": true,
                    "oauth": {"provider": "notion"},
                });
                if let Some(client_id) = mcp_oauth_client_id("notion") {
                    config["oauth"]["clientId"] = json!(client_id);
                }
                (
                    "mcp_remote",
                    "oauth",
                    if config["oauth"].get("clientId").is_some() { "customer" } else { "dcr" },
                    config,
                    "Notion".to_string(),
                )
            }
            "linear" => {
                let mut config = json!({
                    "url": "https://mcp.linear.app/mcp",
                    "sourceTemplateKey": "linear",
                    "quarantineNewEntries": true,
                    "oauth": {
                        "provider": "linear",
                        "authorizationUrl": "https://linear.app/oauth/authorize",
                        "tokenUrl": "https://api.linear.app/oauth/token",
                        "scopes": ["read", "write"],
                    },
                });
                if let Some(client_id) = mcp_oauth_client_id("linear") {
                    config["oauth"]["clientId"] = json!(client_id);
                }
                (
                    "mcp_remote",
                    "oauth",
                    if config["oauth"].get("clientId").is_some() { "customer" } else { "dcr" },
                    config,
                    "Linear".to_string(),
                )
            }
            "context7" => (
                "mcp_remote",
                "none",
                "customer",
                json!({
                    "url": "https://mcp.context7.com/mcp",
                    "sourceTemplateKey": "context7",
                    "quarantineNewEntries": true,
                }),
                "Context7".to_string(),
            ),
            "google-sheets" => {
                let template_exists = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(
                         SELECT 1 FROM tool_stdio_command_templates
                          WHERE company_id = $1
                            AND template_key = 'paperclip.google-sheets'
                            AND status = 'active'
                     )",
                )
                .bind(company_id)
                .fetch_one(&state.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                if !template_exists {
                    return Err(StatusCode::UNPROCESSABLE_ENTITY);
                }
                let mut config = json!({
                    "templateId": "paperclip.google-sheets",
                    "sourceTemplateKey": "google-sheets",
                    "quarantineNewEntries": true,
                });
                if let Some(value) = config_values.get("allowedSpreadsheetIds") {
                    config["allowedSpreadsheetIds"] = value.clone();
                }
                (
                    "local_stdio",
                    "none",
                    "customer",
                    config,
                    "Google Sheets".to_string(),
                )
            }
            "custom-mcp-http" => {
                let url = config_values
                    .get("url")
                    .or_else(|| config_values.get("endpoint"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or(StatusCode::BAD_REQUEST)?;
                let parsed = Url::parse(url).map_err(|_| StatusCode::BAD_REQUEST)?;
                if !matches!(parsed.scheme(), "http" | "https")
                    || parsed.host_str().is_none()
                    || !parsed.username().is_empty()
                    || parsed.password().is_some()
                    || !parsed.fragment().unwrap_or_default().is_empty()
                {
                    return Err(StatusCode::BAD_REQUEST);
                }
                (
                    "mcp_remote",
                    "none",
                    "customer",
                    json!({
                        "url": url,
                        "quarantineNewEntries": true,
                    }),
                    "Custom MCP (HTTP)".to_string(),
                )
            }
            "custom-mcp-stdio" => {
                let template_id = config_values
                    .get("templateId")
                    .or_else(|| config_values.get("template_id"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or(StatusCode::BAD_REQUEST)?;
                let template_exists = crate::routes::tools::builtin_mcp_template_tools(template_id).is_some()
                    || sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(
                             SELECT 1 FROM tool_stdio_command_templates
                              WHERE company_id = $1 AND template_key = $2 AND status = 'active'
                         )",
                    )
                    .bind(company_id)
                    .bind(template_id)
                    .fetch_one(&state.pool)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                if !template_exists {
                    return Err(StatusCode::UNPROCESSABLE_ENTITY);
                }
                (
                    "local_stdio",
                    "none",
                    "customer",
                    json!({
                        "templateId": template_id,
                        "quarantineNewEntries": true,
                    }),
                    "Custom MCP (approved stdio)".to_string(),
                )
            }
            _ => return Err(StatusCode::NOT_FOUND),
        }
    } else {
        let link = link.ok_or(StatusCode::BAD_REQUEST)?;
        let parsed = Url::parse(link).map_err(|_| StatusCode::BAD_REQUEST)?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        let host = parsed.host_str().unwrap_or("MCP server");
        (
            "mcp_remote",
            "none",
            "customer",
            json!({"url": link, "quarantineNewEntries": true}),
            format!("MCP server ({host})"),
        )
    };
    let credential_fields = mcp_gallery_credential_fields(gallery_key);
    let Some(credential_object) = credential_values.as_object() else {
        return Err(StatusCode::BAD_REQUEST);
    };
    if gallery_key.is_some_and(|key| !matches!(key, "zapier" | "github" | "custom-mcp-http"))
        && !credential_object.is_empty()
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if gallery_key.is_some_and(|key| matches!(key, "zapier" | "github"))
        && credential_object
            .keys()
            .any(|key| key != "credentials.authorization")
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    for field in &credential_fields {
        if field.required
            && credential_object
                .get(field.config_path)
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if gallery_key == Some("custom-mcp-http") || gallery_key.is_none() {
        if credential_object
            .keys()
            .any(|key| key != "credentials.authorization" && !key.starts_with("headers."))
        {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if gallery_key.is_some_and(|key| matches!(key, "zapier" | "github")) {
        config["sourceTemplateKey"] = json!(gallery_key);
    }
    let name = request
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&default_name)
        .to_string();
    let create_request = CreateConnectionRequest {
        application_id: request.application_id,
        application_name: Some(name.clone()),
        name,
        transport: Some(transport.to_string()),
        auth_kind: Some(auth_kind.to_string()),
        ownership: Some(ownership.to_string()),
        status: Some("draft".to_string()),
        connection_kind: Some("managed".to_string()),
        config: Some(config.clone()),
        transport_config: Some(config),
        credential_refs: Some(json!([])),
        credential_secret_refs: Some(json!([])),
        enabled: Some(false),
    };
    let (_, Json(connection)) = create_company_tool_connection(
        State(state.clone()),
        Extension(actor.clone()),
        Path(company_id),
        Json(create_request),
    )
    .await?;
    let connection_id = connection
        .get("id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut credential_refs = Vec::new();
    let mut credential_secret_refs = Vec::new();
    for field in credential_fields {
        let Some(value) = credential_object
            .get(field.config_path)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let secret_id = create_mcp_credential_secret(
            &state.pool,
            company_id,
            field.label,
            field.config_path,
            value,
            &actor,
        )
        .await?;
        credential_secret_refs.push(json!({
            "secretId": secret_id,
            "versionSelector": "latest",
            "configPath": field.config_path,
            "required": field.required,
            "label": field.label,
        }));
        if let Some(header_name) = field.header_name {
            credential_refs.push(json!({
                "name": field.config_path,
                "secretId": secret_id,
                "version": "latest",
                "placement": "header",
                "key": header_name,
                "prefix": field.prefix,
            }));
        }
    }
    for (config_path, value) in credential_object
        .iter()
        .filter_map(|(key, value)| key.strip_prefix("headers.").map(|_| (key, value)))
    {
        let header_name = config_path.trim_start_matches("headers.").trim();
        let Some(value) = value.as_str().map(str::trim).filter(|value| !value.is_empty()) else {
            return Err(StatusCode::BAD_REQUEST);
        };
        if !valid_mcp_credential_header_name(header_name) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let secret_id = create_mcp_credential_secret(
            &state.pool,
            company_id,
            header_name,
            config_path,
            value,
            &actor,
        )
        .await?;
        credential_secret_refs.push(json!({
            "secretId": secret_id,
            "versionSelector": "latest",
            "configPath": config_path,
            "required": true,
            "label": header_name,
        }));
        credential_refs.push(json!({
            "name": config_path,
            "secretId": secret_id,
            "version": "latest",
            "placement": "header",
            "key": header_name,
            "prefix": null,
        }));
    }
    if !credential_secret_refs.is_empty() {
        sqlx::query(
            "UPDATE tool_connections
                SET credential_refs = $3, credential_secret_refs = $4, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(connection_id)
        .bind(company_id)
        .bind(json!(credential_refs))
        .bind(json!(credential_secret_refs))
        .execute(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %connection_id, "Failed to attach MCP credential references");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }
    if auth_kind != "oauth" {
        let _ = refresh_connection_catalog(
            State(state.clone()),
            Extension(actor.clone()),
            Path(connection_id),
        )
        .await?;
    }
    let connection_row = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    let application_id: Uuid = connection_row
        .get::<Option<Uuid>, _>("application_id")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let application_row = sqlx::query(
        "SELECT id, company_id, application_key, name, description,
                type AS application_type, status, plugin_id, owner_agent_id,
                owner_user_id, metadata, archived_at, created_at, updated_at,
                NULL::uuid AS legacy_agent_id, NULL::uuid AS legacy_connection_id
           FROM tool_applications WHERE id = $1 AND company_id = $2",
    )
    .bind(application_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let catalog = sqlx::query(
        "SELECT id, tool_name, title, description, input_schema, output_schema,
                annotations, risk_level, status, version_hash, schema_hash
           FROM tool_catalog_entries
          WHERE company_id = $1 AND connection_id = $2
          ORDER BY tool_name ASC",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let catalog_json = catalog
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"),
                "name": row.get::<String, _>("tool_name"),
                "toolName": row.get::<String, _>("tool_name"),
                "title": row.get::<Option<String>, _>("title"),
                "description": row.get::<Option<String>, _>("description"),
                "inputSchema": row.get::<Value, _>("input_schema"),
                "outputSchema": row.get::<Option<Value>, _>("output_schema"),
                "annotations": row.get::<Value, _>("annotations"),
                "riskLevel": row.get::<String, _>("risk_level"),
                "status": row.get::<String, _>("status"),
                "versionHash": row.get::<String, _>("version_hash"),
                "schemaHash": row.get::<Option<String>, _>("schema_hash"),
            })
        })
        .collect::<Vec<_>>();
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "connectionId": connection_id,
            "application": tool_application_json(&application_row),
            "connection": connection_json(&connection_row),
            "catalog": catalog_json,
            "actions": {
                "readOnly": catalog.iter().filter(|row| row.get::<String, _>("risk_level") == "read").map(|row| row.get::<Uuid, _>("id")).collect::<Vec<_>>(),
                "canMakeChanges": catalog.iter().filter(|row| row.get::<String, _>("risk_level") != "read").map(|row| row.get::<Uuid, _>("id")).collect::<Vec<_>>(),
            },
            "suggestedDefaults": {"access": "all_agents", "askFirstRiskLevels": ["write", "destructive"]},
            "auth": if auth_kind == "oauth" { json!({"kind": "oauth", "startUrl": null}) } else { Value::Null },
        })),
    ))
}

#[derive(Debug, Deserialize)]
struct ReconnectToolAppRequest {
    #[serde(rename = "credentialValues")]
    credential_values: Value,
}

/// POST /tool-connections/:id/reconnect —— rotate an API credential and
/// re-run MCP discovery without exposing the secret in the connection row.
async fn reconnect_tool_connection(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<ReconnectToolAppRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Write).await?;
    let Some(credential_object) = request.credential_values.as_object() else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let connection = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    if connection.get::<String, _>("status") == "archived" {
        return Err(StatusCode::CONFLICT);
    }
    let config = connection
        .get::<Option<Value>, _>("config")
        .unwrap_or_else(|| json!({}));
    let source_template_key = config
        .get("sourceTemplateKey")
        .and_then(Value::as_str);
    let fields = mcp_gallery_credential_fields(source_template_key);
    if fields.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if credential_object
        .keys()
        .any(|key| {
            !fields.iter().any(|field| field.config_path == key)
                && !(source_template_key.is_none() && key.starts_with("headers."))
        })
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let old_refs = connection
        .get::<Option<Value>, _>("credential_refs")
        .unwrap_or_else(|| json!([]));
    let old_secret_refs = connection
        .get::<Option<Value>, _>("credential_secret_refs")
        .unwrap_or_else(|| json!([]));
    let mut next_refs = old_refs.as_array().cloned().unwrap_or_default();
    let mut next_secret_refs = old_secret_refs.as_array().cloned().unwrap_or_default();
    let mut rotated = 0usize;
    for field in fields {
        let value = match credential_object
            .get(field.config_path)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(value) => value,
            None if field.required => return Err(StatusCode::BAD_REQUEST),
            None => continue,
        };
        let secret_id = create_mcp_credential_secret(
            &state.pool,
            company_id,
            field.label,
            field.config_path,
            value,
            &actor,
        )
        .await?;
        next_secret_refs.retain(|reference| {
            reference
                .get("configPath")
                .or_else(|| reference.get("config_path"))
                .and_then(Value::as_str)
                != Some(field.config_path)
        });
        next_secret_refs.push(json!({
            "secretId": secret_id,
            "versionSelector": "latest",
            "configPath": field.config_path,
            "required": field.required,
            "label": field.label,
        }));
        if let Some(header_name) = field.header_name {
            next_refs.retain(|reference| {
                reference
                    .get("name")
                    .or_else(|| reference.get("configPath"))
                    .and_then(Value::as_str)
                    != Some(field.config_path)
            });
            next_refs.push(json!({
                "name": field.config_path,
                "secretId": secret_id,
                "version": "latest",
                "placement": "header",
                "key": header_name,
                "prefix": field.prefix,
            }));
        }
        rotated += 1;
    }
    if source_template_key.is_none() {
        for (config_path, value) in credential_object
            .iter()
            .filter_map(|(key, value)| key.strip_prefix("headers.").map(|_| (key, value)))
        {
            let header_name = config_path.trim_start_matches("headers.").trim();
            let Some(value) = value.as_str().map(str::trim).filter(|value| !value.is_empty()) else {
                return Err(StatusCode::BAD_REQUEST);
            };
            if !valid_mcp_credential_header_name(header_name) {
                return Err(StatusCode::BAD_REQUEST);
            }
            let secret_id = create_mcp_credential_secret(
                &state.pool,
                company_id,
                header_name,
                config_path,
                value,
                &actor,
            )
            .await?;
            next_secret_refs.retain(|reference| {
                reference
                    .get("configPath")
                    .or_else(|| reference.get("config_path"))
                    .and_then(Value::as_str)
                    != Some(config_path)
            });
            next_secret_refs.push(json!({
                "secretId": secret_id,
                "versionSelector": "latest",
                "configPath": config_path,
                "required": true,
                "label": header_name,
            }));
            next_refs.retain(|reference| {
                reference
                    .get("name")
                    .or_else(|| reference.get("configPath"))
                    .and_then(Value::as_str)
                    != Some(config_path)
            });
            next_refs.push(json!({
                "name": config_path,
                "secretId": secret_id,
                "version": "latest",
                "placement": "header",
                "key": header_name,
                "prefix": null,
            }));
            rotated += 1;
        }
    }
    if rotated == 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    sqlx::query(
        "UPDATE tool_connections
            SET credential_refs = $3, credential_secret_refs = $4,
                health_status = 'unchecked', health_message = NULL,
                last_error = NULL, updated_at = NOW()
          WHERE id = $1 AND company_id = $2",
    )
    .bind(connection_id)
    .bind(company_id)
    .bind(Value::Array(next_refs))
    .bind(Value::Array(next_secret_refs))
    .execute(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %connection_id, "Failed to rotate MCP connection credential");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let refresh = refresh_connection_catalog(
        State(state.clone()),
        Extension(actor.clone()),
        Path(connection_id),
    )
    .await?;
    let connection = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_app.reconnected",
        &actor,
        "tool_connection",
        connection_id,
        json!({"rotatedCredentialCount": rotated}),
    )
    .await;
    Ok(Json(json!({
        "connection": connection_json(&connection),
        "catalogRefresh": refresh.0,
        "rotatedCredentialCount": rotated,
    })))
}

#[derive(Debug, Deserialize)]
struct FinishToolAppRequest {
    #[serde(rename = "enabledCatalogEntryIds", default)]
    enabled_catalog_entry_ids: Vec<Uuid>,
    #[serde(rename = "askFirstCatalogEntryIds", default)]
    ask_first_catalog_entry_ids: Vec<Uuid>,
    #[serde(rename = "reviewedCatalogEntryIds")]
    reviewed_catalog_entry_ids: Option<Vec<Uuid>>,
    access: Value,
}

async fn finish_tool_app(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, connection_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<FinishToolAppRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let connection = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    if connection.get::<String, _>("status") == "archived" {
        return Err(StatusCode::CONFLICT);
    }
    let enabled_ids = request.enabled_catalog_entry_ids;
    let ask_first_ids = request.ask_first_catalog_entry_ids;
    let reviewed_ids = request.reviewed_catalog_entry_ids.unwrap_or_else(|| {
        enabled_ids
            .iter()
            .chain(ask_first_ids.iter())
            .copied()
            .collect()
    });
    if enabled_ids.len() > 500
        || ask_first_ids.len() > 500
        || reviewed_ids.len() > 500
        || enabled_ids.iter().collect::<HashSet<_>>().len() != enabled_ids.len()
        || ask_first_ids.iter().collect::<HashSet<_>>().len() != ask_first_ids.len()
        || reviewed_ids.iter().collect::<HashSet<_>>().len() != reviewed_ids.len()
        || enabled_ids.iter().any(|id| ask_first_ids.contains(id))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let catalog_rows = sqlx::query(
        "SELECT id, tool_name, risk_level, status
           FROM tool_catalog_entries
          WHERE company_id = $1 AND connection_id = $2",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %connection_id, "Failed to load catalog for MCP app finish");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let catalog_by_id = catalog_rows
        .iter()
        .map(|row| (row.get::<Uuid, _>("id"), row))
        .collect::<std::collections::HashMap<_, _>>();
    if enabled_ids
        .iter()
        .chain(ask_first_ids.iter())
        .chain(reviewed_ids.iter())
        .any(|id| !catalog_by_id.contains_key(id))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let access = request.access;
    let target_agents = match access {
        Value::String(value) if value == "all_agents" => None,
        Value::Object(object) => {
            let Some(values) = object.get("agentIds").and_then(Value::as_array) else {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            };
            if values.is_empty() || values.len() > 250 {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
            let mut ids = Vec::with_capacity(values.len());
            for value in values {
                let Some(value) = value.as_str() else {
                    return Err(StatusCode::UNPROCESSABLE_ENTITY);
                };
                ids.push(Uuid::parse_str(value).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?);
            }
            if ids.iter().collect::<HashSet<_>>().len() != ids.len() {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
            let valid_count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM agents WHERE company_id = $1 AND id = ANY($2)",
            )
            .bind(company_id)
            .bind(&ids)
            .fetch_one(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if valid_count != ids.len() as i64 {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
            Some(ids)
        }
        _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
    };
    let reviewed_at = Utc::now();
    if !reviewed_ids.is_empty() {
        sqlx::query(
            "UPDATE tool_catalog_entries
                SET reviewed_at = $3, reviewed_by_user_id = $4,
                    status = CASE WHEN status = 'quarantined' THEN 'active' ELSE status END,
                    quarantined_at = NULL, quarantine_reason = NULL, updated_at = $3
              WHERE company_id = $1 AND connection_id = $2 AND id = ANY($5)",
        )
        .bind(company_id)
        .bind(connection_id)
        .bind(reviewed_at)
        .bind(actor.principal_id().map(|id| id.to_string()))
        .bind(&reviewed_ids)
        .execute(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %connection_id, "Failed to review MCP catalog entries");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }
    let profile_key = format!("mcp.connection.{}.profile", connection_id.simple());
    let profile_name = format!("MCP access: {}", connection.get::<String, _>("name"));
    let profile = sqlx::query(
        "INSERT INTO tool_profiles
            (company_id, profile_key, name, description, status, default_action,
             new_tools_reviewed_at, metadata)
         VALUES ($1, $2, $3, $4, 'active', 'deny', $5, $6)
         ON CONFLICT (company_id, profile_key) DO UPDATE SET
             name = EXCLUDED.name, description = EXCLUDED.description,
             status = 'active', default_action = 'deny',
             new_tools_reviewed_at = EXCLUDED.new_tools_reviewed_at,
             metadata = EXCLUDED.metadata, updated_at = NOW()
         RETURNING id, profile_key, name, description, status, default_action,
                   new_tools_reviewed_at, metadata, created_at, updated_at",
    )
    .bind(company_id)
    .bind(&profile_key)
    .bind(&profile_name)
    .bind("Access profile created from the MCP app setup review.")
    .bind(reviewed_at)
    .bind(json!({"source": "app_finish", "connectionId": connection_id}))
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %connection_id, "Failed to create MCP app profile");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let profile_id: Uuid = profile.get("id");
    sqlx::query(
        "DELETE FROM tool_profile_entries
          WHERE company_id = $1 AND profile_id = $2 AND connection_id = $3",
    )
    .bind(company_id)
    .bind(profile_id)
    .bind(connection_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !enabled_ids.is_empty() {
        sqlx::query(
            "INSERT INTO tool_profile_entries
                (id, company_id, profile_id, selector_type, effect,
                 connection_id, catalog_entry_id, tool_name)
             SELECT gen_random_uuid(), company_id, $3, 'catalog_entry', 'include',
                    connection_id, id, tool_name
               FROM tool_catalog_entries
              WHERE company_id = $1 AND connection_id = $2 AND id = ANY($4)
             ON CONFLICT DO NOTHING",
        )
        .bind(company_id)
        .bind(connection_id)
        .bind(profile_id)
        .bind(&enabled_ids)
        .execute(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    sqlx::query(
        "DELETE FROM tool_profile_bindings
          WHERE company_id = $1 AND profile_id = $2",
    )
    .bind(company_id)
    .bind(profile_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(agent_ids) = &target_agents {
        for agent_id in agent_ids {
            sqlx::query(
                "INSERT INTO tool_profile_bindings
                    (company_id, profile_id, target_type, target_id, priority,
                     metadata, created_by_user_id)
                 VALUES ($1, $2, 'agent', $3::text, 100, $4, $5)",
            )
            .bind(company_id)
            .bind(profile_id)
            .bind(agent_id)
            .bind(json!({"source": "app_finish"}))
            .bind(actor.principal_id().map(|id| id.to_string()))
            .execute(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    } else {
        sqlx::query(
            "INSERT INTO tool_profile_bindings
                (company_id, profile_id, target_type, target_id, priority,
                 metadata, created_by_user_id)
             VALUES ($1, $2, 'company', $1::text, 100, $3, $4)",
        )
        .bind(company_id)
        .bind(profile_id)
        .bind(json!({"source": "app_finish"}))
        .bind(actor.principal_id().map(|id| id.to_string()))
        .execute(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let mut policy_ids = Vec::new();
    for catalog_entry_id in &ask_first_ids {
        let row = catalog_by_id
            .get(catalog_entry_id)
            .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
        let tool_name: String = row.get("tool_name");
        let policy_name = format!("Ask first {} {}", connection_id.simple(), tool_name);
        let policy = sqlx::query(
            "INSERT INTO tool_policies
                (company_id, name, description, policy_type, priority, enabled,
                 selectors, config, created_by_user_id)
             VALUES ($1, $2, $3, 'require_approval', 50, true, $4, $5, $6)
             ON CONFLICT (company_id, name) DO UPDATE SET
                 description = EXCLUDED.description, enabled = true,
                 selectors = EXCLUDED.selectors, config = EXCLUDED.config,
                 updated_at = NOW()
             RETURNING id",
        )
        .bind(company_id)
        .bind(&policy_name)
        .bind("MCP app setup requires approval for this tool.")
        .bind(json!({
            "connectionId": connection_id,
            "catalogEntryId": catalog_entry_id,
            "toolName": tool_name,
        }))
        .bind(json!({"source": "app_finish"}))
        .bind(actor.principal_id().map(|id| id.to_string()))
        .fetch_one(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %connection_id, "Failed to create MCP ask-first policy");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        policy_ids.push(policy.get::<Uuid, _>("id"));
    }
    sqlx::query(
        "UPDATE tool_connections
            SET status = 'active', enabled = true, health_status = 'healthy',
                health_message = NULL, updated_at = NOW()
          WHERE id = $1 AND company_id = $2",
    )
    .bind(connection_id)
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let connection = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    let entries = sqlx::query(
        "SELECT id, catalog_entry_id, tool_name, effect, connection_id
           FROM tool_profile_entries
          WHERE company_id = $1 AND profile_id = $2
          ORDER BY tool_name ASC",
    )
    .bind(company_id)
    .bind(profile_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let bindings = sqlx::query(
        "SELECT id, target_type, target_id, priority, metadata, created_at, updated_at
           FROM tool_profile_bindings
          WHERE company_id = $1 AND profile_id = $2
          ORDER BY priority ASC, created_at ASC",
    )
    .bind(company_id)
    .bind(profile_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({
        "connection": connection_json(&connection),
        "profile": {
            "id": profile_id,
            "companyId": company_id,
            "profileKey": profile.get::<String, _>("profile_key"),
            "name": profile.get::<String, _>("name"),
            "description": profile.get::<Option<String>, _>("description"),
            "status": profile.get::<String, _>("status"),
            "defaultAction": profile.get::<String, _>("default_action"),
            "newToolsReviewedAt": profile.get::<Option<DateTime<Utc>>, _>("new_tools_reviewed_at"),
            "metadata": profile.get::<Value, _>("metadata"),
            "createdAt": profile.get::<DateTime<Utc>, _>("created_at"),
            "updatedAt": profile.get::<DateTime<Utc>, _>("updated_at"),
        },
        "profileEntries": entries.iter().map(|row| json!({
            "id": row.get::<Uuid, _>("id"),
            "catalogEntryId": row.get::<Option<Uuid>, _>("catalog_entry_id"),
            "toolName": row.get::<Option<String>, _>("tool_name"),
            "effect": row.get::<String, _>("effect"),
            "connectionId": row.get::<Option<Uuid>, _>("connection_id"),
        })).collect::<Vec<_>>(),
        "profileBindings": bindings.iter().map(|row| json!({
            "id": row.get::<Uuid, _>("id"),
            "targetType": row.get::<String, _>("target_type"),
            "targetId": row.get::<String, _>("target_id"),
            "priority": row.get::<i32, _>("priority"),
            "metadata": row.get::<Value, _>("metadata"),
            "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
            "updatedAt": row.get::<DateTime<Utc>, _>("updated_at"),
        })).collect::<Vec<_>>(),
        "policies": policy_ids,
        "access": if target_agents.is_some() { json!({"agentIds": target_agents}) } else { json!("all_agents") },
        "reviewedCatalogEntryIds": reviewed_ids,
    })))
}

/// POST /companies/:cid/tools/mcp/import-json —— 解析 MCP 配置并返回可审阅草稿。
async fn import_mcp_json(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let raw = body
        .get("mcpJson")
        .cloned()
        .ok_or(StatusCode::BAD_REQUEST)?;
    let parsed = match raw {
        Value::String(text) => {
            serde_json::from_str::<Value>(&text).map_err(|_| StatusCode::BAD_REQUEST)?
        }
        value => value,
    };
    let servers = parsed
        .get("mcpServers")
        .and_then(Value::as_object)
        .ok_or(StatusCode::BAD_REQUEST)?;

    let drafts = servers
        .iter()
        .map(|(name, value)| {
            let server = value.as_object();
            let mut warnings = Vec::new();
            let (transport, config, credential_fields) = if let Some(server) = server {
                if let Some(url) = server
                    .get("url")
                    .or_else(|| server.get("endpoint"))
                    .and_then(Value::as_str)
                {
                    let headers = server
                        .get("headers")
                        .and_then(Value::as_object)
                        .map(|headers| {
                            headers
                                .keys()
                                .map(|key| {
                                    warnings.push(format!(
                                        "Header {key} will be stored as a Paperclip secret before activation."
                                    ));
                                    json!({
                                        "configPath": format!("headers.{key}"),
                                        "label": key,
                                        "placement": "header",
                                        "key": key,
                                        "prefix": Value::Null,
                                        "required": true
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    ("mcp_remote", json!({ "url": url }), headers)
                } else if let Some(command) = server.get("command").and_then(Value::as_str) {
                    warnings.push(
                        "Imported stdio commands stay draft-only unless mapped to an approved Paperclip template."
                            .to_string(),
                    );
                    (
                        "local_stdio",
                        json!({
                            "importedCommand": command,
                            "importedArgs": server.get("args").cloned().unwrap_or_else(|| json!([]))
                        }),
                        Vec::new(),
                    )
                } else {
                    warnings.push("Unsupported MCP server entry.".to_string());
                    ("mcp_remote", json!({}), Vec::new())
                }
            } else {
                warnings.push("Unsupported MCP server entry.".to_string());
                ("mcp_remote", json!({}), Vec::new())
            };

            json!({
                "name": name,
                "transport": transport,
                "status": "draft",
                "config": config,
                "credentialRefs": [],
                "credentialFields": credential_fields,
                "warnings": warnings
            })
        })
        .collect::<Vec<_>>();

    if drafts.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(Json(json!({ "drafts": drafts })))
}

/// PATCH /companies/:cid/tools/policies/:policy_id —— 更新策略。
#[derive(Debug, Deserialize)]
struct UpdateToolPolicyRequest {
    name: Option<String>,
    description: Option<Option<String>>,
    #[serde(rename = "policyType", alias = "policy_type")]
    policy_type: Option<String>,
    priority: Option<i32>,
    enabled: Option<bool>,
    selectors: Option<Value>,
    conditions: Option<Option<Value>>,
    config: Option<Option<Value>>,
}
async fn update_company_tool_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, policy_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateToolPolicyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if request.name.as_deref().is_some_and(|name| {
        name.trim().is_empty() || name.trim().chars().count() > 160
    }) || request.description.as_ref().is_some_and(|description| {
        description
            .as_deref()
            .is_some_and(|description| description.chars().count() > 4000)
    }) || request.policy_type.as_deref().is_some_and(|policy_type| {
        !matches!(policy_type, "allow" | "block" | "require_approval" | "rate_limit")
    }) || request.priority.is_some_and(|priority| !(0..=10_000).contains(&priority))
        || request
            .selectors
            .as_ref()
            .is_some_and(|selectors| !selectors.is_object())
        || request.conditions.as_ref().is_some_and(|conditions| {
            conditions
                .as_ref()
                .is_some_and(|conditions| !conditions.is_object())
        })
        || request.config.as_ref().is_some_and(|config| {
            config.as_ref().is_some_and(|config| !config.is_object())
        })
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if request.name.is_none()
        && request.description.is_none()
        && request.policy_type.is_none()
        && request.priority.is_none()
        && request.enabled.is_none()
        && request.selectors.is_none()
        && request.conditions.is_none()
        && request.config.is_none()
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE tool_policies SET
             name = COALESCE($3, name),
             description = CASE WHEN $4 THEN $5 ELSE description END,
             policy_type = COALESCE($6, policy_type),
             priority = COALESCE($7, priority),
             enabled = COALESCE($8, enabled),
             selectors = COALESCE($9, selectors),
             conditions = CASE WHEN $10 THEN $11 ELSE conditions END,
             config = CASE WHEN $12 THEN $13 ELSE config END,
             updated_at = NOW()
          WHERE id = $1 AND company_id = $2 AND policy_type <> 'trust_rule'
         RETURNING id, company_id, name, description, policy_type, priority,
                   enabled, selectors, conditions, config, created_at, updated_at",
    )
    .bind(policy_id)
    .bind(company_id)
    .bind(request.name.as_deref().map(str::trim))
    .bind(request.description.is_some())
    .bind(request.description.as_ref().and_then(|description| description.as_deref()))
    .bind(request.policy_type.as_deref())
    .bind(request.priority)
    .bind(request.enabled)
    .bind(request.selectors.as_ref())
    .bind(request.conditions.is_some())
    .bind(request.conditions.as_ref().and_then(|conditions| conditions.as_ref()))
    .bind(request.config.is_some())
    .bind(request.config.as_ref().and_then(|config| config.as_ref()))
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update tool policy: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok(Json(json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "name": row.get::<String, _>("name"),
        "description": row.get::<Option<String>, _>("description"),
        "policyType": row.get::<String, _>("policy_type"),
        "priority": row.get::<i32, _>("priority"),
        "enabled": row.get::<bool, _>("enabled"),
        "selectors": row.get::<Value, _>("selectors"),
        "conditions": row.get::<Option<Value>, _>("conditions"),
        "config": row.get::<Option<Value>, _>("config"),
        "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
        "updatedAt": row.get::<DateTime<Utc>, _>("updated_at"),
    })))
}

#[derive(Debug, Deserialize, Default)]
struct DuplicateToolPolicyRequest {
    name: Option<String>,
}

/// POST /companies/:cid/tools/policies/:policy_id/duplicate —— 复制策略。
async fn duplicate_tool_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, policy_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<DuplicateToolPolicyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let source = sqlx::query(
        "SELECT name, description, policy_type, priority, selectors, conditions, config
           FROM tool_policies
          WHERE id = $1 AND company_id = $2 AND policy_type <> 'trust_rule'",
    )
    .bind(policy_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %policy_id, "Failed to load policy for duplication");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    let source_name: String = source.get("name");
    let name = request
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{source_name} copy"));
    if name.len() > 160 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let existing_names = sqlx::query_scalar::<_, String>(
        "SELECT name FROM tool_policies WHERE company_id = $1",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let names = existing_names.into_iter().collect::<HashSet<_>>();
    let mut unique_name = name.clone();
    if names.contains(&unique_name) {
        let mut suffix = 2;
        while names.contains(&format!("{name} {suffix}")) {
            suffix += 1;
        }
        unique_name = format!("{name} {suffix}");
    }
    let new_id = Uuid::new_v4();
    let row = sqlx::query(
        "INSERT INTO tool_policies
            (id, company_id, name, description, policy_type, priority, enabled,
             selectors, conditions, config, created_by_user_id)
         VALUES ($1, $2, $3, $4, $5, $6 + 1, false, $7, $8, $9, $10)
         RETURNING id, name, description, policy_type, priority, enabled,
                   selectors, conditions, config, created_at, updated_at",
    )
    .bind(new_id)
    .bind(company_id)
    .bind(&unique_name)
    .bind(source.get::<Option<String>, _>("description"))
    .bind(source.get::<String, _>("policy_type"))
    .bind(source.get::<i32, _>("priority"))
    .bind(source.get::<Value, _>("selectors"))
    .bind(source.get::<Option<Value>, _>("conditions"))
    .bind(source.get::<Option<Value>, _>("config"))
    .bind(match &actor {
        AuthorizationActor::Board { user_id, .. } => Some(user_id.to_string()),
        _ => None,
    })
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %policy_id, "Failed to duplicate policy");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": company_id,
        "name": row.get::<String, _>("name"),
        "description": row.get::<Option<String>, _>("description"),
        "policyType": row.get::<String, _>("policy_type"),
        "priority": row.get::<i32, _>("priority"),
        "enabled": row.get::<bool, _>("enabled"),
        "selectors": row.get::<Value, _>("selectors"),
        "conditions": row.get::<Option<Value>, _>("conditions"),
        "config": row.get::<Option<Value>, _>("config"),
        "duplicatedFrom": policy_id,
        "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
        "updatedAt": row.get::<DateTime<Utc>, _>("updated_at"),
    })))
}

#[derive(Debug, Deserialize)]
struct ReorderToolPoliciesRequest {
    #[serde(rename = "policyIds", alias = "policy_ids")]
    policy_ids: Vec<Uuid>,
}

/// POST /companies/:cid/tools/policies/reorder —— 持久化策略优先级。
async fn reorder_tool_policies(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<ReorderToolPoliciesRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if request.policy_ids.is_empty()
        || request.policy_ids.len() > 500
        || request.policy_ids.iter().collect::<HashSet<_>>().len() != request.policy_ids.len()
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id FROM tool_policies
          WHERE company_id = $1 AND policy_type <> 'trust_rule'",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let all_ids = rows
        .iter()
        .map(|row| row.get::<Uuid, _>("id"))
        .collect::<HashSet<_>>();
    let requested_ids = request.policy_ids.iter().copied().collect::<HashSet<_>>();
    if all_ids != requested_ids {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mut transaction = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for (index, policy_id) in request.policy_ids.iter().enumerate() {
        sqlx::query(
            "UPDATE tool_policies SET priority = $3, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(policy_id)
        .bind(company_id)
        .bind((index as i32 + 1) * 100)
        .execute(&mut *transaction)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    transaction
        .commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let policies = sqlx::query(
        "SELECT id, company_id, name, description, policy_type, priority, enabled,
                selectors, conditions, config, created_at, updated_at
           FROM tool_policies
          WHERE company_id = $1 AND policy_type <> 'trust_rule'
          ORDER BY priority ASC, updated_at DESC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .iter()
    .map(|row| {
        json!({
            "id": row.get::<Uuid, _>("id"),
            "companyId": row.get::<Uuid, _>("company_id"),
            "name": row.get::<String, _>("name"),
            "description": row.get::<Option<String>, _>("description"),
            "policyType": row.get::<String, _>("policy_type"),
            "priority": row.get::<i32, _>("priority"),
            "enabled": row.get::<bool, _>("enabled"),
            "selectors": row.get::<Value, _>("selectors"),
            "conditions": row.get::<Option<Value>, _>("conditions"),
            "config": row.get::<Option<Value>, _>("config"),
            "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
            "updatedAt": row.get::<DateTime<Utc>, _>("updated_at"),
        })
    })
    .collect::<Vec<_>>();
    Ok(Json(json!({ "policies": policies })))
}

/// POST /companies/:cid/tools/policy/test
///
/// Runs Paperclip's decision ladder (block → rate_limit → trust_rule →
/// require_approval → allow → explicit grant → effective profile → default
/// deny) over the company's enabled policies for the requested tool.
async fn test_tool_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let tool_name = body
        .get("toolName")
        .or_else(|| body.get("tool_name"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;

    let rows = sqlx::query(
        "SELECT id, policy_type, selectors, conditions, config, description \
           FROM tool_policies \
          WHERE company_id = $1 AND enabled = true \
          ORDER BY priority ASC, created_at ASC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load tool policies: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let policies: Vec<services::tool_access_contract::PolicySpec> = rows
        .iter()
        .map(|row| {
            let id: Uuid = row.get("id");
            let selectors: Value = row.try_get("selectors").unwrap_or(json!({}));
            // Paperclip selectorMatches narrows by toolName when present.
            let selector_tool_name = selectors
                .get("toolName")
                .and_then(Value::as_str)
                .map(str::to_string);
            let policy_type: String = row.get("policy_type");
            let config: Value = row.try_get("config").unwrap_or(json!({}));
            // Paperclip wraps trust-rule settings under config.trustRule.
            let trust_rule_config = config
                .get("trustRule")
                .or_else(|| config.get("trust_rule"))
                .cloned()
                .filter(|value| value.is_object());
            let rate_limit_exceeded = config
                .get("exceeded")
                .or_else(|| config.get("limited"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            services::tool_access_contract::PolicySpec {
                id: id.to_string(),
                policy_type,
                selector_tool_name,
                description: row.try_get("description").unwrap_or(None),
                trust_rule_config,
                rate_limit_exceeded,
            }
        })
        .collect();

    let arguments = body.get("arguments").cloned();
    let ctx = services::tool_access_contract::EvaluationContext {
        tool_name,
        // Live grant/profile projection is not wired in the test route yet;
        // Paperclip falls through to default deny without them.
        explicit_grant: false,
        profile_allows: false,
        arguments,
        arguments_hash: None,
        catalog_status: None,
        catalog_version_hash: None,
        catalog_schema_hash: None,
        last_rate_limit_state: None,
    };
    let outcome = services::tool_access_contract::decide_tool_access(&policies, &ctx);
    Ok(Json(json!({
        "decision": outcome.decision,
        "reasonCode": outcome.reason_code,
        "message": outcome.message,
        "policyId": outcome.policy_id,
    })))
}

fn empty_json_array() -> Value {
    json!([])
}

fn is_safe_stdio_template_key(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphanumeric())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-'))
}

fn is_safe_env_key(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// POST /companies/:cid/tools/stdio-templates —— 创建管理员 stdio 模板。
#[derive(Debug, Deserialize)]
struct CreateStdioTemplateRequest {
    #[serde(rename = "templateId", alias = "template_key")]
    template_id: String,
    name: String,
    description: Option<String>,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(rename = "envKeys", alias = "env_keys", default)]
    env_keys: Vec<String>,
    #[serde(default = "empty_json_array")]
    tools: Value,
}

async fn create_stdio_template(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateStdioTemplateRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_gateway_permission(&state, &actor, company_id, PermissionKey::TOOLS_ADMIN).await?;
    let template_id = request.template_id.trim().to_string();
    let name = request.name.trim().to_string();
    let command = request.command.trim().to_string();
    let arg_count = request.args.len();
    let env_key_count = request.env_keys.len();
    let tool_count = request.tools.as_array().map_or(0, Vec::len);
    if template_id.is_empty()
        || template_id.len() > 160
        || !is_safe_stdio_template_key(&template_id)
        || name.is_empty()
        || name.len() > 160
        || command.is_empty()
        || command.len() > 2000
        || request.args.len() > 100
        || request.args.iter().any(|arg| arg.len() > 2000)
        || request.env_keys.len() > 200
        || request
            .env_keys
            .iter()
            .any(|key| key.len() > 160 || !is_safe_env_key(key))
        || !request.tools.is_array()
        || request.tools.as_array().is_some_and(|tools| tools.len() > 500)
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    use sqlx::Row;
    let row = sqlx::query(
        "INSERT INTO tool_stdio_command_templates
            (company_id, template_key, name, description, status, command, args, env_keys, tools,
             created_by_agent_id, created_by_user_id)
         VALUES ($1, $2, $3, $4, 'active', $5, $6, $7, $8, $9, $10)
         RETURNING *",
    )
    .bind(company_id)
    .bind(template_id)
    .bind(name)
    .bind(request.description)
    .bind(command)
    .bind(json!(request.args))
    .bind(json!(request.env_keys))
    .bind(request.tools)
    .bind(match &actor {
        AuthorizationActor::Agent { agent_id, .. } => Some(*agent_id),
        _ => None,
    })
    .bind(match &actor {
        AuthorizationActor::Board { user_id, .. } => Some(user_id.to_string()),
        _ => None,
    })
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, "Failed to create stdio template");
        if e.as_database_error()
            .and_then(|database_error| database_error.code())
            .as_deref()
            == Some("23505")
        {
            StatusCode::CONFLICT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    let template = stdio_template_json(&row);
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_stdio_command_template.created",
        &actor,
        "tool_stdio_command_template",
        row.get("id"),
        json!({
            "templateId": template.get("templateId"),
            "command": template.get("command"),
            "argCount": arg_count,
            "envKeyCount": env_key_count,
            "toolCount": tool_count,
        }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(template)))
}

#[derive(Debug, Deserialize, Default)]
struct DisableStdioTemplateRequest {
    reason: Option<String>,
}

/// POST /companies/:cid/tools/stdio-templates/:template_id/disable —— 禁用管理员模板。
async fn disable_stdio_template(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, template_id)): Path<(Uuid, String)>,
    Json(request): Json<DisableStdioTemplateRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_gateway_permission(&state, &actor, company_id, PermissionKey::TOOLS_ADMIN).await?;
    if request.reason.as_deref().is_some_and(|reason| reason.len() > 1000) {
        return Err(StatusCode::BAD_REQUEST);
    }
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE tool_stdio_command_templates
            SET status = 'disabled', disabled_at = COALESCE(disabled_at, NOW()), updated_at = NOW()
          WHERE company_id = $1 AND template_key = $2
         RETURNING *",
    )
    .bind(company_id)
    .bind(template_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, "Failed to disable stdio template");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    let template = stdio_template_json(&row);
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_stdio_command_template.disabled",
        &actor,
        "tool_stdio_command_template",
        row.get("id"),
        json!({
            "templateId": template.get("templateId"),
            "reason": request.reason,
        }),
    )
    .await;
    Ok(Json(template))
}

#[derive(Debug, Deserialize, Default)]
struct TrustRuleScopeRequest {
    #[serde(rename = "includeAgent")]
    include_agent: Option<bool>,
    #[serde(rename = "includeProject")]
    include_project: Option<bool>,
    #[serde(rename = "includeIssue")]
    include_issue: Option<bool>,
    #[serde(rename = "includeApplication")]
    include_application: Option<bool>,
    #[serde(rename = "includeConnection")]
    include_connection: Option<bool>,
    #[serde(rename = "includeCatalogEntry")]
    include_catalog_entry: Option<bool>,
    #[serde(rename = "includeTool")]
    include_tool: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct CreateTrustRuleRequest {
    name: Option<String>,
    description: Option<String>,
    priority: Option<i32>,
    #[serde(rename = "approvalThreshold")]
    approval_threshold: Option<i32>,
    selectors: Option<Value>,
    scope: Option<TrustRuleScopeRequest>,
    #[serde(rename = "argumentFilters")]
    argument_filters: Option<Value>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<String>,
    #[serde(rename = "batchApproval")]
    batch_approval: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct RevokeTrustRuleRequest {
    reason: Option<String>,
}

const TRUST_RULE_REQUIRED_SELECTOR_KEYS: &[&str] = &[
    "agentId",
    "projectId",
    "applicationId",
    "connectionId",
    "toolName",
];
const TRUST_RULE_OPTIONAL_SELECTOR_KEYS: &[&str] = &["issueId", "catalogEntryId"];

fn trust_rule_selector_value(selectors: &Value, key: &str) -> Option<String> {
    selectors
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn validate_trust_rule_argument_filters(
    requested: Option<Value>,
    arguments_hash: &str,
) -> Result<Value, StatusCode> {
    let Some(filters) = requested else {
        return Ok(json!({ "exactHash": arguments_hash }));
    };
    let Some(filters) = filters.as_object() else {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    };
    let allowed_keys = ["allowAny", "exactHash"];
    if filters
        .keys()
        .any(|key| !allowed_keys.iter().any(|allowed| allowed == key))
        || filters
            .get("allowAny")
            .and_then(Value::as_bool)
            == Some(true)
        || filters.contains_key("exactHash")
            && filters.get("exactHash").and_then(Value::as_str) != Some(arguments_hash)
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if filters
        .get("allowAny")
        .is_some_and(|value| !value.is_boolean())
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok(json!({ "exactHash": arguments_hash }))
}

fn validate_trust_rule_batch_approval(requested: Option<Value>) -> Result<Value, StatusCode> {
    let Some(batch) = requested else {
        return Ok(Value::Null);
    };
    if batch.is_null() {
        return Ok(Value::Null);
    }
    let Some(batch_object) = batch.as_object() else {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    };
    let allowed_keys = ["enabled", "maxBatchSize", "windowSeconds"];
    if batch_object
        .keys()
        .any(|key| !allowed_keys.iter().any(|allowed| allowed == key))
        || batch_object
            .get("enabled")
            .is_some_and(|value| !value.is_boolean())
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    for (key, maximum) in [("maxBatchSize", 100_i64), ("windowSeconds", 31_536_000_i64)] {
        if let Some(value) = batch_object.get(key) {
            let Some(number) = value.as_i64() else {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            };
            if number <= 0 || number > maximum {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
        }
    }
    Ok(batch)
}

/// POST /companies/:cid/tools/action-requests/:id/trust-rule —— 将已审批调用提升为精确 trust rule。
async fn create_trust_rule_from_action_request(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, action_request_id)): Path<(Uuid, Uuid)>,
    request: Option<Json<CreateTrustRuleRequest>>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let request = request.map(|Json(value)| value).unwrap_or_default();
    let priority = request.priority.unwrap_or(40);
    let approval_threshold = request.approval_threshold.unwrap_or(2);
    if !(0..=10_000).contains(&priority) || !(1..=50).contains(&approval_threshold) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if request
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty() || name.chars().count() > 160)
        || request
            .description
            .as_deref()
            .is_some_and(|description| description.chars().count() > 4000)
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let expires_at = request
        .expires_at
        .as_deref()
        .map(|value| {
            DateTime::parse_from_rfc3339(value.trim())
                .map(|date| date.with_timezone(&Utc))
                .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)
        })
        .transpose()?;
    let batch_approval = validate_trust_rule_batch_approval(request.batch_approval.clone())?;

    use sqlx::Row;
    let source = sqlx::query(
        "SELECT ar.status AS action_status, ar.invocation_id,
                COALESCE(i.agent_id, ar.requested_by_agent_id) AS agent_id,
                COALESCE(i.issue_id, ar.issue_id) AS issue_id,
                i.run_id, i.application_id, i.connection_id, i.catalog_entry_id,
                i.tool_name, i.arguments_hash, i.arguments_summary,
                i.policy_decision,
                c.version_hash AS catalog_version_hash,
                c.schema_hash AS catalog_schema_hash,
                issue.project_id AS project_id
           FROM tool_action_requests ar
           JOIN tool_invocations i
             ON i.id = ar.invocation_id AND i.company_id = ar.company_id
           LEFT JOIN tool_catalog_entries c
             ON c.id = i.catalog_entry_id AND c.company_id = i.company_id
           LEFT JOIN issues issue
             ON issue.id = COALESCE(i.issue_id, ar.issue_id)
            AND issue.company_id = ar.company_id
          WHERE ar.id = $1 AND ar.company_id = $2",
    )
    .bind(action_request_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %action_request_id, "Failed to load trust rule source action request");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let action_status: String = source.get("action_status");
    if !matches!(action_status.as_str(), "approved" | "executed") {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let policy_decision: Option<String> = source.get("policy_decision");
    if policy_decision.as_deref() != Some("require_approval") {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let arguments_hash: String = source
        .get::<Option<String>, _>("arguments_hash")
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let catalog_entry_id: Option<Uuid> = source.get("catalog_entry_id");
    let catalog_version_hash: Option<String> = source.get("catalog_version_hash");
    let catalog_schema_hash: Option<String> = source.get("catalog_schema_hash");
    if catalog_entry_id.is_some() && catalog_version_hash.is_none() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let argument_filters = validate_trust_rule_argument_filters(
        request.argument_filters,
        &arguments_hash,
    )?;

    let source_agent_id: Option<Uuid> = source.get("agent_id");
    let source_project_id: Option<Uuid> = source.get("project_id");
    let source_issue_id: Option<Uuid> = source.get("issue_id");
    let source_application_id: Option<Uuid> = source.get("application_id");
    let source_connection_id: Option<Uuid> = source.get("connection_id");
    let source_tool_name: String = source.get("tool_name");
    let mut selectors = request.selectors.unwrap_or_else(|| json!({}));
    if !selectors.is_object() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let scope = request.scope.unwrap_or_default();
    let selector_defaults = [
        (
            "agentId",
            source_agent_id.map(|value| value.to_string()),
            scope.include_agent.unwrap_or(true),
        ),
        (
            "projectId",
            source_project_id.map(|value| value.to_string()),
            scope.include_project.unwrap_or(true),
        ),
        (
            "issueId",
            source_issue_id.map(|value| value.to_string()),
            scope.include_issue.unwrap_or(false),
        ),
        (
            "applicationId",
            source_application_id.map(|value| value.to_string()),
            scope.include_application.unwrap_or(true),
        ),
        (
            "connectionId",
            source_connection_id.map(|value| value.to_string()),
            scope.include_connection.unwrap_or(true),
        ),
        (
            "catalogEntryId",
            catalog_entry_id.map(|value| value.to_string()),
            scope.include_catalog_entry.unwrap_or(false),
        ),
        (
            "toolName",
            Some(source_tool_name.clone()),
            scope.include_tool.unwrap_or(true),
        ),
    ];
    if let Some(selectors_object) = selectors.as_object_mut() {
        for (key, value, enabled) in selector_defaults {
            if enabled && value.is_some() && !selectors_object.contains_key(key) {
                if let Some(value) = value {
                    selectors_object.insert(key.to_string(), Value::String(value));
                }
            }
        }
    }
    let selectors_object = selectors
        .as_object()
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let allowed_selector_keys = TRUST_RULE_REQUIRED_SELECTOR_KEYS
        .iter()
        .chain(TRUST_RULE_OPTIONAL_SELECTOR_KEYS.iter())
        .copied()
        .collect::<HashSet<_>>();
    for (key, value) in selectors_object {
        if !allowed_selector_keys.contains(key.as_str())
            || value.as_str().is_none()
        {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        let expected = match key.as_str() {
            "agentId" => source_agent_id.map(|value| value.to_string()),
            "projectId" => source_project_id.map(|value| value.to_string()),
            "issueId" => source_issue_id.map(|value| value.to_string()),
            "applicationId" => source_application_id.map(|value| value.to_string()),
            "connectionId" => source_connection_id.map(|value| value.to_string()),
            "catalogEntryId" => catalog_entry_id.map(|value| value.to_string()),
            "toolName" => Some(source_tool_name.clone()),
            _ => None,
        };
        if expected.as_deref() != value.as_str() {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
    for key in TRUST_RULE_REQUIRED_SELECTOR_KEYS {
        let expected = match *key {
            "agentId" => source_agent_id.map(|value| value.to_string()),
            "projectId" => source_project_id.map(|value| value.to_string()),
            "applicationId" => source_application_id.map(|value| value.to_string()),
            "connectionId" => source_connection_id.map(|value| value.to_string()),
            "toolName" => Some(source_tool_name.clone()),
            _ => None,
        };
        if expected.is_some() && trust_rule_selector_value(&selectors, key).as_deref() != expected.as_deref() {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    let selected_agent_id = trust_rule_selector_value(&selectors, "agentId")
        .map(|value| Uuid::parse_str(&value).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY))
        .transpose()?;
    let selected_project_id = trust_rule_selector_value(&selectors, "projectId")
        .map(|value| Uuid::parse_str(&value).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY))
        .transpose()?;
    let selected_issue_id = trust_rule_selector_value(&selectors, "issueId")
        .map(|value| Uuid::parse_str(&value).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY))
        .transpose()?;
    let approved_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint
           FROM tool_action_requests ar
           JOIN tool_invocations i
             ON i.id = ar.invocation_id AND i.company_id = ar.company_id
           LEFT JOIN tool_catalog_entries c
             ON c.id = i.catalog_entry_id AND c.company_id = i.company_id
           LEFT JOIN issues issue
             ON issue.id = COALESCE(i.issue_id, ar.issue_id)
            AND issue.company_id = ar.company_id
          WHERE ar.company_id = $1
            AND ar.status IN ('approved', 'executed')
            AND i.policy_decision = 'require_approval'
            AND i.tool_name = $2
            AND i.arguments_hash = $3
            AND i.agent_id IS NOT DISTINCT FROM $4
            AND issue.project_id IS NOT DISTINCT FROM $5
            AND COALESCE(i.issue_id, ar.issue_id) IS NOT DISTINCT FROM $6
            AND i.application_id IS NOT DISTINCT FROM $7
            AND i.connection_id IS NOT DISTINCT FROM $8
            AND i.catalog_entry_id IS NOT DISTINCT FROM $9
            AND c.version_hash IS NOT DISTINCT FROM $10
            AND c.schema_hash IS NOT DISTINCT FROM $11",
    )
    .bind(company_id)
    .bind(&source_tool_name)
    .bind(&arguments_hash)
    .bind(selected_agent_id)
    .bind(selected_project_id)
    .bind(selected_issue_id)
    .bind(source_application_id)
    .bind(source_connection_id)
    .bind(catalog_entry_id)
    .bind(&catalog_version_hash)
    .bind(&catalog_schema_hash)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %action_request_id, "Failed to count matching approved tool actions");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if approved_count < i64::from(approval_threshold) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let name = request
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("Trust {source_tool_name} {}", action_request_id.simple()));
    let description = request.description.or_else(|| {
        Some(format!(
            "Progressive-autonomy trust rule promoted from action request {action_request_id}."
        ))
    });
    let trust_rule_config = json!({
        "sourceActionRequestId": action_request_id,
        "sourceInvocationId": source.get::<Uuid, _>("invocation_id"),
        "sourceApprovalCount": approved_count,
        "approvalThreshold": approval_threshold,
        "argumentFilters": argument_filters.clone(),
        "expiresAt": expires_at.clone(),
        "revokedAt": Value::Null,
        "hitCount": 0,
        "lastHitAt": Value::Null,
        "catalogVersionHash": catalog_version_hash,
        "schemaHash": catalog_schema_hash,
        "batchApproval": batch_approval.clone(),
    });
    let config = json!({ "trustRule": trust_rule_config });
    let user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => Some(user_id.to_string()),
        _ => None,
    };
    let policy = sqlx::query(
        "INSERT INTO tool_policies
            (company_id, name, description, policy_type, priority, enabled,
             selectors, conditions, config, created_by_user_id)
         VALUES ($1, $2, $3, 'trust_rule', $4, true, $5, NULL, $6, $7)
         RETURNING id, company_id, name, description, policy_type, priority,
                   enabled, selectors, conditions, config, created_at, updated_at",
    )
    .bind(company_id)
    .bind(&name)
    .bind(description.as_deref())
    .bind(priority)
    .bind(&selectors)
    .bind(&config)
    .bind(user_id.as_deref())
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %action_request_id, "Failed to create promoted trust rule");
        if is_unique_violation(&error) {
            StatusCode::CONFLICT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    let policy_id: Uuid = policy.get("id");
    let invocation_id: Uuid = source.get("invocation_id");
    let source_agent_id: Option<Uuid> = source.get("agent_id");
    let source_run_id: Option<Uuid> = source.get("run_id");
    let source_issue_id: Option<Uuid> = source.get("issue_id");
    let event_metadata = json!({
        "source": "trust_rule_promotion",
        "approvalThreshold": approval_threshold,
        "approvedCount": approved_count,
        "selectors": selectors.clone(),
        "argumentFilters": argument_filters.clone(),
        "expiresAt": expires_at,
    });
    let _ = sqlx::query(
        "INSERT INTO tool_access_audit_events
            (company_id, connection_id, catalog_entry_id, actor_type, actor_id,
             action, outcome, reason_code, correlation_id, details)
         VALUES ($1, $2, $3, 'user', $4, 'tool_access.trust_rule_created',
                 'success', 'trust_rule_promoted_from_approval', $5, $6)",
    )
    .bind(company_id)
    .bind(source_connection_id)
    .bind(catalog_entry_id)
    .bind(user_id.as_deref())
    .bind(invocation_id.to_string())
    .bind(&event_metadata)
    .execute(&state.pool)
    .await;
    let _ = sqlx::query(
        "INSERT INTO tool_call_events
            (company_id, event_type, actor_type, actor_id, agent_id, run_id,
             issue_id, application_id, connection_id, catalog_entry_id,
             invocation_id, action_request_id, tool_name, decision,
             matched_policy_ids, reason_code, outcome, arguments_summary,
             request_hash, request_summary, metadata)
         VALUES ($1, 'trust_rule_created', 'user', $2, $3, $4, $5, $6, $7,
                 $8, $9, $10, $11, 'allow', $12,
                 'trust_rule_promoted_from_approval', 'success', $13, $14,
                 $13, $15)",
    )
    .bind(company_id)
    .bind(user_id.as_deref())
    .bind(source_agent_id)
    .bind(source_run_id)
    .bind(source_issue_id)
    .bind(source_application_id)
    .bind(source_connection_id)
    .bind(catalog_entry_id)
    .bind(invocation_id)
    .bind(action_request_id)
    .bind(&source_tool_name)
    .bind(json!([policy_id]))
    .bind(source.get::<Option<Value>, _>("arguments_summary"))
    .bind(&arguments_hash)
    .bind(&event_metadata)
    .execute(&state.pool)
    .await;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_trust_rule.created",
        &actor,
        "tool_policy",
        policy_id,
        json!({
            "actionRequestId": action_request_id,
            "invocationId": invocation_id,
            "approvalThreshold": approval_threshold,
            "approvedCount": approved_count,
        }),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": policy.get::<Uuid, _>("id"),
            "companyId": policy.get::<Uuid, _>("company_id"),
            "name": policy.get::<String, _>("name"),
            "description": policy.get::<Option<String>, _>("description"),
            "policyType": policy.get::<String, _>("policy_type"),
            "priority": policy.get::<i32, _>("priority"),
            "enabled": policy.get::<bool, _>("enabled"),
            "selectors": policy.get::<Value, _>("selectors"),
            "conditions": policy.get::<Option<Value>, _>("conditions"),
            "config": policy.get::<Option<Value>, _>("config"),
            "createdAt": policy.get::<DateTime<Utc>, _>("created_at"),
            "updatedAt": policy.get::<DateTime<Utc>, _>("updated_at"),
        })),
    ))
}

/// POST /companies/:cid/tools/trust-rules/:rule_id/revoke —— 撤销 trust rule。
async fn revoke_trust_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, rule_id)): Path<(Uuid, Uuid)>,
    request: Option<Json<RevokeTrustRuleRequest>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let request = request.map(|Json(value)| value).unwrap_or_default();
    if request.reason.as_deref().is_some_and(|reason| reason.len() > 1000) {
        return Err(StatusCode::BAD_REQUEST);
    }
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT name, description, policy_type, priority, selectors, conditions, config
           FROM tool_policies
          WHERE id = $1 AND company_id = $2 AND policy_type = 'trust_rule'",
    )
    .bind(rule_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let mut config = row
        .get::<Option<Value>, _>("config")
        .unwrap_or_else(|| json!({}));
    let config_object = config.as_object_mut().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut trust_rule = config_object
        .get("trustRule")
        .or_else(|| config_object.get("trust_rule"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let trust_rule_object = trust_rule
        .as_object_mut()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    trust_rule_object.insert(
        "revokedAt".to_string(),
        json!(Utc::now()),
    );
    trust_rule_object.insert(
        "revocationReason".to_string(),
        request.reason.clone().map_or(Value::Null, Value::String),
    );
    config_object.insert("trustRule".to_string(), trust_rule);
    let updated = sqlx::query(
        "UPDATE tool_policies
            SET enabled = false, config = $3, updated_at = NOW()
          WHERE id = $1 AND company_id = $2
         RETURNING id, name, description, policy_type, priority, enabled,
                   selectors, conditions, config, created_at, updated_at",
    )
    .bind(rule_id)
    .bind(company_id)
    .bind(&config)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_trust_rule.revoked",
        &actor,
        "tool_policy",
        rule_id,
        json!({ "reason": request.reason }),
    )
    .await;
    Ok(Json(json!({
        "id": updated.get::<Uuid, _>("id"),
        "companyId": company_id,
        "name": updated.get::<String, _>("name"),
        "description": updated.get::<Option<String>, _>("description"),
        "policyType": updated.get::<String, _>("policy_type"),
        "priority": updated.get::<i32, _>("priority"),
        "enabled": updated.get::<bool, _>("enabled"),
        "selectors": updated.get::<Value, _>("selectors"),
        "conditions": updated.get::<Option<Value>, _>("conditions"),
        "config": updated.get::<Option<Value>, _>("config"),
        "createdAt": updated.get::<DateTime<Utc>, _>("created_at"),
        "updatedAt": updated.get::<DateTime<Utc>, _>("updated_at"),
    })))
}

async fn update_runtime_slot_state(
    state: &AppState,
    company_id: Uuid,
    slot_id: &str,
    next_status: &str,
) -> Result<Value, StatusCode> {
    let slot_id = Uuid::parse_str(slot_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let row = if next_status == "stopped" {
        sqlx::query(
            "UPDATE tool_runtime_slots
                SET status = 'stopped', stopped_at = NOW(), process_id = NULL,
                    last_error = NULL, updated_at = NOW()
              WHERE id = $1 AND company_id = $2
             RETURNING id, connection_id, slot_key, runtime_kind, status,
                       health_status, process_id, last_error, last_used_at,
                       updated_at",
        )
        .bind(slot_id)
        .bind(company_id)
        .fetch_optional(&state.pool)
        .await
    } else {
        sqlx::query(
            "UPDATE tool_runtime_slots
                SET status = 'running', started_at = NOW(), stopped_at = NULL,
                    last_error = NULL, updated_at = NOW()
              WHERE id = $1 AND company_id = $2
             RETURNING id, connection_id, slot_key, runtime_kind, status,
                       health_status, process_id, last_error, last_used_at,
                       updated_at",
        )
        .bind(slot_id)
        .bind(company_id)
        .fetch_optional(&state.pool)
        .await
    }
    .map_err(|error| {
        tracing::error!(%error, %slot_id, "Failed to update MCP runtime slot");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    use sqlx::Row;
    Ok(json!({
        "id": row.get::<Uuid, _>("id"),
        "connectionId": row.get::<Option<Uuid>, _>("connection_id"),
        "slotKey": row.get::<String, _>("slot_key"),
        "runtimeKind": row.get::<String, _>("runtime_kind"),
        "status": row.get::<String, _>("status"),
        "healthStatus": row.get::<String, _>("health_status"),
        "processId": row.get::<Option<i32>, _>("process_id"),
        "lastError": row.get::<Option<String>, _>("last_error"),
        "lastUsedAt": row.get::<Option<DateTime<Utc>>, _>("last_used_at"),
        "updatedAt": row.get::<DateTime<Utc>, _>("updated_at"),
    }))
}

/// POST /companies/:cid/tools/runtime-slots/:slot_id/stop|restart —— 更新运行时槽位。
async fn stop_runtime_slot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, slot_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
    Ok(Json(update_runtime_slot_state(&state, company_id, &slot_id, "stopped").await?))
}
async fn restart_runtime_slot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, slot_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
    Ok(Json(update_runtime_slot_state(&state, company_id, &slot_id, "running").await?))
}

fn connection_config_roots(
    row: &sqlx::postgres::PgRow,
    sections: &[&str],
) -> Vec<Value> {
    use sqlx::Row;
    let configs = [
        row.get::<Option<Value>, _>("config")
            .unwrap_or_else(|| json!({})),
        row.get::<Option<Value>, _>("transport_config")
            .unwrap_or_else(|| json!({})),
    ];
    let mut roots = Vec::with_capacity(configs.len() * (sections.len() + 1));
    for config in &configs {
        for section in sections {
            if let Some(value) = config.get(*section) {
                roots.push(value.clone());
            }
        }
        roots.push(config.clone());
    }
    roots
}

fn connection_config_string_in_sections(
    row: &sqlx::postgres::PgRow,
    sections: &[&str],
    keys: &[&str],
) -> Option<String> {
    connection_config_roots(row, sections)
        .iter()
        .find_map(|root| {
            keys.iter().find_map(|key| {
                root.get(*key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            })
        })
}

fn connection_config_string(row: &sqlx::postgres::PgRow, keys: &[&str]) -> Option<String> {
    connection_config_string_in_sections(row, &["oauth"], keys)
}

fn config_scope_values(value: &Value) -> Vec<String> {
    match value {
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        Value::String(value) => value
            .split_whitespace()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn connection_config_scope_strings(
    row: &sqlx::postgres::PgRow,
    sections: &[&str],
    keys: &[&str],
) -> Vec<String> {
    connection_config_roots(row, sections)
        .iter()
        .find_map(|root| {
            keys.iter().find_map(|key| {
                let values = root.get(*key).map(config_scope_values)?;
                (!values.is_empty()).then_some(values)
            })
        })
        .unwrap_or_default()
}

fn connection_config_strings(row: &sqlx::postgres::PgRow, keys: &[&str]) -> Vec<String> {
    connection_config_scope_strings(row, &["oauth"], keys)
}

fn connection_config_bool_in_sections(
    row: &sqlx::postgres::PgRow,
    sections: &[&str],
    keys: &[&str],
) -> Option<bool> {
    connection_config_roots(row, sections)
        .iter()
        .find_map(|root| keys.iter().find_map(|key| root.get(*key).and_then(Value::as_bool)))
}

fn connection_config_i64_in_sections(
    row: &sqlx::postgres::PgRow,
    sections: &[&str],
    keys: &[&str],
) -> Option<i64> {
    connection_config_roots(row, sections)
        .iter()
        .find_map(|root| {
            keys.iter().find_map(|key| {
                root.get(*key).and_then(|value| match value {
                    Value::Number(value) => value.as_i64(),
                    Value::String(value) => value.trim().parse().ok(),
                    _ => None,
                })
            })
        })
}

fn normalize_token_scopes(input: Option<AgentConnectionTokenScope>) -> Result<Vec<String>, StatusCode> {
    let values = match input {
        None => Vec::new(),
        Some(AgentConnectionTokenScope::String(value)) => value
            .split_whitespace()
            .map(ToOwned::to_owned)
            .collect(),
        Some(AgentConnectionTokenScope::Array(values)) => values,
    };
    if values.len() > 100
        || values
            .iter()
            .any(|value| value.trim().is_empty() || value.trim().chars().count() > 240)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut unique = Vec::with_capacity(values.len());
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        let value = value.trim().to_string();
        if seen.insert(value.clone()) {
            unique.push(value);
        }
    }
    Ok(unique)
}

fn connection_token_path(row: &sqlx::postgres::PgRow) -> &'static str {
    let sections = &["tokenBroker", "token_broker", "broker"];
    if let Some(path) = connection_config_string_in_sections(
        row,
        sections,
        &["path", "tokenPath", "token_path"],
    ) {
        return match path.as_str() {
            "exchange" => "exchange",
            "oauth_access" | "oauthAccess" => "oauth_access",
            "static" => "static",
            _ => "static",
        };
    }
    if connection_config_string_in_sections(row, sections, &["tokenUrl", "token_url"])
        .is_some()
        || connection_config_string_in_sections(row, &[], &["tokenExchangeUrl", "token_exchange_url"])
            .is_some()
    {
        "exchange"
    } else {
        "static"
    }
}

fn connection_token_parent_scopes(row: &sqlx::postgres::PgRow) -> Vec<String> {
    connection_config_scope_strings(
        row,
        &["tokenBroker", "token_broker", "broker", "oauth"],
        &["parentScopes", "parent_scopes", "scopes", "scope"],
    )
}

fn connection_token_default_scopes(row: &sqlx::postgres::PgRow) -> Vec<String> {
    connection_config_scope_strings(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["defaultScopes", "default_scopes"],
    )
}

fn token_scopes_subset(requested: &[String], parent: &[String]) -> bool {
    requested.is_empty() || (!parent.is_empty() && requested.iter().all(|scope| parent.contains(scope)))
}

fn bounded_token_ttl(row: &sqlx::postgres::PgRow, requested: Option<i64>) -> Result<i32, StatusCode> {
    if requested.is_some_and(|value| !(1..=86_400).contains(&value)) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let configured = connection_config_i64_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["defaultTtlSeconds", "default_ttl_seconds", "ttlSeconds", "ttl_seconds"],
    )
    .unwrap_or(900);
    Ok(requested
        .unwrap_or(configured)
        .clamp(1, 900) as i32)
}

#[derive(Debug)]
struct ConnectionTokenExchangeError {
    status: StatusCode,
    outcome: &'static str,
    code: &'static str,
    metadata: Value,
}

impl ConnectionTokenExchangeError {
    fn configuration(code: &'static str) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            outcome: "failure",
            code,
            metadata: json!({}),
        }
    }

    fn credential(code: &'static str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            outcome: "denied",
            code,
            metadata: json!({}),
        }
    }

    fn internal(code: &'static str) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            outcome: "failure",
            code,
            metadata: json!({}),
        }
    }

    fn upstream(status: StatusCode, code: &'static str, metadata: Value) -> Self {
        Self {
            status,
            outcome: "upstream_error",
            code,
            metadata,
        }
    }
}

#[derive(Debug)]
struct BrokerTokenExchange {
    token: String,
    token_type: String,
    expires_at: DateTime<Utc>,
    scope: Vec<String>,
}

fn broker_reference_secret_id(reference: &Value) -> Option<Uuid> {
    reference
        .get("secretId")
        .or_else(|| reference.get("secret_id"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn broker_reference_version(reference: &Value) -> String {
    reference
        .get("versionSelector")
        .or_else(|| reference.get("version_selector"))
        .or_else(|| reference.get("version"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string()
}

fn broker_reference_name(reference: &Value) -> Option<&str> {
    reference
        .get("name")
        .or_else(|| reference.get("key"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn is_oauth_credential_path(path: &str) -> bool {
    matches!(
        path.to_ascii_lowercase().as_str(),
        "oauth.access_token"
            | "oauth.access-token"
            | "oauth.accesstoken"
            | "oauth.refresh_token"
            | "oauth.refresh-token"
            | "oauth.refreshtoken"
            | "oauth.client_secret"
            | "oauth.client-secret"
            | "oauth.clientsecret"
    )
}

fn is_broker_secret_reference(reference: &Value) -> bool {
    broker_reference_secret_id(reference).is_some()
        && !credential_ref_path(reference).is_some_and(is_oauth_credential_path)
}

fn broker_parent_reference(
    row: &sqlx::postgres::PgRow,
    grant_secret_refs: &Value,
    subject_user_id: Option<&str>,
) -> Option<Value> {
    use sqlx::Row;
    let grant_refs = grant_secret_refs.as_array().cloned().unwrap_or_default();
    let secret_refs = if !grant_refs.is_empty() {
        grant_refs
    } else if subject_user_id.is_none() {
        row.get::<Option<Value>, _>("credential_secret_refs")
            .unwrap_or_else(|| json!([]))
            .as_array()
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let configured_path = connection_config_string_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["parentCredentialConfigPath", "credentialConfigPath", "secretConfigPath"],
    );
    if let Some(configured_path) = configured_path.as_deref() {
        if let Some(reference) = secret_refs.iter().find(|reference| {
            is_broker_secret_reference(reference)
                && credential_ref_path(reference) == Some(configured_path)
        }) {
            return Some(reference.clone());
        }
    } else {
        for preferred_path in ["credentials.deploy_token", "pages.deploy_token"] {
            if let Some(reference) = secret_refs.iter().find(|reference| {
                is_broker_secret_reference(reference)
                    && credential_ref_path(reference) == Some(preferred_path)
            }) {
                return Some(reference.clone());
            }
        }
        if let Some(reference) = secret_refs
            .iter()
            .find(|reference| is_broker_secret_reference(reference))
        {
            return Some(reference.clone());
        }
    }

    if subject_user_id.is_some() {
        return None;
    }
    let configured_name = connection_config_string_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["parentCredentialName", "credentialName"],
    );
    let credential_refs = row
        .get::<Option<Value>, _>("credential_refs")
        .unwrap_or_else(|| json!([]));
    let credential_refs = credential_refs.as_array()?;
    if let Some(configured_name) = configured_name.as_deref() {
        credential_refs
            .iter()
            .find(|reference| {
                broker_reference_secret_id(reference).is_some()
                    && broker_reference_name(reference) == Some(configured_name)
            })
            .cloned()
    } else {
        credential_refs
            .iter()
            .find(|reference| broker_reference_secret_id(reference).is_some())
            .cloned()
    }
}

async fn resolve_broker_parent_credential(
    pool: &sqlx::PgPool,
    row: &sqlx::postgres::PgRow,
    grant_secret_refs: &Value,
    subject_user_id: Option<&str>,
) -> Result<String, ConnectionTokenExchangeError> {
    use sqlx::Row;
    let reference = broker_parent_reference(row, grant_secret_refs, subject_user_id)
        .ok_or_else(|| ConnectionTokenExchangeError::configuration("parent_credential_missing"))?;
    let secret_id = broker_reference_secret_id(&reference)
        .ok_or_else(|| ConnectionTokenExchangeError::configuration("parent_credential_invalid"))?;
    let version = broker_reference_version(&reference);
    let secret_row = if version.eq_ignore_ascii_case("latest") {
        sqlx::query(
            "SELECT v.material, s.provider
               FROM company_secret_versions v
               JOIN company_secrets s ON s.id = v.secret_id
              WHERE s.id = $1 AND s.company_id = $2
                AND s.status = 'active' AND s.deleted_at IS NULL
                AND v.status = 'current' AND v.revoked_at IS NULL
              ORDER BY v.version DESC
              LIMIT 1",
        )
        .bind(secret_id)
        .bind(row.get::<Uuid, _>("company_id"))
        .fetch_optional(pool)
        .await
    } else {
        let version_number = version
            .parse::<i32>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| ConnectionTokenExchangeError::configuration("parent_credential_version_invalid"))?;
        sqlx::query(
            "SELECT v.material, s.provider
               FROM company_secret_versions v
               JOIN company_secrets s ON s.id = v.secret_id
              WHERE s.id = $1 AND s.company_id = $2
                AND s.status = 'active' AND s.deleted_at IS NULL
                AND v.version = $3 AND v.revoked_at IS NULL
              LIMIT 1",
        )
        .bind(secret_id)
        .bind(row.get::<Uuid, _>("company_id"))
        .bind(version_number)
        .fetch_optional(pool)
        .await
    }
    .map_err(|error| {
        tracing::error!(%error, %secret_id, "Failed to resolve broker parent credential");
        ConnectionTokenExchangeError::internal("parent_credential_resolution_failed")
    })?;
    let Some(secret_row) = secret_row else {
        return Err(ConnectionTokenExchangeError::credential("credential_revoked"));
    };
    let provider: String = secret_row.get("provider");
    if provider != "local_encrypted" && provider != "local" {
        return Err(ConnectionTokenExchangeError::configuration(
            "parent_credential_provider_unsupported",
        ));
    }
    let material: Value = secret_row.get("material");
    let value = decrypt_secret_material(&material).map_err(|error| {
        tracing::warn!(%error, %secret_id, "Failed to decrypt broker parent credential");
        ConnectionTokenExchangeError::credential("credential_revoked")
    })?;
    if value.trim().is_empty() {
        return Err(ConnectionTokenExchangeError::credential("credential_revoked"));
    }
    Ok(value)
}

fn broker_exchange_url(row: &sqlx::postgres::PgRow) -> Result<Url, ConnectionTokenExchangeError> {
    let configured = connection_config_string_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["tokenUrl", "token_url", "exchangeTokenUrl", "exchange_token_url"],
    )
    .or_else(|| {
        connection_config_string_in_sections(
            row,
            &[],
            &["tokenExchangeUrl", "token_exchange_url", "pagesTokenExchangeUrl"],
        )
    })
    .ok_or_else(|| ConnectionTokenExchangeError::configuration("exchange_url_missing"))?;
    validate_oauth_endpoint(&configured)
        .map_err(|_| ConnectionTokenExchangeError::configuration("exchange_url_invalid"))
}

fn broker_response_expires_at(
    payload: &Value,
    now: DateTime<Utc>,
    ttl_seconds: i32,
) -> Result<DateTime<Utc>, ConnectionTokenExchangeError> {
    let configured = payload
        .get("expiresAt")
        .or_else(|| payload.get("expires_at"))
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    let expires_in = payload
        .get("expiresIn")
        .or_else(|| payload.get("expires_in"))
        .and_then(|value| match value {
            Value::Number(value) => value.as_i64(),
            Value::String(value) => value.trim().parse::<i64>().ok(),
            _ => None,
        })
        .filter(|value| *value > 0)
        .map(|value| now + Duration::seconds(value));
    let maximum = now + Duration::seconds(i64::from(ttl_seconds));
    let candidate = configured.or(expires_in).unwrap_or(maximum);
    if candidate <= now {
        return Err(ConnectionTokenExchangeError::upstream(
            StatusCode::BAD_GATEWAY,
            "upstream_token_invalid",
            json!({ "reason": "token_expired" }),
        ));
    }
    let bounded = std::cmp::min(candidate, maximum);
    Ok(std::cmp::max(bounded, now + Duration::seconds(1)))
}

async fn exchange_connection_token(
    row: &sqlx::postgres::PgRow,
    grant_secret_refs: &Value,
    subject_user_id: Option<&str>,
    context: &ActiveAgentRunContext,
    issued_scope: &[String],
    ttl_seconds: i32,
    pool: &sqlx::PgPool,
) -> Result<BrokerTokenExchange, ConnectionTokenExchangeError> {
    use sqlx::Row;
    let parent_token = resolve_broker_parent_credential(
        pool,
        row,
        grant_secret_refs,
        subject_user_id,
    )
    .await?;
    let endpoint = broker_exchange_url(row)?;
    let protocol = connection_config_string_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["protocol", "exchangeProtocol", "exchange_protocol"],
    )
    .unwrap_or_else(|| "generic".to_string())
    .to_ascii_lowercase();
    if !matches!(protocol.as_str(), "generic" | "json" | "pages" | "rfc8693" | "rfc_8693") {
        return Err(ConnectionTokenExchangeError::configuration(
            "exchange_protocol_unsupported",
        ));
    }
    let audience = connection_config_string_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["audience"],
    );
    let actor = json!({
        "type": "agent",
        "id": context.agent_id,
        "runId": context.run_id,
        "onBehalfOf": context.responsible_user_id.as_deref().map(|id| format!("user:{id}")),
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| {
            tracing::error!(%error, "Failed to build connection token exchange client");
            ConnectionTokenExchangeError::internal("exchange_client_unavailable")
        })?;
    let response = if matches!(protocol.as_str(), "rfc8693" | "rfc_8693") {
        let actor_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&actor).map_err(|_| {
                ConnectionTokenExchangeError::internal("exchange_actor_serialization_failed")
            })?,
        );
        let mut form = vec![
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:token-exchange".to_string(),
            ),
            ("subject_token", parent_token.clone()),
            (
                "subject_token_type",
                connection_config_string_in_sections(
                    row,
                    &["tokenBroker", "token_broker", "broker"],
                    &["subjectTokenType", "subject_token_type"],
                )
                .unwrap_or_else(|| "urn:ietf:params:oauth:token-type:access_token".to_string()),
            ),
            ("scope", issued_scope.join(" ")),
            ("requested_token_type", connection_config_string_in_sections(
                row,
                &["tokenBroker", "token_broker", "broker"],
                &["requestedTokenType", "requested_token_type"],
            )
            .unwrap_or_else(|| "urn:ietf:params:oauth:token-type:access_token".to_string())),
            ("actor_token", actor_token),
            ("actor_token_type", connection_config_string_in_sections(
                row,
                &["tokenBroker", "token_broker", "broker"],
                &["actorTokenType", "actor_token_type"],
            )
            .unwrap_or_else(|| "urn:ietf:params:oauth:token-type:jwt".to_string())),
        ];
        if let Some(audience) = audience.as_deref() {
            form.push(("audience", audience.to_string()));
        }
        client.post(endpoint).form(&form).send().await
    } else {
        let mut body = if protocol == "pages" {
            let namespace = issued_scope
                .first()
                .and_then(|scope| scope.strip_prefix("pages:publish:ns/"));
            if let Some(namespace) = namespace {
                json!({
                    "namespace": namespace,
                    "ttlSeconds": ttl_seconds,
                    "actions": ["publish"],
                    "actor": actor,
                })
            } else {
                json!({
                    "scope": issued_scope,
                    "ttlSeconds": ttl_seconds,
                    "actor": actor,
                })
            }
        } else {
            json!({
                "scope": issued_scope,
                "ttlSeconds": ttl_seconds,
                "actor": actor,
            })
        };
        if let Some(audience) = audience {
            body["audience"] = json!(audience);
        }
        client
            .post(endpoint)
            .bearer_auth(parent_token)
            .json(&body)
            .send()
            .await
    }
    .map_err(|error| {
        tracing::warn!(%error, connection_id = %row.get::<Uuid, _>("id"), "Connection token exchange request failed");
        ConnectionTokenExchangeError::upstream(
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            json!({}),
        )
    })?;
    let upstream_status = response.status();
    let payload = response.json::<Value>().await.map_err(|error| {
        tracing::warn!(%error, %upstream_status, "Connection token exchange returned invalid JSON");
        ConnectionTokenExchangeError::upstream(
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            json!({ "upstreamStatus": upstream_status.as_u16() }),
        )
    })?;
    if !upstream_status.is_success() {
        let upstream_code = payload
            .get("code")
            .or_else(|| payload.get("error"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.chars().take(128).collect::<String>());
        let credential_revoked = matches!(upstream_status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
            || upstream_code.as_deref() == Some("parent_revoked");
        return Err(ConnectionTokenExchangeError::upstream(
            if credential_revoked {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_GATEWAY
            },
            if credential_revoked {
                "credential_revoked"
            } else {
                "upstream_error"
            },
            json!({
                "upstreamStatus": upstream_status.as_u16(),
                "upstreamCode": upstream_code,
            }),
        ));
    }
    let token = payload
        .get("token")
        .or_else(|| payload.get("access_token"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= 16_384)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ConnectionTokenExchangeError::upstream(
                StatusCode::BAD_GATEWAY,
                "upstream_token_missing",
                json!({ "upstreamStatus": upstream_status.as_u16() }),
            )
        })?;
    let token_type = payload
        .get("tokenType")
        .or_else(|| payload.get("token_type"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= 128)
        .unwrap_or("Bearer")
        .to_string();
    let response_scope = payload
        .get("scope")
        .map(config_scope_values)
        .unwrap_or_default();
    if !response_scope.is_empty() && !token_scopes_subset(&response_scope, issued_scope) {
        return Err(ConnectionTokenExchangeError::upstream(
            StatusCode::BAD_GATEWAY,
            "upstream_scope_exceeds_requested",
            json!({ "scopeCount": response_scope.len() }),
        ));
    }
    let scope = if response_scope.is_empty() {
        issued_scope.to_vec()
    } else {
        response_scope
    };
    let now = Utc::now();
    let expires_at = broker_response_expires_at(&payload, now, ttl_seconds)?;
    Ok(BrokerTokenExchange {
        token,
        token_type,
        expires_at,
        scope,
    })
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AgentConnectionTokenScope {
    String(String),
    Array(Vec<String>),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AgentConnectionTokenSubject {
    #[serde(rename = "app")]
    App,
    #[serde(rename = "user")]
    User {
        #[serde(rename = "userId")]
        user_id: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentConnectionTokenRequest {
    subject: Option<AgentConnectionTokenSubject>,
    scope: Option<AgentConnectionTokenScope>,
    #[serde(rename = "requestedTtlSeconds")]
    requested_ttl_seconds: Option<i64>,
    #[serde(rename = "grantId")]
    grant_id: Option<Uuid>,
}

async fn record_connection_token_issuance(
    state: &AppState,
    context: &ActiveAgentRunContext,
    row: &sqlx::postgres::PgRow,
    path: &str,
    requested_scope: &[String],
    issued_scope: &[String],
    ttl_seconds: Option<i32>,
    expires_at: Option<DateTime<Utc>>,
    token_hash: Option<&str>,
    outcome: &str,
    error_code: Option<&str>,
    metadata: Value,
) -> Result<(), StatusCode> {
    use sqlx::Row;
    sqlx::query(
        "INSERT INTO connection_token_issuances
            (company_id, application_id, connection_id, agent_id, run_id,
             issue_id, project_id, responsible_user_id, path, requested_scope,
             issued_scope, ttl_seconds, expires_at, token_hash, outcome,
             error_code, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                 $14, $15, $16, $17)",
    )
    .bind(context.company_id)
    .bind(row.get::<Option<Uuid>, _>("application_id"))
    .bind(row.get::<Uuid, _>("id"))
    .bind(context.agent_id)
    .bind(context.run_id)
    .bind(context.issue_id)
    .bind(context.project_id)
    .bind(context.responsible_user_id.as_deref())
    .bind(path)
    .bind(json!(requested_scope))
    .bind(json!(issued_scope))
    .bind(ttl_seconds)
    .bind(expires_at)
    .bind(token_hash)
    .bind(outcome)
    .bind(error_code)
    .bind(metadata)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, "Failed to persist connection token issuance");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(())
}
fn configured_oauth_redirect_uri(row: &sqlx::postgres::PgRow) -> Option<String> {
    if let Some(value) = connection_config_string(
        row,
        &["redirectUri", "redirect_uri", "clientRedirectUri", "client_redirect_uri"],
    ) {
        return Some(value);
    }
    let configured = [
        "PAPERCLIP_PUBLIC_URL",
        "PAPERCLIP_AUTH_PUBLIC_BASE_URL",
        "BETTER_AUTH_URL",
        "BETTER_AUTH_BASE_URL",
    ]
    .into_iter()
    .find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })?;
    let mut url = Url::parse(&configured).ok()?;
    url.set_path("/api/tools/oauth/callback");
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

fn random_oauth_token(byte_count: usize) -> String {
    let mut bytes = vec![0_u8; byte_count];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(code_verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartAgentConnectionAuthorizationRequest {
    #[serde(rename = "subjectUserId")]
    subject_user_id: String,
    scopes: Option<Vec<String>>,
    #[serde(rename = "returnTo")]
    return_to: Option<String>,
}

/// POST /agents/me/connections/:connection_id/start-authorization —— 创建 OAuth/PKCE 状态。
async fn start_agent_connection_auth(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    request: Option<Json<StartAgentConnectionAuthorizationRequest>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let context = load_active_agent_run_context(&state, &actor).await?;
    let Json(request) = request.ok_or(StatusCode::BAD_REQUEST)?;
    let subject_user_id = request.subject_user_id.trim().to_string();
    if subject_user_id.is_empty() || subject_user_id.chars().count() > 256 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if context.responsible_user_id.as_deref() != Some(subject_user_id.as_str()) {
        return Err(StatusCode::FORBIDDEN);
    }
    let row = get_connection_by_id(&state, context.company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    if row.get::<String, _>("status") == "archived" {
        return Err(StatusCode::CONFLICT);
    }
    if row.get::<String, _>("auth_kind") != "oauth" {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let endpoints = resolve_oauth_provider_endpoints(&row).await?;
    let return_to = request
        .return_to
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if return_to
        .as_deref()
        .is_some_and(|value| value.chars().count() > 2000)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let redirect_uri = configured_oauth_redirect_uri(&row)
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    validate_oauth_redirect_constraints(&row, &endpoints.provider, &redirect_uri)?;
    let row = ensure_oauth_client_registration(
        &state,
        &actor,
        context.company_id,
        connection_id,
        &row,
        &endpoints,
        &redirect_uri,
    )
    .await?;
    let client_id = connection_config_string(&row, &["clientId", "client_id"])
        .or_else(|| mcp_oauth_client_id(&endpoints.provider))
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let scopes = request
        .scopes
        .unwrap_or_else(|| {
            if endpoints.scopes.is_empty() {
                connection_config_strings(&row, &["scopes", "scope"])
            } else {
                endpoints.scopes.clone()
            }
        });
    if scopes.len() > 100
        || scopes
            .iter()
            .any(|scope| scope.trim().is_empty() || scope.chars().count() > 200)
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    let state_token = random_oauth_token(32);
    let code_verifier = random_oauth_token(48);
    let mut authorization_url =
        Url::parse(&endpoints.authorization_url).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    authorization_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("state", &state_token)
        .append_pair("code_challenge", &pkce_challenge(&code_verifier))
        .append_pair("code_challenge_method", "S256");
    if !scopes.is_empty() {
        authorization_url
            .query_pairs_mut()
            .append_pair("scope", &scopes.join(" "));
    }
    let expires_at = Utc::now() + Duration::minutes(10);
    let requested_scopes = (!scopes.is_empty()).then(|| json!(scopes));
    let mut transaction = state.pool.begin().await.map_err(|e| {
        tracing::error!(%e, "Failed to start OAuth authorization transaction");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query("DELETE FROM tool_oauth_states WHERE expires_at <= NOW()")
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, "Failed to clean expired OAuth authorization states");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    sqlx::query(
        "INSERT INTO tool_oauth_states
            (state, company_id, connection_id, code_verifier,
             created_by_actor_type, created_by_actor_id, subject_user_id,
             requested_scopes, return_to, issue_id, expires_at)
         VALUES ($1, $2, $3, $4, 'agent', $5, $6, $7, $8, $9, $10)",
    )
    .bind(&state_token)
    .bind(context.company_id)
    .bind(connection_id)
    .bind(&code_verifier)
    .bind(context.agent_id.to_string())
    .bind(&subject_user_id)
    .bind(requested_scopes)
    .bind(return_to.as_deref())
    .bind(context.issue_id)
    .bind(expires_at)
    .execute(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, %connection_id, "Failed to persist OAuth authorization state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    transaction.commit().await.map_err(|e| {
        tracing::error!(%e, "Failed to commit OAuth authorization state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(json!({ "url": authorization_url.to_string() })))
}

/// POST /agents/me/connections/:connection_id/token —— Agent token broker。
async fn agent_connection_token(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    headers: HeaderMap,
    request: Option<Json<AgentConnectionTokenRequest>>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let context = load_active_agent_run_context(&state, &actor).await?;
    if let Some(header_run_id) = headers
        .get("x-paperclip-run-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if header_run_id != context.run_id.to_string() {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    let Json(request) = request.ok_or(StatusCode::BAD_REQUEST)?;
    let row = get_connection_by_id(&state, context.company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let requested_scope = normalize_token_scopes(request.scope)?;
    let subject_user_id = match request.subject {
        None | Some(AgentConnectionTokenSubject::App) => None,
        Some(AgentConnectionTokenSubject::User { user_id }) => {
            let user_id = user_id.trim().to_string();
            if user_id.is_empty() || user_id.chars().count() > 500 {
                return Err(StatusCode::BAD_REQUEST);
            }
            if context.responsible_user_id.as_deref() != Some(user_id.as_str()) {
                return Err(StatusCode::FORBIDDEN);
            }
            Some(user_id)
        }
    };
    let path = connection_token_path(&row);
    let ttl_seconds = bounded_token_ttl(&row, request.requested_ttl_seconds)?;
    let parent_scope = connection_token_parent_scopes(&row);
    let issued_scope = if requested_scope.is_empty() {
        let defaults = connection_token_default_scopes(&row);
        if defaults.is_empty() {
            parent_scope.clone()
        } else {
            defaults
        }
    } else {
        requested_scope.clone()
    };
    if !token_scopes_subset(&issued_scope, &parent_scope) {
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            Some(ttl_seconds),
            None,
            None,
            "denied",
            Some("scope_exceeds_parent"),
            json!({ "parentScopeCount": parent_scope.len() }),
        )
        .await?;
        return Err(StatusCode::FORBIDDEN);
    }
    if row.get::<String, _>("status") != "active" || !row.get::<bool, _>("enabled") {
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            None,
            None,
            None,
            "denied",
            Some("connection_not_active"),
            json!({
                "connectionStatus": row.get::<String, _>("status"),
                "enabled": row.get::<bool, _>("enabled")
            }),
        )
        .await?;
        return Err(StatusCode::CONFLICT);
    }
    let health_status = row.get::<String, _>("health_status");
    // Paperclip: TOOL_CONNECTION_ATTENTION_HEALTH_STATUSES is the single source
    // of truth for "this app needs the user's attention". The previous ad-hoc
    // list omitted `degraded` and included the non-canonical `unhealthy`.
    if services::tool_access_contract::is_tool_connection_attention_health(&health_status) {
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            None,
            None,
            None,
            "denied",
            Some("credential_revoked"),
            json!({ "healthStatus": health_status }),
        )
        .await?;
        return Err(StatusCode::CONFLICT);
    }
    if !connection_config_bool_in_sections(
        &row,
        &["tokenBroker", "token_broker", "broker"],
        &["enabled"],
    )
    .unwrap_or(false)
    {
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            None,
            None,
            None,
            "denied",
            Some("broker_not_enabled"),
            json!({}),
        )
        .await?;
        return Err(StatusCode::FORBIDDEN);
    }

    let grant = if let Some(grant_id) = request.grant_id {
        sqlx::query(
            "SELECT id, status, credential_secret_refs
               FROM connection_grants
              WHERE id = $1 AND company_id = $2 AND connection_id = $3
                AND kind = $4
                AND ($5::text IS NULL OR subject_user_id = $5)",
        )
        .bind(grant_id)
        .bind(context.company_id)
        .bind(connection_id)
        .bind(if subject_user_id.is_some() { "user" } else { "workspace" })
        .bind(subject_user_id.as_deref())
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to load connection token grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else if let Some(subject_user_id) = subject_user_id.as_deref() {
        sqlx::query(
            "SELECT id, status, credential_secret_refs
               FROM connection_grants
              WHERE company_id = $1 AND connection_id = $2
                AND kind = 'user' AND subject_user_id = $3
              ORDER BY created_at DESC
              LIMIT 1",
        )
        .bind(context.company_id)
        .bind(connection_id)
        .bind(subject_user_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to load user connection token grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else {
        sqlx::query(
            "INSERT INTO connection_grants
                (company_id, connection_id, kind, status, is_default,
                 created_by_agent_id, credential_secret_refs)
             VALUES ($1, $2, 'workspace', 'active', true, $3, $4)
             ON CONFLICT DO NOTHING",
        )
        .bind(context.company_id)
        .bind(connection_id)
        .bind(context.agent_id)
        .bind(
            row.get::<Option<Value>, _>("credential_secret_refs")
                .unwrap_or_else(|| json!([])),
        )
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to ensure workspace connection token grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sqlx::query(
            "SELECT id, status, credential_secret_refs
               FROM connection_grants
              WHERE company_id = $1 AND connection_id = $2
                AND kind = 'workspace' AND is_default = true
              LIMIT 1",
        )
        .bind(context.company_id)
        .bind(connection_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to load default connection token grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    };
    let Some(grant) = grant else {
        let error_code = if subject_user_id.is_some() {
            "user_authorization_required"
        } else {
            "installation_required"
        };
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            None,
            None,
            None,
            "denied",
            Some(error_code),
            json!({}),
        )
        .await?;
        return Err(StatusCode::CONFLICT);
    };
    let grant_id: Uuid = grant.get("id");
    let grant_status: String = grant.get("status");
    if grant_status != "active" {
        let error_code = if grant_status == "needs_reauthorization" {
            "needs_reauthorization"
        } else {
            "grant_revoked"
        };
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            None,
            None,
            None,
            "denied",
            Some(error_code),
            json!({ "grantId": grant_id }),
        )
        .await?;
        return Err(StatusCode::CONFLICT);
    }

    if path == "static" {
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            Some(ttl_seconds),
            None,
            None,
            "use_env_lease",
            Some("use_env_lease"),
            json!({ "grantId": grant_id }),
        )
        .await?;
        return Ok((StatusCode::CONFLICT, Json(json!({
            "status": "use_env_lease",
            "code": "use_env_lease",
            "connectionId": connection_id,
            "connection": {
                "id": connection_id,
                "uid": row.get::<String, _>("uid")
            },
            "grantId": grant_id,
            "path": "static",
            "message": "This connection uses static credentials. Use an audited environment lease projection instead.",
            "scope": issued_scope,
            "attribution": {
                "agentId": context.agent_id,
                "runId": context.run_id,
                "issueId": context.issue_id,
                "projectId": context.project_id,
                "responsibleUserId": context.responsible_user_id
            }
        }))));
    }

    let grant_secret_refs = grant
        .get::<Option<Value>, _>("credential_secret_refs")
        .unwrap_or_else(|| json!([]));
    if path == "exchange" {
        match exchange_connection_token(
            &row,
            &grant_secret_refs,
            subject_user_id.as_deref(),
            &context,
            &issued_scope,
            ttl_seconds,
            &state.pool,
        )
        .await
        {
            Ok(token) => {
                let effective_ttl = (token.expires_at - Utc::now())
                    .num_seconds()
                    .clamp(1, 900) as i32;
                let token_hash = sha256_hex(&token.token);
                record_connection_token_issuance(
                    &state,
                    &context,
                    &row,
                    path,
                    &requested_scope,
                    &token.scope,
                    Some(effective_ttl),
                    Some(token.expires_at),
                    Some(&token_hash),
                    "success",
                    None,
                    json!({
                        "grantId": grant_id,
                        "tokenType": token.token_type,
                    }),
                )
                .await?;
                sqlx::query(
                    "UPDATE connection_grants
                        SET last_used_at = NOW(), updated_at = NOW()
                      WHERE id = $1 AND company_id = $2",
                )
                .bind(grant_id)
                .bind(context.company_id)
                .execute(&state.pool)
                .await
                .map_err(|error| {
                    tracing::error!(%error, %grant_id, "Failed to update connection grant usage");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                return Ok((
                    StatusCode::OK,
                    Json(json!({
                        "status": "minted",
                        "connectionId": connection_id,
                        "connection": {
                            "id": connection_id,
                            "uid": row.get::<String, _>("uid")
                        },
                        "grantId": grant_id,
                        "path": "exchange",
                        "token": token.token,
                        "tokenType": token.token_type,
                        "expiresAt": token.expires_at,
                        "ttlSeconds": effective_ttl,
                        "scope": token.scope,
                        "attribution": {
                            "agentId": context.agent_id,
                            "runId": context.run_id,
                            "issueId": context.issue_id,
                            "projectId": context.project_id,
                            "responsibleUserId": context.responsible_user_id
                        }
                    })),
                ));
            }
            Err(error) => {
                let ConnectionTokenExchangeError {
                    status,
                    outcome,
                    code,
                    metadata,
                } = error;
                let metadata = match metadata {
                    Value::Object(mut metadata) => {
                        metadata.insert("grantId".to_string(), json!(grant_id));
                        Value::Object(metadata)
                    }
                    _ => json!({ "grantId": grant_id }),
                };
                record_connection_token_issuance(
                    &state,
                    &context,
                    &row,
                    path,
                    &requested_scope,
                    &issued_scope,
                    Some(ttl_seconds),
                    None,
                    None,
                    outcome,
                    Some(code),
                    metadata,
                )
                .await?;
                return Err(status);
            }
        }
    }

    let error_code = if path == "oauth_access" {
        "oauth_access_projection_disabled"
    } else {
        "exchange_not_implemented"
    };
    record_connection_token_issuance(
        &state,
        &context,
        &row,
        path,
        &requested_scope,
        &issued_scope,
        Some(ttl_seconds),
        None,
        None,
        "denied",
        Some(error_code),
        json!({ "grantId": grant_id }),
    )
    .await?;
    Err(StatusCode::UNPROCESSABLE_ENTITY)
}

// ================= Round 3 =================

/// POST /api/tool-connections/:id/health-check —— 对 MCP 做 initialize/tools.list 探测。
async fn connection_health_check(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Write).await?;
    let started = std::time::Instant::now();
    let refreshed = refresh_connection_catalog(
        State(state.clone()),
        Extension(actor.clone()),
        Path(connection_id),
    )
    .await;
    match refreshed {
        Ok(Json(result)) => Ok(Json(json!({
            "connectionId": connection_id,
            "healthy": true,
            "latencyMs": started.elapsed().as_millis(),
            "catalog": result,
        }))),
        Err(status) => {
            let _ = sqlx::query("UPDATE tool_connections SET health_status = 'unhealthy', health_message = $3, health_checked_at = NOW(), last_error = $3, updated_at = NOW() WHERE id = $1 AND company_id = $2")
                .bind(connection_id).bind(company_id).bind(format!("MCP health check failed ({status})"))
                .execute(&state.pool).await;
            Err(status)
        }
    }
}

/// POST /api/tool-connections/:id/test-calls —— 使用真实 MCP catalog/policy/transport 执行测试。
#[derive(Debug, Deserialize)]
struct CreateTestCallRequest {
    #[serde(rename = "agentId")]
    agent_id: Uuid,
    #[serde(rename = "toolName", alias = "tool")]
    tool_name: String,
    parameters: Option<Value>,
}
async fn create_connection_test_call(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<CreateTestCallRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Read).await?;
    require_gateway_any_permission(
        &state,
        &actor,
        company_id,
        &[PermissionKey::TOOLS_USE, PermissionKey::TOOLS_MANAGE_CONNECTIONS],
    )
    .await?;
    match crate::routes::tools::execute_mcp_connection_test_call(
        &state,
        company_id,
        connection_id,
        request.agent_id,
        request.tool_name.trim(),
        request.parameters.unwrap_or_else(|| json!({})),
    )
    .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result))),
        Err(error) if error.contains("was not found in the active catalog") => {
            tracing::warn!(%connection_id, %error, "MCP test tool was not found");
            Err(StatusCode::NOT_FOUND)
        }
        Err(error) if error.contains("does not belong to the company") => {
            tracing::warn!(%connection_id, %error, "MCP test agent is outside the company");
            Err(StatusCode::UNPROCESSABLE_ENTITY)
        }
        Err(error) if error.contains("invalid") || error.contains("requires") => {
            tracing::warn!(%connection_id, %error, "Invalid MCP test call");
            Err(StatusCode::UNPROCESSABLE_ENTITY)
        }
        Err(error) => {
            tracing::warn!(%connection_id, %error, "MCP test call failed");
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}

#[derive(Debug, Deserialize)]
struct DuplicateToolProfileRequest {
    name: String,
    #[serde(rename = "includeAssignments", default)]
    include_assignments: bool,
}

/// POST /api/tool-profiles/:id/duplicate —— 复制 profile、entries，可选复制绑定。
async fn duplicate_tool_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
    Json(request): Json<DuplicateToolProfileRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_profile_company(&state, &actor, profile_id, AccessMode::Write).await?;
    let name = request.name.trim();
    if name.is_empty() || name.len() > 160 {
        return Err(StatusCode::BAD_REQUEST);
    }
    use sqlx::Row;
    let source = sqlx::query(
        "SELECT profile_key, description, default_action, metadata,
                new_tools_reviewed_at
           FROM tool_profiles
          WHERE id = $1 AND company_id = $2",
    )
    .bind(profile_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to load tool profile for duplication");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    let new_profile_id = Uuid::new_v4();
    let profile_key = format!(
        "{}-{}",
        normalize_tool_application_key(name),
        &new_profile_id.simple().to_string()[..8]
    );
    let new_profile = sqlx::query(
        "INSERT INTO tool_profiles
            (id, company_id, profile_key, name, description, status, default_action,
             new_tools_reviewed_at, metadata)
         VALUES ($1, $2, $3, $4, $5, 'active', $6, $7, $8)
         RETURNING id, profile_key, name, description, status, default_action,
                   new_tools_reviewed_at, metadata, created_at, updated_at",
    )
    .bind(new_profile_id)
    .bind(company_id)
    .bind(profile_key)
    .bind(name)
    .bind(source.get::<Option<String>, _>("description"))
    .bind(source.get::<String, _>("default_action"))
    .bind(source.get::<Option<DateTime<Utc>>, _>("new_tools_reviewed_at"))
    .bind(source.get::<Value, _>("metadata"))
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to duplicate tool profile");
        if error
            .as_database_error()
            .and_then(|database_error| database_error.code())
            .as_deref()
            == Some("23505")
        {
            StatusCode::CONFLICT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    sqlx::query(
        "INSERT INTO tool_profile_entries
            (id, company_id, profile_id, selector_type, effect, application_id,
             connection_id, catalog_entry_id, tool_name, risk_level, conditions)
         SELECT gen_random_uuid(), company_id, $2, selector_type, effect,
                application_id, connection_id, catalog_entry_id, tool_name,
                risk_level, conditions
           FROM tool_profile_entries
          WHERE company_id = $1 AND profile_id = $3",
    )
    .bind(company_id)
    .bind(new_profile_id)
    .bind(profile_id)
    .execute(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to duplicate tool profile entries");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if request.include_assignments {
        sqlx::query(
            "INSERT INTO tool_profile_bindings
                (id, company_id, profile_id, target_type, target_id, priority,
                 metadata, created_by_agent_id, created_by_user_id)
             SELECT gen_random_uuid(), company_id, $2, target_type, target_id,
                    priority, metadata, created_by_agent_id, created_by_user_id
               FROM tool_profile_bindings
              WHERE company_id = $1 AND profile_id = $3",
        )
        .bind(company_id)
        .bind(new_profile_id)
        .bind(profile_id)
        .execute(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %profile_id, "Failed to duplicate tool profile bindings");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }
    let entry_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tool_profile_entries WHERE company_id = $1 AND profile_id = $2",
    )
    .bind(company_id)
    .bind(new_profile_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);
    let assignment_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tool_profile_bindings WHERE company_id = $1 AND profile_id = $2",
    )
    .bind(company_id)
    .bind(new_profile_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);
    Ok(Json(json!({
        "id": new_profile.get::<Uuid, _>("id"),
        "companyId": company_id,
        "profileKey": new_profile.get::<String, _>("profile_key"),
        "name": new_profile.get::<String, _>("name"),
        "description": new_profile.get::<Option<String>, _>("description"),
        "status": new_profile.get::<String, _>("status"),
        "defaultAction": new_profile.get::<String, _>("default_action"),
        "newToolsReviewedAt": new_profile.get::<Option<DateTime<Utc>>, _>("new_tools_reviewed_at"),
        "metadata": new_profile.get::<Value, _>("metadata"),
        "duplicatedFrom": profile_id,
        "entryCount": entry_count,
        "assignmentCount": assignment_count,
        "createdAt": new_profile.get::<DateTime<Utc>, _>("created_at"),
        "updatedAt": new_profile.get::<DateTime<Utc>, _>("updated_at"),
    })))
}

#[derive(Debug, Deserialize)]
struct ToolProfileBindingRequest {
    #[serde(rename = "targetType", alias = "target_type")]
    target_type: String,
    #[serde(rename = "targetId", alias = "target_id")]
    target_id: String,
    priority: Option<i32>,
    metadata: Option<Value>,
}

async fn validate_tool_profile_binding_target(
    state: &AppState,
    company_id: Uuid,
    target_type: &str,
    target_id: &str,
) -> Result<(), StatusCode> {
    let target_id = target_id.trim();
    if target_id.is_empty()
        || target_id.len() > 200
        || !matches!(target_type, "company" | "agent" | "project" | "routine" | "issue" | "gateway")
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if target_type == "company" {
        return (target_id == company_id.to_string())
            .then_some(())
            .ok_or(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let target_uuid = Uuid::parse_str(target_id).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let exists = match target_type {
        "agent" => sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1 AND company_id = $2)",
        )
        .bind(target_uuid)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await,
        "project" => sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND company_id = $2)",
        )
        .bind(target_uuid)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await,
        "routine" => sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM routines WHERE id = $1 AND company_id = $2)",
        )
        .bind(target_uuid)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await,
        "issue" => sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND company_id = $2)",
        )
        .bind(target_uuid)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await,
        "gateway" => sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM tool_mcp_gateways WHERE id = $1 AND company_id = $2)",
        )
        .bind(target_uuid)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await,
        _ => unreachable!("target type was validated above"),
    }
    .map_err(|error| {
        tracing::error!(%error, %company_id, %target_type, %target_id, "Failed to validate tool profile binding target");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    exists.then_some(()).ok_or(StatusCode::UNPROCESSABLE_ENTITY)
}

/// POST /companies/:cid/tools/profiles/:profile_id/bind
async fn bind_tool_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, profile_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<ToolProfileBindingRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    ensure_tool_profile_scope(&state, profile_id, company_id).await?;
    validate_tool_profile_binding_target(
        &state,
        company_id,
        request.target_type.trim(),
        request.target_id.trim(),
    )
    .await?;
    let priority = request.priority.unwrap_or(100).clamp(0, 10_000);
    let metadata = request.metadata.unwrap_or_else(|| json!({}));
    if !metadata.is_object() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    use sqlx::Row;
    let row = sqlx::query(
        "INSERT INTO tool_profile_bindings
            (company_id, profile_id, target_type, target_id, priority, metadata,
             created_by_agent_id, created_by_user_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, created_at, updated_at",
    )
    .bind(company_id)
    .bind(profile_id)
    .bind(request.target_type.trim())
    .bind(request.target_id.trim())
    .bind(priority)
    .bind(&metadata)
    .bind(match &actor {
        AuthorizationActor::Agent { agent_id, .. } => Some(*agent_id),
        _ => None,
    })
    .bind(match &actor {
        AuthorizationActor::Board { user_id, .. } => Some(user_id.to_string()),
        _ => None,
    })
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to bind tool profile");
        if error
            .as_database_error()
            .and_then(|database_error| database_error.code())
            .as_deref()
            == Some("23505")
        {
            StatusCode::CONFLICT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": row.get::<Uuid, _>("id"),
            "companyId": company_id,
            "profileId": profile_id,
            "targetType": request.target_type.trim(),
            "targetId": request.target_id.trim(),
            "priority": priority,
            "metadata": metadata,
            "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
            "updatedAt": row.get::<DateTime<Utc>, _>("updated_at"),
        })),
    ))
}

/// POST /companies/:cid/tools/profiles/:profile_id/unbind
async fn unbind_tool_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, profile_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<ToolProfileBindingRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    ensure_tool_profile_scope(&state, profile_id, company_id).await?;
    validate_tool_profile_binding_target(
        &state,
        company_id,
        request.target_type.trim(),
        request.target_id.trim(),
    )
    .await?;
    let deleted = sqlx::query(
        "DELETE FROM tool_profile_bindings
          WHERE company_id = $1 AND profile_id = $2
            AND target_type = $3 AND target_id = $4
         RETURNING id",
    )
    .bind(company_id)
    .bind(profile_id)
    .bind(request.target_type.trim())
    .bind(request.target_id.trim())
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to unbind tool profile");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(json!({ "unbound": deleted.len() })))
}

/// POST /api/tool-profiles/:id/entries —— 添加 profile entry。
async fn create_tool_profile_entry(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
    Json(request): Json<CreateProfileEntryRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let company_id =
        accessible_profile_company(&state, &actor, profile_id, AccessMode::Write).await?;
    let tool_name = request.tool_name.clone().or_else(|| request.tool.clone());
    let selector_type = request.selector_type.as_deref().or_else(|| {
        if tool_name.is_some() {
            Some("tool_name")
        } else {
            None
        }
    });
    let effect = request
        .effect
        .as_deref()
        .or_else(|| request.enabled.map(|enabled| if enabled { "include" } else { "exclude" }))
        .or(Some("include"));
    let normalized = normalize_profile_entry_values(
        selector_type,
        effect,
        request.application_id,
        request.connection_id,
        request.catalog_entry_id,
        tool_name,
        request.risk_level,
        request.conditions,
    )?;
    validate_profile_entry_references(&state, company_id, &normalized).await?;
    let row = sqlx::query(
        "INSERT INTO tool_profile_entries
            (id, company_id, profile_id, selector_type, effect,
             application_id, connection_id, catalog_entry_id, tool_name,
             risk_level, conditions)
         SELECT $1, $2, p.id, $4, $5, $6, $7, $8, $9, $10, $11
           FROM tool_profiles AS p
          WHERE p.id = $3 AND p.company_id = $2
         RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(profile_id)
    .bind(&normalized.selector_type)
    .bind(&normalized.effect)
    .bind(normalized.application_id)
    .bind(normalized.connection_id)
    .bind(normalized.catalog_entry_id)
    .bind(&normalized.tool_name)
    .bind(&normalized.risk_level)
    .bind(&normalized.conditions)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, %profile_id, "Failed to create tool profile entry");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    sqlx::query("UPDATE tool_profiles SET updated_at = NOW() WHERE id = $1 AND company_id = $2")
        .bind(profile_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, %profile_id, "Failed to touch tool profile after entry creation");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((
        StatusCode::CREATED,
        Json(profile_entry_json(&row)),
    ))
}

#[derive(serde::Deserialize)]
struct ReviewNewToolsDecision {
    #[serde(rename = "catalogEntryId")]
    catalog_entry_id: Uuid,
    decision: String,
}

#[derive(serde::Deserialize)]
struct ReviewNewToolsInput {
    decisions: Vec<ReviewNewToolsDecision>,
}

/// POST /api/tool-profiles/:id/new-tools/review
///
/// Paperclip `reviewProfileNewTools`: decisions must cover every currently
/// pending tool exactly once (no duplicates, no omissions); `allow` inserts a
/// `catalog_entry`/`include` profile entry per tool; every decided catalog
/// entry gets `reviewed_at` + reviewer attribution; responds with counts.
async fn review_profile_new_tools(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
    Json(input): Json<ReviewNewToolsInput>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let company_id =
        accessible_profile_company(&state, &actor, profile_id, AccessMode::Write).await?;

    let decisions = input.decisions;
    if decisions.is_empty() || decisions.len() > 250 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    for decision in &decisions {
        if decision.decision != "allow" && decision.decision != "keep_blocked" {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
    let decision_ids: Vec<Uuid> = decisions.iter().map(|d| d.catalog_entry_id).collect();
    let mut seen = std::collections::HashSet::new();
    if !decision_ids.iter().all(|id| seen.insert(*id)) {
        // Paperclip: duplicate catalogEntryId values are a bad request.
        return Err(StatusCode::BAD_REQUEST);
    }

    // Pending = active catalog entries not yet reviewed (same predicate as the
    // GET /new-tools listing).
    let pending = sqlx::query(
        "SELECT c.id, c.connection_id, c.application_id
           FROM tool_catalog_entries c
           JOIN tool_profiles p ON p.id = $1 AND p.company_id = c.company_id
          WHERE c.company_id = $2 AND c.reviewed_at IS NULL AND c.status = 'active'",
    )
    .bind(profile_id)
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list profile new tools: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if pending.is_empty() {
        // Paperclip: no pending tools -> bad request.
        return Err(StatusCode::BAD_REQUEST);
    }
    let pending_ids: std::collections::HashSet<Uuid> =
        pending.iter().map(|row| row.get::<Uuid, _>("id")).collect();
    let covers_every_pending = decision_ids.len() == pending_ids.len()
        && decision_ids.iter().all(|id| pending_ids.contains(id));
    if !covers_every_pending {
        return Err(StatusCode::BAD_REQUEST);
    }

    let allow_ids: Vec<Uuid> = decisions
        .iter()
        .filter(|d| d.decision == "allow")
        .map(|d| d.catalog_entry_id)
        .collect();
    let now_at = chrono::Utc::now();

    // allow -> one catalog_entry/include profile entry per allowed tool.
    let mut entries_created = 0usize;
    for entry_id in &allow_ids {
        let row = pending
            .iter()
            .find(|row| row.get::<Uuid, _>("id") == *entry_id)
            .expect("decision ids verified against pending set");
        let result = sqlx::query(
            "INSERT INTO tool_profile_entries
             (company_id, profile_id, selector_type, effect, application_id, connection_id, catalog_entry_id)
             VALUES ($1, $2, 'catalog_entry', 'include', $3, $4, $5)",
        )
        .bind(company_id)
        .bind(profile_id)
        .bind(row.try_get::<Option<Uuid>, _>("application_id").unwrap_or(None))
        .bind(row.get::<Option<Uuid>, _>("connection_id"))
        .bind(entry_id)
        .execute(&state.pool)
        .await;
        match result {
            Ok(_) => entries_created += 1,
            Err(e) => {
                tracing::error!("Failed to create profile entry: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    // Every decided entry is marked reviewed with reviewer attribution.
    sqlx::query(
        "UPDATE tool_catalog_entries SET reviewed_at = $3, \
         reviewed_by_agent_id = $4, reviewed_by_user_id = $5, updated_at = $3 \
         WHERE company_id = $1 AND id = ANY($2)",
    )
    .bind(company_id)
    .bind(&decision_ids)
    .bind(now_at)
    .bind(match &actor {
        AuthorizationActor::Agent { agent_id, .. } => Some(*agent_id),
        _ => None,
    })
    .bind(match &actor {
        AuthorizationActor::Board { user_id, .. } => Some(*user_id),
        _ => None,
    })
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to mark catalog entries reviewed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Paperclip also stamps the profile-level review timestamp.
    sqlx::query(
        "UPDATE tool_profiles SET new_tools_reviewed_at = $2, updated_at = NOW() \
         WHERE id = $1",
    )
    .bind(profile_id)
    .bind(now_at)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to stamp profile new_tools_reviewed_at: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({
        "profileId": profile_id,
        "reviewedAt": now_at,
        "allowedCount": allow_ids.len(),
        "keptBlockedCount": decisions.len() - allow_ids.len(),
        "entriesCreated": entries_created,
        "reviewedCatalogEntryIds": decision_ids,
    })))
}

#[derive(Debug, Deserialize, Default)]
struct StartBoardConnectionAuthorizationRequest {
    #[serde(rename = "subjectUserId")]
    subject_user_id: Option<String>,
    scopes: Option<Vec<String>>,
    #[serde(rename = "returnTo")]
    return_to: Option<String>,
}

/// POST /api/tools/oauth/:connection_id/start —— 为 board 用户启动 OAuth/PKCE。
async fn tools_oauth_start(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    request: Option<Json<StartBoardConnectionAuthorizationRequest>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id =
        accessible_connection_company(&state, &actor, connection_id, AccessMode::Write).await?;
    let request = request.map(|Json(value)| value).unwrap_or_default();
    let row = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    if row.get::<String, _>("status") == "archived" {
        return Err(StatusCode::CONFLICT);
    }
    if row.get::<String, _>("auth_kind") != "oauth" {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    let subject_user_id = request
        .subject_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if subject_user_id
        .as_deref()
        .is_some_and(|value| value != user_id.to_string())
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let return_to = request
        .return_to
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if return_to
        .as_deref()
        .is_some_and(|value| value.chars().count() > 2000)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let endpoints = resolve_oauth_provider_endpoints(&row).await?;
    let redirect_uri = configured_oauth_redirect_uri(&row)
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    validate_oauth_redirect_constraints(&row, &endpoints.provider, &redirect_uri)?;
    let row = ensure_oauth_client_registration(
        &state,
        &actor,
        company_id,
        connection_id,
        &row,
        &endpoints,
        &redirect_uri,
    )
    .await?;
    let client_id = connection_config_string(&row, &["clientId", "client_id"])
        .or_else(|| mcp_oauth_client_id(&endpoints.provider))
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let scopes = request
        .scopes
        .unwrap_or_else(|| {
            if endpoints.scopes.is_empty() {
                connection_config_strings(&row, &["scopes", "scope"])
            } else {
                endpoints.scopes.clone()
            }
        });
    if scopes.len() > 100
        || scopes
            .iter()
            .any(|scope| scope.trim().is_empty() || scope.chars().count() > 200)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let state_token = random_oauth_token(32);
    let code_verifier = random_oauth_token(48);
    let mut authorization_url =
        Url::parse(&endpoints.authorization_url).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    authorization_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("state", &state_token)
        .append_pair("code_challenge", &pkce_challenge(&code_verifier))
        .append_pair("code_challenge_method", "S256");
    if !scopes.is_empty() {
        authorization_url
            .query_pairs_mut()
            .append_pair("scope", &scopes.join(" "));
    }
    let expires_at = Utc::now() + Duration::minutes(10);
    let requested_scopes = (!scopes.is_empty()).then(|| json!(scopes));
    let mut transaction = state.pool.begin().await.map_err(|error| {
        tracing::error!(%error, %connection_id, "Failed to start board OAuth transaction");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query("DELETE FROM tool_oauth_states WHERE expires_at <= NOW()")
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to clean expired OAuth states");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    sqlx::query(
        "INSERT INTO tool_oauth_states
            (state, company_id, connection_id, code_verifier,
             created_by_actor_type, created_by_actor_id, subject_user_id,
             requested_scopes, return_to, expires_at)
         VALUES ($1, $2, $3, $4, 'user', $5, $6, $7, $8, $9)",
    )
    .bind(&state_token)
    .bind(company_id)
    .bind(connection_id)
    .bind(&code_verifier)
    .bind(user_id.to_string())
    .bind(subject_user_id.as_deref())
    .bind(requested_scopes)
    .bind(return_to.as_deref())
    .bind(expires_at)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        tracing::error!(%error, %connection_id, "Failed to persist board OAuth state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    transaction.commit().await.map_err(|error| {
        tracing::error!(%error, %connection_id, "Failed to commit board OAuth state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(json!({
        "connectionId": connection_id,
        "authorizationUrl": authorization_url.to_string(),
        "url": authorization_url.to_string(),
    })))
}

/// Paperclip's board setup alias.  It binds the pending OAuth state to the
/// requesting board user before delegating to the shared PKCE implementation.
async fn company_connection_oauth_start(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, connection_id)): Path<(Uuid, Uuid)>,
    request: Option<Json<StartBoardConnectionAuthorizationRequest>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Path-scoped variant of the same board flow: the company comes from the
    // route, matching Paperclip `assertToolAppMutationAccess`
    // (`server/src/routes/tool-access.ts:286-296`).
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    let mut request = request.map(|Json(value)| value).unwrap_or_default();
    let subject_user_id = request
        .subject_user_id
        .take()
        .ok_or(StatusCode::BAD_REQUEST)?;
    if subject_user_id.trim() != user_id.to_string() {
        return Err(StatusCode::FORBIDDEN);
    }
    request.subject_user_id = Some(subject_user_id);
    let Json(result) = tools_oauth_start(
        State(state),
        Extension(actor),
        Path(connection_id),
        Some(Json(request)),
    )
    .await?;
    Ok(Json(json!({
        "url": result.get("authorizationUrl").cloned().unwrap_or(Value::Null),
    })))
}

/// POST /api/tool-gateway/runtime-slots/:slot_id/stop
/// 真实更新 tool_runtime_slots 行状态（数据层与 Paperclip 一致）。
/// 注：实际进程启停（spawn/kill）属运行时组件，不在本迁移范围；此处仅对齐状态机。
async fn gateway_slot_stop(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(slot_id): Path<String>,
    Query(query): Query<GatewayRuntimeSlotsQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = query
        .company_id
        .as_deref()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
    Ok(Json(
        update_runtime_slot_state(&state, company_id, &slot_id, "stopped").await?,
    ))
}
/// POST /api/tool-gateway/runtime-slots/:slot_id/restart
/// 真实更新 tool_runtime_slots 行状态（数据层与 Paperclip 一致）。
async fn gateway_slot_restart(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(slot_id): Path<String>,
    Query(query): Query<GatewayRuntimeSlotsQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = query
        .company_id
        .as_deref()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
    Ok(Json(
        update_runtime_slot_state(&state, company_id, &slot_id, "running").await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_audit_cursor_round_trips_without_padding() {
        let cursor = GatewayAuditCursor {
            created_at: Utc::now(),
            id: Uuid::new_v4(),
        };
        let encoded = gateway_audit_cursor_encode(&cursor);
        assert!(!encoded.contains('='));
        assert_eq!(gateway_audit_cursor_decode(&encoded), Some(cursor));
        assert!(gateway_audit_cursor_decode("not-a-cursor").is_none());
    }

    #[test]
    fn gateway_audit_query_helpers_match_paperclip_filters() {
        assert_eq!(gateway_audit_window("1h"), Some(Duration::hours(1)));
        assert_eq!(gateway_audit_window("24h"), Some(Duration::hours(24)));
        assert!(gateway_audit_window("90d").is_none());
        assert_eq!(
            gateway_audit_like_pattern(r"100%_ready\now"),
            r"%100\%\_ready\\now%"
        );
        assert_eq!(
            gateway_audit_outcome("call_completed", Some("allow"), "success"),
            "allowed"
        );
        assert_eq!(
            gateway_audit_outcome("approval_requested", Some("require_approval"), "pending"),
            "asked_first"
        );
        assert_eq!(
            gateway_audit_outcome("call_failed", Some("allow"), "failure"),
            "allowed"
        );
        assert_eq!(
            gateway_audit_outcome("call_denied", Some("deny"), "denied"),
            "blocked"
        );
    }

    #[test]
    fn profile_entry_normalization_supports_paperclip_selectors() {
        let application = normalize_profile_entry_values(
            Some("application"),
            Some("include"),
            Some(Uuid::new_v4()),
            None,
            None,
            None,
            None,
            Some(json!({"source": "fixture"})),
        )
        .expect("application selector should validate");
        assert_eq!(application.selector_type, "application");
        assert_eq!(application.effect, "include");

        let risk = normalize_profile_entry_values(
            Some("risk_level"),
            Some("exclude"),
            None,
            None,
            None,
            None,
            Some("destructive".to_string()),
            None,
        )
        .expect("risk selector should validate");
        assert_eq!(risk.risk_level.as_deref(), Some("destructive"));
        assert!(normalize_profile_entry_values(
            Some("application"),
            Some("include"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .is_err());
    }

    #[test]
    fn oauth_dcr_response_validation_is_fail_closed() {
        let response = json!({
            "client_id": "client-123",
            "client_secret": "secret-123",
            "redirect_uris": ["https://example.test/callback"],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"]
        });
        assert_eq!(
            parse_dcr_string(&response, "client_id", true, 512).expect("client id"),
            Some("client-123".to_string())
        );
        assert!(validate_dcr_array(
            &response,
            "redirect_uris",
            &["https://example.test/callback"]
        )
        .is_ok());
        assert!(parse_dcr_string(&json!({"client_id": " client"}), "client_id", true, 512).is_err());
        assert!(validate_dcr_array(&json!({"response_types": ["token"]}), "response_types", &["code"]).is_err());
    }
}
