//! Tool access read endpoints and the hosted MCP gateway.
//!
//! This module owns the Paperclip-compatible MCP protocol surface: built-in
//! API tools, remote/stdio discovery and execution, gateway sessions and
//! policy/audit integration. The companion `tool_access` module owns the
//! board-facing connection/profile setup workflow.

use axum::{
    body::to_bytes,
    extract::{Extension, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::Engine as _;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures::StreamExt;
use models::{CommentActorType, CreateIssueInput, UpdateIssueInput};
use serde_json::{json, Value};
use services::issue_service::{
    CheckoutInput, IssueQueryFilter, Pagination as IssuePagination, ReleaseInput,
};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::IpAddr;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};
use rand::{rngs::OsRng, RngCore};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::mcp::{request_kind, McpRequestKind, McpToolDefinition};
use crate::paperclip_internal::PaperclipInternalClient;
use services::auth::{AuthorizationAction, AuthorizationActor, AuthorizationService, PermissionKey};
use services::mcp_client_config::{
    named_gateway_client_snippets, named_gateway_endpoint_path,
};
use services::mcp_http::{mcp_http_request_headers, parse_mcp_http_response_body};
use services::secret_provider::{decrypt_secret_material, encrypt_secret_material, sha256_hex};

const TOOL_POLICY_QUERY: &str = r#"SELECT id, policy_type, selectors, config, description
     FROM tool_policies
    WHERE company_id = $1 AND enabled = true
    ORDER BY priority ASC, created_at ASC"#;

const TRUST_RULE_HIT_UPDATE: &str = r#"UPDATE tool_policies SET config = jsonb_set(
     jsonb_set(config, '{trustRule,hitCount}',
         (((COALESCE(config->'trustRule'->>'hitCount','0'))::int + 1))::text::jsonb),
     '{trustRule,lastHitAt}', to_jsonb(NOW()))
 WHERE id = $1"#;
const TOOL_APPROVAL_DESCRIPTION_SUFFIX: &str =
    "Requires human approval: calling it posts an approval card on your task and you will be woken with the result once decided.";

fn hash_gateway_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn random_named_gateway_secret() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn gateway_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-paperclip-tool-gateway-token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bearer_or_gateway_token(headers: &HeaderMap) -> Option<String> {
    gateway_token(headers).or_else(|| {
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| {
                value
                    .strip_prefix("Bearer ")
                    .or_else(|| value.strip_prefix("bearer "))
            })
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

pub(crate) async fn mcp_http_request(
    url: &str,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    mcp_http_request_with_config(url, method, params, None).await
}

/// Execute one MCP Streamable HTTP request using the connection's transport
/// configuration. Paperclip always advertises JSON + SSE and accepts either
/// response shape; keeping that behavior in this shared path also fixes
/// catalog refresh, gateway discovery, and tool calls at once.
pub(crate) async fn mcp_http_request_with_config(
    url: &str,
    method: &str,
    params: Value,
    transport_config: Option<&Value>,
) -> Result<Value, String> {
    mcp_http_request_with_headers(url, method, params, transport_config, None).await
}

pub(crate) async fn mcp_http_request_with_headers(
    url: &str,
    method: &str,
    params: Value,
    transport_config: Option<&Value>,
    resolved_headers: Option<&HashMap<String, String>>,
) -> Result<Value, String> {
    validate_mcp_http_endpoint(url).await?;
    let mut extra_headers = transport_config
        .and_then(|config| config.get("headers"))
        .and_then(Value::as_object)
        .map(|headers| {
            headers
                .iter()
                .filter_map(|(name, value)| {
                    let value = value.as_str()?.to_string();
                    let name = name.trim().to_ascii_lowercase();
                    if !valid_mcp_header_name(&name)
                        || name.is_empty()
                        || matches!(
                            name.as_str(),
                            "accept" | "content-type" | "content-length" | "host" | "connection"
                        )
                        || value.contains('\r')
                        || value.contains('\n')
                    {
                        return None;
                    }
                    Some((name, value))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    if let Some(resolved_headers) = resolved_headers {
        for (name, value) in resolved_headers {
            add_mcp_header(&mut extra_headers, name, value);
        }
    }
    let headers = mcp_http_request_headers(Some(&extra_headers));
    let timeout_ms = transport_config
        .and_then(|config| {
            config
                .get("timeoutMs")
                .or_else(|| config.get("timeout_ms"))
                .or_else(|| config.get("requestTimeoutMs"))
        })
        .and_then(Value::as_u64)
        .unwrap_or(30_000)
        .clamp(1_000, 60_000);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(|error| format!("MCP HTTP client initialization failed: {error}"))?;
    let mut request = client.post(url);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let response = request
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": Uuid::new_v4(),
            "method": method,
            "params": params,
        }))
        .send()
        .await
        .map_err(|error| format!("MCP HTTP request failed: {error}"))?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    if response
        .content_length()
        .is_some_and(|length| length > 1_000_000)
    {
        return Err("MCP response exceeded the 1 MB gateway limit".to_string());
    }
    let mut response_body = Vec::new();
    let mut response_stream = response.bytes_stream();
    while let Some(chunk) = response_stream.next().await {
        let chunk = chunk.map_err(|error| format!("MCP response body read failed: {error}"))?;
        if response_body.len().saturating_add(chunk.len()) > 1_000_000 {
            return Err("MCP response exceeded the 1 MB gateway limit".to_string());
        }
        response_body.extend_from_slice(&chunk);
    }
    let body_text = String::from_utf8(response_body)
        .map_err(|error| format!("MCP response body was not valid UTF-8: {error}"))?;
    let body = parse_mcp_http_response_body(&body_text, content_type.as_deref())
        .map_err(|error| format!("MCP response is not valid JSON/SSE: {error}"))?;
    if !status.is_success() {
        return Err(format!("MCP server returned HTTP {status}"));
    }
    if let Some(error) = body.get("error") {
        return Err(format!("MCP server returned JSON-RPC error: {error}"));
    }
    let mut result = body.get("result").cloned().unwrap_or_else(|| body.clone());
    // A few MCP servers attach an elicitation request beside the JSON-RPC
    // result rather than inside it. Preserve that metadata for the gateway's
    // interaction bridge instead of dropping it while unwrapping `result`.
    if let (Some(body_object), Some(result_object)) = (body.as_object(), result.as_object_mut()) {
        for key in ["elicitation", "elicitationRequest"] {
            if !result_object.contains_key(key) {
                if let Some(value) = body_object.get(key) {
                    result_object.insert(key.to_string(), value.clone());
                }
            }
        }
        if !result_object.contains_key("_meta") {
            if let Some(value) = body_object.get("_meta") {
                result_object.insert("_meta".to_string(), value.clone());
            }
        }
    }
    Ok(result)
}

/// Validate the URL before handing it to reqwest. The deployment-specific
/// private-network policy belongs at connection activation time; this shared
/// transport guard still rejects malformed or non-HTTP endpoints on every
/// execution path and prevents credentials from being sent to URL userinfo.
async fn validate_mcp_http_endpoint(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|_| "MCP endpoint URL is invalid".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("MCP endpoint URL must use http or https and include a host".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() || parsed.fragment().is_some() {
        return Err("MCP endpoint URL must not contain credentials or a fragment".to_string());
    }
    if mcp_private_endpoints_disallowed() {
        let host = parsed
            .host_str()
            .ok_or_else(|| "MCP endpoint URL must include a host".to_string())?
            .trim_matches(['[', ']'])
            .to_ascii_lowercase();
        if host == "localhost" || host.ends_with(".localhost") {
            return Err(
                "MCP endpoint URL cannot target private or reserved network addresses".to_string(),
            );
        }
        if let Ok(address) = host.parse::<IpAddr>() {
            if mcp_private_or_reserved_ip(address) {
                return Err(
                    "MCP endpoint URL cannot target private or reserved network addresses"
                        .to_string(),
                );
            }
        } else {
            let port = parsed.port_or_known_default().ok_or_else(|| {
                "MCP endpoint URL must use a scheme with a known port".to_string()
            })?;
            let addresses = tokio::net::lookup_host((host.as_str(), port))
                .await
                .map_err(|_| "MCP endpoint hostname could not be resolved".to_string())?
                .collect::<Vec<_>>();
            if addresses.is_empty() {
                return Err("MCP endpoint hostname did not resolve".to_string());
            }
            if addresses
                .into_iter()
                .any(|address| mcp_private_or_reserved_ip(address.ip()))
            {
                return Err(
                    "MCP endpoint URL cannot resolve to private or reserved network addresses"
                        .to_string(),
                );
            }
        }
    }
    Ok(())
}

fn mcp_private_endpoints_disallowed() -> bool {
    std::env::var("DEPLOYMENT_MODE")
        .ok()
        .as_deref()
        == Some("authenticated")
        && std::env::var("DEPLOYMENT_EXPOSURE")
            .ok()
            .as_deref()
            == Some("public")
}

fn mcp_private_or_reserved_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let [a, b, c, _] = address.octets();
            a == 0
                || a == 10
                || (a == 100 && (64..=127).contains(&b))
                || a == 127
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 0 && c == 0)
                || (a == 192 && b == 168)
                || (a == 192 && b == 0 && c == 2)
                || (a == 192 && b == 88 && c == 99)
                || (a == 198 && (b == 18 || b == 19))
                || (a == 198 && b == 51 && c == 100)
                || (a == 203 && b == 0 && c == 113)
                || a >= 224
        }
        IpAddr::V6(address) => {
            let segments = address.segments();
            let mapped_ipv4 = if segments[..5] == [0, 0, 0, 0, 0]
                && segments[5] == 0xffff
            {
                Some(IpAddr::V4(std::net::Ipv4Addr::new(
                    (segments[6] >> 8) as u8,
                    segments[6] as u8,
                    (segments[7] >> 8) as u8,
                    segments[7] as u8,
                )))
            } else {
                None
            };
            mapped_ipv4.is_some_and(mcp_private_or_reserved_ip)
                || address.is_unspecified()
                || address.is_loopback()
                || address.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] == 0x0100
                    && segments[1] == 0
                    && segments[2] == 0
                    && segments[3] == 0)
                || (segments[0] == 0x2001
                    && (segments[1] == 0xdb8 || segments[1] == 0x0002))
                || segments[0] == 0x2002
                || (segments[0] == 0x64 && segments[1] == 0xff9b)
        }
    }
}

fn paperclip_builtin_tool_definitions() -> Vec<McpToolDefinition> {
    const TOOLS: &[(&str, &str)] = &[
        ("paperclipMe", "Get the current authenticated Paperclip actor details"),
        ("paperclipInboxLite", "Get the current authenticated agent inbox-lite assignment list"),
        ("paperclipHireAgent", "Request to hire a new agent (creates a pending hire approval when the company requires board approval)"),
        ("paperclipListAgents", "List agents in a company"),
        ("paperclipGetAgent", "Get a single agent by id"),
        ("paperclipListIssues", "List issues for a company with optional filters"),
        ("paperclipGetIssue", "Get a single issue by UUID or identifier"),
        ("paperclipGetHeartbeatContext", "Get compact heartbeat context for an issue"),
        ("paperclipListComments", "List comments for an issue"),
        ("paperclipGetComment", "Get a specific issue comment"),
        ("paperclipListIssueApprovals", "List approvals linked to an issue"),
        ("paperclipListDocuments", "List issue documents"),
        ("paperclipGetDocument", "Get one issue document by key"),
        ("paperclipListDocumentRevisions", "List revisions for an issue document"),
        ("paperclipListProjects", "List projects in a company"),
        ("paperclipGetProject", "Get a project by id"),
        ("paperclipGetIssueWorkspaceRuntime", "Get the current issue execution workspace"),
        ("paperclipControlIssueWorkspaceServices", "Start, stop, or restart workspace services"),
        ("paperclipWaitForIssueWorkspaceService", "Wait for an issue workspace service"),
        ("paperclipListGoals", "List goals in a company"),
        ("paperclipGetGoal", "Get a goal by id"),
        ("paperclipListApprovals", "List approvals in a company"),
        ("paperclipCreateApproval", "Create an approval request"),
        ("paperclipGetApproval", "Get an approval by id"),
        ("paperclipGetApprovalIssues", "List issues linked to an approval"),
        ("paperclipListApprovalComments", "List comments for an approval"),
        ("paperclipCreateIssue", "Create a new issue"),
        ("paperclipUpdateIssue", "Update an issue"),
        ("paperclipCheckoutIssue", "Checkout an issue for an agent"),
        ("paperclipReleaseIssue", "Release an issue checkout"),
        ("paperclipAddComment", "Add a comment to an issue"),
        ("paperclipSuggestTasks", "Create a suggest_tasks interaction"),
        ("paperclipAskUserQuestions", "Create an ask_user_questions interaction"),
        ("paperclipRequestConfirmation", "Create a request_confirmation interaction"),
        ("paperclipRequestCheckboxConfirmation", "Create a checkbox confirmation interaction"),
        ("paperclipUpsertIssueDocument", "Create or update an issue document"),
        ("paperclipRestoreIssueDocumentRevision", "Restore a document revision"),
        ("paperclipLinkIssueApproval", "Link an approval to an issue"),
        ("paperclipUnlinkIssueApproval", "Unlink an approval from an issue"),
        ("paperclipApprovalDecision", "Approve, reject, revise, or resubmit an approval"),
        ("paperclipAddApprovalComment", "Add a comment to an approval"),
        ("paperclipApiRequest", "Make a JSON request to an existing /api endpoint"),
    ];
    // These are Parrot's already-implemented Paperclip API extensions. They
    // were present in the argument schema/dispatcher, but were missing from
    // the registry, which made them impossible to discover or invoke through
    // MCP. Keep them separate from the 41-tool Paperclip standalone contract
    // so parity tests can distinguish upstream tools from Parrot additions.
    const PARROT_EXTENDED_TOOLS: &[(&str, &str)] = &[
        ("paperclipCreateCase", "Create a case"),
        ("paperclipGetCase", "Get a case by id or identifier"),
        ("paperclipUpdateCase", "Update a case"),
        ("paperclipListCases", "List cases in a company"),
        ("paperclipGetCaseChildren", "List child cases"),
        ("paperclipGetCaseEvents", "List case events"),
        ("paperclipGetIssueCases", "List cases linked to an issue"),
        ("paperclipGetCaseDocument", "Get a case document"),
        ("paperclipListCaseDocuments", "List case documents"),
        ("paperclipUpsertCaseDocument", "Create or update a case document"),
        ("paperclipDeleteCaseDocument", "Delete a case document"),
        ("paperclipRestoreCaseDocumentRevision", "Restore a case document revision"),
        ("paperclipLockCaseDocument", "Lock a case document"),
        ("paperclipUnlockCaseDocument", "Unlock a case document"),
        ("paperclipListCaseDocumentRevisions", "List case document revisions"),
        ("paperclipListCaseDocumentAnnotations", "List case document annotations"),
        ("paperclipCreateCaseDocumentAnnotation", "Create a case document annotation"),
        ("paperclipGetCaseDocumentAnnotationThread", "Get a case document annotation thread"),
        ("paperclipReplyCaseDocumentAnnotation", "Reply to a case document annotation"),
        ("paperclipUpdateCaseDocumentAnnotation", "Update a case document annotation"),
        ("paperclipCreateCaseLink", "Link a case to another resource"),
        ("paperclipListIssueAttachments", "List issue attachments"),
        ("paperclipCreateIssueAttachment", "Create an issue attachment"),
        ("paperclipDeleteAttachment", "Delete an attachment"),
        ("paperclipGetAttachmentContent", "Get attachment content"),
        ("paperclipListIssueDocumentAnnotations", "List issue document annotations"),
        ("paperclipCreateIssueDocumentAnnotation", "Create an issue document annotation"),
        ("paperclipGetIssueDocumentAnnotationThread", "Get an issue document annotation thread"),
        ("paperclipReplyIssueDocumentAnnotation", "Reply to an issue document annotation"),
        ("paperclipUpdateIssueDocumentAnnotation", "Update an issue document annotation"),
        ("paperclipListIssueExternalObjects", "List issue external objects"),
        ("paperclipRefreshIssueExternalObjects", "Refresh issue external objects"),
        ("paperclipListIssueFileResources", "List issue file resources"),
        ("paperclipGetIssueFileResourceContent", "Get issue file resource content"),
        ("paperclipResolveIssueFileResource", "Resolve an issue file resource"),
        ("paperclipListLabels", "List labels"),
        ("paperclipCreateLabel", "Create a label"),
        ("paperclipDeleteLabel", "Delete a label"),
        ("paperclipListRoutines", "List routines"),
        ("paperclipGetRoutine", "Get a routine"),
        ("paperclipCreateRoutine", "Create a routine"),
        ("paperclipUpdateRoutine", "Update a routine"),
        ("paperclipListRoutineRevisions", "List routine revisions"),
        ("paperclipRestoreRoutineRevision", "Restore a routine revision"),
        ("paperclipListRoutineDescriptionAnnotations", "List routine description annotations"),
        ("paperclipCreateRoutineDescriptionAnnotation", "Create a routine description annotation"),
        ("paperclipGetRoutineDescriptionAnnotationThread", "Get a routine description annotation thread"),
        ("paperclipReplyRoutineDescriptionAnnotation", "Reply to a routine description annotation"),
        ("paperclipUpdateRoutineDescriptionAnnotation", "Update a routine description annotation"),
        ("paperclipListRoutineRuns", "List routine runs"),
        ("paperclipRunRoutine", "Run a routine"),
        ("paperclipCreateRoutineTrigger", "Create a routine trigger"),
        ("paperclipUpdateRoutineTrigger", "Update a routine trigger"),
        ("paperclipDeleteRoutineTrigger", "Delete a routine trigger"),
        ("paperclipRotateRoutineTriggerSecret", "Rotate a routine trigger secret"),
    ];
    TOOLS
        .iter()
        .chain(PARROT_EXTENDED_TOOLS.iter())
        .map(|(name, description)| {
            let input_schema = match *name {
                "paperclipMe" | "paperclipInboxLite" => serde_json::json!({
                    "type": "object", "properties": {}, "additionalProperties": false
                }),
                "paperclipGetAgent" => serde_json::json!({
                    "type": "object", "properties": {"agentId": {"type": "string"}, "companyId": {"type": ["string", "null"], "format": "uuid"}},
                    "required": ["agentId"], "additionalProperties": false
                }),
                "paperclipListAgents" | "paperclipListProjects" | "paperclipListGoals" => serde_json::json!({
                    "type": "object", "properties": {"companyId": {"type": ["string", "null"], "format": "uuid"}},
                    "additionalProperties": false
                }),
                "paperclipListIssues" => serde_json::json!({
                    "type": "object", "properties": {
                        "companyId": {"type": ["string", "null"], "format": "uuid"},
                        "status": {"type": "string"}, "projectId": {"type": "string", "format": "uuid"},
                        "parentId": {"type": "string", "format": "uuid"}, "goalId": {"type": "string", "format": "uuid"},
                        "assigneeAgentId": {"type": "string", "format": "uuid"},
                        "participantAgentId": {"type": "string", "format": "uuid"},
                        "assigneeUserId": {"type": "string"}, "touchedByUserId": {"type": "string"},
                        "inboxArchivedByUserId": {"type": "string"}, "unreadForUserId": {"type": "string"},
                        "labelId": {"type": "string", "format": "uuid"},
                        "executionWorkspaceId": {"type": "string", "format": "uuid"},
                        "originKind": {"type": "string"}, "originId": {"type": "string"},
                        "includeRoutineExecutions": {"type": "boolean"},
                        "includeLiveDescendantSummary": {"type": "boolean"}, "q": {"type": "string"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 500}, "offset": {"type": "integer", "minimum": 0}
                    }, "additionalProperties": false
                }),
                "paperclipGetIssue" | "paperclipListIssueApprovals" | "paperclipListDocuments" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}},
                    "required": ["issueId"], "additionalProperties": false
                }),
                "paperclipGetHeartbeatContext" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string"},
                        "wakeCommentId": {"type": "string", "format": "uuid"}
                    }, "required": ["issueId"], "additionalProperties": false
                }),
                "paperclipListComments" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}, "after": {"type": "string", "format": "uuid"}, "order": {"type": "string", "enum": ["asc", "desc"]}, "limit": {"type": "integer", "minimum": 1, "maximum": 500}},
                    "required": ["issueId"], "additionalProperties": false
                }),
                "paperclipGetComment" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}, "commentId": {"type": "string"}},
                    "required": ["issueId", "commentId"], "additionalProperties": false
                }),
                "paperclipGetDocument" | "paperclipListDocumentRevisions" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}, "key": {"type": "string", "minLength": 1, "maxLength": 64}},
                    "required": ["issueId", "key"], "additionalProperties": false
                }),
                "paperclipGetProject" => serde_json::json!({
                    "type": "object", "properties": {"projectId": {"type": "string"}, "companyId": {"type": ["string", "null"], "format": "uuid"}},
                    "required": ["projectId"], "additionalProperties": false
                }),
                "paperclipGetGoal" => serde_json::json!({
                    "type": "object", "properties": {"goalId": {"type": "string", "format": "uuid"}},
                    "required": ["goalId"], "additionalProperties": false
                }),
                "paperclipGetApproval" | "paperclipGetApprovalIssues" | "paperclipListApprovalComments" => serde_json::json!({
                    "type": "object", "properties": {"approvalId": {"type": "string", "format": "uuid"}},
                    "required": ["approvalId"], "additionalProperties": false
                }),
                "paperclipCreateIssue" => serde_json::json!({
                    "type": "object", "properties": {
                        "companyId": {"type": ["string", "null"], "format": "uuid"}, "projectId": {"type": ["string", "null"], "format": "uuid"},
                        "projectWorkspaceId": {"type": ["string", "null"], "format": "uuid"}, "goalId": {"type": ["string", "null"], "format": "uuid"},
                        "parentId": {"type": ["string", "null"], "format": "uuid"}, "blockedByIssueIds": {"type": "array", "items": {"type": "string", "format": "uuid"}},
                        "inheritExecutionWorkspaceFromIssueId": {"type": ["string", "null"], "format": "uuid"}, "title": {"type": "string", "minLength": 1},
                        "description": {"type": ["string", "null"]}, "status": {"type": "string", "enum": ["backlog", "todo", "in_progress", "blocked", "in_review", "done", "cancelled"]}, "workMode": {"type": "string", "enum": ["standard", "ask", "planning", "skill_test"]},
                        "harnessKind": {"type": ["string", "null"], "enum": ["skill_test", null]}, "priority": {"type": "string", "enum": ["urgent", "high", "medium", "low", "no_priority"]}, "assigneeAgentId": {"type": ["string", "null"], "format": "uuid"},
                        "assigneeUserId": {"type": ["string", "null"]}, "requestDepth": {"type": "integer", "minimum": 0},
                        "billingCode": {"type": ["string", "null"]}, "assigneeAdapterOverrides": {"type": ["object", "null"]},
                        "createdByUserId": {"type": ["string", "null"]}, "responsibleUserId": {"type": ["string", "null"]},
                        "watchdog": {"type": ["object", "null"], "properties": {"agentId": {"type": "string", "format": "uuid"}, "instructions": {"type": ["string", "null"]}}, "required": ["agentId"], "additionalProperties": false}, "executionPolicy": {"type": ["object", "null"]}, "executionWorkspaceId": {"type": ["string", "null"], "format": "uuid"},
                        "executionWorkspacePreference": {"type": ["string", "null"]}, "executionWorkspaceSettings": {"type": ["object", "null"]},
                        "labelIds": {"type": "array", "items": {"type": "string", "format": "uuid"}}, "watchdogDiscovery": {"type": ["object", "null"], "properties": {"kind": {"type": "string", "enum": ["product_bug"]}, "evidenceMarkdown": {"type": ["string", "null"]}}, "required": ["kind"], "additionalProperties": false}
                    }, "required": ["title"], "additionalProperties": false
                }),
                "paperclipUpdateIssue" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string"}, "projectId": {"type": ["string", "null"], "format": "uuid"},
                        "projectWorkspaceId": {"type": ["string", "null"], "format": "uuid"}, "goalId": {"type": ["string", "null"], "format": "uuid"},
                        "parentId": {"type": ["string", "null"], "format": "uuid"}, "title": {"type": "string"}, "description": {"type": ["string", "null"]},
                        "status": {"type": "string", "enum": ["backlog", "todo", "in_progress", "blocked", "in_review", "done", "cancelled"]}, "workMode": {"type": "string", "enum": ["standard", "ask", "planning", "skill_test"]}, "harnessKind": {"type": ["string", "null"], "enum": ["skill_test", null]},
                        "priority": {"type": "string", "enum": ["urgent", "high", "medium", "low", "no_priority"]}, "assigneeAgentId": {"type": ["string", "null"]}, "assigneeUserId": {"type": ["string", "null"]},
                        "comment": {"type": "string"}, "reviewRequest": {"type": ["object", "null"]}, "hiddenAt": {"type": ["string", "null"], "format": "date-time"}, "reopen": {"type": "boolean"},
                        "resume": {"type": "boolean"}, "interrupt": {"type": "boolean"}, "requestDepth": {"type": "integer", "minimum": 0},
                        "executionPolicy": {"type": ["object", "null"]}, "executionWorkspacePreference": {"type": ["string", "null"]},
                        "executionWorkspaceSettings": {"type": ["object", "null"]}, "labelIds": {"type": "array", "items": {"type": "string", "format": "uuid"}}
                    }, "required": ["issueId"], "additionalProperties": false
                }),
                "paperclipCheckoutIssue" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}, "agentId": {"type": "string"}, "expectedStatuses": {"type": "array", "items": {"type": "string"}}},
                    "required": ["issueId"], "additionalProperties": false
                }),
                "paperclipReleaseIssue" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}, "result": {"type": "string"}, "targetStatus": {"type": "string"}},
                    "required": ["issueId"], "additionalProperties": false
                }),
                "paperclipAddComment" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string"}, "body": {"type": "string", "minLength": 1},
                        "authorType": {"type": "string", "enum": ["user", "agent", "system"]},
                        "presentation": {"type": ["object", "null"], "properties": {
                            "kind": {"type": "string", "enum": ["message", "system_notice"]},
                            "tone": {"type": "string", "enum": ["neutral", "info", "success", "warning", "danger"]},
                            "title": {"type": ["string", "null"], "maxLength": 160}, "detailsDefaultOpen": {"type": "boolean"}
                        }, "additionalProperties": false},
                        "metadata": {"type": ["object", "null"], "properties": {
                            "version": {"type": "integer", "const": 1}, "sourceRunId": {"type": ["string", "null"], "format": "uuid"},
                            "sections": {"type": "array", "minItems": 1, "maxItems": 20, "items": {"type": "object", "properties": {
                                "title": {"type": ["string", "null"], "maxLength": 160}, "rows": {"type": "array", "minItems": 1, "maxItems": 50, "items": {"type": "object", "properties": {
                                    "type": {"type": "string", "enum": ["text", "code", "key_value", "issue_link", "agent_link", "run_link"]},
                                    "label": {"type": ["string", "null"], "maxLength": 120}, "text": {"type": "string", "maxLength": 2000},
                                    "code": {"type": "string", "minLength": 1, "maxLength": 4000}, "language": {"type": ["string", "null"], "maxLength": 40},
                                    "value": {"type": "string", "maxLength": 2000}, "issueId": {"type": ["string", "null"], "format": "uuid"},
                                    "identifier": {"type": ["string", "null"], "maxLength": 80}, "title": {"type": ["string", "null"], "maxLength": 240},
                                    "agentId": {"type": "string", "format": "uuid"}, "name": {"type": ["string", "null"], "maxLength": 160},
                                    "runId": {"type": "string", "format": "uuid"}
                                }, "required": ["type"], "additionalProperties": false}}
                            }, "required": ["rows"], "additionalProperties": false}}
                        }, "required": ["version", "sections"], "additionalProperties": false},
                        "reopen": {"type": "boolean"}, "resume": {"type": "boolean"}, "interrupt": {"type": "boolean"}
                    }, "required": ["issueId", "body"], "additionalProperties": false
                }),
                "paperclipUpsertIssueDocument" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}, "key": {"type": "string", "minLength": 1, "maxLength": 64}, "title": {"type": ["string", "null"]}, "format": {"type": "string", "enum": ["markdown"]}, "body": {"type": "string", "maxLength": 524288}, "changeSummary": {"type": ["string", "null"]}, "baseRevisionId": {"type": ["string", "null"]}},
                    "required": ["issueId", "key", "body"], "additionalProperties": false
                }),
                "paperclipRestoreIssueDocumentRevision" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}, "key": {"type": "string"}, "revisionId": {"type": "string", "format": "uuid"}},
                    "required": ["issueId", "key", "revisionId"], "additionalProperties": false
                }),
                "paperclipLinkIssueApproval" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}, "approvalId": {"type": "string", "format": "uuid"}},
                    "required": ["issueId", "approvalId"], "additionalProperties": false
                }),
                "paperclipUnlinkIssueApproval" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}, "approvalId": {"type": "string", "format": "uuid"}},
                    "required": ["issueId", "approvalId"], "additionalProperties": false
                }),
                "paperclipApprovalDecision" => serde_json::json!({
                    "type": "object", "properties": {"approvalId": {"type": "string"}, "action": {"type": "string", "enum": ["approve", "reject", "requestRevision", "resubmit"]}, "decisionNote": {"type": "string"}, "payloadJson": {"type": "string"}},
                    "required": ["approvalId", "action"], "additionalProperties": false
                }),
                "paperclipAddApprovalComment" => serde_json::json!({
                    "type": "object", "properties": {"approvalId": {"type": "string"}, "body": {"type": "string"}},
                    "required": ["approvalId", "body"], "additionalProperties": false
                }),
                "paperclipCreateApproval" => serde_json::json!({
                    "type": "object", "properties": {
                        "companyId": {"type": ["string", "null"], "format": "uuid"},
                        "type": {"type": "string", "enum": ["hire_agent", "approve_ceo_strategy", "budget_override_required", "request_board_approval"]}, "requestedByAgentId": {"type": ["string", "null"], "format": "uuid"},
                        "payload": {"type": "object"}, "issueIds": {"type": "array", "items": {"type": "string", "format": "uuid"}}
                    }, "required": ["type", "payload"], "additionalProperties": false
                }),
                "paperclipHireAgent" => serde_json::json!({
                    "type": "object",
                    "properties": {
                        "companyId": {"type": ["string", "null"], "format": "uuid"},
                        "name": {"type": "string", "minLength": 1, "maxLength": 255, "description": "Agent name"},
                        "role": {"type": "string", "enum": ["ceo", "vp", "manager", "researcher", "general", "cto", "cmo", "cfo", "security", "engineer", "designer", "pm", "qa", "devops"], "description": "Agent role (Paperclip specialist roles are mapped to Parrot's role buckets)"},
                        "title": {"type": ["string", "null"], "maxLength": 255, "description": "Agent job title"},
                        "icon": {"type": ["string", "null"], "description": "Agent icon"},
                        "reportsTo": {"type": ["string", "null"], "format": "uuid", "description": "ID of the agent this agent reports to"},
                        "capabilities": {"type": ["string", "null"], "description": "Agent capabilities description"},
                        "adapterType": {"type": "string", "description": "Adapter type (e.g., claude_local, anthropic)"},
                        "adapterConfig": {"type": ["object", "null"], "description": "Adapter-specific configuration"},
                        "runtimeConfig": {"type": ["object", "null"], "description": "Runtime configuration"},
                        "permissions": {"type": ["object", "null"], "description": "Agent permissions"},
                        "budgetMonthlyCents": {"type": ["integer", "null"], "description": "Monthly budget in cents"},
                        "defaultEnvironmentId": {"type": ["string", "null"], "format": "uuid", "description": "Default execution environment"},
                        "metadata": {"type": ["object", "null"], "description": "Additional metadata"},
                        "desiredSkills": {"type": ["array", "null"], "items": {"type": "string"}, "description": "List of desired skills"},
                        "instructionsBundle": {"type": ["object", "null"], "description": "Instructions bundle"},
                        "sourceIssueId": {"type": ["string", "null"], "format": "uuid", "description": "Issue that requested this hire"},
                        "sourceIssueIds": {"type": ["array", "null"], "items": {"type": "string", "format": "uuid"}, "description": "Issues that requested this hire"},
                        "issueIds": {"type": ["array", "null"], "items": {"type": "string", "format": "uuid"}, "description": "Legacy alias for sourceIssueIds"}
                    },
                    "required": ["name", "role", "adapterType"],
                    "additionalProperties": false
                }),
                "paperclipSuggestTasks" | "paperclipAskUserQuestions"
                | "paperclipRequestConfirmation" | "paperclipRequestCheckboxConfirmation" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string"}, "idempotencyKey": {"type": ["string", "null"]},
                        "sourceCommentId": {"type": ["string", "null"], "format": "uuid"},
                        "sourceRunId": {"type": ["string", "null"], "format": "uuid"},
                        "title": {"type": ["string", "null"]}, "summary": {"type": ["string", "null"]},
                        "continuationPolicy": {"type": "string"}, "payload": {"type": "object"}
                    }, "required": ["issueId", "payload"], "additionalProperties": false
                }),
                "paperclipGetIssueWorkspaceRuntime" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}},
                    "required": ["issueId"], "additionalProperties": false
                }),
                "paperclipControlIssueWorkspaceServices" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string"}, "action": {"type": "string", "enum": ["start", "stop", "restart"]},
                        "workspaceCommandId": {"type": ["string", "null"]}, "runtimeServiceId": {"type": ["string", "null"], "format": "uuid"},
                        "serviceIndex": {"type": ["integer", "null"], "minimum": 0}
                    }, "required": ["issueId", "action"], "additionalProperties": false
                }),
                "paperclipWaitForIssueWorkspaceService" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string"}, "runtimeServiceId": {"type": ["string", "null"], "format": "uuid"},
                        "serviceName": {"type": ["string", "null"]}, "timeoutSeconds": {"type": "integer", "minimum": 1, "maximum": 300}
                    }, "required": ["issueId"], "additionalProperties": false
                }),
                "paperclipListApprovals" => serde_json::json!({
                    "type": "object", "properties": {
                        "companyId": {"type": ["string", "null"], "format": "uuid"}, "status": {"type": "string"}
                    }, "additionalProperties": false
                }),
                "paperclipListCases" => serde_json::json!({
                    "type": "object", "properties": {
                        "companyId": {"type": ["string", "null"], "format": "uuid"},
                        "type": {"type": "string"}, "types": {"type": "array", "items": {"type": "string"}},
                        "status": {"type": "string"}, "statuses": {"type": "array", "items": {"type": "string"}},
                        "project": {"type": "string", "format": "uuid"}, "projectId": {"type": "string", "format": "uuid"},
                        "projectIds": {"type": "array", "items": {"type": "string", "format": "uuid"}},
                        "includeNoProject": {"type": "boolean"}, "label": {"type": "string", "format": "uuid"},
                        "labelId": {"type": "string", "format": "uuid"}, "parent": {"type": "string", "format": "uuid"},
                        "q": {"type": "string", "maxLength": 200}, "includeAncestors": {"type": "boolean"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 200}
                    }, "additionalProperties": false
                }),
                "paperclipGetCase" => serde_json::json!({
                    "type": "object", "properties": {"caseId": {"type": "string"}},
                    "required": ["caseId"], "additionalProperties": false
                }),
                "paperclipCreateCase" => serde_json::json!({
                    "type": "object", "properties": {
                        "companyId": {"type": ["string", "null"], "format": "uuid"},
                        "projectId": {"type": ["string", "null"], "format": "uuid"},
                        "caseType": {"type": "string", "minLength": 1, "maxLength": 120},
                        "key": {"type": ["string", "null"], "minLength": 1, "maxLength": 512},
                        "title": {"type": "string", "minLength": 1, "maxLength": 500},
                        "summary": {"type": ["string", "null"], "maxLength": 8000},
                        "status": {"type": "string", "enum": ["draft", "in_progress", "in_review", "approved", "done", "cancelled"]},
                        "fields": {"type": ["object", "null"]},
                        "parentCaseId": {"type": ["string", "null"], "format": "uuid"}
                    }, "required": ["caseType", "title"], "additionalProperties": false
                }),
                "paperclipUpdateCase" => serde_json::json!({
                    "type": "object", "properties": {
                        "caseId": {"type": "string"},
                        "projectId": {"type": ["string", "null"], "format": "uuid"},
                        "title": {"type": "string", "minLength": 1, "maxLength": 500},
                        "summary": {"type": ["string", "null"], "maxLength": 8000},
                        "status": {"type": "string", "enum": ["draft", "in_progress", "in_review", "approved", "done", "cancelled"]},
                        "fields": {"type": ["object", "null"]},
                        "parentCaseId": {"type": ["string", "null"], "format": "uuid"},
                        "labelIds": {"type": "array", "items": {"type": "string", "format": "uuid"}, "maxItems": 100}
                    }, "required": ["caseId"], "additionalProperties": false
                }),
                "paperclipListRoutines" => serde_json::json!({
                    "type": "object", "properties": {
                        "companyId": {"type": ["string", "null"], "format": "uuid"}
                    }, "additionalProperties": false
                }),
                "paperclipGetRoutine" => serde_json::json!({
                    "type": "object", "properties": {"routineId": {"type": "string"}},
                    "required": ["routineId"], "additionalProperties": false
                }),
                "paperclipCreateRoutine" => serde_json::json!({
                    "type": "object", "properties": {
                        "companyId": {"type": ["string", "null"], "format": "uuid"},
                        "assigneeAgentId": {"type": ["string", "null"], "format": "uuid"},
                        "title": {"type": "string", "minLength": 1, "maxLength": 500},
                        "description": {"type": ["string", "null"], "maxLength": 200000},
                        "env": {"type": ["object", "null"]}
                    }, "required": ["title"], "additionalProperties": false
                }),
                "paperclipUpdateRoutine" => serde_json::json!({
                    "type": "object", "properties": {
                        "routineId": {"type": "string"},
                        "assigneeAgentId": {"type": ["string", "null"], "format": "uuid"},
                        "title": {"type": "string", "minLength": 1, "maxLength": 500},
                        "description": {"type": ["string", "null"], "maxLength": 200000},
                        "env": {"type": ["object", "null"]}
                    }, "required": ["routineId"], "additionalProperties": false
                }),
                "paperclipListIssueDocumentAnnotations" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string"},
                        "key": {"type": "string", "minLength": 1, "maxLength": 64}
                    }, "required": ["issueId", "key"], "additionalProperties": false
                }),
                "paperclipGetIssueDocumentAnnotationThread" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string"},
                        "key": {"type": "string", "minLength": 1, "maxLength": 64},
                        "threadId": {"type": "string", "format": "uuid"}
                    }, "required": ["issueId", "key", "threadId"], "additionalProperties": false
                }),
                "paperclipCreateIssueDocumentAnnotation" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string"},
                        "key": {"type": "string", "minLength": 1, "maxLength": 64},
                        "body": {"type": "string", "minLength": 1},
                        "selectedText": {"type": "string"},
                        "anchorSelector": {"type": ["object", "null"]},
                        "selector": {"type": ["object", "null"]},
                        "resolved": {"type": "boolean"}
                    }, "required": ["issueId", "key", "body"], "additionalProperties": false
                }),
                "paperclipReplyIssueDocumentAnnotation" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string"},
                        "key": {"type": "string", "minLength": 1, "maxLength": 64},
                        "threadId": {"type": "string", "format": "uuid"},
                        "body": {"type": "string", "minLength": 1}
                    }, "required": ["issueId", "key", "threadId", "body"], "additionalProperties": false
                }),
                "paperclipUpdateIssueDocumentAnnotation" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string"},
                        "key": {"type": "string", "minLength": 1, "maxLength": 64},
                    "threadId": {"type": "string", "format": "uuid"},
                        "resolved": {"type": "boolean"}
                    }, "required": ["issueId", "key", "threadId"], "additionalProperties": false
                }),
                "paperclipListLabels" => serde_json::json!({
                    "type": "object", "properties": {"companyId": {"type": ["string", "null"], "format": "uuid"}}, "additionalProperties": false
                }),
                "paperclipCreateLabel" => serde_json::json!({
                    "type": "object", "properties": {"companyId": {"type": ["string", "null"], "format": "uuid"}, "name": {"type": "string", "minLength": 1}, "color": {"type": "string", "minLength": 1}, "description": {"type": ["string", "null"]}}, "required": ["name", "color"], "additionalProperties": false
                }),
                "paperclipDeleteLabel" => serde_json::json!({
                    "type": "object", "properties": {"labelId": {"type": "string", "format": "uuid"}}, "required": ["labelId"], "additionalProperties": false
                }),
                "paperclipListIssueExternalObjects" | "paperclipRefreshIssueExternalObjects" | "paperclipListIssueFileResources" | "paperclipResolveIssueFileResource" | "paperclipGetIssueFileResourceContent" | "paperclipListIssueAttachments" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string"}}, "required": ["issueId"], "additionalProperties": false
                }),
                "paperclipGetCaseChildren" => serde_json::json!({
                    "type": "object", "properties": {"caseId": {"type": "string"}}, "required": ["caseId"], "additionalProperties": false
                }),
                "paperclipCreateCaseLink" => serde_json::json!({
                    "type": "object", "properties": {"caseId": {"type": "string"}, "issueId": {"type": "string", "format": "uuid"}, "role": {"type": "string", "enum": ["origin", "work", "reference"]}}, "required": ["caseId", "issueId", "role"], "additionalProperties": false
                }),
                "paperclipGetIssueCases" => serde_json::json!({
                    "type": "object", "properties": {"issueId": {"type": "string", "format": "uuid"}}, "required": ["issueId"], "additionalProperties": false
                }),
                "paperclipGetAttachmentContent" | "paperclipDeleteAttachment" => serde_json::json!({
                    "type": "object", "properties": {"attachmentId": {"type": "string", "format": "uuid"}}, "required": ["attachmentId"], "additionalProperties": false
                }),
                "paperclipCreateIssueAttachment" => serde_json::json!({
                    "type": "object", "properties": {
                        "issueId": {"type": "string", "format": "uuid"},
                        "filename": {"type": "string"},
                        "contentType": {"type": "string"},
                        "base64Content": {"type": "string"}
                    }, "required": ["issueId", "filename", "contentType", "base64Content"], "additionalProperties": false
                }),
                "paperclipListCaseDocuments" | "paperclipGetCaseEvents" => serde_json::json!({
                    "type": "object", "properties": {"caseId": {"type": "string"}}, "required": ["caseId"], "additionalProperties": false
                }),
                "paperclipGetCaseDocument" | "paperclipListCaseDocumentRevisions" | "paperclipDeleteCaseDocument" | "paperclipLockCaseDocument" | "paperclipUnlockCaseDocument" | "paperclipListCaseDocumentAnnotations" => serde_json::json!({
                    "type": "object", "properties": {"caseId": {"type": "string"}, "key": {"type": "string"}}, "required": ["caseId", "key"], "additionalProperties": false
                }),
                "paperclipUpsertCaseDocument" | "paperclipCreateCaseDocumentAnnotation" => serde_json::json!({
                    "type": "object", "properties": {"caseId": {"type": "string"}, "key": {"type": "string"}, "body": {"type": "string"}}, "required": ["caseId", "key", "body"], "additionalProperties": false
                }),
                "paperclipGetCaseDocumentAnnotationThread" | "paperclipUpdateCaseDocumentAnnotation" => serde_json::json!({
                    "type": "object", "properties": {"caseId": {"type": "string"}, "key": {"type": "string"}, "threadId": {"type": "string", "format": "uuid"}}, "required": ["caseId", "key", "threadId"], "additionalProperties": false
                }),
                "paperclipRestoreCaseDocumentRevision" => serde_json::json!({
                    "type": "object", "properties": {"caseId": {"type": "string"}, "key": {"type": "string"}, "revisionId": {"type": "string", "format": "uuid"}}, "required": ["caseId", "key", "revisionId"], "additionalProperties": false
                }),
                "paperclipReplyCaseDocumentAnnotation" => serde_json::json!({
                    "type": "object", "properties": {"caseId": {"type": "string"}, "key": {"type": "string"}, "threadId": {"type": "string", "format": "uuid"}, "body": {"type": "string"}}, "required": ["caseId", "key", "threadId", "body"], "additionalProperties": false
                }),
                "paperclipListRoutineRevisions" | "paperclipListRoutineDescriptionAnnotations" | "paperclipCreateRoutineTrigger" | "paperclipListRoutineRuns" | "paperclipRunRoutine" => serde_json::json!({
                    "type": "object", "properties": {"routineId": {"type": "string", "format": "uuid"}}, "required": ["routineId"], "additionalProperties": false
                }),
                "paperclipRestoreRoutineRevision" => serde_json::json!({
                    "type": "object", "properties": {"routineId": {"type": "string", "format": "uuid"}, "revisionId": {"type": "string", "format": "uuid"}}, "required": ["routineId", "revisionId"], "additionalProperties": false
                }),
                "paperclipGetRoutineDescriptionAnnotationThread" | "paperclipUpdateRoutineDescriptionAnnotation" => serde_json::json!({
                    "type": "object", "properties": {"routineId": {"type": "string", "format": "uuid"}, "threadId": {"type": "string", "format": "uuid"}}, "required": ["routineId", "threadId"], "additionalProperties": false
                }),
                "paperclipCreateRoutineDescriptionAnnotation" => serde_json::json!({
                    "type": "object", "properties": {"routineId": {"type": "string", "format": "uuid"}, "body": {"type": "string"}}, "required": ["routineId", "body"], "additionalProperties": false
                }),
                "paperclipReplyRoutineDescriptionAnnotation" => serde_json::json!({
                    "type": "object", "properties": {"routineId": {"type": "string", "format": "uuid"}, "threadId": {"type": "string", "format": "uuid"}, "body": {"type": "string"}}, "required": ["routineId", "threadId", "body"], "additionalProperties": false
                }),
                "paperclipUpdateRoutineTrigger" | "paperclipDeleteRoutineTrigger" | "paperclipRotateRoutineTriggerSecret" => serde_json::json!({
                    "type": "object", "properties": {"triggerId": {"type": "string", "format": "uuid"}}, "required": ["triggerId"], "additionalProperties": false
                }),
                "paperclipApiRequest" => serde_json::json!({
                    "type": "object", "properties": {"method": {"type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"]}, "path": {"type": "string"}, "jsonBody": {"type": "string"}},
                    "required": ["method", "path"], "additionalProperties": false
                }),
                _ => serde_json::json!({
                    "type": "object", "properties": {}, "additionalProperties": false
                }),
            };
            McpToolDefinition {
                name,
                description,
                input_schema,
            }
        })
        .collect()
}

fn paperclip_builtin_tools() -> Vec<Value> {
    paperclip_builtin_tool_definitions()
        .into_iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.input_schema,
                "source": "paperclip_builtin"
            })
        })
        .collect()
}

fn is_paperclip_builtin_tool(name: &str) -> bool {
    paperclip_builtin_tools()
        .iter()
        .any(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
}

fn is_gateway_virtual_tool(name: &str) -> bool {
    matches!(name, "search_tools" | "run_tool")
}

fn allow_first_party_tool_on_default_deny(
    tool_name: &str,
    reason_code: &str,
    has_active_profile: bool,
) -> bool {
    // Paperclip's built-in API tools are part of the agent runtime contract;
    // they must be discoverable even when the company has not configured an
    // optional tool profile. This exception is deliberately narrow: a block,
    // approval requirement, rate limit, or any other explicit decision still
    // wins, and connected/plugin tools remain default-deny.
    reason_code == "deny_default"
        && !has_active_profile
        && (is_paperclip_builtin_tool(tool_name) || is_gateway_virtual_tool(tool_name))
}

fn validate_paperclip_arguments(tool_name: &str, parameters: &Value) -> Result<(), String> {
    let Some(object) = parameters.as_object() else {
        return Err("tool arguments must be a JSON object".to_string());
    };
    let schema = paperclip_builtin_tool_definitions()
        .into_iter()
        .find(|definition| definition.name == tool_name)
        .map(|definition| definition.input_schema)
        .ok_or_else(|| format!("Unknown Paperclip tool: {tool_name}"))?;
    validate_schema_value(parameters, &schema, "$")?;
    let required: &[&str] = match tool_name {
        "paperclipGetAgent" => &["agentId"],
        "paperclipGetIssue"
        | "paperclipGetHeartbeatContext"
        | "paperclipListComments"
        | "paperclipListIssueApprovals"
        | "paperclipListDocuments" => &["issueId"],
        "paperclipGetComment" => &["issueId", "commentId"],
        "paperclipGetDocument" | "paperclipListDocumentRevisions" => &["issueId", "key"],
        "paperclipGetProject" => &["projectId"],
        "paperclipGetGoal" => &["goalId"],
        "paperclipGetApproval" | "paperclipGetApprovalIssues" | "paperclipListApprovalComments" => {
            &["approvalId"]
        }
        "paperclipCreateIssue" => &["title"],
        "paperclipUpdateIssue" | "paperclipCheckoutIssue" | "paperclipReleaseIssue" => &["issueId"],
        "paperclipAddComment" => &["issueId", "body"],
        "paperclipUpsertIssueDocument" => &["issueId", "key", "body"],
        "paperclipRestoreIssueDocumentRevision" => &["issueId", "key", "revisionId"],
        "paperclipLinkIssueApproval" | "paperclipUnlinkIssueApproval" => &["issueId", "approvalId"],
        "paperclipApprovalDecision" => &["approvalId", "action"],
        "paperclipAddApprovalComment" => &["approvalId", "body"],
        "paperclipGetCase" => &["caseId"],
        "paperclipCreateCase" => &["caseType", "title"],
        "paperclipUpdateCase" => &["caseId"],
        "paperclipGetRoutine" => &["routineId"],
        "paperclipCreateRoutine" => &["title"],
        "paperclipUpdateRoutine" => &["routineId"],
        "paperclipListIssueDocumentAnnotations" | "paperclipCreateIssueDocumentAnnotation" => {
            &["issueId", "key"]
        }
        "paperclipGetIssueDocumentAnnotationThread"
        | "paperclipReplyIssueDocumentAnnotation"
        | "paperclipUpdateIssueDocumentAnnotation" => &["issueId", "key", "threadId"],
        "paperclipCreateLabel" => &["name", "color"],
        "paperclipDeleteLabel" => &["labelId"],
        "paperclipListIssueExternalObjects"
        | "paperclipRefreshIssueExternalObjects"
        | "paperclipListIssueFileResources"
        | "paperclipResolveIssueFileResource"
        | "paperclipGetIssueFileResourceContent"
        | "paperclipListIssueAttachments" => &["issueId"],
        "paperclipCreateIssueAttachment" => {
            &["issueId", "filename", "contentType", "base64Content"]
        }
        "paperclipGetCaseChildren" => &["caseId"],
        "paperclipCreateCaseLink" => &["caseId", "issueId", "role"],
        "paperclipGetIssueCases" => &["issueId"],
        "paperclipGetAttachmentContent" | "paperclipDeleteAttachment" => &["attachmentId"],
        "paperclipListCaseDocuments" | "paperclipGetCaseEvents" => &["caseId"],
        "paperclipGetCaseDocument"
        | "paperclipListCaseDocumentRevisions"
        | "paperclipDeleteCaseDocument"
        | "paperclipLockCaseDocument"
        | "paperclipUnlockCaseDocument"
        | "paperclipListCaseDocumentAnnotations" => &["caseId", "key"],
        "paperclipUpsertCaseDocument" | "paperclipCreateCaseDocumentAnnotation" => {
            &["caseId", "key", "body"]
        }
        "paperclipGetCaseDocumentAnnotationThread" | "paperclipUpdateCaseDocumentAnnotation" => {
            &["caseId", "key", "threadId"]
        }
        "paperclipRestoreCaseDocumentRevision" => &["caseId", "key", "revisionId"],
        "paperclipReplyCaseDocumentAnnotation" => &["caseId", "key", "threadId", "body"],
        "paperclipListRoutineRevisions"
        | "paperclipListRoutineDescriptionAnnotations"
        | "paperclipCreateRoutineTrigger"
        | "paperclipListRoutineRuns"
        | "paperclipRunRoutine" => &["routineId"],
        "paperclipRestoreRoutineRevision" => &["routineId", "revisionId"],
        "paperclipGetRoutineDescriptionAnnotationThread"
        | "paperclipUpdateRoutineDescriptionAnnotation" => &["routineId", "threadId"],
        "paperclipCreateRoutineDescriptionAnnotation" => &["routineId", "body"],
        "paperclipReplyRoutineDescriptionAnnotation" => &["routineId", "threadId", "body"],
        "paperclipUpdateRoutineTrigger"
        | "paperclipDeleteRoutineTrigger"
        | "paperclipRotateRoutineTriggerSecret" => &["triggerId"],
        "paperclipApiRequest" => &["method", "path"],
        _ => &[],
    };
    for key in required {
        let present = object.get(*key).filter(|value| !value.is_null()).is_some();
        if !present {
            return Err(format!("{key} is required"));
        }
    }
    for key in [
        "issueId",
        "agentId",
        "projectId",
        "goalId",
        "approvalId",
        "commentId",
        "revisionId",
        "key",
        "body",
        "title",
        "action",
        "method",
        "path",
        "caseId",
        "caseType",
        "routineId",
        "threadId",
        "labelId",
        "name",
        "color",
        "role",
        "attachmentId",
        "triggerId",
        "filename",
        "contentType",
        "base64Content",
    ] {
        if object.contains_key(key) && !object.get(key).is_some_and(Value::is_string) {
            return Err(format!("{key} must be a string"));
        }
    }
    for key in [
        "companyId",
        "projectId",
        "projectWorkspaceId",
        "goalId",
        "parentId",
        "inheritExecutionWorkspaceFromIssueId",
        "assigneeAgentId",
        "executionWorkspaceId",
        "sourceCommentId",
        "sourceRunId",
        "sourceIssueId",
        "baseRevisionId",
        "requestedByAgentId",
        "parentCaseId",
    ] {
        if let Some(value) = object.get(key).filter(|value| !value.is_null()) {
            let raw = value
                .as_str()
                .ok_or_else(|| format!("{key} must be a UUID string"))?;
            Uuid::parse_str(raw).map_err(|_| format!("{key} must be a valid UUID"))?;
        }
    }
    for key in ["blockedByIssueIds", "labelIds", "issueIds", "sourceIssueIds"] {
        if let Some(values) = object.get(key) {
            let values = values
                .as_array()
                .ok_or_else(|| format!("{key} must be an array"))?;
            for value in values {
                let raw = value
                    .as_str()
                    .ok_or_else(|| format!("{key} must contain UUID strings"))?;
                Uuid::parse_str(raw).map_err(|_| format!("{key} must contain valid UUIDs"))?;
            }
        }
    }
    if let Some(limit) = object.get("limit") {
        if !limit.is_u64() || !(1..=500).contains(&limit.as_u64().unwrap_or_default()) {
            return Err("limit must be an integer between 1 and 500".to_string());
        }
    }
    if let Some(order) = object.get("order").and_then(Value::as_str) {
        if !matches!(order, "asc" | "desc") {
            return Err("order must be asc or desc".to_string());
        }
    }
    if matches!(tool_name, "paperclipCreateApproval") {
        if !object.get("type").is_some_and(Value::is_string)
            || !object.get("payload").is_some_and(Value::is_object)
        {
            return Err("type and payload are required for approval creation".to_string());
        }
        if let Some(issue_ids) = object.get("issueIds") {
            if !issue_ids.is_array()
                || issue_ids
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| !id.is_string()))
            {
                return Err("issueIds must be an array of strings".to_string());
            }
        }
    }
    if matches!(
        tool_name,
        "paperclipSuggestTasks"
            | "paperclipAskUserQuestions"
            | "paperclipRequestConfirmation"
            | "paperclipRequestCheckboxConfirmation"
    ) {
        let payload = object.get("payload").ok_or("payload is required")?;
        validate_interaction_payload(tool_name, payload)?;
        if let Some(policy) = object.get("continuationPolicy").and_then(Value::as_str) {
            if !matches!(policy, "none" | "wake_assignee" | "wake_assignee_on_accept") {
                return Err("continuationPolicy is invalid".to_string());
            }
        }
    }
    if let Some(key) = object.get("key").and_then(Value::as_str) {
        let valid = !key.trim().is_empty()
            && key.len() <= 64
            && key
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-');
        if !valid {
            return Err("key must contain only lowercase letters, numbers, '_' or '-' and be at most 64 characters".to_string());
        }
    }
    if let Some(body) = object.get("body").and_then(Value::as_str) {
        if body.is_empty() || body.len() > 524_288 {
            return Err("body must contain between 1 and 524288 characters".to_string());
        }
    }
    if tool_name == "paperclipAddComment" {
        if let Some(presentation) = object.get("presentation").filter(|value| !value.is_null()) {
            let presentation = presentation
                .as_object()
                .ok_or("presentation must be an object or null")?;
            if let Some(kind) = presentation.get("kind").and_then(Value::as_str) {
                if !matches!(kind, "message" | "system_notice") {
                    return Err("presentation.kind must be message or system_notice".to_string());
                }
            }
            if let Some(tone) = presentation.get("tone").and_then(Value::as_str) {
                if !matches!(tone, "neutral" | "info" | "success" | "warning" | "danger") {
                    return Err("presentation.tone is invalid".to_string());
                }
            }
            if let Some(title) = presentation.get("title").filter(|value| !value.is_null()) {
                let title = title
                    .as_str()
                    .ok_or("presentation.title must be a string or null")?;
                if title.trim().is_empty() || title.len() > 160 {
                    return Err("presentation.title must contain 1 to 160 characters".to_string());
                }
            }
            if let Some(value) = presentation.get("detailsDefaultOpen") {
                if !value.is_boolean() {
                    return Err("presentation.detailsDefaultOpen must be a boolean".to_string());
                }
            }
        }
        if let Some(metadata) = object.get("metadata").filter(|value| !value.is_null()) {
            let metadata = metadata
                .as_object()
                .ok_or("metadata must be an object or null")?;
            if metadata.get("version").and_then(Value::as_u64) != Some(1) {
                return Err("metadata.version must be 1".to_string());
            }
            if let Some(source_run_id) =
                metadata.get("sourceRunId").filter(|value| !value.is_null())
            {
                let source_run_id = source_run_id
                    .as_str()
                    .ok_or("metadata.sourceRunId must be a UUID or null")?;
                Uuid::parse_str(source_run_id)
                    .map_err(|_| "metadata.sourceRunId must be a valid UUID".to_string())?;
            }
            let sections = metadata
                .get("sections")
                .and_then(Value::as_array)
                .ok_or("metadata.sections is required")?;
            if sections.is_empty() || sections.len() > 20 {
                return Err("metadata.sections must contain 1 to 20 sections".to_string());
            }
            for section in sections {
                let section = section
                    .as_object()
                    .ok_or("metadata section must be an object")?;
                if let Some(title) = section.get("title").filter(|value| !value.is_null()) {
                    let title = title
                        .as_str()
                        .ok_or("metadata section title must be a string or null")?;
                    if title.trim().is_empty() || title.len() > 160 {
                        return Err(
                            "metadata section title must contain 1 to 160 characters".to_string()
                        );
                    }
                }
                let rows = section
                    .get("rows")
                    .and_then(Value::as_array)
                    .ok_or("metadata section rows is required")?;
                if rows.is_empty() || rows.len() > 50 {
                    return Err("metadata section rows must contain 1 to 50 rows".to_string());
                }
                for row in rows {
                    let row = row.as_object().ok_or("metadata row must be an object")?;
                    let row_type = row
                        .get("type")
                        .and_then(Value::as_str)
                        .ok_or("metadata row type is required")?;
                    if !matches!(
                        row_type,
                        "text" | "code" | "key_value" | "issue_link" | "agent_link" | "run_link"
                    ) {
                        return Err("metadata row type is invalid".to_string());
                    }
                    match row_type {
                        "text" => validate_text_field(row.get("text"), "metadata text")?,
                        "code" => validate_text_field(row.get("code"), "metadata code")?,
                        "key_value" => {
                            validate_text_field(row.get("label"), "metadata key_value label")?;
                            validate_text_field(row.get("value"), "metadata key_value value")?;
                        }
                        "agent_link" => {
                            validate_uuid_field(row.get("agentId"), "metadata agent_link agentId")?
                        }
                        "run_link" => {
                            validate_uuid_field(row.get("runId"), "metadata run_link runId")?
                        }
                        "issue_link" => {
                            let issue_id = row.get("issueId").filter(|value| !value.is_null());
                            let identifier = row.get("identifier").filter(|value| !value.is_null());
                            if issue_id.is_none() && identifier.is_none() {
                                return Err("metadata issue_link requires issueId or identifier"
                                    .to_string());
                            }
                            if let Some(issue_id) = issue_id {
                                validate_uuid_field(Some(issue_id), "metadata issue_link issueId")?;
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
    if let Some(title) = object.get("title").and_then(Value::as_str) {
        if title.trim().is_empty() {
            return Err("title must not be empty".to_string());
        }
    }
    if let Some(status) = object.get("status").and_then(Value::as_str) {
        if !matches!(
            status,
            "backlog" | "todo" | "in_progress" | "blocked" | "in_review" | "done" | "cancelled"
        ) {
            return Err("status is not a valid Paperclip issue status".to_string());
        }
    }
    if let Some(work_mode) = object.get("workMode").and_then(Value::as_str) {
        if !matches!(work_mode, "standard" | "ask" | "planning" | "skill_test") {
            return Err("workMode is not a valid Paperclip issue work mode".to_string());
        }
    }
    if let Some(priority) = object.get("priority").and_then(Value::as_str) {
        if !matches!(
            priority,
            "urgent" | "high" | "medium" | "low" | "no_priority"
        ) {
            return Err("priority is not a valid Paperclip issue priority".to_string());
        }
    }
    if tool_name == "paperclipApprovalDecision" {
        let action = object
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(
            action,
            "approve" | "reject" | "requestRevision" | "resubmit"
        ) {
            return Err("action must be approve, reject, requestRevision, or resubmit".to_string());
        }
        if action == "resubmit" {
            let payload = object
                .get("payloadJson")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            serde_json::from_str::<Value>(payload)
                .map_err(|error| format!("invalid payloadJson: {error}"))?;
        }
    }
    if tool_name == "paperclipApiRequest" {
        let method = object
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(method, "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
            return Err("method must be GET, POST, PUT, PATCH, or DELETE".to_string());
        }
        if let Some(json_body) = object.get("jsonBody").and_then(Value::as_str) {
            serde_json::from_str::<Value>(json_body)
                .map_err(|error| format!("invalid jsonBody: {error}"))?;
        }
    }
    Ok(())
}

fn validate_schema_value(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    if let Some(constant) = schema.get("const") {
        if value != constant {
            return Err(format!("{path} must equal {constant}"));
        }
    }
    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        if !enum_values.iter().any(|candidate| candidate == value) {
            return Err(format!("{path} is not one of the allowed values"));
        }
    }

    let matches_type = |type_name: &str| match type_name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    };
    if let Some(type_value) = schema.get("type") {
        let valid = match type_value {
            Value::String(type_name) => matches_type(type_name),
            Value::Array(types) => types.iter().filter_map(Value::as_str).any(matches_type),
            _ => false,
        };
        if !valid {
            return Err(format!("{path} has an invalid type"));
        }
    }

    if let Some(string) = value.as_str() {
        if let Some(minimum) = schema.get("minLength").and_then(Value::as_u64) {
            if string.chars().count() < minimum as usize {
                return Err(format!("{path} is shorter than the minimum length"));
            }
        }
        if let Some(maximum) = schema.get("maxLength").and_then(Value::as_u64) {
            if string.chars().count() > maximum as usize {
                return Err(format!("{path} exceeds the maximum length"));
            }
        }
        if let Some(format) = schema.get("format").and_then(Value::as_str) {
            match format {
                "uuid" => {
                    Uuid::parse_str(string).map_err(|_| format!("{path} must be a valid UUID"))?;
                }
                "date-time" => {
                    chrono::DateTime::parse_from_rfc3339(string)
                        .map_err(|_| format!("{path} must be a valid RFC3339 date-time"))?;
                }
                _ => {}
            }
        }
    }
    if let Some(number) = value.as_f64() {
        if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64) {
            if number < minimum {
                return Err(format!("{path} is below the minimum"));
            }
        }
        if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64) {
            if number > maximum {
                return Err(format!("{path} is above the maximum"));
            }
        }
    }
    if let Some(array) = value.as_array() {
        if let Some(minimum) = schema.get("minItems").and_then(Value::as_u64) {
            if array.len() < minimum as usize {
                return Err(format!("{path} has fewer items than allowed"));
            }
        }
        if let Some(maximum) = schema.get("maxItems").and_then(Value::as_u64) {
            if array.len() > maximum as usize {
                return Err(format!("{path} has more items than allowed"));
            }
        }
        if let Some(item_schema) = schema.get("items") {
            for (index, item) in array.iter().enumerate() {
                validate_schema_value(item, item_schema, &format!("{path}[{index}]"))?;
            }
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if object.get(name).is_none() || object.get(name).is_some_and(Value::is_null) {
                    return Err(format!("{path}.{name} is required"));
                }
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
            for key in object.keys() {
                if properties.is_none_or(|properties| !properties.contains_key(key)) {
                    return Err(format!("{path}.{key} is not an allowed property"));
                }
            }
        }
        if let Some(properties) = properties {
            for (key, child_schema) in properties {
                if let Some(child) = object.get(key) {
                    validate_schema_value(child, child_schema, &format!("{path}.{key}"))?;
                }
            }
        }
    }
    Ok(())
}

fn validate_text_field(value: Option<&Value>, name: &str) -> Result<(), String> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{name} must be a string"))?;
    if value.trim().is_empty() || value.len() > 4000 {
        return Err(format!("{name} must contain 1 to 4000 characters"));
    }
    Ok(())
}

fn validate_uuid_field(value: Option<&Value>, name: &str) -> Result<(), String> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{name} must be a UUID"))?;
    Uuid::parse_str(value).map_err(|_| format!("{name} must be a valid UUID"))?;
    Ok(())
}

fn validate_interaction_payload(tool_name: &str, payload: &Value) -> Result<(), String> {
    let object = payload.as_object().ok_or("payload must be an object")?;
    if object.get("version").and_then(Value::as_u64) != Some(1) {
        return Err("payload.version must be 1".to_string());
    }
    if tool_name == "paperclipSuggestTasks" {
        let tasks = object
            .get("tasks")
            .and_then(Value::as_array)
            .ok_or("payload.tasks is required")?;
        if tasks.is_empty() || tasks.len() > 50 {
            return Err("payload.tasks must contain 1 to 50 tasks".to_string());
        }
        let mut keys = std::collections::HashSet::new();
        for task in tasks {
            let task = task.as_object().ok_or("suggested task must be an object")?;
            let key = task
                .get("clientKey")
                .and_then(Value::as_str)
                .filter(|v| !v.trim().is_empty())
                .ok_or("task.clientKey is required")?;
            if key.len() > 120 || !keys.insert(key) {
                return Err("task.clientKey must be unique and at most 120 characters".to_string());
            }
            let title = task
                .get("title")
                .and_then(Value::as_str)
                .filter(|v| !v.trim().is_empty())
                .ok_or("task.title is required")?;
            if title.len() > 240 {
                return Err("task.title must be at most 240 characters".to_string());
            }
            if let Some(priority) = task
                .get("priority")
                .filter(|v| !v.is_null())
                .and_then(Value::as_str)
            {
                if !matches!(
                    priority,
                    "urgent" | "high" | "medium" | "low" | "no_priority"
                ) {
                    return Err("task.priority is invalid".to_string());
                }
            }
            if let Some(work_mode) = task
                .get("workMode")
                .filter(|v| !v.is_null())
                .and_then(Value::as_str)
            {
                if !matches!(work_mode, "standard" | "ask" | "planning" | "skill_test") {
                    return Err("task.workMode is invalid".to_string());
                }
            }
            for key in ["parentId", "assigneeAgentId", "projectId", "goalId"] {
                if let Some(value) = task.get(key).filter(|v| !v.is_null()) {
                    validate_uuid_field(Some(value), &format!("task.{key}"))?;
                }
            }
            if task.get("assigneeAgentId").is_some_and(|v| !v.is_null())
                && task.get("assigneeUserId").is_some_and(|v| !v.is_null())
            {
                return Err("suggested tasks can only target one assignee".to_string());
            }
        }
        return Ok(());
    }
    if tool_name == "paperclipAskUserQuestions" {
        let questions = object
            .get("questions")
            .and_then(Value::as_array)
            .ok_or("payload.questions is required")?;
        if questions.is_empty() || questions.len() > 10 {
            return Err("payload.questions must contain 1 to 10 questions".to_string());
        }
        let mut question_ids = std::collections::HashSet::new();
        for question in questions {
            let question = question.as_object().ok_or("question must be an object")?;
            let id = question
                .get("id")
                .and_then(Value::as_str)
                .filter(|v| !v.trim().is_empty())
                .ok_or("question.id is required")?;
            if id.len() > 120 || !question_ids.insert(id) {
                return Err("question.id must be unique and at most 120 characters".to_string());
            }
            let selection = question
                .get("selectionMode")
                .and_then(Value::as_str)
                .ok_or("question.selectionMode is required")?;
            if !matches!(selection, "single" | "multi") {
                return Err("question.selectionMode is invalid".to_string());
            }
            validate_text_field(question.get("prompt"), "question.prompt")?;
            let options = question
                .get("options")
                .and_then(Value::as_array)
                .ok_or("question.options is required")?;
            if options.is_empty() || options.len() > 10 {
                return Err("question.options must contain 1 to 10 options".to_string());
            }
        }
    } else {
        let prompt = object
            .get("prompt")
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .ok_or("payload.prompt is required")?;
        if prompt.len() > 1000 {
            return Err("payload.prompt must be at most 1000 characters".to_string());
        }
        if tool_name != "paperclipRequestCheckboxConfirmation" {
            return Ok(());
        }
        let options = object
            .get("options")
            .and_then(Value::as_array)
            .ok_or("payload.options is required")?;
        if options.is_empty() || options.len() > 20 {
            return Err("payload.options must contain 1 to 20 options".to_string());
        }
        let mut option_ids = std::collections::HashSet::new();
        for option in options {
            let option = option
                .as_object()
                .ok_or("checkbox option must be an object")?;
            let id = option
                .get("id")
                .and_then(Value::as_str)
                .filter(|v| !v.trim().is_empty())
                .ok_or("option.id is required")?;
            if id.len() > 120 || !option_ids.insert(id) {
                return Err("option.id must be unique and at most 120 characters".to_string());
            }
            validate_text_field(option.get("label"), "option.label")?;
        }
        if let Some(min) = object.get("minSelected").and_then(Value::as_i64) {
            if min < 0 || min as usize > options.len() {
                return Err("minSelected is invalid".to_string());
            }
        }
    }
    Ok(())
}

fn parameters_have_only(parameters: &Value, allowed: &[&str]) -> bool {
    parameters
        .as_object()
        .is_some_and(|object| object.keys().all(|key| allowed.contains(&key.as_str())))
}

fn optional_uuid_parameter(parameters: &Value, key: &str) -> Result<Option<Uuid>, String> {
    parameters
        .get(key)
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| format!("{key} must be a UUID string"))
                .and_then(|value| {
                    Uuid::parse_str(value).map_err(|_| format!("{key} must be a valid UUID"))
                })
        })
        .transpose()
}

async fn direct_paperclip_service_call(
    state: &AppState,
    company_id: Uuid,
    agent_id: Uuid,
    run_id: Option<Uuid>,
    tool_name: &str,
    parameters: &Value,
) -> Result<Option<Value>, String> {
    let value = match tool_name {
        "paperclipMe" => {
            let agent = state
                .agent_service
                .get_by_id(agent_id)
                .await
                .map_err(|error| error.to_string())?;
            if agent.company_id != company_id {
                return Err(
                    "authenticated agent does not belong to the gateway company".to_string()
                );
            }
            Some(serde_json::to_value(agent).map_err(|error| error.to_string())?)
        }
        "paperclipInboxLite" => Some(
            state
                .agent_service
                .inbox_lite(agent_id)
                .await
                .map_err(|error| error.to_string())?,
        ),
        "paperclipListAgents" if parameters_have_only(parameters, &["companyId"]) => Some(
            serde_json::to_value(
                state
                    .agent_service
                    .list(company_id)
                    .await
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?,
        ),
        "paperclipGetAgent"
            if parameters_have_only(parameters, &["agentId", "companyId"])
                && parameters
                    .get("agentId")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .is_some() =>
        {
            let requested_id = Uuid::parse_str(
                parameters
                    .get("agentId")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .map_err(|error| error.to_string())?;
            let agent = state
                .agent_service
                .get_by_id(requested_id)
                .await
                .map_err(|error| error.to_string())?;
            if agent.company_id != company_id {
                return Err("agent does not belong to the gateway company".to_string());
            }
            Some(serde_json::to_value(agent).map_err(|error| error.to_string())?)
        }
        "paperclipGetIssue"
            if parameters_have_only(parameters, &["issueId"])
                && parameters
                    .get("issueId")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .is_some() =>
        {
            let issue_id = Uuid::parse_str(
                parameters
                    .get("issueId")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .map_err(|error| error.to_string())?;
            state
                .issue_service
                .get(issue_id, company_id)
                .await
                .map_err(|error| error.to_string())?
                .map(|issue| serde_json::to_value(issue).map_err(|error| error.to_string()))
                .transpose()?
        }
        "paperclipListIssues"
            if parameters_have_only(
                parameters,
                &[
                    "companyId",
                    "status",
                    "priority",
                    "assigneeAgentId",
                    "assigneeUserId",
                    "projectId",
                    "parentId",
                    "goalId",
                    "participantAgentId",
                    "touchedByUserId",
                    "inboxArchivedByUserId",
                    "unreadForUserId",
                    "labelId",
                    "executionWorkspaceId",
                    "originKind",
                    "originId",
                    "q",
                    "limit",
                    "offset",
                ],
            ) =>
        {
            let statuses = parameters
                .get("status")
                .and_then(Value::as_str)
                .map(|status| {
                    status
                        .split(',')
                        .map(|value| {
                            serde_json::from_value(Value::String(value.trim().to_string()))
                                .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let priorities = parameters
                .get("priority")
                .and_then(Value::as_str)
                .map(|priority| {
                    priority
                        .split(',')
                        .map(|value| {
                            serde_json::from_value(Value::String(value.trim().to_string()))
                                .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let filter = IssueQueryFilter {
                status: statuses,
                priority: priorities,
                assignee_agent_id: optional_uuid_parameter(parameters, "assigneeAgentId")?,
                assignee_user_id: optional_uuid_parameter(parameters, "assigneeUserId")?,
                project_id: optional_uuid_parameter(parameters, "projectId")?,
                parent_id: optional_uuid_parameter(parameters, "parentId")?,
                goal_id: optional_uuid_parameter(parameters, "goalId")?,
                participant_agent_id: optional_uuid_parameter(parameters, "participantAgentId")?,
                touched_by_user_id: optional_uuid_parameter(parameters, "touchedByUserId")?,
                inbox_archived_by_user_id: optional_uuid_parameter(
                    parameters,
                    "inboxArchivedByUserId",
                )?,
                unread_for_user_id: optional_uuid_parameter(parameters, "unreadForUserId")?,
                label_id: optional_uuid_parameter(parameters, "labelId")?,
                execution_workspace_id: optional_uuid_parameter(
                    parameters,
                    "executionWorkspaceId",
                )?,
                origin_kind: parameters
                    .get("originKind")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                origin_id: parameters
                    .get("originId")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                search_query: parameters
                    .get("q")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            };
            let pagination = IssuePagination {
                limit: parameters
                    .get("limit")
                    .and_then(Value::as_i64)
                    .unwrap_or(50)
                    .clamp(1, 500),
                offset: parameters
                    .get("offset")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .max(0),
                cursor: None,
            };
            Some(
                serde_json::to_value(
                    state
                        .issue_service
                        .list(company_id, &filter, &pagination)
                        .await
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?,
            )
        }
        "paperclipCreateIssue"
            if parameters_have_only(
                parameters,
                &[
                    "companyId",
                    "projectId",
                    "goalId",
                    "title",
                    "description",
                    "status",
                    "priority",
                    "parentId",
                    "assigneeAgentId",
                    "assigneeUserId",
                ],
            ) =>
        {
            let mut input: CreateIssueInput =
                serde_json::from_value(object_without(parameters, &["companyId"]))
                    .map_err(|error| format!("invalid create issue input: {error}"))?;
            input.company_id = company_id;
            Some(
                serde_json::to_value(
                    state
                        .issue_service
                        .create(input)
                        .await
                        .map_err(|error| error.to_string())?
                        .issue,
                )
                .map_err(|error| error.to_string())?,
            )
        }
        "paperclipUpdateIssue"
            if parameters_have_only(
                parameters,
                &[
                    "issueId",
                    "title",
                    "description",
                    "status",
                    "priority",
                    "assigneeAgentId",
                    "assigneeUserId",
                ],
            ) && parameters
                .get("issueId")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                .is_some() =>
        {
            let issue_id = Uuid::parse_str(
                parameters
                    .get("issueId")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .map_err(|error| error.to_string())?;
            let input: UpdateIssueInput =
                serde_json::from_value(object_without(parameters, &["issueId"]))
                    .map_err(|error| format!("invalid update issue input: {error}"))?;
            Some(
                serde_json::to_value(
                    state
                        .issue_service
                        .update(issue_id, company_id, input)
                        .await
                        .map_err(|error| error.to_string())?
                        .issue,
                )
                .map_err(|error| error.to_string())?,
            )
        }
        "paperclipCheckoutIssue"
            if parameters_have_only(parameters, &["issueId", "agentId", "expectedStatuses"])
                && parameters
                    .get("issueId")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .is_some() =>
        {
            let issue_id = Uuid::parse_str(
                parameters
                    .get("issueId")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .map_err(|error| error.to_string())?;
            let active_run_id =
                run_id.ok_or("paperclipCheckoutIssue requires an active heartbeat run")?;
            let requested_agent =
                optional_uuid_parameter(parameters, "agentId")?.unwrap_or(agent_id);
            if requested_agent != agent_id {
                return Err("checkout agentId must match the gateway agent".to_string());
            }
            let expected_statuses = parameters
                .get("expectedStatuses")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_else(|| {
                    vec![
                        "todo".to_string(),
                        "backlog".to_string(),
                        "blocked".to_string(),
                    ]
                });
            state
                .issue_service
                .checkout(
                    issue_id,
                    company_id,
                    CheckoutInput {
                        agent_id: Some(agent_id),
                        user_id: None,
                        expected_statuses,
                        checkout_run_id: active_run_id,
                    },
                )
                .await
                .map_err(|error| error.to_string())?;
            sqlx::query(
                "UPDATE issues SET assignee_agent_id = $2, checkout_run_id = $3, execution_run_id = $3, updated_at = NOW() WHERE id = $1 AND company_id = $4",
            )
            .bind(issue_id)
            .bind(agent_id)
            .bind(active_run_id)
            .bind(company_id)
            .execute(&state.pool)
            .await
            .map_err(|error| error.to_string())?;
            Some(
                serde_json::to_value(
                    state
                        .issue_service
                        .get(issue_id, company_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "checked out issue disappeared".to_string())?,
                )
                .map_err(|error| error.to_string())?,
            )
        }
        "paperclipReleaseIssue"
            if parameters_have_only(parameters, &["issueId", "result", "targetStatus"])
                && parameters
                    .get("issueId")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .is_some() =>
        {
            let issue_id = Uuid::parse_str(
                parameters
                    .get("issueId")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .map_err(|error| error.to_string())?;
            let active_run_id =
                run_id.ok_or("paperclipReleaseIssue requires an active heartbeat run")?;
            state
                .issue_service
                .release(
                    issue_id,
                    company_id,
                    ReleaseInput {
                        release_run_id: active_run_id,
                        result: parameters
                            .get("result")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        target_status: parameters
                            .get("targetStatus")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                    },
                )
                .await
                .map_err(|error| error.to_string())?;
            sqlx::query(
                "UPDATE issues SET checkout_run_id = NULL, execution_run_id = NULL, execution_locked_at = NULL, updated_at = NOW() WHERE id = $1 AND company_id = $2",
            )
            .bind(issue_id)
            .bind(company_id)
            .execute(&state.pool)
            .await
            .map_err(|error| error.to_string())?;
            Some(
                serde_json::to_value(
                    state
                        .issue_service
                        .get(issue_id, company_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "released issue disappeared".to_string())?,
                )
                .map_err(|error| error.to_string())?,
            )
        }
        "paperclipAddComment"
            if parameters_have_only(parameters, &["issueId", "body", "metadata"])
                && parameters
                    .get("issueId")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .is_some() =>
        {
            let issue_id = Uuid::parse_str(
                parameters
                    .get("issueId")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .map_err(|error| error.to_string())?;
            Some(
                serde_json::to_value(
                    state
                        .issue_comment_service
                        .add_comment(
                            issue_id,
                            parameters
                                .get("body")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            CommentActorType::Agent,
                            Some(agent_id),
                            run_id,
                            parameters
                                .get("metadata")
                                .filter(|value| !value.is_null())
                                .cloned(),
                        )
                        .await
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?,
            )
        }
        _ => None,
    };
    Ok(value)
}

struct ManagedMcpStdioProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    stderr: Arc<Mutex<String>>,
    stderr_task: JoinHandle<()>,
    initialize_response: Value,
}

struct McpStdioSlot {
    process: Option<ManagedMcpStdioProcess>,
    last_used_at: Instant,
    restart_window_started_at: Option<Instant>,
    restart_count: u32,
    restart_backoff_until: Option<Instant>,
}

impl McpStdioSlot {
    fn new() -> Self {
        Self {
            process: None,
            last_used_at: Instant::now(),
            restart_window_started_at: None,
            restart_count: 0,
            restart_backoff_until: None,
        }
    }

    fn record_failure(&mut self, now: Instant, restart_window: Duration, backoff_base: Duration, backoff_max: Duration) {
        let in_same_window = self
            .restart_window_started_at
            .is_some_and(|started| now.duration_since(started) <= restart_window);
        if !in_same_window {
            self.restart_window_started_at = Some(now);
            self.restart_count = 1;
        } else {
            self.restart_count = self.restart_count.saturating_add(1);
        }
        let exponent = self.restart_count.saturating_sub(1).min(6);
        let multiplier = 1_u32 << exponent;
        self.restart_backoff_until = Some(
            now + backoff_base
                .checked_mul(multiplier)
                .unwrap_or(backoff_max)
                .min(backoff_max),
        );
    }

    fn reset_after_success(&mut self) {
        self.restart_window_started_at = None;
        self.restart_count = 0;
        self.restart_backoff_until = None;
    }
}

struct McpStdioSupervisor {
    slots: Mutex<HashMap<String, Arc<Mutex<McpStdioSlot>>>>,
    max_slots: usize,
    idle_ttl: Duration,
    restart_window: Duration,
    restart_backoff: Duration,
    restart_backoff_max: Duration,
    restart_limit: u32,
}

impl McpStdioSupervisor {
    fn new() -> Self {
        let env_duration = |name: &str, default: u64, minimum: u64| {
            std::env::var(name)
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .map(|value| value.max(minimum))
                .unwrap_or(default)
        };
        let max_slots = std::env::var("PARROT_MCP_STDIO_MAX_SLOTS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(16);
        Self {
            slots: Mutex::new(HashMap::new()),
            max_slots,
            idle_ttl: Duration::from_secs(env_duration("PARROT_MCP_STDIO_IDLE_TTL_SECONDS", 60, 1)),
            restart_window: Duration::from_secs(env_duration("PARROT_MCP_STDIO_RESTART_WINDOW_SECONDS", 60, 1)),
            restart_backoff: Duration::from_millis(env_duration("PARROT_MCP_STDIO_RESTART_BACKOFF_MS", 1_000, 0)),
            restart_backoff_max: Duration::from_millis(env_duration("PARROT_MCP_STDIO_RESTART_BACKOFF_MAX_MS", 60_000, 1)),
            restart_limit: std::env::var("PARROT_MCP_STDIO_RESTART_LIMIT")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(3),
        }
    }

    async fn evict_idle_slots(&self) {
        let entries = self
            .slots
            .lock()
            .await
            .iter()
            .map(|(key, slot)| (key.clone(), Arc::clone(slot)))
            .collect::<Vec<_>>();
        for (key, slot) in entries {
            let mut guard = slot.lock().await;
            if guard.last_used_at.elapsed() < self.idle_ttl {
                continue;
            }
            let process = guard.process.take();
            drop(guard);
            if let Some(process) = process {
                terminate_mcp_stdio_process(process).await;
            }
            let mut slots = self.slots.lock().await;
            if slots.get(&key).is_some_and(|current| Arc::ptr_eq(current, &slot)) {
                slots.remove(&key);
            }
        }
    }

    async fn slot_for(&self, key: String) -> Result<Arc<Mutex<McpStdioSlot>>, String> {
        self.evict_idle_slots().await;
        let mut slots = self.slots.lock().await;
        if let Some(slot) = slots.get(&key) {
            return Ok(Arc::clone(slot));
        }
        if slots.len() >= self.max_slots {
            return Err(format!(
                "MCP stdio runtime capacity exhausted (max {} active processes)",
                self.max_slots
            ));
        }
        let slot = Arc::new(Mutex::new(McpStdioSlot::new()));
        slots.insert(key, Arc::clone(&slot));
        Ok(slot)
    }

    async fn request(
        &self,
        command: &str,
        args: &[String],
        method: &str,
        params: Value,
        environment: Option<&HashMap<String, String>>,
    ) -> Result<Value, String> {
        let key = mcp_stdio_slot_key(command, args, environment);
        let slot = self.slot_for(key).await?;
        let mut slot = slot.lock().await;
        slot.ensure_process(
            command,
            args,
            environment,
            self.restart_window,
            self.restart_backoff,
            self.restart_backoff_max,
            self.restart_limit,
        )
        .await?;
        let result = {
            let process = slot
                .process
                .as_mut()
                .ok_or_else(|| "MCP stdio process was not started".to_string())?;
            if method == "initialize" {
                Ok(process.initialize_response.clone())
            } else {
                let request_id = Uuid::new_v4().to_string();
                write_mcp_stdio_message(
                    &mut process.stdin,
                    Some(&request_id),
                    method,
                    params,
                )
                .await?;
                read_mcp_stdio_response(&mut process.stdout, &request_id, Duration::from_secs(30)).await
            }
        };
        slot.last_used_at = Instant::now();
        match result {
            Ok(result) => {
                slot.reset_after_success();
                Ok(result)
            }
            Err(error) => {
                let process = slot.process.take();
                slot.record_failure(
                    Instant::now(),
                    self.restart_window,
                    self.restart_backoff,
                    self.restart_backoff_max,
                );
                drop(slot);
                if let Some(process) = process {
                    let stderr = process.stderr.clone();
                    terminate_mcp_stdio_process(process).await;
                    let stderr = stderr.lock().await.trim().to_string();
                    if !stderr.is_empty() {
                        return Err(format!(
                            "{error}; MCP stderr: {}",
                            stderr.chars().take(4_000).collect::<String>()
                        ));
                    }
                }
                Err(error)
            }
        }
    }
}

impl McpStdioSlot {
    async fn ensure_process(
        &mut self,
        command: &str,
        args: &[String],
        environment: Option<&HashMap<String, String>>,
        restart_window: Duration,
        restart_backoff: Duration,
        restart_backoff_max: Duration,
        restart_limit: u32,
    ) -> Result<(), String> {
        if let Some(process) = self.process.as_mut() {
            match process.child.try_wait() {
                Ok(None) => return Ok(()),
                Ok(Some(_)) => {
                    let process = self.process.take();
                    if let Some(process) = process {
                        terminate_mcp_stdio_process(process).await;
                    }
                    self.record_failure(Instant::now(), restart_window, restart_backoff, restart_backoff_max);
                }
                Err(error) => {
                    let process = self.process.take();
                    if let Some(process) = process {
                        terminate_mcp_stdio_process(process).await;
                    }
                    self.record_failure(Instant::now(), restart_window, restart_backoff, restart_backoff_max);
                    return Err(format!("MCP stdio process status check failed: {error}"));
                }
            }
        }
        let now = Instant::now();
        if self.restart_count >= restart_limit {
            return Err("MCP stdio restart storm suppression is active".to_string());
        }
        if self.restart_backoff_until.is_some_and(|until| until > now) {
            return Err("MCP stdio restart backoff is active".to_string());
        }
        match spawn_mcp_stdio_process(command, args, environment).await {
            Ok(process) => {
                self.process = Some(process);
                Ok(())
            }
            Err(error) => {
                self.record_failure(now, restart_window, restart_backoff, restart_backoff_max);
                Err(error)
            }
        }
    }
}

fn mcp_stdio_slot_key(
    command: &str,
    args: &[String],
    environment: Option<&HashMap<String, String>>,
) -> String {
    let mut material = format!("command={command}\nargs={args:?}\n");
    if let Some(environment) = environment {
        let mut values = environment
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        values.sort_unstable_by(|left, right| left.0.cmp(right.0));
        material.push_str(&format!("env={values:?}"));
    }
    sha256_hex(&material)
}

async fn spawn_mcp_stdio_process(
    command: &str,
    args: &[String],
    environment: Option<&HashMap<String, String>>,
) -> Result<ManagedMcpStdioProcess, String> {
    let mut child_command = Command::new(command);
    child_command
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(environment) = environment {
        child_command.env_clear();
        child_command.envs(environment);
    }
    let mut child = child_command
        .spawn()
        .map_err(|error| format!("MCP stdio process spawn failed: {error}"))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "MCP stdio stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "MCP stdio stdout unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "MCP stdio stderr unavailable".to_string())?;
    let stderr_buffer = Arc::new(Mutex::new(String::new()));
    let stderr_buffer_for_task = Arc::clone(&stderr_buffer);
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut output = stderr_buffer_for_task.lock().await;
            output.push_str(&line);
            output.push('\n');
            if output.len() > 16_000 {
                let keep_from = output.len() - 16_000;
                output.drain(..keep_from);
            }
        }
    });
    let mut process = ManagedMcpStdioProcess {
        child,
        stdin,
        stdout: BufReader::new(stdout).lines(),
        stderr: stderr_buffer,
        stderr_task,
        initialize_response: Value::Null,
    };
    let initialize_id = Uuid::new_v4().to_string();
    let initialize_result = async {
        write_mcp_stdio_message(
            &mut process.stdin,
            Some(&initialize_id),
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "parrot-tool-gateway", "version": env!("CARGO_PKG_VERSION")}
            }),
        )
        .await?;
        let initialize_result = read_mcp_stdio_response(&mut process.stdout, &initialize_id, Duration::from_secs(30)).await?;
        write_mcp_stdio_message(
            &mut process.stdin,
            None,
            "notifications/initialized",
            serde_json::json!({}),
        )
        .await?;
        Ok::<Value, String>(initialize_result)
    }
    .await;
    match initialize_result {
        Ok(initialize_response) => {
            process.initialize_response = initialize_response;
            Ok(process)
        }
        Err(error) => {
            let stderr = process.stderr.clone();
            terminate_mcp_stdio_process(process).await;
            let stderr = stderr.lock().await.trim().to_string();
            if stderr.is_empty() {
                Err(error)
            } else {
                Err(format!("{error}; MCP stderr: {}", stderr.chars().take(4_000).collect::<String>()))
            }
        }
    }
}

async fn terminate_mcp_stdio_process(mut process: ManagedMcpStdioProcess) {
    let _ = process.child.kill().await;
    let _ = process.child.wait().await;
    process.stderr_task.abort();
}

fn mcp_stdio_supervisor() -> &'static Arc<McpStdioSupervisor> {
    static SUPERVISOR: OnceLock<Arc<McpStdioSupervisor>> = OnceLock::new();
    SUPERVISOR.get_or_init(|| Arc::new(McpStdioSupervisor::new()))
}

pub(crate) async fn mcp_stdio_request(
    command: &str,
    args: &[String],
    method: &str,
    params: Value,
) -> Result<Value, String> {
    mcp_stdio_request_with_env(command, args, method, params, None).await
}

/// Execute an MCP stdio request with an optional, explicitly allow-listed
/// environment. Local MCP commands are reviewed through a Paperclip-compatible
/// command template; callers that supply an environment must therefore pass
/// only the variables approved by that template.
pub(crate) async fn mcp_stdio_request_with_env(
    command: &str,
    args: &[String],
    method: &str,
    params: Value,
    environment: Option<&HashMap<String, String>>,
) -> Result<Value, String> {
    if command.trim().is_empty() {
        return Err("MCP stdio command is empty".to_string());
    }
    mcp_stdio_supervisor()
        .request(command, args, method, params, environment)
        .await
}

async fn write_mcp_stdio_message(
    stdin: &mut ChildStdin,
    id: Option<&str>,
    method: &str,
    params: Value,
) -> Result<(), String> {
    let mut message = serde_json::Map::new();
    message.insert("jsonrpc".to_string(), Value::String("2.0".to_string()));
    if let Some(id) = id {
        message.insert("id".to_string(), Value::String(id.to_string()));
    }
    message.insert("method".to_string(), Value::String(method.to_string()));
    message.insert("params".to_string(), params);
    let encoded = serde_json::to_vec(&Value::Object(message)).map_err(|error| error.to_string())?;
    stdin
        .write_all(&encoded)
        .await
        .map_err(|error| format!("MCP stdio write failed: {error}"))?;
    stdin
        .write_all(b"\n")
        .await
        .map_err(|error| format!("MCP stdio write failed: {error}"))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("MCP stdio flush failed: {error}"))
}

async fn read_mcp_stdio_response(
    stdout: &mut Lines<BufReader<ChildStdout>>,
    expected_id: &str,
    timeout: Duration,
) -> Result<Value, String> {
    loop {
        let line = tokio::time::timeout(timeout, stdout.next_line())
            .await
            .map_err(|_| format!("MCP stdio request {expected_id} timed out"))?
            .map_err(|error| format!("MCP stdio read failed: {error}"))?
            .ok_or_else(|| format!("MCP stdio exited before responding to request {expected_id}"))?;
        if line.len() > 1_000_000 {
            return Err("MCP stdio response exceeded the 1 MB gateway limit".to_string());
        }
        let message: Value = serde_json::from_str(line.trim())
            .map_err(|error| format!("MCP stdio returned invalid JSON: {error}"))?;
        if message.get("method").and_then(Value::as_str) == Some("elicitation/create") {
            // The server may ask for user input as a JSON-RPC request while a
            // tools/call response is pending. Return it to the gateway bridge
            // so it can create an issue interaction instead of silently
            // discarding the request as an unrelated response.
            return Ok(message);
        }
        let Some(id) = message.get("id") else {
            // Notifications and server-initiated messages do not complete the
            // request that is currently pending.
            continue;
        };
        let matches = id.as_str() == Some(expected_id) || id.to_string() == expected_id;
        if !matches {
            continue;
        }
        if let Some(error) = message.get("error") {
            return Err(format!("MCP stdio returned JSON-RPC error: {error}"));
        }
        return Ok(message.get("result").cloned().unwrap_or(message));
    }
}

fn connection_url(config: &Value) -> Option<String> {
    config
        .get("url")
        .or_else(|| config.get("endpoint"))
        .or_else(|| config.get("remoteUrl"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

#[derive(Debug, Clone)]
pub(crate) struct McpStdioTemplate {
    pub(crate) template_id: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) env_keys: Vec<String>,
    pub(crate) tools: Value,
    pub(crate) builtin: bool,
}

/// Paperclip ships a deterministic in-process stdio fixture for validating
/// catalog discovery and profile policy without depending on an external
/// executable. Keep the same fixture available to Parrot's local example and
/// smoke-test paths; production user templates still require a real command.
pub(crate) fn builtin_mcp_template_tools(template_id: &str) -> Option<Value> {
    if template_id != "paperclip.synthetic-todo-kv" {
        return None;
    }
    Some(json!([
        {
            "name": "list_items",
            "description": "List synthetic todo items.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "create_item",
            "description": "Create a synthetic todo item.",
            "inputSchema": {"type": "object", "properties": {"title": {"type": "string"}}, "required": ["title"], "additionalProperties": false},
            "annotations": {"readOnlyHint": false}
        },
        {
            "name": "mark_done",
            "description": "Mark a synthetic todo item done.",
            "inputSchema": {"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"], "additionalProperties": false},
            "annotations": {"readOnlyHint": false}
        },
        {
            "name": "delete_item",
            "description": "Delete a synthetic todo item.",
            "inputSchema": {"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"], "additionalProperties": false},
            "annotations": {"destructiveHint": true}
        },
        {
            "name": "get_value",
            "description": "Read a synthetic KV value.",
            "inputSchema": {"type": "object", "properties": {"key": {"type": "string"}}, "required": ["key"], "additionalProperties": false},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "set_value",
            "description": "Write a synthetic KV value.",
            "inputSchema": {"type": "object", "properties": {"key": {"type": "string"}, "value": {}}, "required": ["key", "value"], "additionalProperties": false},
            "annotations": {"readOnlyHint": false}
        }
    ]))
}

pub(crate) fn builtin_mcp_request(
    template_id: &str,
    method: &str,
    params: &Value,
) -> Result<Value, String> {
    let tools = builtin_mcp_template_tools(template_id)
        .ok_or_else(|| format!("unknown built-in MCP template {template_id}"))?;
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": template_id, "version": "1.0.0"}
        })),
        "tools/list" => Ok(json!({"tools": tools})),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| "built-in MCP tools/call requires a tool name".to_string())?;
            if !tools
                .as_array()
                .is_some_and(|tools| tools.iter().any(|tool| tool.get("name").and_then(Value::as_str) == Some(name)))
            {
                return Err(format!("built-in MCP tool {name} was not found"));
            }
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let state = builtin_mcp_state()
                .lock()
                .map_err(|_| "built-in MCP fixture state is poisoned".to_string())?;
            let mut state = state;
            let state = state.entry(template_id.to_string()).or_default();
            let content = match name {
                "list_items" => json!([{"type": "text", "text": serde_json::to_string(&state.items.values().collect::<Vec<_>>()).unwrap_or_else(|_| "[]".to_string())}]),
                "get_value" => {
                    let key = arguments.get("key").and_then(Value::as_str).unwrap_or_default();
                    let value = state.values.get(key).cloned().unwrap_or(Value::Null);
                    json!([{"type": "text", "text": format!("value={value}")}])
                }
                "create_item" => {
                    let id = format!("item-{}", state.next_item);
                    state.next_item = state.next_item.saturating_add(1);
                    let item = json!({
                        "id": id,
                        "title": arguments.get("title").and_then(Value::as_str).unwrap_or_default(),
                        "done": false,
                    });
                    state.items.insert(id.clone(), item);
                    json!([{"type": "text", "text": format!("created:{id}")}])
                }
                "mark_done" => {
                    let id = arguments.get("id").and_then(Value::as_str).unwrap_or_default();
                    let Some(item) = state.items.get_mut(id) else {
                        return Err(format!("built-in MCP item {id} was not found"));
                    };
                    item["done"] = json!(true);
                    json!([{"type": "text", "text": "done"}])
                }
                "delete_item" => {
                    let id = arguments.get("id").and_then(Value::as_str).unwrap_or_default();
                    if state.items.remove(id).is_none() {
                        return Err(format!("built-in MCP item {id} was not found"));
                    }
                    json!([{"type": "text", "text": "deleted"}])
                }
                "set_value" => {
                    let key = arguments.get("key").and_then(Value::as_str).unwrap_or_default();
                    let value = arguments.get("value").cloned().unwrap_or(Value::Null);
                    state.values.insert(key.to_string(), value);
                    json!([{"type": "text", "text": "set"}])
                }
                _ => return Err(format!("built-in MCP tool {name} was not found")),
            };
            Ok(json!({"content": content}))
        }
        _ => Err(format!("built-in MCP method {method} was not found")),
    }
}

struct BuiltinMcpState {
    next_item: u64,
    items: HashMap<String, Value>,
    values: HashMap<String, Value>,
}

impl Default for BuiltinMcpState {
    fn default() -> Self {
        Self {
            next_item: 1,
            items: HashMap::new(),
            values: HashMap::new(),
        }
    }
}

fn builtin_mcp_state() -> &'static StdMutex<HashMap<String, BuiltinMcpState>> {
    static STATE: OnceLock<StdMutex<HashMap<String, BuiltinMcpState>>> = OnceLock::new();
    STATE.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn configured_mcp_stdio_template_id(
    connection_config: &Value,
    transport_config: &Value,
) -> Option<String> {
    connection_config
        .get("templateId")
        .or_else(|| connection_config.get("template_id"))
        .or_else(|| transport_config.get("templateId"))
        .or_else(|| transport_config.get("template_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn json_string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve the reviewed local stdio command. A configured template is the
/// source of truth for command, arguments, environment allow-list, and the
/// declared tool descriptors; raw command fields remain a compatibility
/// fallback for old draft connections that predate the template table.
pub(crate) async fn resolve_mcp_stdio_template(
    state: &AppState,
    company_id: Uuid,
    connection_config: &Value,
    transport_config: &Value,
) -> Result<Option<McpStdioTemplate>, String> {
    let Some(template_id) = configured_mcp_stdio_template_id(connection_config, transport_config)
    else {
        return Ok(None);
    };
    if let Some(tools) = builtin_mcp_template_tools(&template_id) {
        return Ok(Some(McpStdioTemplate {
            template_id,
            command: String::new(),
            args: Vec::new(),
            env_keys: Vec::new(),
            tools,
            builtin: true,
        }));
    }
    let row = sqlx::query(
        "SELECT template_key, status, command, args, env_keys, tools
           FROM tool_stdio_command_templates
          WHERE company_id = $1 AND template_key = $2",
    )
    .bind(company_id)
    .bind(&template_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| format!("MCP stdio template lookup failed: {error}"))?
    .ok_or_else(|| format!("MCP stdio template {template_id} was not found"))?;
    let status: String = row.get("status");
    if status != "active" {
        return Err(format!("MCP stdio template {template_id} is not active"));
    }
    let command: String = row.get("command");
    if command.trim().is_empty() {
        return Err(format!("MCP stdio template {template_id} has no executable command"));
    }
    Ok(Some(McpStdioTemplate {
        template_id: row.get("template_key"),
        command,
        args: json_string_array(&row.get::<Value, _>("args")),
        env_keys: json_string_array(&row.get::<Value, _>("env_keys")),
        tools: row.get("tools"),
        builtin: false,
    }))
}

fn mcp_stdio_template_tool_name(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.get("name").and_then(Value::as_str))
}

fn mcp_stdio_template_allows_tool(template: &McpStdioTemplate, tool_name: &str) -> bool {
    let Some(tools) = template.tools.as_array() else {
        return false;
    };
    tools
        .iter()
        .filter_map(mcp_stdio_template_tool_name)
        .any(|name| name == tool_name)
}

pub(crate) fn mcp_stdio_environment(
    connection_config: &Value,
    transport_config: &Value,
    template: &McpStdioTemplate,
) -> HashMap<String, String> {
    let mut environment = HashMap::new();
    // Preserve only the small set of process variables needed to resolve a
    // command and start a child process. Credentials and arbitrary server
    // process state are never inherited by a reviewed MCP command.
    for key in ["PATH", "Path", "SystemRoot", "WINDIR", "COMSPEC", "PATHEXT"] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.to_string(), value);
        }
    }
    let config_env = connection_config
        .get("env")
        .or_else(|| transport_config.get("env"))
        .and_then(Value::as_object);
    for key in &template.env_keys {
        if let Some(value) = config_env
            .and_then(|values| values.get(key))
            .and_then(Value::as_str)
        {
            environment.insert(key.clone(), value.to_string());
        }
    }
    environment
}

#[derive(Debug, Clone)]
struct McpCatalogTool {
    catalog_entry_id: Uuid,
    connection_id: Uuid,
    connection_uid: String,
    connection_name: String,
    transport: String,
    transport_config: Value,
    connection_config: Value,
    credential_refs: Value,
    credential_secret_refs: Value,
    connection_status: String,
    enabled: bool,
    health_status: String,
    application_id: Uuid,
    application_key: Option<String>,
    application_name: String,
    application_type: String,
    catalog_name: String,
    upstream_tool_name: String,
    title: Option<String>,
    description: Option<String>,
    input_schema: Value,
    output_schema: Option<Value>,
    annotations: Value,
    version_hash: Option<String>,
    schema_hash: Option<String>,
    risk_level: String,
    is_read_only: bool,
    is_write: bool,
    is_destructive: bool,
    gateway_name: String,
}

fn mcp_slug_segment(value: &str, fallback: &str) -> String {
    let mut slug = String::new();
    for ch in value.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 64 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        fallback.to_string()
    } else {
        slug.to_string()
    }
}

fn mcp_short_stable_id(id: Uuid) -> String {
    id.simple().to_string()[..8].to_string()
}

fn mcp_on_demand_enabled(config: &Value) -> bool {
    let raw = config
        .get("onDemandTools")
        .or_else(|| config.get("loadToolsOnDemand"));
    raw.and_then(Value::as_bool).unwrap_or(false)
        || raw
            .and_then(Value::as_object)
            .and_then(|value| value.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn valid_mcp_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+'
                    | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~')
        })
}

fn add_mcp_header(headers: &mut HashMap<String, String>, name: &str, value: &str) {
    let name = name.trim().to_ascii_lowercase();
    if !valid_mcp_header_name(&name)
        || matches!(
            name.as_str(),
            "accept" | "content-type" | "content-length" | "host" | "connection"
        )
        || value.contains('\r')
        || value.contains('\n')
    {
        return;
    }
    headers.insert(name, value.to_string());
}

fn collect_mcp_static_headers(value: &Value, headers: &mut HashMap<String, String>) {
    match value {
        Value::Object(values) => {
            for (name, value) in values {
                if let Some(value) = value.as_str() {
                    add_mcp_header(headers, name, value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                let Some(object) = value.as_object() else {
                    continue;
                };
                let Some(name) = object
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| object.get("key").and_then(Value::as_str))
                else {
                    continue;
                };
                if let Some(value) = object.get("value").and_then(Value::as_str) {
                    add_mcp_header(headers, name, value);
                }
            }
        }
        _ => {}
    }
}

fn configured_mcp_static_headers(
    connection_config: &Value,
    transport_config: &Value,
) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    if let Some(value) = transport_config.get("headers") {
        collect_mcp_static_headers(value, &mut headers);
    }
    if let Some(policy) = connection_config
        .get("headerPolicy")
        .or_else(|| connection_config.get("header_policy"))
        .and_then(Value::as_object)
    {
        if let Some(value) = policy.get("staticHeaders") {
            collect_mcp_static_headers(value, &mut headers);
        } else if let Some(value) = policy.get("static_headers") {
            collect_mcp_static_headers(value, &mut headers);
        }
    }
    if let Some(policy) = transport_config
        .get("headerPolicy")
        .or_else(|| transport_config.get("header_policy"))
        .and_then(Value::as_object)
    {
        if let Some(value) = policy
            .get("staticHeaders")
            .or_else(|| policy.get("static_headers"))
        {
            collect_mcp_static_headers(value, &mut headers);
        }
    }
    headers
}

fn mcp_sensitive_passthrough_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if name.starts_with("x-paperclip-") {
        return true;
    }
    if matches!(
        name.as_str(),
        "authorization" | "proxy-authorization" | "cookie" | "set-cookie"
    ) {
        return true;
    }
    let parts = name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty());
    parts.clone().any(|part| {
        matches!(
            part,
            "auth" | "authorization" | "cookie" | "secret" | "session" | "token"
        )
    }) || name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .windows(2)
        .any(|parts| parts == ["api", "key"])
}

fn mcp_header_policy<'a>(
    connection_config: &'a Value,
    transport_config: &'a Value,
) -> Option<&'a serde_json::Map<String, Value>> {
    connection_config
        .get("headerPolicy")
        .or_else(|| connection_config.get("header_policy"))
        .and_then(Value::as_object)
        .or_else(|| {
            transport_config
                .get("headerPolicy")
                .or_else(|| transport_config.get("header_policy"))
                .and_then(Value::as_object)
        })
}

fn mcp_string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn mcp_header_context_value(
    field: &str,
    company_id: Uuid,
    agent_id: Option<Uuid>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
    run_id: Option<Uuid>,
    session_id: Uuid,
    correlation_id: Uuid,
) -> Option<String> {
    match field {
        "company_id" => Some(company_id.to_string()),
        "agent_id" => agent_id.map(|value| value.to_string()),
        "issue_id" => issue_id.map(|value| value.to_string()),
        "project_id" => project_id.map(|value| value.to_string()),
        "run_id" => run_id.map(|value| value.to_string()),
        "gateway_session_id" => Some(session_id.to_string()),
        "correlation_id" => Some(correlation_id.to_string()),
        _ => None,
    }
}

fn apply_mcp_connection_header_policy(
    headers: &mut HashMap<String, String>,
    connection_config: &Value,
    transport_config: &Value,
    caller_headers: Option<&HeaderMap>,
    company_id: Uuid,
    agent_id: Option<Uuid>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
    run_id: Option<Uuid>,
    session_id: Option<Uuid>,
) {
    let Some(policy) = mcp_header_policy(connection_config, transport_config) else {
        return;
    };
    let passthrough = policy
        .get("passthrough")
        .or_else(|| policy.get("callerPassthrough"))
        .and_then(Value::as_object);
    let passthrough_enabled = passthrough
        .and_then(|value| value.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mut allowlist = mcp_string_array(
        passthrough.and_then(|value| {
            value
                .get("allow")
                .or_else(|| value.get("allowedHeaders"))
        }),
    );
    allowlist.extend(mcp_string_array(
        policy
            .get("allowedPassthroughHeaders")
            .or_else(|| policy.get("allowed_passthrough_headers")),
    ));
    allowlist.sort();
    allowlist.dedup();

    if passthrough_enabled {
        if let Some(caller_headers) = caller_headers {
            for (name, value) in caller_headers {
                let name = name.as_str().to_ascii_lowercase();
                let Ok(value) = value.to_str() else {
                    continue;
                };
                if !valid_mcp_header_name(&name)
                    || mcp_sensitive_passthrough_header(&name)
                    || !allowlist.iter().any(|allowed| allowed == &name)
                    || headers
                        .keys()
                        .any(|existing| existing.eq_ignore_ascii_case(&name))
                {
                    continue;
                }
                add_mcp_header(headers, &name, value);
            }
        }
    }

    let metadata = policy
        .get("metadata")
        .or_else(|| policy.get("generatedMetadata"))
        .and_then(Value::as_object);
    let mut metadata_fields = mcp_string_array(metadata.and_then(|value| {
        value
            .get("forward")
            .or_else(|| value.get("headers"))
            .or_else(|| value.get("allowedHeaders"))
    }));
    metadata_fields.extend(mcp_string_array(
        policy
            .get("forwardContextHeaders")
            .or_else(|| policy.get("forward_context_headers")),
    ));
    let Some(session_id) = session_id else {
        return;
    };
    metadata_fields.sort();
    metadata_fields.dedup();
    let correlation_id = Uuid::new_v4();
    for field in metadata_fields {
        let Some(value) = mcp_header_context_value(
            &field,
            company_id,
            agent_id,
            issue_id,
            project_id,
            run_id,
            session_id,
            correlation_id,
        ) else {
            continue;
        };
        let header_name = format!("x-paperclip-{}", field.replace('_', "-"));
        if headers
            .keys()
            .any(|existing| existing.eq_ignore_ascii_case(&header_name))
        {
            continue;
        }
        add_mcp_header(headers, &header_name, &value);
    }
}

fn mcp_secret_id(reference: &Value) -> Option<Uuid> {
    reference
        .get("secretId")
        .or_else(|| reference.get("secret_id"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn mcp_secret_version(reference: &Value) -> String {
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

fn mcp_secret_header_name(reference: &Value) -> Option<String> {
    let direct = reference
        .get("key")
        .or_else(|| reference.get("header"))
        .or_else(|| reference.get("headerName"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(name) = direct {
        return Some(name.to_string());
    }
    let path = reference
        .get("configPath")
        .or_else(|| reference.get("config_path"))
        .and_then(Value::as_str)?;
    if matches!(
        path.to_ascii_lowercase().as_str(),
        "oauth.access_token" | "oauth.access-token" | "oauth.accesstoken"
    ) {
        // OAuth callback persistence stores the access-token binding in
        // credentialSecretRefs (the secret reference intentionally has no
        // plaintext header metadata).  The MCP transport projection is
        // always the standard Bearer Authorization header.
        return Some("Authorization".to_string());
    }
    path.strip_prefix("headers.")
        .or_else(|| path.strip_prefix("header."))
        .map(str::to_string)
}

fn mcp_secret_header_prefix(reference: &Value) -> &'static str {
    let path = reference
        .get("configPath")
        .or_else(|| reference.get("config_path"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if matches!(
        path.to_ascii_lowercase().as_str(),
        "oauth.access_token" | "oauth.access-token" | "oauth.accesstoken"
    ) {
        "Bearer "
    } else {
        ""
    }
}

const MCP_OAUTH_REFRESH_SKEW_SECONDS: i64 = 60;
const MCP_OAUTH_REFRESH_LEASE_SECONDS: i64 = 30;
const MCP_OAUTH_REFRESH_WAIT_SECONDS: u64 = 5;

#[derive(Debug)]
struct McpOAuthTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: DateTime<Utc>,
    token_type: Option<String>,
    scope: Option<String>,
}

#[derive(Debug)]
struct McpOAuthRefreshFailure {
    message: String,
    reauthorization_required: bool,
}

fn mcp_oauth_config(config: &Value) -> Option<&serde_json::Map<String, Value>> {
    config.get("oauth").and_then(Value::as_object)
}

fn mcp_oauth_string(config: &Value, keys: &[&str]) -> Option<String> {
    let oauth = mcp_oauth_config(config);
    keys.iter().find_map(|key| {
        oauth
            .and_then(|value| value.get(*key))
            .or_else(|| config.get(*key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn mcp_oauth_expiry(config: &Value) -> Option<DateTime<Utc>> {
    mcp_oauth_config(config)
        .and_then(|oauth| oauth.get("expiresAt").or_else(|| oauth.get("expires_at")))
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn mcp_oauth_refresh_required(config: &Value) -> bool {
    mcp_oauth_expiry(config).is_some_and(|expires_at| {
        expires_at <= Utc::now() + ChronoDuration::seconds(MCP_OAUTH_REFRESH_SKEW_SECONDS)
    })
}

fn mcp_oauth_lease(config: &Value) -> Option<(String, DateTime<Utc>)> {
    let lease = mcp_oauth_config(config)?.get("refreshLease")?.as_object()?;
    let id = lease.get("id")?.as_str()?.trim();
    let expires_at = lease
        .get("expiresAt")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))?;
    (!id.is_empty()).then(|| (id.to_string(), expires_at))
}

fn mcp_oauth_reference(refs: &Value, path: &str) -> Option<Value> {
    refs.as_array()?.iter().find_map(|reference| {
        let configured_path = reference
            .get("configPath")
            .or_else(|| reference.get("config_path"))
            .or_else(|| reference.get("path"))
            .and_then(Value::as_str)?;
        configured_path.eq_ignore_ascii_case(path).then(|| reference.clone())
    })
}

fn mcp_oauth_reference_secret_id(reference: &Value) -> Option<Uuid> {
    reference
        .get("secretId")
        .or_else(|| reference.get("secret_id"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn mcp_oauth_reference_version(reference: &Value) -> Result<Option<i32>, String> {
    let selector = reference
        .get("versionSelector")
        .or_else(|| reference.get("version_selector"))
        .or_else(|| reference.get("version"))
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("latest");
    if selector.eq_ignore_ascii_case("latest") {
        return Ok(None);
    }
    selector
        .parse::<i32>()
        .ok()
        .filter(|version| *version > 0)
        .map(Some)
        .ok_or_else(|| format!("OAuth credential version is invalid: {selector}"))
}

async fn load_mcp_oauth_secret(
    state: &AppState,
    company_id: Uuid,
    reference: Option<&Value>,
) -> Result<Option<String>, String> {
    let Some(reference) = reference else {
        return Ok(None);
    };
    let Some(secret_id) = mcp_oauth_reference_secret_id(reference) else {
        return Err("OAuth credential reference has no valid secret id".to_string());
    };
    let version = mcp_oauth_reference_version(reference)?;
    let material = if let Some(version) = version {
        sqlx::query_scalar::<_, Value>(
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
        .fetch_optional(&state.pool)
        .await
    } else {
        sqlx::query_scalar::<_, Value>(
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
        .fetch_optional(&state.pool)
        .await
    }
    .map_err(|error| format!("OAuth credential lookup failed: {error}"))?;
    let Some(material) = material else {
        return Ok(None);
    };
    decrypt_secret_material(&material)
        .map(Some)
        .map_err(|error| format!("OAuth credential could not be decrypted: {error}"))
}

fn mcp_oauth_replace_reference(refs: &Value, path: &str, replacement: Value) -> Value {
    let mut values = refs.as_array().cloned().unwrap_or_default();
    values.retain(|reference| {
        let configured_path = reference
            .get("configPath")
            .or_else(|| reference.get("config_path"))
            .or_else(|| reference.get("path"))
            .and_then(Value::as_str);
        !configured_path.is_some_and(|value| value.eq_ignore_ascii_case(path))
    });
    values.push(replacement);
    Value::Array(values)
}

async fn rotate_mcp_oauth_secret_version(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    company_id: Uuid,
    secret_id: Uuid,
    value: &str,
) -> Result<(), String> {
    let (material, digest) = encrypt_secret_material(value)
        .map_err(|error| format!("OAuth credential encryption failed: {error}"))?;
    let latest_version = sqlx::query_scalar::<_, i32>(
        "SELECT latest_version FROM company_secrets
          WHERE id = $1 AND company_id = $2 AND deleted_at IS NULL
          FOR UPDATE",
    )
    .bind(secret_id)
    .bind(company_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("OAuth secret lock failed: {error}"))?
    .ok_or_else(|| format!("OAuth secret {secret_id} was not found"))?;
    let next_version = latest_version.max(1) + 1;
    sqlx::query(
        "UPDATE company_secret_versions
            SET status = 'superseded', revoked_at = NOW()
          WHERE secret_id = $1 AND status = 'current'",
    )
    .bind(secret_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("OAuth secret rotation failed: {error}"))?;
    sqlx::query(
        "INSERT INTO company_secret_versions
            (secret_id, version, material, value_sha256, fingerprint_sha256, status)
         VALUES ($1, $2, $3, $4, $4, 'current')",
    )
    .bind(secret_id)
    .bind(next_version)
    .bind(material)
    .bind(digest)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("OAuth secret version insert failed: {error}"))?;
    sqlx::query(
        "UPDATE company_secrets
            SET latest_version = $2, last_rotated_at = NOW(), updated_at = NOW(), status = 'active'
          WHERE id = $1 AND company_id = $3",
    )
    .bind(secret_id)
    .bind(next_version)
    .bind(company_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("OAuth secret metadata update failed: {error}"))?;
    Ok(())
}

async fn create_mcp_oauth_secret(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    company_id: Uuid,
    connection_id: Uuid,
    kind: &str,
    label: &str,
    value: &str,
) -> Result<Uuid, String> {
    let (material, digest) = encrypt_secret_material(value)
        .map_err(|error| format!("OAuth credential encryption failed: {error}"))?;
    let secret_id = Uuid::new_v4();
    let key = format!("tool_connection_{connection_id}_oauth_{kind}");
    sqlx::query(
        "INSERT INTO company_secrets
            (id, company_id, scope, key, name, provider, status, managed_mode, description)
         VALUES ($1, $2, 'company', $3, $4, 'local_encrypted', 'active',
                 'paperclip_managed', $5)",
    )
    .bind(secret_id)
    .bind(company_id)
    .bind(key)
    .bind(label)
    .bind("OAuth credential rotated by the MCP runtime")
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("OAuth secret creation failed: {error}"))?;
    sqlx::query(
        "INSERT INTO company_secret_versions
            (secret_id, version, material, value_sha256, fingerprint_sha256, status)
         VALUES ($1, 1, $2, $3, $3, 'current')",
    )
    .bind(secret_id)
    .bind(material)
    .bind(digest)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("OAuth secret version creation failed: {error}"))?;
    Ok(secret_id)
}

fn mcp_oauth_secret_reference(secret_id: Uuid, path: &str, required: bool, label: &str) -> Value {
    serde_json::json!({
        "secretId": secret_id,
        "versionSelector": "latest",
        "configPath": path,
        "required": required,
        "label": label,
    })
}

fn mcp_oauth_config_without_lease(config: &Value) -> Value {
    let mut root = config.as_object().cloned().unwrap_or_default();
    let mut oauth = root
        .get("oauth")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    oauth.remove("refreshLease");
    root.insert("oauth".to_string(), Value::Object(oauth));
    Value::Object(root)
}

fn mcp_oauth_config_with_lease(config: &Value, lease_id: Uuid, expires_at: DateTime<Utc>) -> Value {
    let mut root = config.as_object().cloned().unwrap_or_default();
    let mut oauth = root
        .get("oauth")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    oauth.insert(
        "refreshLease".to_string(),
        serde_json::json!({
            "id": lease_id,
            "expiresAt": expires_at,
        }),
    );
    root.insert("oauth".to_string(), Value::Object(oauth));
    Value::Object(root)
}

async fn request_mcp_oauth_refresh(
    token_url: &str,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<McpOAuthTokenResponse, McpOAuthRefreshFailure> {
    if let Err(error) = validate_mcp_http_endpoint(token_url).await {
        return Err(McpOAuthRefreshFailure {
            message: error,
            reauthorization_required: false,
        });
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| McpOAuthRefreshFailure {
            message: format!("OAuth refresh client initialization failed: {error}"),
            reauthorization_required: false,
        })?;
    let mut form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
        ("client_id", client_id.to_string()),
    ];
    if let Some(client_secret) = client_secret {
        form.push(("client_secret", client_secret.to_string()));
    }
    let response = client
        .post(token_url)
        .header("content-type", "application/x-www-form-urlencoded")
        .form(&form)
        .send()
        .await
        .map_err(|error| McpOAuthRefreshFailure {
            message: format!("OAuth refresh request failed: {error}"),
            reauthorization_required: false,
        })?;
    let status = response.status();
    let payload = response.json::<Value>().await.map_err(|error| McpOAuthRefreshFailure {
        message: format!("OAuth refresh endpoint returned invalid JSON: {error}"),
        reauthorization_required: false,
    })?;
    let provider_error = payload.get("error").and_then(Value::as_str).unwrap_or_default();
    if !status.is_success() || !provider_error.is_empty() {
        return Err(McpOAuthRefreshFailure {
            message: if provider_error.is_empty() {
                format!("OAuth refresh endpoint returned HTTP {status}")
            } else {
                format!("OAuth refresh endpoint rejected the credential: {provider_error}")
            },
            reauthorization_required: provider_error == "invalid_grant",
        });
    }
    let access_token = payload
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| McpOAuthRefreshFailure {
            message: "OAuth refresh endpoint did not return an access token".to_string(),
            reauthorization_required: false,
        })?;
    let expires_in = payload
        .get("expires_in")
        .and_then(|value| match value {
            Value::Number(value) => value.as_i64(),
            Value::String(value) => value.parse::<i64>().ok(),
            _ => None,
        })
        .filter(|value| (1..=31_536_000).contains(value))
        .unwrap_or(3600);
    Ok(McpOAuthTokenResponse {
        access_token: access_token.to_string(),
        refresh_token: payload
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        expires_at: Utc::now() + ChronoDuration::seconds(expires_in),
        token_type: payload
            .get("token_type")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        scope: payload
            .get("scope")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    })
}

async fn mark_mcp_oauth_reauthorization_required(
    state: &AppState,
    company_id: Uuid,
    connection_id: Uuid,
) -> Result<(), String> {
    let mut transaction = state
        .pool
        .begin()
        .await
        .map_err(|error| format!("OAuth reauthorization transaction failed: {error}"))?;
    let row = sqlx::query(
        "SELECT config, credential_secret_refs FROM tool_connections
          WHERE id = $1 AND company_id = $2 FOR UPDATE",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("OAuth connection lock failed: {error}"))?;
    let Some(row) = row else {
        return Err("OAuth connection was removed while refreshing".to_string());
    };
    let config: Value = row.get("config");
    let refs: Value = row.get("credential_secret_refs");
    let next_refs = Value::Array(
        refs.as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|reference| {
                let path = reference
                    .get("configPath")
                    .or_else(|| reference.get("config_path"))
                    .or_else(|| reference.get("path"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                !matches!(
                    path.to_ascii_lowercase().as_str(),
                    "oauth.access_token" | "oauth.access-token" | "oauth.accesstoken"
                        | "oauth.refresh_token" | "oauth.refresh-token" | "oauth.refreshtoken"
                )
            })
            .collect(),
    );
    let mut next_config = mcp_oauth_config_without_lease(&config);
    if let Some(root) = next_config.as_object_mut() {
        if let Some(oauth) = root.get_mut("oauth").and_then(Value::as_object_mut) {
            oauth.insert("expiresAt".to_string(), Value::Null);
            oauth.insert("reauthorizationRequiredAt".to_string(), json!(Utc::now()));
        }
    }
    sqlx::query(
        "UPDATE tool_connections
            SET config = $3, credential_secret_refs = $4, status = 'draft', enabled = false,
                health_status = 'error', health_message = 'OAuth authorization expired; reconnect required',
                last_error = 'oauth_reauthorization_required', updated_at = NOW()
          WHERE id = $1 AND company_id = $2",
    )
    .bind(connection_id)
    .bind(company_id)
    .bind(next_config)
    .bind(next_refs)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("OAuth reauthorization update failed: {error}"))?;
    transaction
        .commit()
        .await
        .map_err(|error| format!("OAuth reauthorization commit failed: {error}"))
}

async fn persist_mcp_oauth_refresh(
    state: &AppState,
    company_id: Uuid,
    connection_id: Uuid,
    lease_id: Uuid,
    token: McpOAuthTokenResponse,
) -> Result<(Value, Value), String> {
    let mut transaction = state
        .pool
        .begin()
        .await
        .map_err(|error| format!("OAuth refresh transaction failed: {error}"))?;
    let row = sqlx::query(
        "SELECT config, credential_secret_refs FROM tool_connections
          WHERE id = $1 AND company_id = $2 FOR UPDATE",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("OAuth refresh connection lock failed: {error}"))?;
    let Some(row) = row else {
        return Err("OAuth connection was removed while refreshing".to_string());
    };
    let config: Value = row.get("config");
    let refs: Value = row.get("credential_secret_refs");
    let active_lease = mcp_oauth_lease(&config).map(|(id, _)| id);
    if active_lease.as_deref() != Some(lease_id.to_string().as_str()) {
        if !mcp_oauth_refresh_required(&config) {
            transaction
                .commit()
                .await
                .map_err(|error| format!("OAuth refresh convergence failed: {error}"))?;
            return Ok((config, refs));
        }
        return Err("OAuth refresh lease was superseded while the request was in flight".to_string());
    }

    let access_reference = mcp_oauth_reference(&refs, "oauth.access_token");
    let access_id = if let Some(reference) = access_reference.as_ref() {
        let secret_id = mcp_oauth_reference_secret_id(reference)
            .ok_or_else(|| "OAuth access token reference is invalid".to_string())?;
        rotate_mcp_oauth_secret_version(
            &mut transaction,
            company_id,
            secret_id,
            &token.access_token,
        )
        .await?;
        secret_id
    } else {
        create_mcp_oauth_secret(
            &mut transaction,
            company_id,
            connection_id,
            "access_token",
            "OAuth access token",
            &token.access_token,
        )
        .await?
    };
    let mut next_refs = mcp_oauth_replace_reference(
        &refs,
        "oauth.access_token",
        mcp_oauth_secret_reference(access_id, "oauth.access_token", true, "OAuth access token"),
    );
    if let Some(refresh_token) = token.refresh_token.as_deref() {
        let refresh_reference = mcp_oauth_reference(&refs, "oauth.refresh_token");
        let refresh_id = if let Some(reference) = refresh_reference.as_ref() {
            let secret_id = mcp_oauth_reference_secret_id(reference)
                .ok_or_else(|| "OAuth refresh token reference is invalid".to_string())?;
            rotate_mcp_oauth_secret_version(
                &mut transaction,
                company_id,
                secret_id,
                refresh_token,
            )
            .await?;
            secret_id
        } else {
            create_mcp_oauth_secret(
                &mut transaction,
                company_id,
                connection_id,
                "refresh_token",
                "OAuth refresh token",
                refresh_token,
            )
            .await?
        };
        next_refs = mcp_oauth_replace_reference(
            &next_refs,
            "oauth.refresh_token",
            mcp_oauth_secret_reference(refresh_id, "oauth.refresh_token", false, "OAuth refresh token"),
        );
    }
    let mut next_config = mcp_oauth_config_without_lease(&config);
    if let Some(root) = next_config.as_object_mut() {
        let oauth = root.entry("oauth").or_insert_with(|| json!({}));
        if let Some(oauth) = oauth.as_object_mut() {
            oauth.insert("expiresAt".to_string(), json!(token.expires_at));
            oauth.insert("refreshedAt".to_string(), json!(Utc::now()));
            oauth.remove("reauthorizationRequiredAt");
            if let Some(token_type) = token.token_type {
                oauth.insert("tokenType".to_string(), json!(token_type));
            }
            if let Some(scope) = token.scope {
                oauth.insert("scope".to_string(), json!(scope));
            }
        }
    }
    sqlx::query(
        "UPDATE tool_connections
            SET config = $3, credential_secret_refs = $4, status = 'active', enabled = true,
                health_status = 'unchecked', health_message = 'OAuth credentials refreshed; health check pending',
                last_error = NULL, updated_at = NOW()
          WHERE id = $1 AND company_id = $2",
    )
    .bind(connection_id)
    .bind(company_id)
    .bind(&next_config)
    .bind(&next_refs)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("OAuth refreshed credential update failed: {error}"))?;
    transaction
        .commit()
        .await
        .map_err(|error| format!("OAuth refresh commit failed: {error}"))?;
    Ok((next_config, next_refs))
}

async fn refresh_mcp_oauth_if_needed(
    state: &AppState,
    company_id: Uuid,
    connection_id: Uuid,
    connection_config: &Value,
    credential_secret_refs: &Value,
) -> Result<(Value, Value), String> {
    if !mcp_oauth_refresh_required(connection_config) {
        return Ok((connection_config.clone(), credential_secret_refs.clone()));
    }
    let started_at = std::time::Instant::now();
    let (lease_id, leased_config, leased_refs) = loop {
        let mut transaction = state
            .pool
            .begin()
            .await
            .map_err(|error| format!("OAuth refresh lease transaction failed: {error}"))?;
        let row = sqlx::query(
            "SELECT config, credential_secret_refs FROM tool_connections
              WHERE id = $1 AND company_id = $2 FOR UPDATE",
        )
        .bind(connection_id)
        .bind(company_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| format!("OAuth refresh lease lookup failed: {error}"))?;
        let Some(row) = row else {
            return Err("OAuth connection was removed while refreshing".to_string());
        };
        let current_config: Value = row.get("config");
        let current_refs: Value = row.get("credential_secret_refs");
        if !mcp_oauth_refresh_required(&current_config) {
            transaction
                .commit()
                .await
                .map_err(|error| format!("OAuth refresh lease convergence failed: {error}"))?;
            return Ok((current_config, current_refs));
        }
        if let Some((current_lease, lease_expires_at)) = mcp_oauth_lease(&current_config) {
            if lease_expires_at > Utc::now() {
                transaction
                    .commit()
                    .await
                    .map_err(|error| format!("OAuth refresh wait commit failed: {error}"))?;
                if started_at.elapsed() >= Duration::from_secs(MCP_OAUTH_REFRESH_WAIT_SECONDS) {
                    return Err("OAuth credential refresh is already in progress".to_string());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
                let _ = current_lease;
                continue;
            }
            return Err(
                "The previous OAuth refresh outcome is unknown; reconnect this connection before retrying"
                    .to_string(),
            );
        }
        let lease_id = Uuid::new_v4();
        let lease_expires_at = Utc::now() + ChronoDuration::seconds(MCP_OAUTH_REFRESH_LEASE_SECONDS);
        let leased_config = mcp_oauth_config_with_lease(&current_config, lease_id, lease_expires_at);
        sqlx::query(
            "UPDATE tool_connections SET config = $3, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(connection_id)
        .bind(company_id)
        .bind(&leased_config)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("OAuth refresh lease acquisition failed: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("OAuth refresh lease commit failed: {error}"))?;
        break (lease_id, leased_config, current_refs);
    };

    let token_url = mcp_oauth_string(
        &leased_config,
        &["tokenUrl", "token_url", "tokenEndpoint", "token_endpoint"],
    )
    .ok_or_else(|| "OAuth connection has no token endpoint".to_string())?;
    let client_id = mcp_oauth_string(&leased_config, &["clientId", "client_id"])
        .or_else(|| {
            let provider = mcp_oauth_string(&leased_config, &["provider"])?;
            let normalized = provider
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() {
                        character.to_ascii_uppercase()
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
            std::env::var(format!("PARROT_TOOL_OAUTH_{normalized}_CLIENT_ID")).ok()
        })
        .ok_or_else(|| "OAuth connection has no client id".to_string())?;
    let refresh_reference = mcp_oauth_reference(&leased_refs, "oauth.refresh_token")
        .ok_or_else(|| "OAuth credentials have expired and no refresh token is available".to_string())?;
    let refresh_token = load_mcp_oauth_secret(state, company_id, Some(&refresh_reference))
        .await?
        .ok_or_else(|| "OAuth refresh token is missing or revoked".to_string())?;
    let client_secret = load_mcp_oauth_secret(
        state,
        company_id,
        mcp_oauth_reference(&leased_refs, "oauth.client_secret").as_ref(),
    )
    .await?;
    let token = match request_mcp_oauth_refresh(
        &token_url,
        &client_id,
        client_secret.as_deref(),
        &refresh_token,
    )
    .await
    {
        Ok(token) => token,
        Err(failure) => {
            if failure.reauthorization_required {
                mark_mcp_oauth_reauthorization_required(state, company_id, connection_id).await?;
            }
            return Err(failure.message);
        }
    };
    persist_mcp_oauth_refresh(state, company_id, connection_id, lease_id, token).await
}

pub(crate) async fn resolve_mcp_connection_headers(
    state: &AppState,
    company_id: Uuid,
    connection_id: Uuid,
    connection_config: &Value,
    transport_config: &Value,
    credential_refs: &Value,
    credential_secret_refs: &Value,
) -> Result<HashMap<String, String>, String> {
    let (connection_config, credential_secret_refs) = refresh_mcp_oauth_if_needed(
        state,
        company_id,
        connection_id,
        connection_config,
        credential_secret_refs,
    )
    .await?;
    let mut headers = configured_mcp_static_headers(&connection_config, transport_config);
    for references in [credential_refs, &credential_secret_refs] {
        let Some(references) = references.as_array() else {
            continue;
        };
        for reference in references {
            if reference
                .get("placement")
                .and_then(Value::as_str)
                .is_some_and(|placement| placement != "header")
            {
                continue;
            }
            let Some(header_name) = mcp_secret_header_name(reference) else {
                // Non-header credentials (for example oauth.refresh_token)
                // are intentionally not projected into an HTTP header.
                continue;
            };
            let required = reference
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let Some(secret_id) = mcp_secret_id(reference) else {
                if required {
                    return Err(format!(
                        "MCP connection {connection_id} has an invalid credential reference for {header_name}"
                    ));
                }
                continue;
            };
            let version = mcp_secret_version(reference);
            let material = if version.eq_ignore_ascii_case("latest") {
                sqlx::query_scalar::<_, Value>(
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
                .fetch_optional(&state.pool)
                .await
            } else {
                let version_number = version.parse::<i32>().map_err(|_| {
                    format!("MCP credential version must be latest or an integer, got {version}")
                })?;
                sqlx::query_scalar::<_, Value>(
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
                .bind(version_number)
                .fetch_optional(&state.pool)
                .await
            }
            .map_err(|error| format!("MCP credential lookup failed: {error}"))?;
            let Some(material) = material else {
                if required {
                    return Err(format!(
                        "MCP credential {secret_id} is missing or revoked for connection {connection_id}"
                    ));
                }
                continue;
            };
            let value = services::secret_provider::decrypt_secret_material(&material)
                .map_err(|error| format!("MCP credential {secret_id} could not be decrypted: {error}"))?;
            let prefix = reference
                .get("prefix")
                .and_then(Value::as_str)
                .unwrap_or_else(|| mcp_secret_header_prefix(reference));
            add_mcp_header(&mut headers, &header_name, &format!("{prefix}{value}"));
        }
    }
    Ok(headers)
}

fn mcp_gateway_base_name(tool: &McpCatalogTool) -> String {
    let connection_namespace = format!(
        "{}-{}",
        mcp_slug_segment(
            tool.application_key
                .as_deref()
                .unwrap_or(&tool.connection_name),
            "mcp"
        ),
        mcp_short_stable_id(tool.connection_id)
    );
    format!(
        "mcp.{}:{}",
        connection_namespace,
        mcp_slug_segment(&tool.upstream_tool_name, "tool")
    )
}

fn mcp_catalog_tool_json(tool: &McpCatalogTool) -> Value {
    serde_json::json!({
        "name": tool.gateway_name,
        "title": tool.title.clone().unwrap_or_else(|| tool.upstream_tool_name.clone()),
        "description": tool.description.clone().unwrap_or_else(|| format!("Connected MCP tool {} from {}.", tool.upstream_tool_name, tool.connection_name)),
        "inputSchema": tool.input_schema,
        "outputSchema": tool.output_schema,
        "annotations": tool.annotations,
        "source": "mcp_catalog",
        "provider": if tool.transport == "local_stdio" { "mcp_local_stdio" } else { "mcp_remote_http" },
        "applicationId": tool.application_id,
        "applicationKey": tool.application_key,
        "applicationName": tool.application_name,
        "connectionId": tool.connection_id,
        "connectionUid": tool.connection_uid,
        "connectionStatus": tool.connection_status,
        "enabled": tool.enabled,
        "healthStatus": tool.health_status,
        "applicationType": tool.application_type,
        "catalogEntryId": tool.catalog_entry_id,
        "catalogName": tool.catalog_name,
        "upstreamToolName": tool.upstream_tool_name,
        "risk": tool.risk_level,
        "isReadOnly": tool.is_read_only,
        "isWrite": tool.is_write,
        "isDestructive": tool.is_destructive,
    })
}

async fn load_mcp_catalog_tools(
    state: &AppState,
    company_id: Uuid,
) -> Result<Vec<McpCatalogTool>, String> {
    let rows = sqlx::query(
        "SELECT c.id AS catalog_entry_id, c.connection_id, c.name AS catalog_name,
                c.tool_name AS upstream_tool_name, c.title, c.description,
                c.input_schema, c.output_schema, c.annotations, c.version_hash, c.schema_hash,
                c.risk_level,
                c.is_read_only, c.is_write, c.is_destructive,
                tc.uid AS connection_uid, tc.name AS connection_name,
                tc.transport, tc.transport_config, tc.config AS connection_config,
                tc.credential_refs, tc.credential_secret_refs,
                tc.status AS connection_status, tc.enabled, tc.health_status,
                ta.id AS application_id, ta.application_key,
                ta.name AS application_name, ta.type AS application_type
           FROM tool_catalog_entries c
           JOIN tool_connections tc ON tc.id = c.connection_id
           JOIN tool_applications ta ON ta.id = tc.application_id
          WHERE c.company_id = $1
            AND tc.company_id = $1
            AND ta.company_id = $1
            AND c.entry_kind = 'tool'
            AND c.status = 'active'
            AND c.quarantined_at IS NULL
            AND tc.transport IN ('mcp_remote', 'local_stdio')
            AND tc.status = 'active'
            AND tc.enabled = true
            AND tc.health_status IN ('ok', 'healthy')
            AND ta.status = 'active'
          ORDER BY tc.name ASC, c.name ASC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| format!("MCP catalog query failed: {error}"))?;

    let mut tools = rows
        .into_iter()
        .filter_map(|row| {
            let transport: String = row.get("transport");
            let application_type = row
                .get::<Option<String>, _>("application_type")
                .unwrap_or_default();
            let expected_type = if transport == "local_stdio" {
                "mcp_stdio"
            } else {
                "mcp_http"
            };
            if application_type != expected_type {
                return None;
            }
            Some(McpCatalogTool {
                catalog_entry_id: row.get("catalog_entry_id"),
                connection_id: row.get("connection_id"),
                connection_uid: row.get("connection_uid"),
                connection_name: row.get("connection_name"),
                transport,
                transport_config: row.get("transport_config"),
                connection_config: row.get("connection_config"),
                credential_refs: row.get("credential_refs"),
                credential_secret_refs: row.get("credential_secret_refs"),
                connection_status: row.get("connection_status"),
                enabled: row.get("enabled"),
                health_status: row.get("health_status"),
                application_id: row.get("application_id"),
                application_key: row.get("application_key"),
                application_name: row.get("application_name"),
                application_type,
                catalog_name: row.get("catalog_name"),
                upstream_tool_name: row.get("upstream_tool_name"),
                title: row.get("title"),
                description: row.get("description"),
                input_schema: row.get("input_schema"),
                output_schema: row.get("output_schema"),
                annotations: row.get("annotations"),
                version_hash: row.get("version_hash"),
                schema_hash: row.get("schema_hash"),
                risk_level: row.get("risk_level"),
                is_read_only: row.get("is_read_only"),
                is_write: row.get("is_write"),
                is_destructive: row.get("is_destructive"),
                gateway_name: String::new(),
            })
        })
        .collect::<Vec<_>>();
    let mut base_name_counts = HashMap::new();
    for tool in &tools {
        *base_name_counts.entry(mcp_gateway_base_name(tool)).or_insert(0usize) += 1;
    }
    for tool in &mut tools {
        let base_name = mcp_gateway_base_name(tool);
        tool.gateway_name = if base_name_counts.get(&base_name).copied().unwrap_or(0) > 1 {
            format!("{}-{}", base_name, mcp_short_stable_id(tool.catalog_entry_id))
        } else {
            base_name
        };
    }
    Ok(tools)
}

async fn find_mcp_catalog_tool(
    state: &AppState,
    company_id: Uuid,
    gateway_name: &str,
) -> Result<Option<McpCatalogTool>, String> {
    Ok(load_mcp_catalog_tools(state, company_id)
        .await?
        .into_iter()
        .find(|tool| tool.gateway_name == gateway_name))
}

fn virtual_mcp_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "name": "search_tools",
            "displayName": "Search available tools",
            "description": "Search connected MCP tools without loading every target tool into the tool list.",
            "inputSchema": {"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"}},"additionalProperties":false},
            "source": "paperclip_virtual",
            "provider": "paperclip_virtual",
        }),
        serde_json::json!({
            "name": "run_tool",
            "displayName": "Run a selected tool",
            "description": "Run a selected connected MCP tool after applying the target tool's policy checks.",
            "inputSchema": {"type":"object","properties":{"tool":{"type":"string"},"arguments":{"type":"object"}},"required":["tool"],"additionalProperties":false},
            "source": "paperclip_virtual",
            "provider": "paperclip_virtual",
        }),
    ]
}

async fn execute_catalog_mcp_tool(
    state: &AppState,
    company_id: Uuid,
    tool: &McpCatalogTool,
    parameters: Value,
    invocation_id: Option<Uuid>,
    caller_headers: Option<&HeaderMap>,
    session_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    run_id: Option<Uuid>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
) -> Result<Value, String> {
    if tool.transport == "mcp_remote" {
        let url = connection_url(&tool.transport_config)
            .ok_or_else(|| "MCP connection has no remote URL".to_string())?;
        let headers = resolve_mcp_connection_headers(
            state,
            company_id,
            tool.connection_id,
            &tool.connection_config,
            &tool.transport_config,
            &tool.credential_refs,
            &tool.credential_secret_refs,
        )
        .await?;
        let mut headers = headers;
        apply_mcp_connection_header_policy(
            &mut headers,
            &tool.connection_config,
            &tool.transport_config,
            caller_headers,
            company_id,
            agent_id,
            issue_id,
            project_id,
            run_id,
            session_id,
        );
        let result = mcp_http_request_with_headers(
            &url,
            "tools/call",
            serde_json::json!({
                "name": tool.upstream_tool_name,
                "arguments": parameters,
            }),
            Some(&tool.transport_config),
            Some(&headers),
        )
        .await;
        let result = match result {
            Ok(result) => {
                mark_mcp_connection_health(state, company_id, tool.connection_id, true, None).await;
                result
            }
            Err(error) => {
                mark_mcp_connection_health(
                    state,
                    company_id,
                    tool.connection_id,
                    false,
                    Some(&error),
                )
                .await;
                return Err(error);
            }
        };
        if let Some(request) = extract_mcp_elicitation_request(&result) {
            return defer_mcp_elicitation(
                state,
                company_id,
                tool,
                request,
                invocation_id,
                session_id,
                agent_id,
                run_id,
                issue_id,
            )
            .await;
        }
        return Ok(result);
    }
    let template = resolve_mcp_stdio_template(
        state,
        company_id,
        &tool.connection_config,
        &tool.transport_config,
    )
    .await?;
    let (command, args, environment) = match template.as_ref() {
        Some(template) => {
            if !mcp_stdio_template_allows_tool(template, &tool.upstream_tool_name) {
                return Err(format!(
                    "MCP stdio template {} does not expose tool {}",
                    template.template_id, tool.upstream_tool_name
                ));
            }
            (
                template.command.as_str(),
                template.args.clone(),
                Some(mcp_stdio_environment(
                    &tool.connection_config,
                    &tool.transport_config,
                    template,
                )),
            )
        }
        None => {
            return Err(
                "MCP local stdio connection requires an active approved command template"
                    .to_string(),
            );
        }
    };
    let result = if template.as_ref().is_some_and(|template| template.builtin) {
        builtin_mcp_request(
            &template
                .as_ref()
                .map(|template| template.template_id.as_str())
                .unwrap_or_default(),
            "tools/call",
            &serde_json::json!({
                "name": tool.upstream_tool_name,
                "arguments": parameters,
            }),
        )
    } else {
        mcp_stdio_request_with_env(
            command,
            &args,
            "tools/call",
            serde_json::json!({
                "name": tool.upstream_tool_name,
                "arguments": parameters,
            }),
            environment.as_ref(),
        )
        .await
    };
    let result = match result {
        Ok(result) => {
            mark_mcp_connection_health(state, company_id, tool.connection_id, true, None).await;
            result
        }
        Err(error) => {
            mark_mcp_connection_health(
                state,
                company_id,
                tool.connection_id,
                false,
                Some(&error),
            )
            .await;
            return Err(error);
        }
    };
    if let Some(request) = extract_mcp_elicitation_request(&result) {
        return defer_mcp_elicitation(
            state,
            company_id,
            tool,
            request,
            invocation_id,
            session_id,
            agent_id,
            run_id,
            issue_id,
        )
        .await;
    }
    Ok(result)
}

async fn mark_mcp_connection_health(
    state: &AppState,
    company_id: Uuid,
    connection_id: Uuid,
    healthy: bool,
    error_message: Option<&str>,
) {
    let result = if healthy {
        sqlx::query(
            "UPDATE tool_connections
                SET health_status = 'healthy', health_message = NULL,
                    health_checked_at = NOW(), last_healthy_at = NOW(),
                    last_error = NULL, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(connection_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
    } else {
        sqlx::query(
            "UPDATE tool_connections
                SET health_status = 'unhealthy', health_message = $3,
                    health_checked_at = NOW(), last_error = $3, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(connection_id)
        .bind(company_id)
        .bind(error_message.unwrap_or("MCP connection request failed"))
        .execute(&state.pool)
        .await
    };
    if let Err(error) = result {
        tracing::warn!(%error, %connection_id, "failed to persist MCP connection health");
    }
}

async fn execute_legacy_mcp_tool(
    state: &AppState,
    company_id: Uuid,
    connection_id: Uuid,
    transport: &str,
    transport_config: &Value,
    upstream_tool_name: &str,
    parameters: Value,
) -> Result<Value, String> {
    let connection = sqlx::query(
        "SELECT config, credential_refs, credential_secret_refs
           FROM tool_connections
          WHERE id = $1 AND company_id = $2",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| format!("MCP connection lookup failed: {error}"))?
    .ok_or_else(|| "MCP connection not found".to_string())?;
    let connection_config: Value = connection.get("config");
    let credential_refs: Value = connection.get("credential_refs");
    let credential_secret_refs: Value = connection.get("credential_secret_refs");
    if transport == "mcp_remote" {
        let url = connection_url(transport_config)
            .ok_or_else(|| "MCP connection has no remote URL".to_string())?;
        let headers = resolve_mcp_connection_headers(
            state,
            company_id,
            connection_id,
            &connection_config,
            transport_config,
            &credential_refs,
            &credential_secret_refs,
        )
        .await?;
        return mcp_http_request_with_headers(
            &url,
            "tools/call",
            serde_json::json!({
                "name": upstream_tool_name,
                "arguments": parameters,
            }),
            Some(transport_config),
            Some(&headers),
        )
        .await;
    }
    let template = resolve_mcp_stdio_template(
        state,
        company_id,
        &connection_config,
        transport_config,
    )
    .await?
    .ok_or_else(|| "MCP local stdio connection requires an active approved command template".to_string())?;
    if !mcp_stdio_template_allows_tool(&template, upstream_tool_name) {
        return Err(format!(
            "MCP stdio template {} does not expose tool {}",
            template.template_id, upstream_tool_name
        ));
    }
    if template.builtin {
        return builtin_mcp_request(
            &template.template_id,
            "tools/call",
            &serde_json::json!({
                "name": upstream_tool_name,
                "arguments": parameters,
            }),
        );
    }
    let environment = mcp_stdio_environment(&connection_config, transport_config, &template);
    mcp_stdio_request_with_env(
        &template.command,
        &template.args,
        "tools/call",
        serde_json::json!({
            "name": upstream_tool_name,
            "arguments": parameters,
        }),
        Some(&environment),
    )
    .await
}

async fn execute_mcp_connection(
    state: &AppState,
    company_id: Uuid,
    tool_name: &str,
    parameters: Value,
    invocation_id: Option<Uuid>,
    session_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    run_id: Option<Uuid>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
) -> Result<Value, String> {
    // Paperclip dispatches connected tools through the reviewed catalog entry,
    // not through a fresh tools/list response or an arbitrary connection uid.
    // Keep the legacy uid form below for already-persisted approval records
    // created before catalog names were introduced.
    if let Some(tool) = find_mcp_catalog_tool(state, company_id, tool_name).await? {
        return execute_catalog_mcp_tool(
            state,
            company_id,
            &tool,
            parameters,
            invocation_id,
            None,
            session_id,
            agent_id,
            run_id,
            issue_id,
            project_id,
        )
        .await;
    }
    let raw = tool_name
        .strip_prefix("mcp.")
        .ok_or("MCP tool name must start with mcp.")?;
    let (uid, upstream_name) = raw
        .split_once(':')
        .ok_or("MCP tool name must be mcp.<connection>:<tool>")?;
    let connection = sqlx::query("SELECT id, transport, transport_config FROM tool_connections WHERE company_id=$1 AND uid=$2 AND enabled=true")
        .bind(company_id).bind(uid).fetch_optional(&state.pool).await.map_err(|error| error.to_string())?
        .ok_or("MCP connection not found or disabled")?;
    let connection_id: Uuid = connection.get("id");
    let transport: String = connection.get("transport");
    let config: Value = connection.get("transport_config");
    execute_legacy_mcp_tool(
        state,
        company_id,
        connection_id,
        &transport,
        &config,
        upstream_name,
        parameters,
    )
    .await
}

/// Execute a connection-bound MCP tool from the Tool Access "test call" UI.
///
/// Paperclip runs test calls through the same catalog, policy, invocation and
/// approval machinery as an agent call, but without a heartbeat run. Keeping
/// this entry point here prevents the management route from falling back to a
/// fake `passed` response or bypassing the reviewed catalog.
pub(crate) async fn execute_mcp_connection_test_call(
    state: &AppState,
    company_id: Uuid,
    connection_id: Uuid,
    agent_id: Uuid,
    tool_name: &str,
    parameters: Value,
) -> Result<Value, String> {
    let agent_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM agents WHERE id = $1 AND company_id = $2
         )",
    )
    .bind(agent_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| format!("failed to validate test-call agent: {error}"))?;
    if !agent_exists {
        return Err("test-call agent does not belong to the company".to_string());
    }

    let tool = load_mcp_catalog_tools(state, company_id)
        .await?
        .into_iter()
        .find(|candidate| {
            candidate.connection_id == connection_id
                && (candidate.gateway_name == tool_name
                    || candidate.upstream_tool_name == tool_name)
        })
        .ok_or_else(|| format!("MCP tool {tool_name} was not found in the active catalog"))?;
    let parameters = if parameters.is_null() {
        serde_json::json!({})
    } else {
        parameters
    };
    validate_schema_value(&parameters, &tool.input_schema, "$")?;
    let arguments_summary = serde_json::json!({
        "valueType": "object",
        "keys": parameters.as_object().map_or(0, serde_json::Map::len),
    });
    let decision = gateway_decision_full_for_catalog_with_gateway(
        state,
        company_id,
        Some(agent_id),
        &tool.gateway_name,
        true,
        &tool,
        Some(&parameters),
        None,
        None,
        None,
    )
    .await;

    if decision.decision == "rate_limited" {
        return Ok(serde_json::json!({
            "decision": "off",
            "error": "Tool access rate limit exceeded.",
            "reasonCode": "rate_limited",
            "rateLimitState": decision.rate_limit_state,
        }));
    }
    if decision.decision == "deny" {
        let invocation_id = match reserve_gateway_invocation(
            state,
            company_id,
            Some(agent_id),
            None,
            Some(connection_id),
            &tool.gateway_name,
            &parameters,
            &arguments_summary,
            "deny",
            "denied",
            None,
            Some("policy_denied"),
            Some("Tool call denied by policy"),
        )
        .await
        .map_err(|(_, Json(error))| error.to_string())?
        {
            GatewayInvocationReservation::Created { invocation_id } => invocation_id,
            GatewayInvocationReservation::Replayed((_, Json(value))) => return Ok(value),
        };
        let _ = sqlx::query(
            "INSERT INTO tool_call_events
                (company_id, event_type, actor_type, actor_id, agent_id, connection_id,
                 tool_name, decision, outcome, invocation_id, reason_code, metadata)
             VALUES ($1, 'call_denied', 'agent', $2, $3, $4, $5, 'deny', 'denied', $6,
                     'policy_denied', '{\"source\":\"test\"}'::jsonb)",
        )
        .bind(company_id)
        .bind(agent_id.to_string())
        .bind(agent_id)
        .bind(connection_id)
        .bind(&tool.gateway_name)
        .bind(invocation_id)
        .execute(&state.pool)
        .await;
        return Ok(serde_json::json!({
            "decision": "off",
            "invocationId": invocation_id,
            "error": "Tool call denied by policy",
            "reasonCode": "policy_denied",
        }));
    }
    if decision.decision == "require_approval" {
        let (invocation_id, action_request_id) = match reserve_gateway_approval(
            state,
            company_id,
            Some(agent_id),
            None,
            None,
            Some(connection_id),
            &tool.gateway_name,
            &parameters,
            &arguments_summary,
            "require_approval",
            None,
        )
        .await
        .map_err(|(_, Json(error))| error.to_string())?
        {
            GatewayApprovalReservation::Created {
                invocation_id,
                action_id,
            } => (invocation_id, action_id),
            GatewayApprovalReservation::Replayed((_, Json(value))) => return Ok(value),
        };
        let _ = sqlx::query(
            "INSERT INTO tool_call_events
                (company_id, event_type, actor_type, actor_id, agent_id, connection_id,
                 tool_name, decision, outcome, invocation_id, action_request_id, reason_code,
                 metadata)
             VALUES ($1, 'approval_requested', 'agent', $2, $3, $4, $5,
                     'require_approval', 'pending', $6, $7, 'policy_requires_approval',
                     '{\"source\":\"test\"}'::jsonb)",
        )
        .bind(company_id)
        .bind(agent_id.to_string())
        .bind(agent_id)
        .bind(connection_id)
        .bind(&tool.gateway_name)
        .bind(invocation_id)
        .bind(action_request_id)
        .execute(&state.pool)
        .await;
        return Ok(serde_json::json!({
            "decision": "ask_first",
            "status": "pending",
            "invocationId": invocation_id,
            "actionRequestId": action_request_id,
        }));
    }

    let invocation_id = match reserve_gateway_invocation(
        state,
        company_id,
        Some(agent_id),
        None,
        Some(connection_id),
        &tool.gateway_name,
        &parameters,
        &arguments_summary,
        "allow",
        "executing",
        None,
        None,
        None,
    )
    .await
    .map_err(|(_, Json(error))| error.to_string())?
    {
        GatewayInvocationReservation::Created { invocation_id } => invocation_id,
        GatewayInvocationReservation::Replayed((_, Json(value))) => return Ok(value),
    };
    let result = execute_catalog_mcp_tool(
        state,
        company_id,
        &tool,
        parameters,
        Some(invocation_id),
        None,
        None,
        Some(agent_id),
        None,
        None,
        None,
    )
    .await;
    match result {
        Ok(value) => {
            let _ = sqlx::query(
                "UPDATE tool_invocations
                    SET status = 'succeeded', result_summary = $2,
                        completed_at = NOW(), updated_at = NOW()
                  WHERE id = $1",
            )
            .bind(invocation_id)
            .bind(serde_json::json!({"valueType": "json"}))
            .execute(&state.pool)
            .await;
            let _ = sqlx::query(
                "INSERT INTO tool_call_events
                    (company_id, event_type, actor_type, actor_id, agent_id, connection_id,
                     tool_name, decision, outcome, invocation_id, metadata)
                 VALUES ($1, 'call_completed', 'agent', $2, $3, $4, $5, 'allow',
                         'success', $6, '{\"source\":\"test\"}'::jsonb)",
            )
            .bind(company_id)
            .bind(agent_id.to_string())
            .bind(agent_id)
            .bind(connection_id)
            .bind(&tool.gateway_name)
            .bind(invocation_id)
            .execute(&state.pool)
            .await;
            Ok(serde_json::json!({
                "decision": "allowed",
                "invocationId": invocation_id,
                "result": value,
            }))
        }
        Err(error) => {
            let _ = sqlx::query(
                "UPDATE tool_invocations
                    SET status = 'failed', error_code = 'mcp_tool_execution_failed',
                        error_message = $2, completed_at = NOW(), updated_at = NOW()
                  WHERE id = $1",
            )
            .bind(invocation_id)
            .bind(&error)
            .execute(&state.pool)
            .await;
            let _ = sqlx::query(
                "INSERT INTO tool_call_events
                    (company_id, event_type, actor_type, actor_id, agent_id, connection_id,
                     tool_name, decision, outcome, invocation_id, reason_code, error_message,
                     metadata)
                 VALUES ($1, 'call_failed', 'agent', $2, $3, $4, $5, 'allow', 'failure',
                         $6, 'mcp_tool_execution_failed', $7, '{\"source\":\"test\"}'::jsonb)",
            )
            .bind(company_id)
            .bind(agent_id.to_string())
            .bind(agent_id)
            .bind(connection_id)
            .bind(&tool.gateway_name)
            .bind(invocation_id)
            .bind(&error)
            .execute(&state.pool)
            .await;
            Err(error)
        }
    }
}

async fn load_gateway_session(
    state: &AppState,
    token: &str,
) -> Result<sqlx::postgres::PgRow, (StatusCode, Json<Value>)> {
    let token_hash = hash_gateway_token(token);
    // A named gateway token is a durable credential rather than a heartbeat
    // run credential. Materialize its session lazily so the existing session
    // table remains the single source of truth for last-use/revocation while
    // still allowing agent_id/run_id to be NULL.
    let _ = sqlx::query(
        "INSERT INTO tool_gateway_sessions
            (id, company_id, agent_id, run_id, issue_id, token_hash, expires_at,
             gateway_id, gateway_token_id, gateway_public_id,
             client_subject_type, client_subject_id, client_name, mcp_session_id)
         SELECT gen_random_uuid(), g.company_id, g.agent_id, NULL, g.issue_id, t.token_hash,
                COALESCE(t.expires_at, NOW() + INTERVAL '10 years'),
                t.gateway_id, t.id, g.gateway_public_id,
                t.subject_type, t.subject_id, t.client_label, NULL
           FROM tool_mcp_gateway_tokens t
           JOIN tool_mcp_gateways g ON g.id = t.gateway_id AND g.company_id = t.company_id
          WHERE t.token_hash = $1 AND t.revoked_at IS NULL
         ON CONFLICT (token_hash) DO NOTHING",
    )
    .bind(&token_hash)
    .execute(&state.pool)
    .await;
    let _ = sqlx::query(
        "UPDATE tool_gateway_sessions
            SET mcp_session_id = COALESCE(mcp_session_id, id::text),
                updated_at = NOW()
          WHERE token_hash = $1",
    )
    .bind(&token_hash)
    .execute(&state.pool)
    .await;
    let row = sqlx::query(
        "SELECT s.id, s.company_id, s.agent_id, s.run_id, s.issue_id, s.project_id,
                s.expires_at, s.revoked_at,
                r.status::text AS run_status,
                t.id AS gateway_token_id, t.allowed_actions AS gateway_token_allowed_actions,
                t.expires_at AS gateway_token_expires_at, t.revoked_at AS gateway_token_revoked_at,
                g.id AS gateway_id, g.gateway_public_id, g.status AS gateway_status
           FROM tool_gateway_sessions s
           LEFT JOIN heartbeat_runs r ON r.id = s.run_id
           LEFT JOIN tool_mcp_gateway_tokens t ON t.token_hash = s.token_hash
           LEFT JOIN tool_mcp_gateways g ON g.id = t.gateway_id
          WHERE s.token_hash = $1",
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error.to_string()})),
        )
    })?;
    let Some(row) = row else {
        return Err(
            mcp_gateway_auth_failure_response(state, token, "gateway_token_invalid", "Tool gateway session is invalid")
                .await,
        );
    };
    let revoked_at: Option<chrono::DateTime<chrono::Utc>> = row.get("revoked_at");
    let expires_at: chrono::DateTime<chrono::Utc> = row.get("expires_at");
    if revoked_at.is_some() || expires_at <= chrono::Utc::now() {
        return Err(
            mcp_gateway_auth_failure_response(
                state,
                token,
                "gateway_session_expired",
                "Tool gateway session is expired or revoked",
            )
            .await,
        );
    }
    let gateway_token_id: Option<Uuid> = row.get("gateway_token_id");
    if gateway_token_id.is_some() {
        let gateway_status: Option<String> = row.get("gateway_status");
        let token_revoked_at: Option<chrono::DateTime<chrono::Utc>> =
            row.get("gateway_token_revoked_at");
        let token_expires_at: Option<chrono::DateTime<chrono::Utc>> =
            row.get("gateway_token_expires_at");
        if gateway_status.as_deref() != Some("active")
            || token_revoked_at.is_some()
            || token_expires_at.is_some_and(|expires_at| expires_at <= chrono::Utc::now())
        {
            let reason_code = if gateway_status.as_deref() != Some("active") {
                "gateway_disabled"
            } else if token_revoked_at.is_some() {
                "gateway_token_revoked"
            } else {
                "gateway_token_expired"
            };
            return Err(
                mcp_gateway_auth_failure_response(
                    state,
                    token,
                    reason_code,
                    "Tool gateway token is expired, revoked, or inactive",
                )
                .await,
            );
        }
    } else {
        let run_status: Option<String> = row.get("run_status");
        if !matches!(run_status.as_deref(), Some("queued") | Some("running")) {
            return Err(
                mcp_gateway_auth_failure_response(
                    state,
                    token,
                    "gateway_token_run_inactive",
                    "Tool gateway run is no longer active",
                )
                .await,
            );
        }
    }
    let _ = sqlx::query(
        "UPDATE tool_gateway_sessions SET last_used_at = NOW(), updated_at = NOW() WHERE id = $1",
    )
    .bind(row.get::<Uuid, _>("id"))
    .execute(&state.pool)
    .await;
    if gateway_token_id.is_some() {
        let _ = sqlx::query(
            "UPDATE tool_mcp_gateway_tokens SET last_used_at = NOW(), updated_at = NOW()
             WHERE id = $1",
        )
        .bind(gateway_token_id)
        .execute(&state.pool)
        .await;
    }
    Ok(row)
}

fn gateway_token_action_allowed(session: &sqlx::postgres::PgRow, action: &str) -> bool {
    let allowed_actions: Option<Value> = session
        .try_get("gateway_token_allowed_actions")
        .unwrap_or(None);
    let Some(allowed_actions) = allowed_actions else {
        // Per-run sessions predate named gateway tokens and have no action
        // list; their normal tool policy remains authoritative.
        return true;
    };
    allowed_actions
        .as_array()
        .is_some_and(|actions| actions.iter().any(|value| value.as_str() == Some(action)))
}

fn mcp_protocol_rate_limit(method: &str) -> (i64, i32) {
    if method == "initialize" {
        // Paperclip's session setup bucket: 30 requests per minute.
        (60_000, 30)
    } else {
        // Paperclip's gateway protocol bucket: 300 requests per minute.
        (60_000, 300)
    }
}

/// Increment a durable, company-scoped MCP protocol counter. The operation is
/// deliberately atomic so concurrent requests cannot bypass the limit between
/// a read and an update. `reason_code` is returned to callers as part of the
/// structured rate-limit state so auth throttles and protocol throttles remain
/// distinguishable in logs and clients.
async fn consume_mcp_rate_limit_counter(
    state: &AppState,
    company_id: Uuid,
    counter_key: &str,
    window_ms: i64,
    limit: i32,
    reason_code: &str,
) -> Result<Option<Value>, String> {
    let now = chrono::Utc::now();
    let now_ms = now.timestamp_millis();
    let window_start_ms = now_ms.div_euclid(window_ms) * window_ms;
    let window_start = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(window_start_ms)
        .ok_or_else(|| "MCP rate-limit window timestamp is invalid".to_string())?;
    let reset_at = window_start + chrono::Duration::milliseconds(window_ms);
    sqlx::query(
        "DELETE FROM tool_gateway_rate_limit_counters
          WHERE company_id = $1 AND reset_at <= NOW()",
    )
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|error| format!("MCP protocol rate-limit cleanup failed: {error}"))?;
    let (count, reset_at): (i32, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO tool_gateway_rate_limit_counters
            (company_id, counter_key, window_start_at, window_ms, \"limit\", count, reset_at)
         VALUES ($1, $2, $3, $4, $5, 1, $6)
         ON CONFLICT (company_id, counter_key, window_start_at)
         DO UPDATE SET
             count = LEAST(
                 tool_gateway_rate_limit_counters.count + 1,
                 tool_gateway_rate_limit_counters.\"limit\" + 1
             ),
             window_ms = EXCLUDED.window_ms,
             \"limit\" = EXCLUDED.\"limit\",
             reset_at = EXCLUDED.reset_at,
             updated_at = NOW()
         RETURNING count, reset_at",
    )
    .bind(company_id)
    .bind(counter_key)
    .bind(window_start)
    .bind(window_ms as i32)
    .bind(limit)
    .bind(reset_at)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| format!("MCP protocol rate-limit increment failed: {error}"))?;
    if count <= limit {
        return Ok(None);
    }
    let retry_after_ms = (reset_at - now).num_milliseconds().max(0);
    Ok(Some(serde_json::json!({
        "reasonCode": reason_code,
        "limit": limit,
        "count": count,
        "windowMs": window_ms,
        "retryAfterMs": retry_after_ms,
    })))
}

/// Throttle repeated invalid/revoked named-gateway credentials in the same
/// way as Paperclip. Unknown tokens cannot be attributed to a company, so they
/// still receive a normal 401; known token/session hashes are bounded by both
/// a gateway bucket and a token bucket.
async fn mcp_gateway_auth_failure_response(
    state: &AppState,
    token: &str,
    reason_code: &str,
    message: &str,
) -> (StatusCode, Json<Value>) {
    let token_hash = hash_gateway_token(token);
    let named_gateway = sqlx::query(
        "SELECT t.company_id, t.gateway_id, g.gateway_public_id
           FROM tool_mcp_gateway_tokens t
           JOIN tool_mcp_gateways g ON g.id = t.gateway_id AND g.company_id = t.company_id
          WHERE t.token_hash = $1
          LIMIT 1",
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await;
    let context = match named_gateway {
        Ok(Some(row)) => Some((
            row.get::<Uuid, _>("company_id"),
            format!("gateway:{}", row.get::<Uuid, _>("gateway_id")),
            format!("gateway:{}", row.get::<String, _>("gateway_public_id")),
        )),
        Ok(None) => sqlx::query("SELECT company_id FROM tool_gateway_sessions WHERE token_hash = $1")
            .bind(&token_hash)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten()
            .map(|row| {
                (
                    row.get::<Uuid, _>("company_id"),
                    "session:unknown".to_string(),
                    "session:unknown".to_string(),
                )
            }),
        Err(error) => {
            tracing::warn!(%error, "MCP gateway auth-failure context lookup failed");
            None
        }
    };
    let Some((company_id, gateway_key, gateway_label)) = context else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": message, "reasonCode": reason_code})),
        );
    };
    let gateway_state = consume_mcp_rate_limit_counter(
        state,
        company_id,
        &format!("mcp_gateway_auth_failure:{gateway_key}"),
        5 * 60 * 1000,
        20,
        "gateway_auth_throttled",
    )
    .await;
    let token_state = consume_mcp_rate_limit_counter(
        state,
        company_id,
        &format!(
            "mcp_gateway_auth_failure:{gateway_label}:token:{}",
            &token_hash[..24]
        ),
        5 * 60 * 1000,
        20,
        "gateway_auth_throttled",
    )
    .await;
    let gateway_state = gateway_state.ok().flatten();
    let token_state = token_state.ok().flatten();
    if gateway_state.is_some() || token_state.is_some() {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "MCP gateway authentication was throttled",
                "reasonCode": "gateway_auth_throttled",
                "gateway": gateway_state,
                "token": token_state,
            })),
        );
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": message, "reasonCode": reason_code})),
    )
}

async fn consume_mcp_protocol_rate_limit(
    state: &AppState,
    company_id: Uuid,
    session_id: Uuid,
    method: &str,
) -> Result<Option<Value>, String> {
    let (window_ms, limit) = mcp_protocol_rate_limit(method);
    let counter_key = format!(
        "{}:session:{}",
        if method == "initialize" {
            "session_setup"
        } else {
            "gateway_request"
        },
        session_id
    );
    consume_mcp_rate_limit_counter(
        state,
        company_id,
        &counter_key,
        window_ms,
        limit,
        "gateway_rate_limited",
    )
    .await
}

async fn mcp_token_request_rate_limit_response(
    state: &AppState,
    company_id: Uuid,
    scope: &str,
) -> Option<(StatusCode, Json<Value>)> {
    match consume_mcp_rate_limit_counter(
        state,
        company_id,
        &format!("mcp_gateway_token_request:{scope}"),
        60_000,
        120,
        "gateway_token_request_rate_limited",
    )
    .await
    {
        Ok(Some(rate_limit_state)) => Some((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "MCP gateway token requests are rate limited",
                "reasonCode": "gateway_token_request_rate_limited",
                "rateLimitState": rate_limit_state,
            })),
        )),
        Ok(None) => None,
        Err(error) => {
            tracing::error!(%error, %company_id, "MCP gateway token rate-limit check failed");
            Some((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "MCP gateway token rate-limit check failed",
                    "reasonCode": "gateway_rate_limit_unavailable",
                })),
            ))
        }
    }
}

fn mcp_protocol_rate_limited_error(
    id: Value,
    state: Value,
    session_id: Option<Uuid>,
) -> Response {
    mcp_response(
        StatusCode::TOO_MANY_REQUESTS,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32000,
                "message": "MCP gateway protocol rate limit exceeded",
                "data": state,
            }
        }),
        session_id,
    )
}

async fn create_gateway_session(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let company_id = body
        .get("companyId")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let agent_id = body
        .get("agentId")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let run_id = body
        .get("runId")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let (Some(company_id), Some(agent_id), Some(run_id)) = (company_id, agent_id, run_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "companyId, agentId, and runId are required"})),
        );
    };
    let valid = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM heartbeat_runs WHERE id = $1 AND company_id = $2 AND agent_id = $3 AND status IN ('queued','running'))",
    )
    .bind(run_id).bind(company_id).bind(agent_id)
    .fetch_one(&state.pool).await.unwrap_or(false);
    if !valid {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "runId is not an active run for this agent"})),
        );
    }
    if let Some(response) = mcp_token_request_rate_limit_response(
        &state,
        company_id,
        &format!("session:{agent_id}:{run_id}"),
    )
    .await
    {
        return response;
    }
    let session_id = Uuid::new_v4();
    let token = format!("ptg_{}", Uuid::new_v4().simple());
    let expires_at = chrono::Utc::now()
        + chrono::Duration::milliseconds(
            body.get("ttlMs")
                .and_then(Value::as_i64)
                .unwrap_or(30 * 60 * 1000)
                .clamp(60_000, 24 * 60 * 60 * 1000),
        );
    let result = sqlx::query(
        "INSERT INTO tool_gateway_sessions (id, company_id, agent_id, run_id, issue_id, token_hash, expires_at)
         SELECT $1, $2, $3, $4, NULLIF(context_snapshot->>'issueId', '')::uuid, $5, $6 FROM heartbeat_runs WHERE id = $4",
    )
    .bind(session_id).bind(company_id).bind(agent_id).bind(run_id).bind(hash_gateway_token(&token)).bind(expires_at)
    .execute(&state.pool).await;
    if let Err(error) = result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error.to_string()})),
        );
    }
    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "sessionId": session_id,
            "token": token,
            "expiresAt": expires_at,
            "toolsUrl": "/api/tool-gateway/tools",
            "callUrl": "/api/tool-gateway/tools/call",
        })),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GatewaySessionRevokeScope {
    company_id: Uuid,
    agent_id: Option<Uuid>,
    run_id: Option<Uuid>,
}

fn gateway_session_revoke_scope(
    actor: &AuthorizationActor,
    body: Option<&Value>,
) -> Result<GatewaySessionRevokeScope, (StatusCode, Json<Value>)> {
    match actor {
        AuthorizationActor::Board { .. } => {
            let company_id = body
                .and_then(|value| value.get("companyId"))
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok());
            let Some(company_id) = company_id else {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "companyId is required",
                        "reasonCode": "company_required"
                    })),
                ));
            };
            Ok(GatewaySessionRevokeScope {
                company_id,
                agent_id: None,
                run_id: None,
            })
        }
        AuthorizationActor::Agent {
            company_id,
            agent_id,
            run_id,
            ..
        } => Ok(GatewaySessionRevokeScope {
            company_id: *company_id,
            agent_id: Some(*agent_id),
            run_id: *run_id,
        }),
        AuthorizationActor::None => Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Board or agent authentication required",
                "reasonCode": "authentication_required"
            })),
        )),
    }
}

async fn revoke_gateway_session(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    body: Option<Json<Value>>,
) -> impl IntoResponse {
    let body = body.map(|Json(value)| value);
    let scope = match gateway_session_revoke_scope(&actor, body.as_ref()) {
        Ok(scope) => scope,
        Err(response) => return response,
    };
    if crate::routes::assert_company_access(&actor, scope.company_id, false).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Company access denied",
                "reasonCode": "company_access_denied"
            })),
        );
    }

    // Keep the company lookup separate from the agent scope check so a caller
    // cannot use a session UUID to revoke another run's session while still
    // preserving Paperclip's wrong-company not-found behavior.
    let existing = sqlx::query(
        "SELECT agent_id, run_id
           FROM tool_gateway_sessions
          WHERE id = $1 AND company_id = $2",
    )
    .bind(session_id)
    .bind(scope.company_id)
    .fetch_optional(&state.pool)
    .await;
    let existing = match existing {
        Ok(Some(row)) => row,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Tool gateway session not found",
                    "reasonCode": "session_not_found"
                })),
            )
        }
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": error.to_string()})),
            )
        }
    };
    if let Some(agent_id) = scope.agent_id {
        let existing_agent_id: Option<Uuid> = existing.get("agent_id");
        let existing_run_id: Option<Uuid> = existing.get("run_id");
        if existing_agent_id != Some(agent_id)
            || scope
                .run_id
                .is_some_and(|run_id| existing_run_id != Some(run_id))
        {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "Tool gateway session is outside the authenticated agent scope",
                    "reasonCode": "session_scope_mismatch"
                })),
            );
        }
    }

    // Updating an already revoked row is intentionally idempotent, matching
    // Paperclip's service behavior and avoiding a race-dependent 404.
    let updated = sqlx::query(
        "UPDATE tool_gateway_sessions SET revoked_at = NOW(), updated_at = NOW()
          WHERE id = $1 AND company_id = $2 RETURNING id, revoked_at",
    )
    .bind(session_id)
    .bind(scope.company_id)
    .fetch_optional(&state.pool)
    .await;
    match updated {
        Ok(Some(row)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "sessionId": row.get::<Uuid, _>("id"),
                "revokedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("revoked_at"),
            })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Tool gateway session not found",
                "reasonCode": "session_not_found"
            })),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error.to_string()})),
        ),
    }
}

async fn mcp_session_info(State(state): State<AppState>, headers: HeaderMap) -> Response {
    mcp_session_info_for_gateway(state, headers, None).await
}

async fn gateway_matches_selector(
    state: &AppState,
    headers: &HeaderMap,
    selector: &str,
) -> Result<(), Response> {
    let Some(token) = bearer_or_gateway_token(headers) else {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Bearer token is required"
            })),
        )
            .into_response());
    };
    // Materialize/validate named-token sessions before checking the gateway
    // selector. Per-run sessions have no named gateway association and are
    // intentionally not accepted on this route.
    if let Err((status, Json(error))) = load_gateway_session(state, &token).await {
        return Err(mcp_error(
            status,
            Value::Null,
            -32001,
            error
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("MCP gateway authentication failed"),
            None,
        ));
    }
    let matches = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
           SELECT 1
             FROM tool_mcp_gateway_tokens t
             JOIN tool_mcp_gateways g ON g.id = t.gateway_id AND g.company_id = t.company_id
             JOIN tool_gateway_sessions s ON s.token_hash = t.token_hash
            WHERE t.token_hash = $1
            AND (g.gateway_public_id = $2 OR g.id::text = $2)
            AND g.status = 'active'
            AND t.revoked_at IS NULL
            AND (t.expires_at IS NULL OR t.expires_at > NOW())
        )",
    )
    .bind(hash_gateway_token(&token))
    .bind(selector)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);
    if matches {
        Ok(())
    } else {
        Err(mcp_error(
            StatusCode::NOT_FOUND,
            Value::Null,
            -32001,
            "MCP gateway is not available for this session",
            None,
        ))
    }
}

async fn mcp_session_info_for_gateway(
    state: AppState,
    headers: HeaderMap,
    selector: Option<String>,
) -> Response {
    if let Some(selector) = selector.as_deref() {
        if let Err(response) = gateway_matches_selector(&state, &headers, selector).await {
            return response;
        }
    }
    let Some(token) = bearer_or_gateway_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Bearer token is required"})),
        )
            .into_response();
    };
    let session = match load_gateway_session(&state, &token).await {
        Ok(row) => row,
        Err((status, Json(error))) => return (status, Json(error)).into_response(),
    };
    let session_id: Uuid = session.get("id");
    if let Some(request_session_id) = headers
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
    {
        if request_session_id != session_id.to_string() {
            return mcp_error(
                StatusCode::BAD_REQUEST,
                Value::Null,
                -32600,
                "Mcp-Session-Id does not match the gateway session",
                Some(session_id),
            );
        }
    }
    if headers
        .get("accept")
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .any(|item| item.trim() == "text/event-stream")
        })
        .unwrap_or(false)
    {
        // Flush an ordinary SSE comment immediately so clients receive the
        // response headers without waiting for the keep-alive interval. Do
        // not emit a proprietary JSON-RPC notification here: Codex treats
        // unknown unsolicited notifications as a failed transport and
        // reconnects in a loop. The pending tail keeps the channel open.
        let initial_comment = futures::stream::once(async {
            Ok::<Event, Infallible>(Event::default().comment("mcp stream ready"))
        });
        let stream = initial_comment.chain(futures::stream::pending::<Result<Event, Infallible>>());
        let mut response = Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response();
        response.headers_mut().insert(
            "mcp-protocol-version",
            HeaderValue::from_static("2025-03-26"),
        );
        if let Ok(value) = HeaderValue::from_str(&session_id.to_string()) {
            response.headers_mut().insert("mcp-session-id", value);
        }
        return response;
    }
    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({
            "transport": "streamable_http",
            "authentication": "bearer",
            "sessionId": session_id,
            "runId": session.get::<Option<Uuid>, _>("run_id"),
        })),
    )
        .into_response();
    response.headers_mut().insert(
        "mcp-protocol-version",
        HeaderValue::from_static("2025-03-26"),
    );
    response
}

async fn mcp_session_info_named(
    Path(selector): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    mcp_session_info_for_gateway(state, headers, Some(selector)).await
}

async fn close_mcp_session(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    close_mcp_session_for_gateway(state, headers, None).await
}

async fn close_mcp_session_for_gateway(
    state: AppState,
    headers: HeaderMap,
    selector: Option<String>,
) -> Response {
    if let Some(selector) = selector.as_deref() {
        if let Err(response) = gateway_matches_selector(&state, &headers, selector).await {
            return response;
        }
    }
    let Some(token) = bearer_or_gateway_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Bearer token is required"})),
        )
            .into_response();
    };
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok());
    if session_id.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Mcp-Session-Id is required"})),
        )
            .into_response();
    }
    let updated = sqlx::query(
        "UPDATE tool_gateway_sessions SET revoked_at = NOW(), updated_at = NOW()
         WHERE token_hash = $1 AND ($2::text IS NULL OR id::text = $2) AND revoked_at IS NULL RETURNING id, revoked_at",
    )
    .bind(hash_gateway_token(&token))
    .bind(session_id)
    .fetch_optional(&state.pool)
    .await;
    match updated {
        Ok(Some(row)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "sessionId": row.get::<Uuid, _>("id"),
                "revokedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("revoked_at"),
            })),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "MCP session not found"})),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}

async fn close_mcp_session_named(
    Path(selector): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    close_mcp_session_for_gateway(state, headers, Some(selector)).await
}

fn mcp_response(status: StatusCode, body: Value, session_id: Option<Uuid>) -> Response {
    let mut response = (status, Json(body)).into_response();
    response.headers_mut().insert(
        "mcp-protocol-version",
        HeaderValue::from_static("2025-03-26"),
    );
    if let Some(session_id) = session_id {
        if let Ok(value) = HeaderValue::from_str(&session_id.to_string()) {
            response.headers_mut().insert("mcp-session-id", value);
        }
    }
    response
}

fn mcp_accepted(session_id: Option<Uuid>) -> Response {
    mcp_response(StatusCode::ACCEPTED, Value::Null, session_id)
}

fn mcp_accepts_json_or_sse(headers: &HeaderMap) -> bool {
    let Some(value) = headers.get("accept").and_then(|value| value.to_str().ok()) else {
        return true;
    };
    value
        .split(',')
        .map(str::trim)
        .any(|item| item == "*/*" || item == "application/json" || item == "text/event-stream")
}

fn mcp_wants_sse(headers: &HeaderMap) -> bool {
    let Some(value) = headers.get("accept").and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let accepts_json = value
        .split(',')
        .map(str::trim)
        .any(|item| item == "*/*" || item == "application/json");
    !accepts_json
        && value
            .split(',')
            .map(str::trim)
            .any(|item| item == "text/event-stream")
}

fn mcp_error(
    status: StatusCode,
    id: Value,
    code: i64,
    message: impl Into<String>,
    session_id: Option<Uuid>,
) -> Response {
    mcp_response(
        status,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": code, "message": message.into()},
        }),
        session_id,
    )
}

fn normalize_mcp_wire_result(result: Value) -> Result<Value, String> {
    let Some(record) = result.as_object() else {
        return Ok(serde_json::json!({
            "content": [{"type": "text", "text": result.to_string()}],
            "isError": false
        }));
    };
    let Some(content) = record.get("content") else {
        let mut tool_result = serde_json::json!({
            "content": [{"type": "text", "text": result.to_string()}],
            "isError": record.get("isError").and_then(Value::as_bool).unwrap_or(false)
        });
        if record
            .get("structuredContent")
            .is_some_and(Value::is_object)
        {
            tool_result["structuredContent"] = record["structuredContent"].clone();
        } else {
            tool_result["structuredContent"] = Value::Object(record.clone());
        }
        return Ok(tool_result);
    };
    let Some(content) = content.as_array() else {
        return Err("MCP tool result content must be an array".to_string());
    };
    let mut normalized_content = Vec::with_capacity(content.len());
    for item in content {
        let Some(item_object) = item.as_object() else {
            return Err("MCP tool result content items must be objects".to_string());
        };
        let Some(kind) = item_object.get("type").and_then(Value::as_str) else {
            return Err("MCP tool result content items require a type".to_string());
        };
        if kind == "text"
            && item_object
                .get("text")
                .and_then(Value::as_str)
                .is_none()
        {
            return Err("MCP text content requires a string text field".to_string());
        }
        normalized_content.push(item.clone());
    }
    let mut tool_result = serde_json::json!({
        "content": normalized_content,
        "isError": record.get("isError").and_then(Value::as_bool).unwrap_or(false)
    });
    if record
        .get("structuredContent")
        .is_some_and(Value::is_object)
    {
        tool_result["structuredContent"] = record["structuredContent"].clone();
    }
    Ok(tool_result)
}

#[derive(Debug, Clone)]
struct McpElicitationRequest {
    message: String,
    requested_schema: Option<Value>,
}

/// MCP servers use both the draft `elicitationRequest` spelling and the
/// current `elicitation` spelling. Some Streamable HTTP implementations put
/// the request in `_meta`; accepting all of these shapes keeps the bridge
/// compatible with the Paperclip gateway and older MCP servers.
fn extract_mcp_elicitation_request(value: &Value) -> Option<McpElicitationRequest> {
    let record = value.as_object()?;
    let meta = record.get("_meta").and_then(Value::as_object);
    let candidate = if record.get("method").and_then(Value::as_str) == Some("elicitation/create") {
        record.get("params")
    } else {
        record
            .get("elicitation")
            .or_else(|| record.get("elicitationRequest"))
            .or_else(|| meta.and_then(|meta| meta.get("elicitation")))
            .or_else(|| meta.and_then(|meta| meta.get("elicitationRequest")))
    };
    let candidate = candidate?.as_object()?;
    let message = candidate
        .get("message")
        .or_else(|| candidate.get("prompt"))
        .or_else(|| candidate.get("title"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("The MCP tool needs more information before it can continue.")
        .chars()
        .take(500)
        .collect::<String>();
    let requested_schema = candidate
        .get("requestedSchema")
        .or_else(|| candidate.get("schema"))
        .or_else(|| candidate.get("inputSchema"))
        .filter(|value| value.is_object())
        .cloned();
    Some(McpElicitationRequest {
        message,
        requested_schema,
    })
}

fn mcp_elicitation_enum_options(value: &Value) -> Vec<Value> {
    value
        .as_array()
        .map(|values| {
            values
                .iter()
                .take(10)
                .enumerate()
                .map(|(index, value)| {
                    let label = match value {
                        Value::String(value) => value.clone(),
                        Value::Number(value) => value.to_string(),
                        Value::Bool(value) => value.to_string(),
                        _ => format!("Option {}", index + 1),
                    };
                    serde_json::json!({
                        "id": mcp_slug_segment(&label, &format!("option-{}", index + 1)),
                        "label": label.chars().take(120).collect::<String>(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn mcp_elicitation_questions(request: &McpElicitationRequest) -> Vec<Value> {
    let schema = request.requested_schema.as_ref().and_then(Value::as_object);
    let properties = schema
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object);
    let required = schema
        .and_then(|schema| schema.get("required"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let mut questions = Vec::new();
    if let Some(properties) = properties {
        for (key, raw_property) in properties.iter().take(10) {
            let property = raw_property.as_object();
            let enum_values = property
                .and_then(|property| property.get("enum"))
                .map(mcp_elicitation_enum_options)
                .unwrap_or_default();
            let prompt = property
                .and_then(|property| property.get("title"))
                .or_else(|| property.and_then(|property| property.get("description")))
                .and_then(Value::as_str)
                .unwrap_or(key)
                .chars()
                .take(500)
                .collect::<String>();
            let options = if enum_values.is_empty() {
                vec![serde_json::json!({"id":"answer","label":"Provide answer"})]
            } else {
                enum_values
            };
            questions.push(serde_json::json!({
                "id": key.chars().take(120).collect::<String>(),
                "prompt": prompt,
                "helpText": if options.len() == 1 && options[0]["id"] == "answer" {
                    Value::String("Use Other to enter the requested value.".to_string())
                } else {
                    Value::Null
                },
                "selectionMode": "single",
                "required": required.contains(key.as_str()),
                "options": options,
            }));
        }
    }
    if questions.is_empty() {
        questions.push(serde_json::json!({
            "id": "response",
            "prompt": request.message,
            "helpText": "Use Other to enter the requested response.",
            "selectionMode": "single",
            "required": true,
            "options": [{"id":"answer","label":"Provide response"}],
        }));
    }
    questions
}

/// Convert an MCP elicitation into the existing issue-thread interaction
/// workflow. The invocation is intentionally left in `awaiting_approval` so
/// the regular interaction continuation can wake the agent after a user
/// submits answers; no remote call is retried in this request.
async fn defer_mcp_elicitation(
    state: &AppState,
    company_id: Uuid,
    tool: &McpCatalogTool,
    request: McpElicitationRequest,
    invocation_id: Option<Uuid>,
    session_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    run_id: Option<Uuid>,
    issue_id: Option<Uuid>,
) -> Result<Value, String> {
    let Some(invocation_id) = invocation_id else {
        return Err("MCP elicitation is not supported without a recorded gateway invocation".to_string());
    };
    let Some(issue_id) = issue_id else {
        return Err("MCP elicitation is not supported for non-interactive gateway clients".to_string());
    };
    let issue = sqlx::query_as::<_, models::Issue>(
        "SELECT * FROM issues WHERE id = $1 AND company_id = $2",
    )
    .bind(issue_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| format!("MCP elicitation issue lookup failed: {error}"))?
    .ok_or_else(|| "MCP elicitation issue context is no longer available".to_string())?;
    let interaction = services::IssueThreadInteractionService::new(state.pool.clone())
        .create(
            &issue,
            models::CreateThreadInteractionInput {
                kind: "ask_user_questions".to_string(),
                payload: serde_json::json!({
                    "version": 1,
                    "title": request.message,
                    "submitLabel": "Send response",
                    "questions": mcp_elicitation_questions(&request),
                }),
                title: Some("Tool needs input".to_string()),
                summary: Some(format!(
                    "{} asked for more information before it can continue.",
                    tool.gateway_name
                )),
                continuation_policy: "wake_assignee".to_string(),
                resolver_policy: None,
                idempotency_key: Some(format!("mcp-elicitation:{invocation_id}")),
                addressee_agent_id: None,
                source_run_id: run_id,
                source_comment_id: None,
            },
            services::InteractionCreator {
                agent_id,
                user_id: None,
            },
        )
        .await
        .map_err(|error| format!("MCP elicitation interaction creation failed: {error}"))?;
    let error_message =
        "Remote MCP tool requested additional input; an issue interaction was created.";
    sqlx::query(
        "UPDATE tool_invocations
            SET status = 'awaiting_approval', error_code = 'elicitation_required',
                error_message = $2, updated_at = NOW()
          WHERE id = $1",
    )
    .bind(invocation_id)
    .bind(error_message)
    .execute(&state.pool)
    .await
    .map_err(|error| format!("MCP elicitation invocation update failed: {error}"))?;
    let actor_type = if agent_id.is_some() { "agent" } else { "system" };
    let actor_id = agent_id.map(|value| value.to_string());
    sqlx::query(
        "INSERT INTO tool_call_events
            (company_id, event_type, actor_type, actor_id, agent_id, run_id, issue_id,
             application_id, connection_id, catalog_entry_id, tool_name, decision, outcome,
             invocation_id, reason_code, metadata, error_code, error_message)
         VALUES ($1, 'call_failed', $2, $3, $4, $5, $6, $7, $8, $9, $10,
                 'defer_runtime', 'pending', $11, 'elicitation_required', $12, $13, $14)",
    )
    .bind(company_id)
    .bind(actor_type)
    .bind(actor_id)
    .bind(agent_id)
    .bind(run_id)
    .bind(issue_id)
    .bind(tool.application_id)
    .bind(tool.connection_id)
    .bind(tool.catalog_entry_id)
    .bind(&tool.gateway_name)
    .bind(invocation_id)
    .bind(serde_json::json!({
        "interactionId": interaction.id,
        "gatewaySessionId": session_id,
        "elicitation": {
            "message": request.message,
            "requestedSchema": request.requested_schema,
        },
    }))
    .bind("elicitation_required")
    .bind(error_message)
    .execute(&state.pool)
    .await
    .map_err(|error| format!("MCP elicitation event creation failed: {error}"))?;
    Err(format!("MCP_ELICITATION_REQUIRED:{}", interaction.id))
}

fn mcp_elicitation_interaction_id(error: &str) -> Option<Uuid> {
    error
        .strip_prefix("MCP_ELICITATION_REQUIRED:")
        .and_then(|value| Uuid::parse_str(value.trim()).ok())
}

async fn mcp_session_protocol(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    mcp_session_protocol_for_gateway(state, headers, body, None).await
}

async fn mcp_session_protocol_for_gateway(
    state: AppState,
    headers: HeaderMap,
    body: axum::body::Bytes,
    selector: Option<String>,
) -> Response {
    if let Some(selector) = selector.as_deref() {
        if let Err(response) = gateway_matches_selector(&state, &headers, selector).await {
            return response;
        }
    }
    // Codex opens the server-to-client Streamable HTTP channel with an empty
    // POST (rather than GET) after initialize. Treat that as the same
    // long-lived SSE channel as GET; parsing it as JSON would produce a parse
    // error and the Codex connector reports the tool call as "cancelled".
    if body.is_empty()
        && headers.get("mcp-session-id").is_some()
        && headers
            .get("accept")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(',')
                    .any(|item| item.trim() == "text/event-stream")
            })
    {
        return mcp_session_info_for_gateway(state, headers, selector).await;
    }
    let body: Value = match serde_json::from_slice(&body) {
        Ok(body) => body,
        Err(error) => {
            return mcp_error(
                StatusCode::BAD_REQUEST,
                Value::Null,
                -32700,
                format!("Parse error: {error}"),
                None,
            )
        }
    };
    if !mcp_accepts_json_or_sse(&headers) {
        return mcp_error(
            StatusCode::NOT_ACCEPTABLE,
            Value::Null,
            -32600,
            "Accept must include application/json or text/event-stream",
            None,
        );
    }
    let wants_sse = mcp_wants_sse(&headers);

    if let Value::Array(batch) = body {
        if batch.is_empty() {
            return mcp_error(
                StatusCode::BAD_REQUEST,
                Value::Null,
                -32600,
                "JSON-RPC batch must not be empty",
                None,
            );
        }
        let mut responses = Vec::new();
        let mut session_header = None;
        let mut status = StatusCode::OK;
        for item in batch {
            let response =
                mcp_session_protocol_json(State(state.clone()), headers.clone(), Json(item)).await;
            status = if response.status() == StatusCode::ACCEPTED {
                status
            } else if response.status().is_client_error() || response.status().is_server_error() {
                response.status()
            } else {
                status
            };
            if session_header.is_none() {
                session_header = response.headers().get("mcp-session-id").cloned();
            }
            if response.status() == StatusCode::ACCEPTED {
                continue;
            }
            let bytes = match to_bytes(response.into_body(), usize::MAX).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    return mcp_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Value::Null,
                        -32603,
                        "failed to encode MCP batch response",
                        None,
                    )
                }
            };
            if !bytes.is_empty() {
                match serde_json::from_slice::<Value>(&bytes) {
                    Ok(value) => responses.push(value),
                    Err(_) => {
                        return mcp_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Value::Null,
                            -32603,
                            "MCP batch response was not JSON",
                            None,
                        )
                    }
                }
            }
        }
        if responses.is_empty() {
            return mcp_response(
                StatusCode::ACCEPTED,
                Value::Null,
                session_header.and_then(|value| value.to_str().ok()?.parse().ok()),
            );
        }
        let response = mcp_response(
            status,
            Value::Array(responses),
            session_header.and_then(|value| value.to_str().ok()?.parse().ok()),
        );
        if !wants_sse {
            return response;
        }
        let session_id = response.headers().get("mcp-session-id").cloned();
        let bytes = match to_bytes(response.into_body(), usize::MAX).await {
            Ok(bytes) => bytes,
            Err(_) => {
                return mcp_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Value::Null,
                    -32603,
                    "failed to encode MCP batch SSE response",
                    None,
                )
            }
        };
        let stream = futures::stream::once(async move {
            Ok::<Event, Infallible>(
                Event::default()
                    .event("message")
                    .data(String::from_utf8_lossy(&bytes).to_string()),
            )
        });
        let mut sse_response = Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response();
        *sse_response.status_mut() = status;
        if let Some(session_id) = session_id {
            sse_response
                .headers_mut()
                .insert("mcp-session-id", session_id);
        }
        return sse_response;
    }
    let response = mcp_session_protocol_json(State(state), headers, Json(body)).await;
    if !wants_sse || response.status() == StatusCode::ACCEPTED {
        return response;
    }
    let status = response.status();
    let session_id = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = match to_bytes(response.into_body(), usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return mcp_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                Value::Null,
                -32603,
                "failed to encode MCP SSE response",
                None,
            )
        }
    };
    let data = String::from_utf8_lossy(&bytes).to_string();
    let stream = futures::stream::once(async move {
        Ok::<Event, Infallible>(Event::default().event("message").data(data))
    });
    let mut sse_response = Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response();
    *sse_response.status_mut() = status;
    if let Some(session_id) = session_id.and_then(|value| HeaderValue::from_str(&value).ok()) {
        sse_response
            .headers_mut()
            .insert("mcp-session-id", session_id);
    }
    sse_response
}

async fn mcp_session_protocol_named(
    Path(selector): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    mcp_session_protocol_for_gateway(state, headers, body, Some(selector)).await
}

async fn mcp_session_protocol_json(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = bearer_or_gateway_token(&headers) else {
        return mcp_response(
            StatusCode::UNAUTHORIZED,
            serde_json::json!({
                "jsonrpc": "2.0", "id": body.get("id").cloned().unwrap_or(Value::Null),
                "error": {"code": -32001, "message": "Bearer token is required"}
            }),
            None,
        );
    };
    let session = match load_gateway_session(&state, &token).await {
        Ok(row) => row,
        Err((status, Json(error))) => {
            return mcp_response(
                status,
                serde_json::json!({
                    "jsonrpc": "2.0", "id": body.get("id").cloned().unwrap_or(Value::Null),
                    "error": {"code": -32001, "message": error.get("error").cloned().unwrap_or(Value::String("Invalid session".into()))}
                }),
                None,
            );
        }
    };
    let session_id: Uuid = session.get("id");
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    if body.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return mcp_error(
            StatusCode::BAD_REQUEST,
            id,
            -32600,
            "jsonrpc must be '2.0'",
            Some(session_id),
        );
    }
    let request_session_id = headers
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok());
    if let Some(request_session_id) = request_session_id {
        if request_session_id != session_id.to_string() {
            return mcp_error(
                StatusCode::BAD_REQUEST,
                id,
                -32600,
                "Mcp-Session-Id does not match the gateway session",
                Some(session_id),
            );
        }
    }
    let Some(method) = body.get("method").and_then(Value::as_str) else {
        return mcp_error(
            StatusCode::BAD_REQUEST,
            id,
            -32600,
            "method is required",
            Some(session_id),
        );
    };
    match consume_mcp_protocol_rate_limit(&state, session.get("company_id"), session_id, method)
        .await
    {
        Ok(Some(rate_limit_state)) => {
            return mcp_protocol_rate_limited_error(id.clone(), rate_limit_state, Some(session_id));
        }
        Ok(None) => {}
        Err(error) => {
            tracing::error!(%error, %session_id, "MCP protocol rate-limit check failed");
            return mcp_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                id,
                -32603,
                "MCP protocol rate-limit check failed",
                Some(session_id),
            );
        }
    }
    if method == "initialize" && request_session_id.is_some() {
        return mcp_error(
            StatusCode::BAD_REQUEST,
            id,
            -32600,
            "initialize must not include Mcp-Session-Id",
            Some(session_id),
        );
    }
    if method != "initialize" && request_session_id.is_none() {
        return mcp_error(
            StatusCode::BAD_REQUEST,
            id,
            -32600,
            "Mcp-Session-Id is required after initialize",
            Some(session_id),
        );
    }
    let request_kind = request_kind(body.get("id").is_some());
    if request_kind == McpRequestKind::Notification && method != "notifications/initialized" {
        return mcp_error(
            StatusCode::BAD_REQUEST,
            Value::Null,
            -32600,
            "requests must include id",
            Some(session_id),
        );
    }
    match method {
        "initialize" => mcp_response(
            StatusCode::OK,
            serde_json::json!({
                "jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "Parrot Agent MCP Gateway", "version": env!("CARGO_PKG_VERSION")}
                }
            }),
            Some(session_id),
        ),
        "notifications/initialized" => mcp_accepted(Some(session_id)),
        "tools/list" => {
            if !gateway_token_action_allowed(&session, "tools/list") {
                return mcp_error(
                    StatusCode::FORBIDDEN,
                    id,
                    -32003,
                    "Gateway token is not allowed to list tools",
                    Some(session_id),
                );
            }
            let (status, Json(value)) = list_gateway_tools(State(state), headers).await;
            if !status.is_success() {
                return mcp_response(
                    status,
                    serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":"Tool discovery failed"}}),
                    Some(session_id),
                );
            }
            let tools = match value {
                Value::Array(tools) => tools,
                _ => Vec::new(),
            };
            mcp_response(
                StatusCode::OK,
                serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"tools":tools}}),
                Some(session_id),
            )
        }
        "tools/call" => {
            if !gateway_token_action_allowed(&session, "tools/call") {
                return mcp_error(
                    StatusCode::FORBIDDEN,
                    id,
                    -32003,
                    "Gateway token is not allowed to call tools",
                    Some(session_id),
                );
            }
            let params = body
                .get("params")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if name.is_empty() {
                return mcp_response(
                    StatusCode::BAD_REQUEST,
                    serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"params.name is required"}}),
                    Some(session_id),
                );
            }
            let call_body = serde_json::json!({
                "tool": name,
                "parameters": params.get("arguments").cloned().unwrap_or_else(|| serde_json::json!({})),
                "idempotencyKey": params.get("idempotencyKey").cloned().unwrap_or(Value::Null)
            });
            let (status, Json(value)) =
                call_gateway_tool(State(state), headers, Json(call_body)).await;
            if !status.is_success() {
                return mcp_response(
                    status,
                    serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":value.get("error").cloned().unwrap_or(Value::String("Tool call failed".into())),"data":value}}),
                    Some(session_id),
                );
            }
            // Allowed calls wrap the upstream value under `result`; policy
            // holds (require_approval) return a decision envelope directly
            // because there is no upstream result yet. Preserve that envelope
            // so MCP clients can receive and display `actionRequestId`.
            let result = value
                .get("result")
                .cloned()
                .unwrap_or_else(|| value.clone());
            let tool_result = match normalize_mcp_wire_result(result) {
                Ok(tool_result) => tool_result,
                Err(error) => {
                    return mcp_response(
                        StatusCode::BAD_GATEWAY,
                        serde_json::json!({
                            "jsonrpc":"2.0",
                            "id":id,
                            "error":{"code":-32000,"message":error}
                        }),
                        Some(session_id),
                    )
                }
            };
            mcp_response(
                StatusCode::OK,
                serde_json::json!({"jsonrpc":"2.0","id":id,"result":tool_result}),
                Some(session_id),
            )
        }
        _ => mcp_error(
            StatusCode::OK,
            id,
            -32601,
            "Method not found",
            Some(session_id),
        ),
    }
}

async fn list_gateway_tools(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let Some(token) = bearer_or_gateway_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Tool gateway session token is required"})),
        );
    };
    let session = match load_gateway_session(&state, &token).await {
        Ok(row) => row,
        Err(response) => return response,
    };
    if !gateway_token_action_allowed(&session, "tools/list") {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Gateway token is not allowed to list tools",
                "reasonCode": "gateway_token_action_denied"
            })),
        );
    }
    let rows = sqlx::query("SELECT id, plugin_key, manifest FROM plugins WHERE status = 'ready'")
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
    let mut tools: Vec<Value> = rows.into_iter().flat_map(|row| {
        let plugin_id: Uuid = row.get("id");
        let plugin_key: String = row.get("plugin_key");
        let manifest: Value = row.get("manifest");
        manifest.get("tools").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().filter_map(move |tool| {
            let name = tool.get("name").and_then(Value::as_str).or_else(|| tool.as_str())?;
            Some(serde_json::json!({"name": name, "description": tool.get("description").and_then(Value::as_str).unwrap_or(""), "inputSchema": tool.get("inputSchema").cloned().unwrap_or_else(|| serde_json::json!({"type":"object","properties":{}})), "pluginId": plugin_id, "pluginKey": plugin_key}))
        })
    }).collect();
    let company_id: Uuid = session.get("company_id");
    let catalog_tools = match load_mcp_catalog_tools(&state, company_id).await {
        Ok(tools) => tools,
        Err(error) => {
            tracing::error!(%company_id, %error, "Failed to load MCP tool catalog");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "MCP tool catalog unavailable",
                    "reasonCode": "mcp_catalog_unavailable"
                })),
            );
        }
    };
    let agent_id: Option<Uuid> = session.get("agent_id");
    let gateway_id: Option<Uuid> = session.get("gateway_id");
    let issue_id: Option<Uuid> = session.get("issue_id");
    let project_id: Option<Uuid> = session.get("project_id");
    let mut has_visible_on_demand_tools = false;
    for tool in catalog_tools.iter().filter(|tool| {
        tool.transport == "mcp_remote" && mcp_on_demand_enabled(&tool.connection_config)
    }) {
        let decision = gateway_decision_full_for_catalog_with_gateway(
            &state,
            company_id,
            agent_id,
            &tool.gateway_name,
            false,
            tool,
            None,
            gateway_id,
            issue_id,
            project_id,
        )
        .await;
        if decision.decision != "deny" {
            has_visible_on_demand_tools = true;
            break;
        }
    }
    tools.extend(
        catalog_tools
            .iter()
            .filter(|tool| !mcp_on_demand_enabled(&tool.connection_config))
            .map(mcp_catalog_tool_json),
    );
    if has_visible_on_demand_tools {
        tools.extend(virtual_mcp_tools());
    }
    tools.extend(paperclip_builtin_tools());
    let catalog_by_name = catalog_tools
        .iter()
        .map(|tool| (tool.gateway_name.as_str(), tool))
        .collect::<HashMap<_, _>>();
    let mut visible = Vec::with_capacity(tools.len());
    for tool in tools.drain(..) {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut tool = tool;
        // Named gateways may be intentionally agent-less. Paperclip's
        // heartbeat context hides agent-bound first-party/plugin tools in that
        // case; connected catalog tools remain governed by the gateway
        // profile and can still be listed explicitly.
        if agent_id.is_none()
            && (is_paperclip_builtin_tool(&name) || tool.get("pluginId").is_some())
        {
            continue;
        }
        let decision = match catalog_by_name.get(name.as_str()) {
            Some(catalog) => {
                gateway_decision_full_for_catalog_with_gateway(
                    &state,
                    company_id,
                    agent_id,
                    &name,
                    false,
                    catalog,
                    None,
                    gateway_id,
                    issue_id,
                    project_id,
                )
                .await
                .decision
            }
            None => {
                gateway_decision_for_gateway(
                    &state,
                    company_id,
                    agent_id,
                    &name,
                    gateway_id,
                    issue_id,
                    project_id,
                )
                    .await
            }
        };
        if decision == "deny" {
            continue;
        }
        if decision == "require_approval" {
            let description = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            tool["description"] = Value::String(if description.is_empty() {
                TOOL_APPROVAL_DESCRIPTION_SUFFIX.to_string()
            } else {
                format!("{description} {TOOL_APPROVAL_DESCRIPTION_SUFFIX}")
            });
        }
        visible.push(tool);
    }
    (StatusCode::OK, Json(Value::Array(visible)))
}

async fn execute_virtual_search_tools(
    state: &AppState,
    company_id: Uuid,
    agent_id: Option<Uuid>,
    gateway_id: Option<Uuid>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
    parameters: &Value,
) -> Result<Value, String> {
    let params = parameters
        .as_object()
        .ok_or_else(|| "search_tools arguments must be an object".to_string())?;
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .clamp(1, 50) as usize;
    let catalog_tools = load_mcp_catalog_tools(state, company_id).await?;
    let mut visible = Vec::new();
    for tool in catalog_tools.into_iter().filter(|tool| {
        tool.transport == "mcp_remote" && mcp_on_demand_enabled(&tool.connection_config)
    }) {
        let matches_query = query.is_empty()
            || [
                tool.gateway_name.as_str(),
                tool.title.as_deref().unwrap_or_default(),
                tool.description.as_deref().unwrap_or_default(),
                tool.application_key.as_deref().unwrap_or_default(),
                tool.upstream_tool_name.as_str(),
            ]
            .iter()
            .any(|value| value.to_ascii_lowercase().contains(&query));
        if !matches_query {
            continue;
        }
        let decision = gateway_decision_full_for_catalog_with_gateway(
            state,
            company_id,
            agent_id,
            &tool.gateway_name,
            false,
            &tool,
            None,
            gateway_id,
            issue_id,
            project_id,
        )
        .await
        .decision;
        if decision != "deny" {
            visible.push(mcp_catalog_tool_json(&tool));
        }
        if visible.len() >= limit {
            break;
        }
    }
    Ok(serde_json::json!({"tools": visible}))
}

/// Load the profile decision inputs for the ladder's profile fallback stage.
async fn load_profile_decision(
    state: &AppState,
    company_id: Uuid,
    agent_id: Option<Uuid>,
    tool_name: &str,
    catalog_entry_id: Option<Uuid>,
    connection_id: Option<Uuid>,
    application_id: Option<Uuid>,
    risk_level: Option<&str>,
    gateway_id: Option<Uuid>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
) -> (bool, bool, bool) {
    // Paperclip: an exclude entry on the current profile blocks that profile;
    // otherwise defaultAction=allow or an include match allows it. Match all
    // five selector types from the shared schema, not only legacy tool-name
    // entries.
    // LEFT JOIN is important: a profile with defaultAction=allow and no
    // entries is still an allow profile.
    if let Some(gateway_id) = gateway_id {
        // Older Parrot rows stored the profile directly on the gateway but
        // predated the generic binding row. Backfill that representation
        // lazily so both schemas participate in the same policy ladder.
        let _ = sqlx::query(
            "INSERT INTO tool_profile_bindings (company_id, profile_id, target_type, target_id)
             SELECT company_id, profile_id, 'gateway', id::text
               FROM tool_mcp_gateways
              WHERE id = $1 AND company_id = $2 AND profile_id IS NOT NULL
             ON CONFLICT (company_id, target_type, target_id, profile_id)
             DO NOTHING",
        )
        .bind(gateway_id)
        .bind(company_id)
        .execute(&state.pool)
        .await;
    }
    // Paperclip first narrows bindings to the most specific matching scope
    // (gateway > issue > routine > agent > project > company), then evaluates
    // profiles in binding priority/creation order. Keeping that scope step in
    // SQL prevents a broad company profile from overriding a narrower agent
    // or gateway profile.
    let rows: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        "WITH matching_bindings AS (
             SELECT b.profile_id, b.priority, b.created_at, b.target_type,
                    CASE b.target_type
                        WHEN 'gateway' THEN 0
                        WHEN 'issue' THEN 1
                        WHEN 'routine' THEN 2
                        WHEN 'agent' THEN 3
                        WHEN 'project' THEN 4
                        WHEN 'company' THEN 5
                        ELSE 99
                    END AS scope_rank
               FROM tool_profile_bindings b
              WHERE b.company_id = $1
                AND ((b.target_type = 'agent' AND $2::uuid IS NOT NULL AND b.target_id = $2::text)
                  OR (b.target_type = 'company' AND b.target_id = $1::text)
                  OR (b.target_type = 'project' AND $7::uuid IS NOT NULL AND b.target_id = $7::text)
                  OR (b.target_type = 'issue' AND $8::uuid IS NOT NULL AND b.target_id = $8::text)
                  OR (b.target_type = 'gateway' AND $6::uuid IS NOT NULL AND b.target_id = $6::text))
         ), winning_scope AS (
             SELECT MIN(scope_rank) AS scope_rank FROM matching_bindings
         )
         SELECT p.id, p.default_action, e.effect
           FROM matching_bindings b
           JOIN winning_scope w ON w.scope_rank = b.scope_rank
           JOIN tool_profiles p
             ON p.id = b.profile_id AND p.company_id = $1
           LEFT JOIN tool_profile_entries e
             ON e.profile_id = p.id AND e.company_id = p.company_id
            AND ((e.selector_type = 'tool_name' AND (e.tool_name = $3 OR e.tool_name = '*'))
                 OR (e.selector_type = 'catalog_entry' AND $4::uuid IS NOT NULL AND e.catalog_entry_id = $4)
                 OR (e.selector_type = 'connection' AND $5::uuid IS NOT NULL AND e.connection_id = $5)
                 OR (e.selector_type = 'application' AND $9::uuid IS NOT NULL AND e.application_id = $9)
                 OR (e.selector_type = 'risk_level' AND $10::text IS NOT NULL AND e.risk_level = $10))
          WHERE p.status = 'active'
          ORDER BY b.priority ASC, b.created_at ASC, p.id ASC,
                   CASE WHEN e.selector_type = 'tool_name' AND e.tool_name = $3 THEN 0
                        WHEN e.selector_type = 'catalog_entry' AND $4::uuid IS NOT NULL AND e.catalog_entry_id = $4 THEN 1
                        WHEN e.selector_type = 'connection' AND $5::uuid IS NOT NULL AND e.connection_id = $5 THEN 2
                        WHEN e.selector_type = 'application' AND $9::uuid IS NOT NULL AND e.application_id = $9 THEN 3
                        WHEN e.selector_type = 'risk_level' AND $10::text IS NOT NULL AND e.risk_level = $10 THEN 4
                        ELSE 3 END",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(tool_name)
    .bind(catalog_entry_id)
    .bind(connection_id)
    .bind(gateway_id)
    .bind(project_id)
    .bind(issue_id)
    .bind(application_id)
    .bind(risk_level)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let has_active_profile = !rows.is_empty();
    let mut profile_order = Vec::new();
    let mut profile_states: HashMap<Uuid, (String, bool, bool)> = HashMap::new();
    for (profile_id, default_action, effect) in rows {
        let state = profile_states.entry(profile_id).or_insert_with(|| {
            profile_order.push(profile_id);
            (default_action, false, false)
        });
        match effect.as_deref() {
            Some("exclude") | Some("deny") => state.1 = true,
            Some("include") | Some("allow") => state.2 = true,
            _ => {}
        }
    }
    for profile_id in profile_order {
        let Some((default_action, excluded, included)) = profile_states.remove(&profile_id) else {
            continue;
        };
        // An excluded entry disables only this profile, matching Paperclip's
        // per-profile loop; a later profile in the same winning scope may
        // still explicitly allow the call.
        if !excluded && (default_action == "allow" || included) {
            return (false, true, has_active_profile);
        }
    }
    (false, false, has_active_profile)
}

/// Structured decision from the ladder, carrying rate-limit state so callers
/// can shape Paperclip's 429 response.
#[derive(Debug, Clone)]
pub(crate) struct GatewayDecision {
    /// deny | allow | require_approval | rate_limited
    pub decision: String,
    pub rate_limit_state: Option<serde_json::Value>,
}

async fn gateway_decision_for_gateway(
    state: &AppState,
    company_id: Uuid,
    agent_id: Option<Uuid>,
    tool_name: &str,
    gateway_id: Option<Uuid>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
) -> String {
    gateway_decision_full_with_context(
        state,
        company_id,
        agent_id,
        tool_name,
        false,
        None,
        None,
        gateway_id,
        issue_id,
        project_id,
    )
    .await
    .decision
}

async fn gateway_decision_full_for_gateway(
    state: &AppState,
    company_id: Uuid,
    agent_id: Option<Uuid>,
    tool_name: &str,
    consume: bool,
    gateway_id: Option<Uuid>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
) -> GatewayDecision {
    gateway_decision_full_with_context(
        state,
        company_id,
        agent_id,
        tool_name,
        consume,
        None,
        None,
        gateway_id,
        issue_id,
        project_id,
    )
    .await
}

/// Paperclip `decide()`: `consumeRateLimit === true` for real calls (each
/// consumes one token per matching rate_limit policy); discovery/list
/// evaluation passes consume=false and observes without consuming.
async fn gateway_decision_full_for_catalog_with_gateway(
    state: &AppState,
    company_id: Uuid,
    agent_id: Option<Uuid>,
    tool_name: &str,
    consume: bool,
    catalog: &McpCatalogTool,
    arguments: Option<&Value>,
    gateway_id: Option<Uuid>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
) -> GatewayDecision {
    gateway_decision_full_with_context(
        state,
        company_id,
        agent_id,
        tool_name,
        consume,
        Some(catalog),
        arguments,
        gateway_id,
        issue_id,
        project_id,
    )
    .await
}

async fn gateway_decision_full_with_context(
    state: &AppState,
    company_id: Uuid,
    agent_id: Option<Uuid>,
    tool_name: &str,
    consume: bool,
    catalog: Option<&McpCatalogTool>,
    arguments: Option<&Value>,
    gateway_id: Option<Uuid>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
) -> GatewayDecision {
    let rows = match sqlx::query(TOOL_POLICY_QUERY)
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(_) => {
            return GatewayDecision {
                decision: "deny".into(),
                rate_limit_state: None,
            };
        }
    };
    use sqlx::Row;
    let policies: Vec<services::tool_access_contract::PolicySpec> = rows
        .iter()
        .map(|row| {
            let id: Uuid = row.get("id");
            let selectors: serde_json::Value =
                row.try_get("selectors").unwrap_or(serde_json::json!({}));
            let selector_tool_name = selectors
                .get("toolName")
                .or_else(|| selectors.get("tool_name"))
                .or_else(|| selectors.get("tool"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let config: serde_json::Value = row.try_get("config").unwrap_or(serde_json::json!({}));
            let trust_rule_config = config
                .get("trustRule")
                .or_else(|| config.get("trust_rule"))
                .cloned()
                .filter(|value| value.is_object());
            let rate_limit_exceeded = config
                .get("exceeded")
                .or_else(|| config.get("limited"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            services::tool_access_contract::PolicySpec {
                id: id.to_string(),
                policy_type: row.get("policy_type"),
                selector_tool_name,
                description: row.try_get("description").unwrap_or(None),
                trust_rule_config,
                rate_limit_exceeded,
            }
        })
        .collect();
    // Parse rate-limit rules while the DB rows are in scope.
    let mut rate_rules: std::collections::HashMap<String, services::tool_access_contract::RateLimitRule> =
        Default::default();
    for row in rows.iter() {
        let id: Uuid = row.get("id");
        let policy_type: String = row.get("policy_type");
        if policy_type != "rate_limit" {
            continue;
        }
        let config: serde_json::Value = row.try_get("config").unwrap_or(serde_json::json!({}));
        if let Some(rule) = services::tool_access_contract::rate_limit_rule(&config) {
            rate_rules.insert(id.to_string(), rule);
        }
    }

    let (explicit_grant, profile_allows, has_active_profile) = load_profile_decision(
        state,
        company_id,
        agent_id,
        tool_name,
        catalog.map(|tool| tool.catalog_entry_id),
        catalog.map(|tool| tool.connection_id),
        catalog.map(|tool| tool.application_id),
        catalog.map(|tool| tool.risk_level.as_str()),
        gateway_id,
        issue_id,
        project_id,
    )
    .await;
    let tool_name_string = tool_name.to_string();
    let ctx = services::tool_access_contract::EvaluationContext {
        tool_name: tool_name_string.clone(),
        explicit_grant,
        profile_allows,
        arguments: arguments.cloned(),
        arguments_hash: None,
        catalog_status: catalog.map(|_| "active".to_string()),
        catalog_version_hash: catalog.and_then(|tool| tool.version_hash.clone()),
        catalog_schema_hash: catalog.and_then(|tool| tool.schema_hash.clone()),
        last_rate_limit_state: None,
    };

    // Paperclip enforces each matching rate_limit policy inside the ladder
    // (consume=true on a real call). Pre-compute exceeded flags per policy so
    // the pure ladder sees the live counter state.
    let mut exceeded_policies: std::collections::HashSet<String> = Default::default();
    for policy in &policies {
        if policy.policy_type != "rate_limit" || policy.rate_limit_exceeded {
            continue;
        }
        let Some(rule) = rate_rules.get(&policy.id) else {
            continue;
        };
        {
            let bucket = services::tool_access_contract::rate_bucket(
                &rule,
                &services::tool_access_contract::RateLimitContext {
                    company_id: company_id.to_string(),
                    agent_id: agent_id.map(|id| id.to_string()),
                    application_id: catalog.map(|tool| tool.application_id.to_string()),
                    connection_id: catalog.map(|tool| tool.connection_id.to_string()),
                    tool_name: tool_name_string.clone(),
                },
            );
            let Ok(policy_uuid) = policy.id.parse::<Uuid>() else {
                return GatewayDecision {
                    decision: "deny".into(),
                    rate_limit_state: None,
                };
            };
            match services::tool_access_contract::enforce_rate_limit_full(
                &state.pool,
                company_id,
                &policy_uuid,
                &bucket,
                rule,
                chrono::Utc::now(),
                consume,
            )
            .await
            {
                Ok(state) => {
                    if state.limited {
                        exceeded_policies.insert(policy.id.clone());
                    }
                }
                Err(_) => {
                    return GatewayDecision {
                        decision: "deny".into(),
                        rate_limit_state: None,
                    };
                }
            }
        }
    }
    let policies: Vec<services::tool_access_contract::PolicySpec> = policies
        .into_iter()
        .map(|mut policy| {
            if exceeded_policies.contains(&policy.id) {
                policy.rate_limit_exceeded = true;
            }
            policy
        })
        .collect();
    let outcome = services::tool_access_contract::decide_tool_access(&policies, &ctx);

    // Paperclip recordTrustRuleHit: a live trust-rule allow bumps hitCount /
    // lastHitAt on the policy config.
    if outcome.reason_code == "allow_trust_rule" {
        if let Some(policy_id) = &outcome.policy_id {
            let policy_id = policy_id.parse::<Uuid>().ok();
            if let Some(policy_id) = policy_id {
                let _ = sqlx::query(TRUST_RULE_HIT_UPDATE)
                .bind(policy_id)
                .execute(&state.pool)
                .await;
                let _ = sqlx::query(
                    "INSERT INTO tool_call_events
                        (company_id, event_type, actor_type, agent_id, connection_id, tool_name, decision, outcome, reason_code)
                     VALUES ($1, 'trust_rule_used', 'agent', $2, NULL, $3, 'allow', 'success', 'allow_trust_rule')",
                )
                .bind(company_id)
                .bind(agent_id)
                .bind(tool_name)
                .execute(&state.pool)
                .await;
            }
        }
    }

    let decision = match outcome.decision {
        "allow" | "require_approval" | "rate_limited" => outcome.decision.to_string(),
        "deny"
            if agent_id.is_some()
                && allow_first_party_tool_on_default_deny(
                tool_name,
                outcome.reason_code,
                has_active_profile,
            ) =>
        {
            "allow".to_string()
        }
        "deny" => "deny".to_string(),
        _ => "deny".to_string(),
    };
    GatewayDecision {
        decision,
        rate_limit_state: outcome
            .rate_limit_state
            .map(|s| serde_json::to_value(&s).unwrap_or(serde_json::Value::Null)),
    }
}

fn path_part(value: Option<&Value>, name: &str) -> Result<String, String> {
    let value = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is required"))?;
    Ok(urlencoding::encode(value).into_owned())
}

fn query_string(parameters: &Value, omit: &[&str]) -> String {
    let Some(object) = parameters.as_object() else {
        return String::new();
    };
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in object {
        if omit.iter().any(|item| *item == key) || value.is_null() {
            continue;
        }
        if let Some(values) = value.as_array() {
            for value in values {
                query.append_pair(key, &value.to_string().trim_matches('"').to_string());
            }
        } else if let Some(value) = value.as_str() {
            query.append_pair(key, value);
        } else {
            query.append_pair(key, &value.to_string());
        }
    }
    query.finish()
}

fn object_without(parameters: &Value, omitted: &[&str]) -> Value {
    let Some(object) = parameters.as_object() else {
        return parameters.clone();
    };
    Value::Object(
        object
            .iter()
            .filter(|(key, _)| !omitted.iter().any(|omitted| omitted == key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

fn build_hire_agent_request_body(parameters: &Value) -> Result<Value, String> {
    let mut body = object_without(
        parameters,
        &["companyId", "issueIds", "requestedByAgentId"],
    );
    let object = body
        .as_object_mut()
        .ok_or("hire agent arguments must be an object")?;
    if object
        .get("adapterConfig")
        .is_none_or(Value::is_null)
    {
        object.insert("adapterConfig".to_string(), serde_json::json!({}));
    }
    if object
        .get("runtimeConfig")
        .is_none_or(Value::is_null)
    {
        object.insert("runtimeConfig".to_string(), serde_json::json!({}));
    }
    if object.get("sourceIssueIds").is_none_or(Value::is_null) {
        if let Some(issue_ids) = parameters.get("issueIds").filter(|value| !value.is_null()) {
            object.insert("sourceIssueIds".to_string(), issue_ids.clone());
        }
    }
    Ok(body)
}

fn optional_query(parameters: &Value, key: &str) -> String {
    parameters
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("{}={}", key, urlencoding::encode(value)))
        .unwrap_or_default()
}

fn validate_paperclip_api_path(path: &str) -> Result<(), String> {
    if !path.starts_with('/') || path.contains("..") {
        return Err("path must start with / and must not contain '..'".to_string());
    }
    if path.starts_with("/tool-gateway/") || path.starts_with("/mcp/") {
        return Err("paperclipApiRequest cannot call gateway/session endpoints".to_string());
    }
    Ok(())
}

#[derive(Debug)]
struct PaperclipBuiltinToolError {
    upstream_status: Option<StatusCode>,
    message: String,
}

impl PaperclipBuiltinToolError {
    fn upstream(status: StatusCode, message: String) -> Self {
        Self {
            upstream_status: Some(status),
            message,
        }
    }

    fn response_status(&self) -> StatusCode {
        match self.upstream_status {
            Some(status) if status.is_client_error() || status.is_server_error() => status,
            _ => StatusCode::BAD_GATEWAY,
        }
    }

    fn reason_code(&self) -> &'static str {
        if self.upstream_status.is_some() {
            "paperclip_tool_upstream_failed"
        } else {
            "paperclip_tool_call_failed"
        }
    }
}

impl std::fmt::Display for PaperclipBuiltinToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(formatter)
    }
}

impl From<String> for PaperclipBuiltinToolError {
    fn from(message: String) -> Self {
        Self {
            upstream_status: None,
            message,
        }
    }
}

impl From<&str> for PaperclipBuiltinToolError {
    fn from(message: &str) -> Self {
        Self::from(message.to_string())
    }
}

async fn call_paperclip_builtin_tool(
    state: &AppState,
    token: &str,
    company_id: Uuid,
    agent_id: Uuid,
    run_id: Option<Uuid>,
    tool_name: &str,
    parameters: &Value,
) -> Result<Value, PaperclipBuiltinToolError> {
    if tool_name == "paperclipWaitForIssueWorkspaceService" {
        let issue_id = parameters
            .get("issueId")
            .and_then(Value::as_str)
            .ok_or("issueId is required")?;
        let timeout_seconds = parameters
            .get("timeoutSeconds")
            .and_then(Value::as_u64)
            .unwrap_or(60)
            .clamp(1, 300);
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_seconds);
        loop {
            let current = Box::pin(call_paperclip_builtin_tool(
                state,
                token,
                company_id,
                agent_id,
                run_id,
                "paperclipGetIssueWorkspaceRuntime",
                &serde_json::json!({"issueId": issue_id}),
            ))
            .await?;
            let services = current
                .get("runtimeServices")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let selected = services.iter().find(|service| {
                let id_matches = parameters
                    .get("runtimeServiceId")
                    .and_then(Value::as_str)
                    .map(|id| service.get("id").and_then(Value::as_str) == Some(id))
                    .unwrap_or(false);
                let name_matches = parameters
                    .get("serviceName")
                    .and_then(Value::as_str)
                    .map(|name| service.get("serviceName").and_then(Value::as_str) == Some(name))
                    .unwrap_or(false);
                (id_matches
                    || name_matches
                    || (parameters.get("runtimeServiceId").is_none()
                        && parameters.get("serviceName").is_none()))
                    && service.get("status").and_then(Value::as_str) == Some("running")
                    && service.get("healthStatus").and_then(Value::as_str) != Some("unhealthy")
            });
            if let Some(service) = selected {
                return Ok(
                    serde_json::json!({"workspace": current.get("workspace"), "service": service}),
                );
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(serde_json::json!({
                    "timedOut": true,
                    "latestWorkspace": current.get("workspace"),
                    "latestRuntimeServices": services,
                }));
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
    let parameters = if tool_name == "paperclipControlIssueWorkspaceServices"
        && parameters.get("workspaceId").is_none()
    {
        let issue_id = parameters
            .get("issueId")
            .and_then(Value::as_str)
            .ok_or("issueId is required")?;
        let runtime = Box::pin(call_paperclip_builtin_tool(
            state,
            token,
            company_id,
            agent_id,
            run_id,
            "paperclipGetIssueWorkspaceRuntime",
            &serde_json::json!({"issueId": issue_id}),
        ))
        .await?;
        let workspace_id = runtime
            .get("workspace")
            .and_then(|workspace| workspace.get("id"))
            .and_then(Value::as_str)
            .ok_or("Issue has no current execution workspace")?;
        let mut enriched = parameters.clone();
        enriched
            .as_object_mut()
            .ok_or("tool arguments must be an object")?
            .insert(
                "workspaceId".to_string(),
                Value::String(workspace_id.to_string()),
            );
        enriched
    } else {
        parameters.clone()
    };
    if let Some(result) =
        direct_paperclip_service_call(state, company_id, agent_id, run_id, tool_name, &parameters)
            .await?
    {
        return Ok(result);
    }
    let (method, path, body) = match tool_name {
        "paperclipMe" => ("GET", "/agents/me".to_string(), None),
        "paperclipInboxLite" => ("GET", "/agents/me/inbox-lite".to_string(), None),
        "paperclipListAgents" => ("GET", format!("/companies/{}/agents", company_id), None),
        "paperclipGetAgent" => (
            "GET",
            format!(
                "/agents/{}{}",
                path_part(parameters.get("agentId"), "agentId")?,
                {
                    let company = optional_query(&parameters, "companyId");
                    if company.is_empty() {
                        String::new()
                    } else {
                        format!("?{company}")
                    }
                }
            ),
            None,
        ),
        "paperclipListIssues" => {
            let query = query_string(&parameters, &["companyId"]);
            let path = if query.is_empty() {
                format!("/companies/{company_id}/issues")
            } else {
                format!("/companies/{company_id}/issues?{query}")
            };
            ("GET", path, None)
        }
        "paperclipGetIssue" => (
            "GET",
            format!(
                "/issues/{}",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            None,
        ),
        "paperclipGetHeartbeatContext" => (
            "GET",
            format!(
                "/issues/{}/heartbeat-context{}",
                path_part(parameters.get("issueId"), "issueId")?,
                if let Some(wake_comment_id) =
                    parameters.get("wakeCommentId").and_then(Value::as_str)
                {
                    format!("?wakeCommentId={}", urlencoding::encode(wake_comment_id))
                } else {
                    String::new()
                }
            ),
            None,
        ),
        "paperclipListComments" => (
            "GET",
            format!(
                "/issues/{}/comments{}",
                path_part(parameters.get("issueId"), "issueId")?,
                {
                    let query = query_string(&parameters, &["issueId"]);
                    if query.is_empty() {
                        String::new()
                    } else {
                        format!("?{query}")
                    }
                }
            ),
            None,
        ),
        "paperclipGetComment" => (
            "GET",
            format!(
                "/issues/{}/comments/{}",
                path_part(parameters.get("issueId"), "issueId")?,
                path_part(parameters.get("commentId"), "commentId")?
            ),
            None,
        ),
        "paperclipListIssueApprovals" => (
            "GET",
            format!(
                "/issues/{}/approvals",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            None,
        ),
        "paperclipListDocuments" => (
            "GET",
            format!(
                "/issues/{}/documents",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            None,
        ),
        "paperclipGetDocument" => (
            "GET",
            format!(
                "/issues/{}/documents/{}",
                path_part(parameters.get("issueId"), "issueId")?,
                path_part(parameters.get("key"), "key")?
            ),
            None,
        ),
        "paperclipListDocumentRevisions" => (
            "GET",
            format!(
                "/issues/{}/documents/{}/revisions",
                path_part(parameters.get("issueId"), "issueId")?,
                path_part(parameters.get("key"), "key")?
            ),
            None,
        ),
        "paperclipListProjects" => ("GET", format!("/companies/{company_id}/projects"), None),
        "paperclipGetProject" => (
            "GET",
            format!(
                "/projects/{}{}",
                path_part(parameters.get("projectId"), "projectId")?,
                {
                    let company = optional_query(&parameters, "companyId");
                    if company.is_empty() {
                        String::new()
                    } else {
                        format!("?{company}")
                    }
                }
            ),
            None,
        ),
        "paperclipListGoals" => ("GET", format!("/companies/{company_id}/goals"), None),
        "paperclipGetGoal" => (
            "GET",
            format!("/goals/{}", path_part(parameters.get("goalId"), "goalId")?),
            None,
        ),
        "paperclipListApprovals" => {
            let status = optional_query(&parameters, "status");
            let path = format!(
                "/companies/{company_id}/approvals{}",
                if status.is_empty() {
                    String::new()
                } else {
                    format!("?{status}")
                }
            );
            ("GET", path, None)
        }
        "paperclipCreateApproval" => (
            "POST",
            format!("/companies/{company_id}/approvals"),
            Some({
                let payload = parameters
                    .get("payload")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                serde_json::json!({
                    "type": parameters.get("type").cloned().unwrap_or_else(|| serde_json::json!("create_resource")),
                    "requestedByAgentId": agent_id,
                    "payload": payload,
                    "issueIds": parameters.get("issueIds").cloned().unwrap_or_else(|| serde_json::json!([])),
                })
            }),
        ),
        "paperclipHireAgent" => {
            // Reuse the canonical hire endpoint so the request follows the
            // same path as the UI/skill: permission check, pending agent
            // creation, source-issue linking, and (when configured) board
            // approval are handled in one place. The old implementation
            // posted a hand-built approval directly, which skipped the
            // pending-agent identity and made the downstream approval path
            // diverge from Paperclip.
            (
                "POST",
                format!("/companies/{company_id}/agent-hires"),
                Some(build_hire_agent_request_body(&parameters)?),
            )
        }
        "paperclipGetApproval" => (
            "GET",
            format!(
                "/approvals/{}",
                path_part(parameters.get("approvalId"), "approvalId")?
            ),
            None,
        ),
        "paperclipGetApprovalIssues" => (
            "GET",
            format!(
                "/approvals/{}/issues",
                path_part(parameters.get("approvalId"), "approvalId")?
            ),
            None,
        ),
        "paperclipListApprovalComments" => (
            "GET",
            format!(
                "/approvals/{}/comments",
                path_part(parameters.get("approvalId"), "approvalId")?
            ),
            None,
        ),
        "paperclipAddApprovalComment" => (
            "POST",
            format!(
                "/approvals/{}/comments",
                path_part(parameters.get("approvalId"), "approvalId")?
            ),
            Some(serde_json::json!({
                "body": parameters.get("body").and_then(Value::as_str).ok_or("body is required")?
            })),
        ),
        "paperclipApprovalDecision" => {
            let approval_id = path_part(parameters.get("approvalId"), "approvalId")?;
            let action = parameters
                .get("action")
                .and_then(Value::as_str)
                .ok_or("action is required")?;
            let suffix = match action {
                "approve" => "approve",
                "reject" => "reject",
                "requestRevision" => "request-revision",
                "resubmit" => "resubmit",
                _ => return Err(format!("unsupported approval action: {action}").into()),
            };
            let body = if action == "resubmit" {
                let payload = parameters
                    .get("payloadJson")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                serde_json::json!({"payload": serde_json::from_str::<Value>(payload).map_err(|error| format!("invalid payloadJson: {error}"))?})
            } else {
                serde_json::json!({"decisionNote": parameters.get("decisionNote")})
            };
            (
                "POST",
                format!("/approvals/{approval_id}/{suffix}"),
                Some(body),
            )
        }
        "paperclipCreateIssue" => (
            "POST",
            format!("/companies/{company_id}/issues"),
            Some(object_without(&parameters, &["issueId"])),
        ),
        "paperclipUpdateIssue" => (
            "PATCH",
            format!(
                "/issues/{}",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            Some(parameters.clone()),
        ),
        "paperclipCheckoutIssue" => (
            "POST",
            format!(
                "/issues/{}/checkout",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            {
                let active_run_id =
                    run_id.ok_or("paperclipCheckoutIssue requires an active heartbeat run")?;
                let requested_agent = parameters
                    .get("agentId")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| agent_id.to_string());
                Some(serde_json::json!({
                    "agentId": requested_agent,
                    "expectedStatuses": parameters.get("expectedStatuses").cloned().unwrap_or_else(|| serde_json::json!(["todo", "backlog", "blocked"])),
                    "checkoutRunId": active_run_id
                }))
            },
        ),
        "paperclipReleaseIssue" => (
            "POST",
            format!(
                "/issues/{}/release",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            Some({
                let active_run_id =
                    run_id.ok_or("paperclipReleaseIssue requires an active heartbeat run")?;
                let mut body = serde_json::json!({ "releaseRunId": active_run_id });
                if let Some(object) = body.as_object_mut() {
                    for key in ["result", "targetStatus"] {
                        if let Some(value) = parameters.get(key).filter(|value| !value.is_null()) {
                            object.insert(key.to_string(), value.clone());
                        }
                    }
                }
                body
            }),
        ),
        "paperclipAddComment" => (
            "POST",
            format!(
                "/issues/{}/comments",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            Some({
                let mut body = object_without(&parameters, &["issueId", "authorType"]);
                if let Some(object) = body.as_object_mut() {
                    object.insert("actor_type".to_string(), Value::String("agent".to_string()));
                    object.insert("actor_id".to_string(), Value::String(agent_id.to_string()));
                    if let Some(run_id) = run_id {
                        object.insert(
                            "actor_run_id".to_string(),
                            Value::String(run_id.to_string()),
                        );
                    }
                }
                body
            }),
        ),
        "paperclipSuggestTasks"
        | "paperclipAskUserQuestions"
        | "paperclipRequestConfirmation"
        | "paperclipRequestCheckboxConfirmation" => {
            let kind = match tool_name {
                "paperclipSuggestTasks" => "suggest_tasks",
                "paperclipAskUserQuestions" => "ask_user_questions",
                "paperclipRequestConfirmation" => "request_confirmation",
                _ => "request_checkbox_confirmation",
            };
            (
                "POST",
                format!(
                    "/issues/{}/interactions",
                    path_part(parameters.get("issueId"), "issueId")?
                ),
                Some({
                    let mut body = object_without(&parameters, &["issueId"]);
                    if let Some(object) = body.as_object_mut() {
                        object.insert("kind".to_string(), Value::String(kind.to_string()));
                    }
                    body
                }),
            )
        }
        "paperclipUpsertIssueDocument" => (
            "PUT",
            format!(
                "/issues/{}/documents/{}",
                path_part(parameters.get("issueId"), "issueId")?,
                path_part(parameters.get("key"), "key")?
            ),
            Some(object_without(&parameters, &["issueId", "key"])),
        ),
        "paperclipRestoreIssueDocumentRevision" => (
            "POST",
            format!(
                "/issues/{}/documents/{}/revisions/{}/restore",
                path_part(parameters.get("issueId"), "issueId")?,
                path_part(parameters.get("key"), "key")?,
                path_part(parameters.get("revisionId"), "revisionId")?
            ),
            Some(serde_json::json!({})),
        ),
        "paperclipGetIssueWorkspaceRuntime" => (
            "GET",
            format!(
                "/issues/{}/heartbeat-context",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            None,
        ),
        "paperclipControlIssueWorkspaceServices" => {
            let runtime = Box::pin(call_paperclip_builtin_tool(
                state,
                token,
                company_id,
                agent_id,
                run_id,
                "paperclipGetIssueWorkspaceRuntime",
                &serde_json::json!({
                    "issueId": parameters.get("issueId").cloned().unwrap_or(Value::Null)
                }),
            ))
            .await?;
            let workspace_id = runtime
                .get("workspace")
                .and_then(|workspace| workspace.get("id"))
                .and_then(Value::as_str)
                .ok_or("Issue has no current execution workspace")?;
            let action = path_part(parameters.get("action"), "action")?;
            (
                "POST",
                format!("/execution-workspaces/{workspace_id}/runtime-services/{action}"),
                Some(object_without(
                    &parameters,
                    &["issueId", "action"],
                )),
            )
        }
        "paperclipWaitForIssueWorkspaceService" => (
            "GET",
            format!(
                "/issues/{}/heartbeat-context",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            None,
        ),
        "paperclipLinkIssueApproval" => (
            "POST",
            format!(
                "/issues/{}/approvals",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            Some(serde_json::json!({"approvalId": parameters.get("approvalId")})),
        ),
        "paperclipUnlinkIssueApproval" => (
            "DELETE",
            format!(
                "/issues/{}/approvals/{}",
                path_part(parameters.get("issueId"), "issueId")?,
                path_part(parameters.get("approvalId"), "approvalId")?
            ),
            None,
        ),
        "paperclipListCases" => {
            let query = query_string(&parameters, &["companyId"]);
            let path = if query.is_empty() {
                format!("/companies/{company_id}/cases")
            } else {
                format!("/companies/{company_id}/cases?{query}")
            };
            ("GET", path, None)
        }
        "paperclipGetCase" => (
            "GET",
            format!("/cases/{}", path_part(parameters.get("caseId"), "caseId")?),
            None,
        ),
        "paperclipListRoutines" => ("GET", format!("/companies/{company_id}/routines"), None),
        "paperclipGetRoutine" => (
            "GET",
            format!(
                "/routines/{}",
                path_part(parameters.get("routineId"), "routineId")?
            ),
            None,
        ),
        "paperclipCreateRoutine" => (
            "POST",
            format!("/companies/{company_id}/routines"),
            Some({
                let mut body = serde_json::json!({
                    "title": parameters.get("title").cloned().ok_or("title is required")?
                });
                if let Some(obj) = body.as_object_mut() {
                    for key in ["assigneeAgentId", "description", "env"] {
                        if let Some(value) = parameters.get(key).filter(|v| !v.is_null()) {
                            obj.insert(key.to_string(), value.clone());
                        }
                    }
                }
                body
            }),
        ),
        "paperclipUpdateRoutine" => (
            "PATCH",
            format!(
                "/routines/{}",
                path_part(parameters.get("routineId"), "routineId")?
            ),
            Some({
                let mut body = serde_json::json!({});
                if let Some(obj) = body.as_object_mut() {
                    for key in ["assigneeAgentId", "title", "description", "env"] {
                        if let Some(value) = parameters.get(key).filter(|v| !v.is_null()) {
                            obj.insert(key.to_string(), value.clone());
                        }
                    }
                }
                body
            }),
        ),
        "paperclipListIssueDocumentAnnotations" => (
            "GET",
            format!(
                "/issues/{}/documents/{}/annotations",
                path_part(parameters.get("issueId"), "issueId")?,
                path_part(parameters.get("key"), "key")?
            ),
            None,
        ),
        "paperclipGetIssueDocumentAnnotationThread" => (
            "GET",
            format!(
                "/issues/{}/documents/{}/annotations/{}",
                path_part(parameters.get("issueId"), "issueId")?,
                path_part(parameters.get("key"), "key")?,
                path_part(parameters.get("threadId"), "threadId")?
            ),
            None,
        ),
        "paperclipCreateIssueDocumentAnnotation" => (
            "POST",
            format!(
                "/issues/{}/documents/{}/annotations",
                path_part(parameters.get("issueId"), "issueId")?,
                path_part(parameters.get("key"), "key")?
            ),
            Some({
                let mut body = serde_json::json!({
                    "body": parameters.get("body").cloned().ok_or("body is required")?
                });
                if let Some(obj) = body.as_object_mut() {
                    for key in ["selectedText", "anchorSelector", "selector", "resolved"] {
                        if let Some(value) = parameters.get(key).filter(|v| !v.is_null()) {
                            obj.insert(key.to_string(), value.clone());
                        }
                    }
                }
                body
            }),
        ),
        "paperclipReplyIssueDocumentAnnotation" => (
            "POST",
            format!(
                "/issues/{}/documents/{}/annotations/{}/comments",
                path_part(parameters.get("issueId"), "issueId")?,
                path_part(parameters.get("key"), "key")?,
                path_part(parameters.get("threadId"), "threadId")?
            ),
            Some(serde_json::json!({
                "body": parameters.get("body").cloned().ok_or("body is required")?
            })),
        ),
        "paperclipUpdateIssueDocumentAnnotation" => (
            "PATCH",
            format!(
                "/issues/{}/documents/{}/annotations/{}",
                path_part(parameters.get("issueId"), "issueId")?,
                path_part(parameters.get("key"), "key")?,
                path_part(parameters.get("threadId"), "threadId")?
            ),
            Some({
                let mut body = serde_json::json!({});
                if let Some(obj) = body.as_object_mut() {
                    if let Some(value) = parameters.get("resolved").filter(|v| !v.is_null()) {
                        obj.insert("resolved".to_string(), value.clone());
                    }
                }
                body
            }),
        ),
        "paperclipListLabels" => ("GET", format!("/companies/{company_id}/labels"), None),
        "paperclipCreateLabel" => (
            "POST",
            format!("/companies/{company_id}/labels"),
            Some({
                let mut body = serde_json::json!({
                    "name": parameters.get("name").cloned().ok_or("name is required")?,
                    "color": parameters.get("color").cloned().ok_or("color is required")?
                });
                if let Some(obj) = body.as_object_mut() {
                    if let Some(value) = parameters.get("description").filter(|v| !v.is_null()) {
                        obj.insert("description".to_string(), value.clone());
                    }
                }
                body
            }),
        ),
        "paperclipDeleteLabel" => (
            "DELETE",
            format!(
                "/labels/{}",
                path_part(parameters.get("labelId"), "labelId")?
            ),
            None,
        ),
        "paperclipListIssueExternalObjects" => (
            "GET",
            format!(
                "/issues/{}/external-objects",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            None,
        ),
        "paperclipRefreshIssueExternalObjects" => (
            "POST",
            format!(
                "/issues/{}/external-objects/refresh",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            Some(serde_json::json!({})),
        ),
        "paperclipListIssueFileResources" => (
            "GET",
            format!(
                "/issues/{}/file-resources/list",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            None,
        ),
        "paperclipResolveIssueFileResource" => (
            "GET",
            format!(
                "/issues/{}/file-resources/resolve",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            None,
        ),
        "paperclipGetIssueFileResourceContent" => (
            "GET",
            format!(
                "/issues/{}/file-resources/content",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            None,
        ),
        "paperclipGetCaseChildren" => (
            "GET",
            format!(
                "/cases/{}/children",
                path_part(parameters.get("caseId"), "caseId")?
            ),
            None,
        ),
        "paperclipCreateCaseLink" => (
            "POST",
            format!(
                "/cases/{}/links",
                path_part(parameters.get("caseId"), "caseId")?
            ),
            Some(serde_json::json!({
                "issueId": parameters.get("issueId").cloned().ok_or("issueId is required")?,
                "role": parameters.get("role").cloned().ok_or("role is required")?
            })),
        ),
        "paperclipGetIssueCases" => (
            "GET",
            format!(
                "/issues/{}/cases",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            None,
        ),
        "paperclipListIssueAttachments" => (
            "GET",
            format!(
                "/issues/{}/attachments",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            None,
        ),
        "paperclipCreateIssueAttachment" => (
            "POST",
            format!(
                "/companies/{company_id}/issues/{}/attachments",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            Some(serde_json::json!({
                "filename": parameters.get("filename").cloned().ok_or("filename is required")?,
                "contentType": parameters.get("contentType").cloned().ok_or("contentType is required")?,
                "base64Content": parameters.get("base64Content").cloned().ok_or("base64Content is required")?
            })),
        ),
        "paperclipGetAttachmentContent" => (
            "GET",
            format!(
                "/attachments/{}/content",
                path_part(parameters.get("attachmentId"), "attachmentId")?
            ),
            None,
        ),
        "paperclipDeleteAttachment" => (
            "DELETE",
            format!(
                "/attachments/{}",
                path_part(parameters.get("attachmentId"), "attachmentId")?
            ),
            None,
        ),
        "paperclipListCaseDocuments" => (
            "GET",
            format!(
                "/cases/{}/documents",
                path_part(parameters.get("caseId"), "caseId")?
            ),
            None,
        ),
        "paperclipGetCaseDocument" => (
            "GET",
            format!(
                "/cases/{}/documents/{}",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?
            ),
            None,
        ),
        "paperclipUpsertCaseDocument" => (
            "PUT",
            format!(
                "/cases/{}/documents/{}",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?
            ),
            Some(
                serde_json::json!({"body": parameters.get("body").cloned().ok_or("body is required")?}),
            ),
        ),
        "paperclipListCaseDocumentRevisions" => (
            "GET",
            format!(
                "/cases/{}/documents/{}/revisions",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?
            ),
            None,
        ),
        "paperclipRestoreCaseDocumentRevision" => (
            "POST",
            format!(
                "/cases/{}/documents/{}/revisions/{}/restore",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?,
                path_part(parameters.get("revisionId"), "revisionId")?
            ),
            Some(serde_json::json!({})),
        ),
        "paperclipDeleteCaseDocument" => (
            "DELETE",
            format!(
                "/cases/{}/documents/{}",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?
            ),
            None,
        ),
        "paperclipLockCaseDocument" => (
            "POST",
            format!(
                "/cases/{}/documents/{}/lock",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?
            ),
            Some(serde_json::json!({})),
        ),
        "paperclipUnlockCaseDocument" => (
            "POST",
            format!(
                "/cases/{}/documents/{}/unlock",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?
            ),
            Some(serde_json::json!({})),
        ),
        "paperclipGetCaseEvents" => (
            "GET",
            format!(
                "/cases/{}/events",
                path_part(parameters.get("caseId"), "caseId")?
            ),
            None,
        ),
        "paperclipListCaseDocumentAnnotations" => (
            "GET",
            format!(
                "/cases/{}/documents/{}/annotations",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?
            ),
            None,
        ),
        "paperclipGetCaseDocumentAnnotationThread" => (
            "GET",
            format!(
                "/cases/{}/documents/{}/annotations/{}",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?,
                path_part(parameters.get("threadId"), "threadId")?
            ),
            None,
        ),
        "paperclipCreateCaseDocumentAnnotation" => (
            "POST",
            format!(
                "/cases/{}/documents/{}/annotations",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?
            ),
            Some(
                serde_json::json!({"body": parameters.get("body").cloned().ok_or("body is required")?}),
            ),
        ),
        "paperclipReplyCaseDocumentAnnotation" => (
            "POST",
            format!(
                "/cases/{}/documents/{}/annotations/{}/comments",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?,
                path_part(parameters.get("threadId"), "threadId")?
            ),
            Some(
                serde_json::json!({"body": parameters.get("body").cloned().ok_or("body is required")?}),
            ),
        ),
        "paperclipUpdateCaseDocumentAnnotation" => (
            "PATCH",
            format!(
                "/cases/{}/documents/{}/annotations/{}",
                path_part(parameters.get("caseId"), "caseId")?,
                path_part(parameters.get("key"), "key")?,
                path_part(parameters.get("threadId"), "threadId")?
            ),
            Some({
                let mut body = serde_json::json!({});
                if let Some(obj) = body.as_object_mut() {
                    if let Some(value) = parameters.get("resolved").filter(|v| !v.is_null()) {
                        obj.insert("resolved".to_string(), value.clone());
                    }
                }
                body
            }),
        ),
        "paperclipListRoutineRevisions" => (
            "GET",
            format!(
                "/routines/{}/revisions",
                path_part(parameters.get("routineId"), "routineId")?
            ),
            None,
        ),
        "paperclipRestoreRoutineRevision" => (
            "POST",
            format!(
                "/routines/{}/revisions/{}/restore",
                path_part(parameters.get("routineId"), "routineId")?,
                path_part(parameters.get("revisionId"), "revisionId")?
            ),
            Some(serde_json::json!({})),
        ),
        "paperclipListRoutineDescriptionAnnotations" => (
            "GET",
            format!(
                "/routines/{}/description/annotations",
                path_part(parameters.get("routineId"), "routineId")?
            ),
            None,
        ),
        "paperclipGetRoutineDescriptionAnnotationThread" => (
            "GET",
            format!(
                "/routines/{}/description/annotations/{}",
                path_part(parameters.get("routineId"), "routineId")?,
                path_part(parameters.get("threadId"), "threadId")?
            ),
            None,
        ),
        "paperclipCreateRoutineDescriptionAnnotation" => (
            "POST",
            format!(
                "/routines/{}/description/annotations",
                path_part(parameters.get("routineId"), "routineId")?
            ),
            Some(
                serde_json::json!({"body": parameters.get("body").cloned().ok_or("body is required")?}),
            ),
        ),
        "paperclipReplyRoutineDescriptionAnnotation" => (
            "POST",
            format!(
                "/routines/{}/description/annotations/{}/comments",
                path_part(parameters.get("routineId"), "routineId")?,
                path_part(parameters.get("threadId"), "threadId")?
            ),
            Some(
                serde_json::json!({"body": parameters.get("body").cloned().ok_or("body is required")?}),
            ),
        ),
        "paperclipUpdateRoutineDescriptionAnnotation" => (
            "PATCH",
            format!(
                "/routines/{}/description/annotations/{}",
                path_part(parameters.get("routineId"), "routineId")?,
                path_part(parameters.get("threadId"), "threadId")?
            ),
            Some({
                let mut body = serde_json::json!({});
                if let Some(obj) = body.as_object_mut() {
                    if let Some(value) = parameters.get("resolved").filter(|v| !v.is_null()) {
                        obj.insert("resolved".to_string(), value.clone());
                    }
                }
                body
            }),
        ),
        "paperclipCreateRoutineTrigger" => (
            "POST",
            format!(
                "/routines/{}/triggers",
                path_part(parameters.get("routineId"), "routineId")?
            ),
            Some(serde_json::json!({})),
        ),
        "paperclipUpdateRoutineTrigger" => (
            "PATCH",
            format!(
                "/routine-triggers/{}",
                path_part(parameters.get("triggerId"), "triggerId")?
            ),
            Some(serde_json::json!({})),
        ),
        "paperclipDeleteRoutineTrigger" => (
            "DELETE",
            format!(
                "/routine-triggers/{}",
                path_part(parameters.get("triggerId"), "triggerId")?
            ),
            None,
        ),
        "paperclipRotateRoutineTriggerSecret" => (
            "POST",
            format!(
                "/routine-triggers/{}/rotate-secret",
                path_part(parameters.get("triggerId"), "triggerId")?
            ),
            Some(serde_json::json!({})),
        ),
        "paperclipListRoutineRuns" => (
            "GET",
            format!(
                "/routines/{}/runs",
                path_part(parameters.get("routineId"), "routineId")?
            ),
            None,
        ),
        "paperclipRunRoutine" => (
            "POST",
            format!(
                "/routines/{}/run",
                path_part(parameters.get("routineId"), "routineId")?
            ),
            Some(serde_json::json!({})),
        ),
        "paperclipCreateCase" => (
            "POST",
            format!("/companies/{company_id}/cases"),
            Some({
                let mut body = serde_json::json!({
                    "caseType": parameters.get("caseType").cloned().ok_or("caseType is required")?,
                    "title": parameters.get("title").cloned().ok_or("title is required")?
                });
                if let Some(obj) = body.as_object_mut() {
                    for key in [
                        "projectId",
                        "key",
                        "summary",
                        "status",
                        "fields",
                        "parentCaseId",
                    ] {
                        if let Some(value) = parameters.get(key).filter(|v| !v.is_null()) {
                            obj.insert(key.to_string(), value.clone());
                        }
                    }
                }
                body
            }),
        ),
        "paperclipUpdateCase" => (
            "PATCH",
            format!("/cases/{}", path_part(parameters.get("caseId"), "caseId")?),
            Some({
                let mut body = serde_json::json!({});
                if let Some(obj) = body.as_object_mut() {
                    for key in [
                        "projectId",
                        "title",
                        "summary",
                        "status",
                        "fields",
                        "parentCaseId",
                        "labelIds",
                    ] {
                        if let Some(value) = parameters.get(key).filter(|v| !v.is_null()) {
                            obj.insert(key.to_string(), value.clone());
                        }
                    }
                }
                body
            }),
        ),
        "paperclipApiRequest" => {
            let method = parameters
                .get("method")
                .and_then(Value::as_str)
                .ok_or("method is required")?;
            if !matches!(method, "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
                return Err(format!("unsupported HTTP method: {method}").into());
            }
            let path = parameters
                .get("path")
                .and_then(Value::as_str)
                .ok_or("path is required")?;
            validate_paperclip_api_path(path)?;
            let body = parameters
                .get("jsonBody")
                .and_then(Value::as_str)
                .map(serde_json::from_str)
                .transpose()
                .map_err(|error| format!("invalid jsonBody: {error}"))?;
            (method, path.to_string(), body)
        }
        _ => return Err(format!("Unknown Paperclip tool: {tool_name}").into()),
    };
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|error| format!("invalid HTTP method: {error}"))?;
    let client = PaperclipInternalClient::new(token, run_id);
    let (status, value) = client.request(method.clone(), &path, body).await?;
    if !status.is_success() {
        return Err(PaperclipBuiltinToolError::upstream(
            status,
            format!("{} {} failed with {}: {}", method, path, status, value),
        ));
    }
    if tool_name == "paperclipGetIssueWorkspaceRuntime" {
        let workspace = value
            .get("currentExecutionWorkspace")
            .cloned()
            .or_else(|| value.get("workspace").cloned())
            .filter(|workspace| !workspace.is_null());
        let runtime_services = workspace
            .as_ref()
            .and_then(|workspace| workspace.get("runtimeServices"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        return Ok(serde_json::json!({
            "context": value,
            "workspace": workspace,
            "runtimeServices": runtime_services,
        }));
    }
    Ok(value)
}

enum GatewayInvocationReservation {
    Created { invocation_id: Uuid },
    Replayed((StatusCode, Json<Value>)),
}

enum GatewayApprovalReservation {
    Created {
        invocation_id: Uuid,
        action_id: Uuid,
    },
    Replayed((StatusCode, Json<Value>)),
}

fn gateway_idempotency_key(
    headers: &HeaderMap,
    body: &Value,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    let raw = if let Some(value) = body.get("idempotencyKey") {
        if value.is_null() {
            return Ok(None);
        }
        let Some(raw) = value.as_str() else {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "idempotencyKey must be a string or null",
                    "reasonCode": "invalid_idempotency_key"
                })),
            ));
        };
        raw.to_string()
    } else {
        let Some(raw) = headers
            .get("idempotency-key")
            .and_then(|value| value.to_str().ok())
        else {
            return Ok(None);
        };
        raw.to_string()
    };
    let key = raw.trim();
    if key.is_empty() {
        return Ok(None);
    }
    if key.len() > 255 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "idempotencyKey must be at most 255 characters",
                "reasonCode": "invalid_idempotency_key"
            })),
        ));
    }
    Ok(Some(key.to_string()))
}

async fn replay_gateway_invocation(
    state: &AppState,
    row: sqlx::postgres::PgRow,
    requested_tool_name: &str,
    requested_arguments_hash: &str,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let invocation_id: Uuid = row.get("id");
    let stored_tool_name: String = row.get("tool_name");
    if stored_tool_name != requested_tool_name {
        return Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "Idempotency key was already used for a different tool",
                "reasonCode": "idempotency_key_reused",
                "invocationId": invocation_id
            })),
        ));
    }
    let stored_arguments_hash: Option<String> = row.try_get("arguments_hash").unwrap_or(None);
    if stored_arguments_hash
        .as_deref()
        .is_some_and(|hash| hash != requested_arguments_hash)
    {
        return Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "Idempotency key was already used for different arguments",
                "reasonCode": "idempotency_key_reused",
                "invocationId": invocation_id
            })),
        ));
    }

    let status: String = row.get("status");
    let error_code: Option<String> = row.try_get("error_code").unwrap_or(None);
    let error_message: Option<String> = row.try_get("error_message").unwrap_or(None);
    match status.as_str() {
        "pending" | "awaiting_approval" => {
            let action = sqlx::query(
                "SELECT id, status
                   FROM tool_action_requests
                  WHERE company_id = $1 AND invocation_id = $2
                  ORDER BY created_at DESC
                  LIMIT 1",
            )
            .bind(row.get::<Uuid, _>("company_id"))
            .bind(invocation_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": error.to_string()})),
                )
            })?;
            let Some(action) = action else {
                return Ok((
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": "Idempotent tool invocation is still being initialized",
                        "reasonCode": "idempotency_in_progress",
                        "status": "pending",
                        "invocationId": invocation_id
                    })),
                ));
            };
            return Ok((
                StatusCode::OK,
                Json(serde_json::json!({
                    "decision": "require_approval",
                    "status": action.get::<String, _>("status"),
                    "replayed": true,
                    "invocationId": invocation_id,
                    "actionRequestId": action.get::<Uuid, _>("id")
                })),
            ));
        }
        "succeeded" => {
            let result_summary: Option<Value> = row.try_get("result_summary").unwrap_or(None);
            Ok((
                StatusCode::OK,
                Json(serde_json::json!({
                    "decision": "allowed",
                    "status": "replayed",
                    "replayed": true,
                    "invocationId": invocation_id,
                    "result": result_summary.unwrap_or(Value::Null)
                })),
            ))
        }
        "executing" => Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "Tool invocation is still executing",
                "reasonCode": "idempotency_in_progress",
                "status": "executing",
                "invocationId": invocation_id
            })),
        )),
        "denied" | "rate_limited" => {
            let status_code = if status == "rate_limited" {
                StatusCode::TOO_MANY_REQUESTS
            } else {
                StatusCode::FORBIDDEN
            };
            Ok((
                status_code,
                Json(serde_json::json!({
                    "error": error_message.unwrap_or_else(|| {
                        if status == "rate_limited" {
                            "Tool call was rate limited".to_string()
                        } else {
                            "Tool call denied by policy".to_string()
                        }
                    }),
                    "reasonCode": error_code.unwrap_or_else(|| {
                        if status == "rate_limited" {
                            "rate_limited".to_string()
                        } else {
                            "policy_denied".to_string()
                        }
                    }),
                    "decision": if status == "rate_limited" { "rate_limited" } else { "deny" },
                    "replayed": true,
                    "invocationId": invocation_id
                })),
            ))
        }
        "failed" => Ok((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": error_message.unwrap_or_else(|| "Tool call failed".to_string()),
                "reasonCode": error_code.unwrap_or_else(|| "tool_execution_failed".to_string()),
                "replayed": true,
                "invocationId": invocation_id
            })),
        )),
        // Paperclip counts cancelled/timed_out as terminal invocation states,
        // so an idempotent replay must return the recorded outcome instead of
        // reporting an unsupported state.
        "cancelled" => Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": error_message.unwrap_or_else(|| "Tool call was cancelled".to_string()),
                "reasonCode": error_code.unwrap_or_else(|| "tool_call_cancelled".to_string()),
                "status": "cancelled",
                "replayed": true,
                "invocationId": invocation_id
            })),
        )),
        "timed_out" => Ok((
            StatusCode::GATEWAY_TIMEOUT,
            Json(serde_json::json!({
                "error": error_message.unwrap_or_else(|| "Tool call timed out".to_string()),
                "reasonCode": error_code.unwrap_or_else(|| "tool_call_timed_out".to_string()),
                "status": "timed_out",
                "replayed": true,
                "invocationId": invocation_id
            })),
        )),
        _ => Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "Idempotent tool invocation has an unsupported state",
                "reasonCode": "idempotency_state_unsupported",
                "status": status,
                "invocationId": invocation_id
            })),
        )),
    }
}

async fn load_gateway_invocation_replay(
    state: &AppState,
    company_id: Uuid,
    idempotency_key: &str,
    tool_name: &str,
    arguments_hash: &str,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let row = sqlx::query(
        "SELECT id, company_id, tool_name, arguments_hash, status, error_code,
                error_message, result_summary
           FROM tool_invocations
          WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(company_id)
    .bind(idempotency_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error.to_string()})),
        )
    })?;
    let Some(row) = row else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Idempotency conflict could not be resolved",
                "reasonCode": "idempotency_conflict_unresolved"
            })),
        ));
    };
    replay_gateway_invocation(state, row, tool_name, arguments_hash).await
}

async fn reserve_gateway_invocation(
    state: &AppState,
    company_id: Uuid,
    agent_id: Option<Uuid>,
    run_id: Option<Uuid>,
    connection_id: Option<Uuid>,
    tool_name: &str,
    parameters: &Value,
    arguments_summary: &Value,
    policy_decision: &str,
    status: &str,
    idempotency_key: Option<&str>,
    error_code: Option<&str>,
    error_message: Option<&str>,
) -> Result<GatewayInvocationReservation, (StatusCode, Json<Value>)> {
    let invocation_id = Uuid::new_v4();
    let arguments_hash = hash_gateway_token(&parameters.to_string());
    let now = chrono::Utc::now();
    let started_at = (status == "executing").then_some(now);
    let completed_at = matches!(status, "denied" | "failed" | "succeeded").then_some(now);
    let inserted = sqlx::query(
        "INSERT INTO tool_invocations
            (id, company_id, idempotency_key, actor_type, actor_id, agent_id, run_id,
             connection_id, tool_name, arguments_hash, arguments_summary, policy_decision,
             status, error_code, error_message, started_at, completed_at)
         VALUES ($1,$2,$3,
                 CASE WHEN $5::uuid IS NULL THEN 'system' ELSE 'agent' END,
                 COALESCE($4, $2::text),$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
         ON CONFLICT (company_id, idempotency_key) DO NOTHING
         RETURNING id",
    )
    .bind(invocation_id)
    .bind(company_id)
    .bind(idempotency_key)
    .bind(agent_id.map(|id| id.to_string()))
    .bind(agent_id)
    .bind(run_id)
    .bind(connection_id)
    .bind(tool_name)
    .bind(&arguments_hash)
    .bind(arguments_summary)
    .bind(policy_decision)
    .bind(status)
    .bind(error_code)
    .bind(error_message)
    .bind(started_at)
    .bind(completed_at)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error.to_string()})),
        )
    })?;
    if inserted.is_some() {
        return Ok(GatewayInvocationReservation::Created { invocation_id });
    }
    let Some(idempotency_key) = idempotency_key else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":"Tool invocation was not inserted"})),
        ));
    };
    load_gateway_invocation_replay(
        state,
        company_id,
        idempotency_key,
        tool_name,
        &arguments_hash,
    )
    .await
    .map(GatewayInvocationReservation::Replayed)
}

async fn reserve_gateway_approval(
    state: &AppState,
    company_id: Uuid,
    agent_id: Option<Uuid>,
    run_id: Option<Uuid>,
    issue_id: Option<Uuid>,
    connection_id: Option<Uuid>,
    tool_name: &str,
    parameters: &Value,
    arguments_summary: &Value,
    policy_decision: &str,
    idempotency_key: Option<&str>,
) -> Result<GatewayApprovalReservation, (StatusCode, Json<Value>)> {
    let invocation_id = Uuid::new_v4();
    let action_id = Uuid::new_v4();
    let arguments_hash = hash_gateway_token(&parameters.to_string());
    let mut tx = state.pool.begin().await.map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error.to_string()})),
        )
    })?;
    let inserted = sqlx::query(
        "INSERT INTO tool_invocations
            (id, company_id, idempotency_key, actor_type, actor_id, agent_id, run_id,
             connection_id, tool_name, arguments_hash, arguments_summary, policy_decision,
             status, approval_state)
         VALUES ($1,$2,$3,
                 CASE WHEN $5::uuid IS NULL THEN 'system' ELSE 'agent' END,
                 COALESCE($4, $2::text),$5,$6,$7,$8,$9,$10,$11,'pending','pending')
         ON CONFLICT (company_id, idempotency_key) DO NOTHING
         RETURNING id",
    )
    .bind(invocation_id)
    .bind(company_id)
    .bind(idempotency_key)
    .bind(agent_id.map(|id| id.to_string()))
    .bind(agent_id)
    .bind(run_id)
    .bind(connection_id)
    .bind(tool_name)
    .bind(&arguments_hash)
    .bind(arguments_summary)
    .bind(policy_decision)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error.to_string()})),
        )
    })?;
    if inserted.is_some() {
        if let Err(error) = sqlx::query(
            "INSERT INTO tool_action_requests
                (id, company_id, invocation_id, issue_id, status,
                 canonical_arguments_hash, canonical_arguments_summary, signed_arguments,
                 preview_markdown, requested_by_agent_id)
             VALUES ($1,$2,$3,$4,'pending',$5,$6,$7,$8,$9)",
        )
        .bind(action_id)
        .bind(company_id)
        .bind(invocation_id)
        .bind(issue_id)
        .bind(&arguments_hash)
        .bind(arguments_summary)
        .bind(parameters.to_string())
        .bind(format!("Tool call requires approval: {tool_name}"))
        .bind(agent_id)
        .execute(&mut *tx)
        .await
        {
            let _ = tx.rollback().await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": error.to_string()})),
            ));
        }
        if let Err(error) = tx.commit().await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": error.to_string()})),
            ));
        }
        return Ok(GatewayApprovalReservation::Created {
            invocation_id,
            action_id,
        });
    }
    let _ = tx.rollback().await;
    let Some(idempotency_key) = idempotency_key else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":"Tool invocation was not inserted"})),
        ));
    };
    load_gateway_invocation_replay(
        state,
        company_id,
        idempotency_key,
        tool_name,
        &arguments_hash,
    )
    .await
    .map(GatewayApprovalReservation::Replayed)
}

async fn call_gateway_tool(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let Some(token) = bearer_or_gateway_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Tool gateway session token is required"})),
        );
    };
    let session = match load_gateway_session(&state, &token).await {
        Ok(row) => row,
        Err(response) => return response,
    };
    if !gateway_token_action_allowed(&session, "tools/call") {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Gateway token is not allowed to call tools",
                "reasonCode": "gateway_token_action_denied"
            })),
        );
    }
    let tool_name = body
        .get("tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(tool_name) = tool_name else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "tool is required and must be a string"})),
        );
    };
    let idempotency_key = match gateway_idempotency_key(&headers, &body) {
        Ok(key) => key,
        Err(response) => return response,
    };
    let company_id: Uuid = session.get("company_id");
    let agent_id: Option<Uuid> = session.get("agent_id");
    let run_id: Option<Uuid> = session.get("run_id");
    let gateway_id: Option<Uuid> = session.get("gateway_id");
    let issue_id: Option<Uuid> = session.get("issue_id");
    let project_id: Option<Uuid> = session.get("project_id");
    let agent_id_text = agent_id.map(|id| id.to_string());
    if is_gateway_virtual_tool(tool_name) {
        let parameters = body
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if tool_name == "search_tools" {
            let decision = gateway_decision_for_gateway(
                &state,
                company_id,
                agent_id,
                tool_name,
                gateway_id,
                issue_id,
                project_id,
            )
            .await;
            if decision == "deny" {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "Tool call denied by policy",
                        "reasonCode": "policy_denied",
                        "decision": "deny"
                    })),
                );
            }
            return match execute_virtual_search_tools(
                &state,
                company_id,
                agent_id,
                gateway_id,
                issue_id,
                project_id,
                &parameters,
            )
            .await
            {
                Ok(value) => (
                    StatusCode::OK,
                    Json(serde_json::json!({"decision":"allowed","result":value})),
                ),
                Err(error) => (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({
                        "error": error,
                        "reasonCode": "virtual_tool_execution_failed"
                    })),
                ),
            };
        }

        let Some(target_name) = parameters
            .get("tool")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "run_tool requires a target tool name",
                    "reasonCode": "invalid_tool_arguments"
                })),
            );
        };
        if is_gateway_virtual_tool(target_name) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "run_tool cannot target another virtual tool",
                    "reasonCode": "invalid_tool_arguments"
                })),
            );
        }
        let target = match find_mcp_catalog_tool(&state, company_id, target_name).await {
            Ok(Some(tool))
                if tool.transport == "mcp_remote"
                    && mcp_on_demand_enabled(&tool.connection_config) => tool,
            Ok(_) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "error": "Target tool is not an on-demand remote MCP tool",
                        "reasonCode": "tool_not_found"
                    })),
                )
            }
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": error,
                        "reasonCode": "mcp_catalog_unavailable"
                    })),
                )
            }
        };
        let target_parameters = parameters
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        // Re-enter the normal gateway path so the selected catalog tool gets
        // the same argument validation, policy, approval, idempotency, audit,
        // and execution behavior as a directly listed tool.
        let _ = target;
        return Box::pin(call_gateway_tool(
            State(state),
            headers,
            Json(serde_json::json!({
                "tool": target_name,
                "parameters": target_parameters
            })),
        ))
        .await;
    }
    if tool_name.starts_with("paperclip") {
        let Some(agent_id) = agent_id else {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "This Paperclip tool requires an agent-bound gateway session",
                    "reasonCode": "agent_context_required"
                })),
            );
        };
        let parameters = body
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if !is_paperclip_builtin_tool(tool_name) {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Paperclip tool not found", "reasonCode": "tool_not_found",
                })),
            );
        }
        if let Err(error) = validate_paperclip_arguments(tool_name, &parameters) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": error, "reasonCode": "invalid_tool_arguments",
                })),
            );
        }
        let arguments_summary = serde_json::json!({
            "valueType": "object",
            "keys": parameters.as_object().map(|value| value.len()).unwrap_or(0)
        });
        let decision = gateway_decision_full_for_gateway(
            &state,
            company_id,
            Some(agent_id),
            tool_name,
            true,
            gateway_id,
            issue_id,
            project_id,
        )
        .await;
        if decision.decision == "rate_limited" {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": "Tool access rate limit exceeded.",
                    "reasonCode": "rate_limited",
                    "rateLimitState": decision.rate_limit_state,
                })),
            );
        }
        let decision = decision.decision;
        if decision == "deny" {
            let invocation_id = match reserve_gateway_invocation(
                &state,
                company_id,
                Some(agent_id),
                run_id,
                None,
                tool_name,
                &parameters,
                &arguments_summary,
                "deny",
                "denied",
                idempotency_key.as_deref(),
                Some("policy_denied"),
                Some("Tool call denied by policy"),
            )
            .await
            {
                Ok(GatewayInvocationReservation::Created { invocation_id }) => invocation_id,
                Ok(GatewayInvocationReservation::Replayed(response)) => return response,
                Err(response) => return response,
            };
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "Tool call denied by policy", "reasonCode": "policy_denied",
                    "decision": "deny", "invocationId": invocation_id,
                })),
            );
        }
        if decision == "require_approval" {
            let issue_id: Option<Uuid> = session.get("issue_id");
            let (invocation_id, action_id) = match reserve_gateway_approval(
                &state,
                company_id,
                Some(agent_id),
                run_id,
                issue_id,
                None,
                tool_name,
                &parameters,
                &arguments_summary,
                &decision,
                idempotency_key.as_deref(),
            )
            .await
            {
                Ok(GatewayApprovalReservation::Created {
                    invocation_id,
                    action_id,
                }) => (invocation_id, action_id),
                Ok(GatewayApprovalReservation::Replayed(response)) => return response,
                Err(response) => return response,
            };
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "decision": "require_approval", "invocationId": invocation_id,
                    "actionRequestId": action_id, "status": "pending",
                })),
            );
        }
        let invocation_id = match reserve_gateway_invocation(
            &state,
            company_id,
            Some(agent_id),
            run_id,
            None,
            tool_name,
            &parameters,
            &arguments_summary,
            "allow",
            "executing",
            idempotency_key.as_deref(),
            None,
            None,
        )
        .await
        {
            Ok(GatewayInvocationReservation::Created { invocation_id }) => invocation_id,
            Ok(GatewayInvocationReservation::Replayed(response)) => return response,
            Err(response) => return response,
        };
        let result = call_paperclip_builtin_tool(
            &state,
            &token,
            company_id,
            agent_id,
            run_id,
            tool_name,
            &parameters,
        )
        .await;
        return match result {
            Ok(value) => {
                let _ = sqlx::query(
                    "UPDATE tool_invocations SET status='succeeded', result_summary=$2,
                     completed_at=NOW(), updated_at=NOW() WHERE id=$1",
                )
                .bind(invocation_id)
                .bind(serde_json::json!({"valueType": "json"}))
                .execute(&state.pool)
                .await;
                let _ = sqlx::query(
                    "INSERT INTO tool_call_events
                     (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,
                      decision,outcome,invocation_id)
                     VALUES ($1,'call_completed','agent',$2,$3,$4,$5,$6,'success',$7)",
                )
                .bind(company_id)
                .bind(agent_id.to_string())
                .bind(agent_id)
                .bind(run_id)
                .bind(tool_name)
                .bind(&decision)
                .bind(invocation_id)
                .execute(&state.pool)
                .await;
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "decision": "allowed",
                        "result": value,
                        "invocationId": invocation_id
                    })),
                )
            }
            Err(error) => {
                let response_status = error.response_status();
                let reason_code = error.reason_code();
                let error_message = error.to_string();
                let upstream_status = error.upstream_status.map(|status| status.as_u16());
                let _ = sqlx::query(
                    "UPDATE tool_invocations SET status='failed', error_message=$2,
                     completed_at=NOW(), updated_at=NOW() WHERE id=$1",
                )
                .bind(invocation_id)
                .bind(&error_message)
                .execute(&state.pool)
                .await;
                (
                    response_status,
                    Json(serde_json::json!({
                        "error": error_message,
                        "reasonCode": reason_code,
                        "upstreamStatus": upstream_status,
                        "invocationId": invocation_id
                    })),
                )
            }
        };
    }
    let plugin = sqlx::query("SELECT id, manifest FROM plugins WHERE status = 'ready' AND EXISTS (SELECT 1 FROM jsonb_array_elements(manifest->'tools') item WHERE item->>'name' = $1)")
        .bind(tool_name).fetch_optional(&state.pool).await.unwrap_or(None);
    if plugin.is_none() && tool_name.starts_with("mcp.") {
        let catalog_tool = find_mcp_catalog_tool(&state, company_id, tool_name)
            .await
            .ok()
            .flatten();
        let catalog_target = catalog_tool
            .clone()
            .map(|tool| {
                (
                    tool.connection_id,
                    tool.transport,
                    tool.transport_config,
                    tool.upstream_tool_name,
                )
            });
        let legacy_target = if catalog_target.is_none() {
            let raw = &tool_name[4..];
            if let Some((uid, upstream_name)) = raw.split_once(':') {
                sqlx::query("SELECT id, transport, transport_config FROM tool_connections WHERE company_id=$1 AND uid=$2 AND enabled=true")
                    .bind(company_id)
                    .bind(uid)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None)
                    .map(|connection| {
                        (
                            connection.get("id"),
                            connection.get("transport"),
                            connection.get("transport_config"),
                            upstream_name.to_string(),
                        )
                    })
            } else {
                None
            }
        } else {
            None
        };
        if let Some((connection_id, transport, config, upstream_name)) =
            catalog_target.or(legacy_target)
        {
                let parameters = body
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let decision = match catalog_tool.as_ref() {
                    Some(catalog) => {
                        gateway_decision_full_for_catalog_with_gateway(
                            &state,
                            company_id,
                            agent_id,
                            tool_name,
                            true,
                            catalog,
                            Some(&parameters),
                            gateway_id,
                            issue_id,
                            project_id,
                        )
                        .await
                    }
                    None => {
                        gateway_decision_full_for_gateway(
                            &state,
                            company_id,
                            agent_id,
                            tool_name,
                            true,
                            gateway_id,
                            issue_id,
                            project_id,
                        )
                        .await
                    }
                };
                if decision.decision == "rate_limited" {
                    return (
                        StatusCode::TOO_MANY_REQUESTS,
                        Json(serde_json::json!({
                            "error": "Tool access rate limit exceeded.",
                            "reasonCode": "rate_limited",
                            "rateLimitState": decision.rate_limit_state,
                        })),
                    );
                }
                let decision = decision.decision;
                let args_summary = serde_json::json!({"valueType":"object","keys":parameters.as_object().map(|value| value.len()).unwrap_or(0)});
                if decision == "deny" {
                    let invocation_id = match reserve_gateway_invocation(
                        &state,
                        company_id,
                        agent_id,
                        run_id,
                        Some(connection_id),
                        tool_name,
                        &parameters,
                        &args_summary,
                        "deny",
                        "denied",
                        idempotency_key.as_deref(),
                        Some("policy_denied"),
                        Some("Tool call denied by policy"),
                    )
                    .await
                    {
                        Ok(GatewayInvocationReservation::Created { invocation_id }) => invocation_id,
                        Ok(GatewayInvocationReservation::Replayed(response)) => return response,
                        Err(response) => return response,
                    };
                    return (
                        StatusCode::FORBIDDEN,
                        Json(
                            serde_json::json!({"error":"Tool call denied by policy","reasonCode":"policy_denied","decision":"deny","invocationId":invocation_id}),
                        ),
                    );
                }
                if decision == "require_approval" {
                    let issue_id: Option<Uuid> = session.get("issue_id");
                    let (invocation_id, action_id) = match reserve_gateway_approval(
                        &state,
                        company_id,
                        agent_id,
                        run_id,
                        issue_id,
                        Some(connection_id),
                        tool_name,
                        &parameters,
                        &args_summary,
                        &decision,
                        idempotency_key.as_deref(),
                    )
                    .await
                    {
                        Ok(GatewayApprovalReservation::Created {
                            invocation_id,
                            action_id,
                        }) => (invocation_id, action_id),
                        Ok(GatewayApprovalReservation::Replayed(response)) => return response,
                        Err(response) => return response,
                    };
                    return (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "decision": "require_approval",
                            "invocationId": invocation_id,
                            "actionRequestId": action_id,
                            "status": "pending"
                        })),
                    );
                }
                let invocation_id = match reserve_gateway_invocation(
                    &state,
                    company_id,
                    agent_id,
                    run_id,
                    Some(connection_id),
                    tool_name,
                    &parameters,
                    &args_summary,
                    &decision,
                    "executing",
                    idempotency_key.as_deref(),
                    None,
                    None,
                )
                .await
                {
                    Ok(GatewayInvocationReservation::Created { invocation_id }) => invocation_id,
                    Ok(GatewayInvocationReservation::Replayed(response)) => return response,
                    Err(response) => return response,
                };
                let result = if let Some(catalog) = catalog_tool.as_ref() {
                    execute_catalog_mcp_tool(
                        &state,
                        company_id,
                        catalog,
                        parameters,
                        Some(invocation_id),
                        Some(&headers),
                        session.get("id"),
                        agent_id,
                        run_id,
                        session.get("issue_id"),
                        session.get("project_id"),
                    )
                    .await
                } else {
                    execute_legacy_mcp_tool(
                        &state,
                        company_id,
                        connection_id,
                        &transport,
                        &config,
                        &upstream_name,
                        parameters,
                    )
                    .await
                };
                return match result {
                    Ok(value) => {
                        let _ = sqlx::query("UPDATE tool_invocations SET status='succeeded',result_summary=$2,completed_at=NOW(),updated_at=NOW() WHERE id=$1").bind(invocation_id).bind(serde_json::json!({"valueType":"json"})).execute(&state.pool).await;
                        let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,connection_id,tool_name,decision,outcome,invocation_id) VALUES ($1,'call_completed',CASE WHEN $3::uuid IS NULL THEN 'system' ELSE 'agent' END,COALESCE($2,$1::text),$3,$4,$5,$6,$7,'success',$8)").bind(company_id).bind(agent_id_text.clone()).bind(agent_id).bind(run_id).bind(connection_id).bind(tool_name).bind(&decision).bind(invocation_id).execute(&state.pool).await;
                        (
                            StatusCode::OK,
                            Json(
                                serde_json::json!({"decision":"allowed","invocationId":invocation_id,"result":value}),
                            ),
                        )
                    }
                    Err(error) => {
                        if let Some(interaction_id) = mcp_elicitation_interaction_id(&error) {
                            return (
                                StatusCode::CONFLICT,
                                Json(serde_json::json!({
                                    "error": "MCP tool requested additional input",
                                    "reasonCode": "elicitation_required",
                                    "invocationId": invocation_id,
                                    "interactionId": interaction_id,
                                })),
                            );
                        }
                        let _ = sqlx::query("UPDATE tool_invocations SET status='failed',error_message=$2,completed_at=NOW(),updated_at=NOW() WHERE id=$1").bind(invocation_id).bind(&error).execute(&state.pool).await;
                        let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,connection_id,tool_name,decision,outcome,invocation_id,reason_code,error_message) VALUES ($1,'call_failed',CASE WHEN $3::uuid IS NULL THEN 'system' ELSE 'agent' END,COALESCE($2,$1::text),$3,$4,$5,$6,$7,'failure',$8,'mcp_tool_execution_failed',$9)").bind(company_id).bind(agent_id_text.clone()).bind(agent_id).bind(run_id).bind(connection_id).bind(tool_name).bind(&decision).bind(invocation_id).bind(&error).execute(&state.pool).await;
                        (
                            StatusCode::BAD_GATEWAY,
                            Json(
                                serde_json::json!({"error":error,"reasonCode":"mcp_tool_execution_failed","invocationId":invocation_id}),
                            ),
                        )
                    }
                };
            }
    }
    let Some(plugin) = plugin else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Tool not found", "reasonCode": "tool_not_found"})),
        );
    };
    if agent_id.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Plugin tools require an agent-bound gateway session",
                "reasonCode": "agent_context_required"
            })),
        );
    }
    let plugin_id: Uuid = plugin.get("id");
    let parameters = body
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let decision = gateway_decision_full_for_gateway(
        &state,
        company_id,
        agent_id,
        tool_name,
        true,
        gateway_id,
        issue_id,
        project_id,
    )
    .await;
    if decision.decision == "rate_limited" {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "Tool access rate limit exceeded.",
                "reasonCode": "rate_limited",
                "rateLimitState": decision.rate_limit_state,
            })),
        );
    }
    let decision = decision.decision;
    let args_summary = serde_json::json!({"valueType":"object","keys":parameters.as_object().map(|value| value.len()).unwrap_or(0)});
    if decision == "deny" {
        let invocation_id = match reserve_gateway_invocation(
            &state,
            company_id,
            agent_id,
            run_id,
            None,
            tool_name,
            &parameters,
            &args_summary,
            "deny",
            "denied",
            idempotency_key.as_deref(),
            Some("policy_denied"),
            Some("Tool call denied by policy"),
        )
        .await
        {
            Ok(GatewayInvocationReservation::Created { invocation_id }) => invocation_id,
            Ok(GatewayInvocationReservation::Replayed(response)) => return response,
            Err(response) => return response,
        };
        let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,invocation_id,reason_code) VALUES ($1,'call_denied','agent',$2,$3,$4,$5,'deny','denied',$6,'policy_denied')")
        .bind(company_id).bind(agent_id_text.clone()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).execute(&state.pool).await;
        return (
            StatusCode::FORBIDDEN,
            Json(
                serde_json::json!({"error":"Tool call denied by policy","reasonCode":"policy_denied","decision":"deny","invocationId":invocation_id}),
            ),
        );
    }
    if decision == "require_approval" {
        let issue_id: Option<Uuid> = session.get("issue_id");
        let (invocation_id, action_id) = match reserve_gateway_approval(
            &state,
            company_id,
            agent_id,
            run_id,
            issue_id,
            None,
            tool_name,
            &parameters,
            &args_summary,
            &decision,
            idempotency_key.as_deref(),
        )
        .await
        {
            Ok(GatewayApprovalReservation::Created {
                invocation_id,
                action_id,
            }) => (invocation_id, action_id),
            Ok(GatewayApprovalReservation::Replayed(response)) => return response,
            Err(response) => return response,
        };
        let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,invocation_id,action_request_id,reason_code) VALUES ($1,'approval_requested','agent',$2,$3,$4,$5,'require_approval','pending',$6,$7,'policy_requires_approval')")
            .bind(company_id).bind(agent_id_text.clone()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).bind(action_id).execute(&state.pool).await;
        return (
            StatusCode::OK,
            Json(
                serde_json::json!({"decision":"require_approval","invocationId":invocation_id,"actionRequestId":action_id,"status":"pending"}),
            ),
        );
    }
    let invocation_id = match reserve_gateway_invocation(
        &state,
        company_id,
        agent_id,
        run_id,
        None,
        tool_name,
        &parameters,
        &args_summary,
        &decision,
        "executing",
        idempotency_key.as_deref(),
        None,
        None,
    )
    .await
    {
        Ok(GatewayInvocationReservation::Created { invocation_id }) => invocation_id,
        Ok(GatewayInvocationReservation::Replayed(response)) => return response,
        Err(response) => return response,
    };
    let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,arguments_summary,invocation_id) VALUES ($1,'call_started','agent',$2,$3,$4,$5,'allow','pending',$6,$7)")
        .bind(company_id).bind(agent_id_text.clone()).bind(agent_id).bind(run_id).bind(tool_name).bind(&args_summary).bind(invocation_id).execute(&state.pool).await;
    let result = state
        .plugin_service
        .dispatch_tool(plugin_id, tool_name, parameters)
        .await;
    match result {
        Ok(value) => {
            let _ = sqlx::query("UPDATE tool_invocations SET status='succeeded', result_summary=$2, completed_at=NOW(), updated_at=NOW() WHERE id=$1")
                .bind(invocation_id).bind(serde_json::json!({"valueType":"json"})).execute(&state.pool).await;
            let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,invocation_id,result_summary) VALUES ($1,'call_completed','agent',$2,$3,$4,$5,'allow','success',$6,$7)")
                .bind(company_id).bind(agent_id_text.clone()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).bind(serde_json::json!({"valueType":"json"})).execute(&state.pool).await;
            (
                StatusCode::OK,
                Json(
                    serde_json::json!({"decision":"allowed","invocationId":invocation_id,"result":value}),
                ),
            )
        }
        Err(error) => {
            let message = error.to_string();
            let _ = sqlx::query("UPDATE tool_invocations SET status='failed', error_message=$2, completed_at=NOW(), updated_at=NOW() WHERE id=$1")
                .bind(invocation_id).bind(&message).execute(&state.pool).await;
            let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,invocation_id,error_message) VALUES ($1,'call_failed','agent',$2,$3,$4,$5,'allow','failure',$6,$7)")
                .bind(company_id).bind(agent_id_text.clone()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).bind(&message).execute(&state.pool).await;
            (
                StatusCode::BAD_GATEWAY,
                Json(
                    serde_json::json!({"error":message,"reasonCode":"tool_execution_failed","invocationId":invocation_id}),
                ),
            )
        }
    }
}

async fn approve_gateway_action(
    Path(action_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let company_id = body
        .get("companyId")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let Some(company_id) = company_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"companyId is required"})),
        );
    };
    if crate::routes::assert_board(&actor).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error":"Board authentication required",
                "reasonCode":"authentication_required"
            })),
        );
    }
    if crate::routes::assert_company_access(&actor, company_id, false).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error":"Company access denied",
                "reasonCode":"company_access_denied"
            })),
        );
    }
    let candidate = match sqlx::query(
        "SELECT ar.status, i.tool_name
           FROM tool_action_requests ar
           JOIN tool_invocations i ON i.id = ar.invocation_id
          WHERE ar.id = $1 AND ar.company_id = $2",
    )
    .bind(action_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error":"Action request not found"})),
            )
        }
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":error.to_string()})),
            )
        }
    };
    if candidate.get::<String, _>("status") != "pending" {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error":"Action request is not pending"})),
        );
    }
    let tool_name: String = candidate.get("tool_name");
    let plugin = sqlx::query("SELECT id FROM plugins WHERE status='ready' AND EXISTS (SELECT 1 FROM jsonb_array_elements(manifest->'tools') item WHERE item->>'name'=$1)")
        .bind(&tool_name).fetch_optional(&state.pool).await.unwrap_or(None);
    if plugin.is_none() && !tool_name.starts_with("mcp.") {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"Tool not found"})),
        );
    }

    // Claim the action and mark its invocation in one transaction. The
    // pending predicate is the concurrency boundary: only one approval
    // request can transition a row into executing and dispatch the tool.
    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":error.to_string()})),
            )
        }
    };
    let row = match sqlx::query(
        "UPDATE tool_action_requests AS ar
            SET status = 'executing', decided_at = NOW(), updated_at = NOW()
           FROM tool_invocations AS i
          WHERE ar.id = $1
            AND ar.company_id = $2
            AND ar.status = 'pending'
            AND i.id = ar.invocation_id
            AND i.company_id = ar.company_id
       RETURNING ar.invocation_id, ar.signed_arguments, i.tool_name, i.agent_id, i.run_id, i.issue_id",
    )
    .bind(action_id)
    .bind(company_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            let _ = tx.rollback().await;
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({"error":"Action request has already been claimed"})),
            )
        }
        Err(error) => {
            let _ = tx.rollback().await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":error.to_string()})),
            )
        }
    };
    let invocation_id: Uuid = row.get("invocation_id");
    let tool_name: String = row.get("tool_name");
    let agent_id: Option<Uuid> = row.get("agent_id");
    let run_id: Option<Uuid> = row.get("run_id");
    let issue_id: Option<Uuid> = row.get("issue_id");
    let parameters = row
        .get::<Option<String>, _>("signed_arguments")
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Err(error) = sqlx::query("UPDATE tool_invocations SET status='executing', policy_decision='allow', started_at=COALESCE(started_at,NOW()), updated_at=NOW() WHERE id=$1 AND company_id=$2")
        .bind(invocation_id)
        .bind(company_id)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":error.to_string()})),
        );
    }
    if let Err(error) = tx.commit().await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":error.to_string()})),
        );
    }
    let result = if let Some(plugin) = plugin {
        let plugin_id: Uuid = plugin.get("id");
        state
            .plugin_service
            .dispatch_tool(plugin_id, &tool_name, parameters)
            .await
            .map_err(|error| error.to_string())
    } else if tool_name.starts_with("mcp.") {
        execute_mcp_connection(
            &state,
            company_id,
            &tool_name,
            parameters,
            Some(invocation_id),
            None,
            agent_id,
            run_id,
            issue_id,
            None,
        )
        .await
    } else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"Tool not found"})),
        );
    };
    match result {
        Ok(value) => {
            let _ = sqlx::query("UPDATE tool_action_requests SET status='executed', resolved_at=NOW(), updated_at=NOW() WHERE id=$1").bind(action_id).execute(&state.pool).await;
            let _ = sqlx::query("UPDATE tool_invocations SET status='succeeded', result_summary=$2, completed_at=NOW(), updated_at=NOW() WHERE id=$1").bind(invocation_id).bind(serde_json::json!({"valueType":"json"})).execute(&state.pool).await;
            let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,invocation_id,action_request_id,reason_code) VALUES ($1,'call_completed',$2,$3,$4,$5,$6,'allow','success',$7,$8,'approved_action_executed')")
                .bind(company_id)
                .bind(actor.actor_type())
                .bind(actor.principal_id().map(|value| value.to_string()))
                .bind(agent_id)
                .bind(run_id)
                .bind(&tool_name)
                .bind(invocation_id)
                .bind(action_id)
                .execute(&state.pool)
                .await;
            (
                StatusCode::OK,
                Json(
                    serde_json::json!({"decision":"allowed","invocationId":invocation_id,"actionRequestId":action_id,"result":value}),
                ),
            )
        }
        Err(error) => {
            let message = error.to_string();
            let _ = sqlx::query("UPDATE tool_action_requests SET status='failed', resolved_at=NOW(), updated_at=NOW() WHERE id=$1").bind(action_id).execute(&state.pool).await;
            let _ = sqlx::query("UPDATE tool_invocations SET status='failed', error_message=$2, completed_at=NOW(), updated_at=NOW() WHERE id=$1").bind(invocation_id).bind(&message).execute(&state.pool).await;
            (
                StatusCode::BAD_GATEWAY,
                Json(
                    serde_json::json!({"error":message,"reasonCode":"approved_tool_execution_failed","actionRequestId":action_id}),
                ),
            )
        }
    }
}

async fn decline_gateway_action(
    Path(action_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let company_id = body
        .get("companyId")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let Some(company_id) = company_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"companyId is required"})),
        );
    };
    if crate::routes::assert_board(&actor).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error":"Board authentication required",
                "reasonCode":"authentication_required"
            })),
        );
    }
    if crate::routes::assert_company_access(&actor, company_id, false).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error":"Company access denied",
                "reasonCode":"company_access_denied"
            })),
        );
    }
    let updated = sqlx::query("UPDATE tool_action_requests SET status='declined', resolved_at=NOW(), updated_at=NOW() WHERE id=$1 AND company_id=$2 AND status='pending' RETURNING id, invocation_id")
        .bind(action_id).bind(company_id).fetch_optional(&state.pool).await.unwrap_or(None);
    let Some(row) = updated else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"Pending action request not found"})),
        );
    };
    let invocation_id: Uuid = row.get("invocation_id");
    let _ = sqlx::query("UPDATE tool_invocations SET status='denied', error_code='approval_declined', completed_at=NOW(), updated_at=NOW() WHERE id=$1").bind(invocation_id).execute(&state.pool).await;
    (
        StatusCode::OK,
        Json(serde_json::json!({"id":action_id,"invocationId":invocation_id,"status":"declined"})),
    )
}

async fn require_named_gateway_admin(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> Result<(), (StatusCode, Json<Value>)> {
    if crate::routes::assert_board(actor).is_err() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Board authentication required",
                "reasonCode": "authentication_required"
            })),
        ));
    }
    if crate::routes::assert_company_access(actor, company_id, true).is_err() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Company access denied",
                "reasonCode": "company_access_denied"
            })),
        ));
    }
    let decision = AuthorizationService::decide(
        &state.pool,
        actor,
        &AuthorizationAction::Permission {
            key: PermissionKey::from_const(PermissionKey::TOOLS_ADMIN),
        },
        Some(company_id),
    )
    .await;
    if !decision.allowed {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Missing permission: tools:admin",
                "reasonCode": "permission_denied"
            })),
        ));
    }
    Ok(())
}

fn add_named_gateway_client_snippets(mut gateway: Value) -> Value {
    let name = gateway
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("gateway")
        .to_owned();
    let public_id = gateway
        .get("gatewayPublicId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if let Some(object) = gateway.as_object_mut() {
        object.insert(
            "endpointPath".to_string(),
            Value::String(named_gateway_endpoint_path(&public_id)),
        );
        object.insert(
            "clientSnippets".to_string(),
            named_gateway_client_snippets(&name, &public_id),
        );
    }
    gateway
}

async fn list_named_gateways(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> impl IntoResponse {
    if let Err(response) = require_named_gateway_admin(&state, &actor, company_id).await {
        return response;
    }
    let gateways = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(
          'id',g.id,'companyId',g.company_id,'gatewayPublicId',g.gateway_public_id,
          'name',g.name,'slug',g.slug,'displaySlug',g.display_slug,
          'description',g.description,'status',g.status,
          'profileId',g.profile_id,'defaultProfileMode',g.default_profile_mode,
          'contextScopeType',g.context_scope_type,'contextScopeId',g.context_scope_id,
          'agentId',g.agent_id,'projectId',g.project_id,'issueId',g.issue_id,
          'approvalIssueId',g.approval_issue_id,'authConfig',g.auth_config,
          'headerPolicy',g.header_policy,'metadataPolicy',g.metadata_policy,
          'onDemandToolsConfig',g.on_demand_tools_config,
          'metadata',g.metadata,'createdByAgentId',g.created_by_agent_id,
          'createdByUserId',g.created_by_user_id,'archivedAt',g.archived_at,
          'createdAt',g.created_at,'updatedAt',g.updated_at,
          'tokens',COALESCE((SELECT jsonb_agg(jsonb_build_object(
              'id',t.id,'gatewayId',t.gateway_id,'name',t.name,
              'tokenPrefix',t.token_prefix,'subjectType',t.subject_type,
              'subjectId',t.subject_id,'clientLabel',t.client_label,
              'ownerNote',t.owner_note,'allowedActions',t.allowed_actions,
              'expiresAt',t.expires_at,'expiryOverrideReason',t.expiry_override_reason,
              'expiryOverrideByUserId',t.expiry_override_by_user_id,
              'expiryOverrideByAgentId',t.expiry_override_by_agent_id,
              'expiryOverrideAt',t.expiry_override_at,'lastUsedAt',t.last_used_at,
              'revokedAt',t.revoked_at,'createdByAgentId',t.created_by_agent_id,
              'createdByUserId',t.created_by_user_id,'createdAt',t.created_at,
              'updatedAt',t.updated_at) ORDER BY t.created_at DESC)
              FROM tool_mcp_gateway_tokens t
             WHERE t.gateway_id=g.id AND t.company_id=g.company_id),'[]'::jsonb)
        ) ORDER BY g.name),'[]'::jsonb)
           FROM tool_mcp_gateways g
          WHERE g.company_id=$1 AND g.status <> 'archived'",
    )
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(Value::Array(vec![]));
    let gateways = match gateways {
        Value::Array(rows) => Value::Array(
            rows.into_iter()
                .map(add_named_gateway_client_snippets)
                .collect(),
        ),
        other => other,
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({"gateways": gateways})),
    )
}

async fn create_named_gateway(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(response) = require_named_gateway_admin(&state, &actor, company_id).await {
        return response;
    }
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let Some(name) = name else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"name is required"})),
        );
    };
    let slug = body
        .get("slug")
        .or_else(|| body.get("displaySlug"))
        .and_then(Value::as_str)
        .unwrap_or(name)
        .trim()
        .to_lowercase()
        .replace(' ', "-");
    if slug.is_empty()
        || slug.len() > 120
        || !slug.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
        })
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"slug must contain only letters, numbers, '-', '_' or '.'"})),
        );
    }
    let profile_id = body
        .get("profileId")
        .and_then(Value::as_str)
        .map(Uuid::parse_str)
        .transpose();
    let Ok(profile_id) = profile_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"profileId must be a valid UUID"})),
        );
    };
    if let Some(profile_id) = profile_id {
        let profile_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM tool_profiles WHERE id = $1 AND company_id = $2)",
        )
        .bind(profile_id)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);
        if !profile_exists {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"profileId does not belong to the company"})),
            );
        }
    }
    let parse_context_id = |key: &str| {
        body.get(key)
            .and_then(Value::as_str)
            .map(Uuid::parse_str)
            .transpose()
    };
    let agent_id = match parse_context_id("agentId") {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"gateway context ids must be valid UUIDs"})),
            )
        }
    };
    let project_id = match parse_context_id("projectId") {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"gateway context ids must be valid UUIDs"})),
            )
        }
    };
    let issue_id = match parse_context_id("issueId") {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"gateway context ids must be valid UUIDs"})),
            )
        }
    };
    let approval_issue_id = match parse_context_id("approvalIssueId") {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"gateway context ids must be valid UUIDs"})),
            )
        }
    };
    let default_profile_mode = body
        .get("defaultProfileMode")
        .and_then(Value::as_str)
        .unwrap_or("gateway_only");
    if !matches!(
        default_profile_mode,
        "gateway_only" | "inherit_context_then_gateway" | "gateway_then_context"
    ) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"defaultProfileMode is invalid"})),
        );
    }
    let context_scope_type = body
        .get("contextScopeType")
        .and_then(Value::as_str)
        .unwrap_or("none");
    if !matches!(context_scope_type, "none" | "company" | "project" | "routine" | "issue" | "agent") {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"contextScopeType is invalid"})),
        );
    }
    let metadata = body.get("metadata").cloned().unwrap_or_else(|| json!({}));
    if !metadata.is_object() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"metadata must be an object"})),
        );
    }
    let auth_config = body.get("authConfig").cloned().unwrap_or_else(|| json!({
        "version": 1,
        "bearer": {
            "enabled": true,
            "tokenPrefix": "pcgw",
            "defaultTtlSeconds": 7_776_000,
            "requireFiniteExpiry": true,
            "longLivedTokenRequiresOverride": true,
        },
        "oauth": {"enabled": false, "reservedFor": "v1_5", "dynamicClientRegistration": false, "authorizationCodePkce": false}
    }));
    let header_policy = body.get("headerPolicy").cloned().unwrap_or_else(|| json!({
        "version": 1,
        "callerPassthrough": {"enabled": false, "allowedHeaders": []},
        "staticHeaders": [],
        "generatedMetadata": {"enabled": false, "allowedHeaders": []},
        "responseHeaders": {"forwardMcpRequiredHeaders": true, "forwardSafeCacheHeaders": true}
    }));
    let metadata_policy = body.get("metadataPolicy").cloned().unwrap_or_else(|| json!({
        "version": 1,
        "forwardCompanyId": false,
        "forwardGatewayId": false,
        "forwardProjectId": false,
        "forwardIssueId": false,
        "forwardAgentId": false,
        "forwardRunId": false,
        "forwardCorrelationId": true
    }));
    let on_demand_tools_config = body.get("onDemandToolsConfig").cloned().unwrap_or_else(|| json!({
        "enabled": false, "searchToolName": "search_tools", "runToolName": "run_tool"
    }));
    let created_by_user_id = actor.principal_id().map(|value| value.to_string());
    let row = sqlx::query("INSERT INTO tool_mcp_gateways (company_id,name,slug,display_slug,description,profile_id,default_profile_mode,context_scope_type,context_scope_id,agent_id,project_id,issue_id,approval_issue_id,auth_config,header_policy,metadata_policy,on_demand_tools_config,metadata,created_by_user_id) VALUES ($1,$2,$3,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) RETURNING id,gateway_public_id,created_at,updated_at")
        .bind(company_id).bind(name).bind(&slug).bind(body.get("description").and_then(Value::as_str))
        .bind(profile_id).bind(default_profile_mode).bind(context_scope_type)
        .bind(body.get("contextScopeId").and_then(Value::as_str))
        .bind(agent_id).bind(project_id).bind(issue_id).bind(approval_issue_id)
        .bind(&auth_config).bind(&header_policy).bind(&metadata_policy).bind(&on_demand_tools_config)
        .bind(metadata.clone()).bind(created_by_user_id.clone()).fetch_one(&state.pool).await;
    match row {
        Ok(row) => {
            if let Some(profile_id) = profile_id {
                if let Err(error) = sqlx::query(
                    "INSERT INTO tool_profile_bindings
                        (company_id, profile_id, target_type, target_id)
                     VALUES ($1, $2, 'gateway', $3::text)
                     ON CONFLICT (company_id, target_type, target_id, profile_id)
                     DO NOTHING",
                )
                .bind(company_id)
                .bind(profile_id)
                .bind(row.get::<Uuid, _>("id"))
                .execute(&state.pool)
                .await
                {
                    tracing::error!(%error, "failed to bind profile to named MCP gateway");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error":"gateway profile binding failed"})),
                    );
                }
            }
            let gateway_public_id = row.get::<String, _>("gateway_public_id");
            let endpoint_path = named_gateway_endpoint_path(&gateway_public_id);
            let client_snippets = named_gateway_client_snippets(name, &gateway_public_id);
            (
                StatusCode::CREATED,
                Json(
                    serde_json::json!({"id":row.get::<Uuid,_>("id"),"companyId":company_id,"gatewayPublicId":gateway_public_id,"endpointPath":endpoint_path,"name":name,"slug":slug,"displaySlug":slug,"description":body.get("description"),"status":"active","profileId":profile_id,"defaultProfileMode":default_profile_mode,"contextScopeType":context_scope_type,"contextScopeId":body.get("contextScopeId"),"agentId":agent_id,"projectId":project_id,"issueId":issue_id,"approvalIssueId":approval_issue_id,"authConfig":auth_config,"headerPolicy":header_policy,"metadataPolicy":metadata_policy,"onDemandToolsConfig":on_demand_tools_config,"metadata":metadata,"createdByUserId":created_by_user_id,"tokens":[],"clientSnippets":client_snippets,"createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")}),
                ),
            )
        }
        Err(error) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error":error.to_string()})),
        ),
    }
}

async fn update_named_gateway(
    Path(gateway_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let company_id = body
        .get("companyId")
        .and_then(Value::as_str)
        .and_then(|v| Uuid::parse_str(v).ok());
    let Some(company_id) = company_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"companyId is required"})),
        );
    };
    if let Err(response) = require_named_gateway_admin(&state, &actor, company_id).await {
        return response;
    }
    let previous_profile_id = match sqlx::query(
        "SELECT profile_id FROM tool_mcp_gateways WHERE id = $1 AND company_id = $2",
    )
    .bind(gateway_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(row) => row.map(|row| row.get::<Option<Uuid>, _>("profile_id")),
        Err(error) => {
            tracing::error!(%error, %gateway_id, "Failed to load named gateway before update");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":"Failed to load gateway"})),
            );
        }
    };
    let profile_id = body
        .get("profileId")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    if let Some(profile_id) = profile_id {
        let profile_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM tool_profiles WHERE id = $1 AND company_id = $2)",
        )
        .bind(profile_id)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);
        if !profile_exists {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"profileId does not belong to the company"})),
            );
        }
    }
    let next_slug = body
        .get("slug")
        .or_else(|| body.get("displaySlug"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .map(|value| value.replace(' ', "-"));
    if next_slug.as_deref().is_some_and(|slug| {
        slug.len() > 120
            || !slug
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    }) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"slug must contain only letters, numbers, '-', '_' or '.'"})),
        );
    }
    let status = body.get("status").and_then(Value::as_str);
    if status.is_some_and(|status| !matches!(status, "draft" | "active" | "disabled" | "archived")) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"status is invalid"})),
        );
    }
    let default_profile_mode = body.get("defaultProfileMode").and_then(Value::as_str);
    if default_profile_mode.is_some_and(|mode| {
        !matches!(mode, "gateway_only" | "inherit_context_then_gateway" | "gateway_then_context")
    }) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"defaultProfileMode is invalid"})),
        );
    }
    let context_scope_type = body.get("contextScopeType").and_then(Value::as_str);
    if context_scope_type.is_some_and(|scope| {
        !matches!(scope, "none" | "company" | "project" | "routine" | "issue" | "agent")
    }) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"contextScopeType is invalid"})),
        );
    }
    let metadata = body.get("metadata");
    if metadata.is_some_and(|value| !value.is_object()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"metadata must be an object"})),
        );
    }
    let row = sqlx::query("UPDATE tool_mcp_gateways SET name=COALESCE($3,name), slug=COALESCE($4,slug), display_slug=COALESCE($4,display_slug), description=COALESCE($5,description), status=COALESCE($6,status), profile_id=COALESCE($7,profile_id), default_profile_mode=COALESCE($8,default_profile_mode), context_scope_type=COALESCE($9,context_scope_type), context_scope_id=COALESCE($10,context_scope_id), agent_id=COALESCE($11,agent_id), project_id=COALESCE($12,project_id), issue_id=COALESCE($13,issue_id), approval_issue_id=COALESCE($14,approval_issue_id), auth_config=COALESCE($15,auth_config), header_policy=COALESCE($16,header_policy), metadata_policy=COALESCE($17,metadata_policy), on_demand_tools_config=COALESCE($18,on_demand_tools_config), metadata=COALESCE($19,metadata), updated_at=NOW() WHERE id=$1 AND company_id=$2 RETURNING id,gateway_public_id,name,slug,display_slug,description,status,profile_id,default_profile_mode,context_scope_type,context_scope_id,agent_id,project_id,issue_id,approval_issue_id,auth_config,header_policy,metadata_policy,on_demand_tools_config,metadata,created_by_agent_id,created_by_user_id,archived_at,created_at,updated_at")
        .bind(gateway_id)
        .bind(company_id)
        .bind(body.get("name").and_then(Value::as_str))
        .bind(next_slug.as_deref())
        .bind(body.get("description").and_then(Value::as_str))
        .bind(status)
        .bind(profile_id)
        .bind(default_profile_mode)
        .bind(context_scope_type)
        .bind(body.get("contextScopeId").and_then(Value::as_str))
        .bind(body.get("agentId").and_then(Value::as_str).and_then(|value| Uuid::parse_str(value).ok()))
        .bind(body.get("projectId").and_then(Value::as_str).and_then(|value| Uuid::parse_str(value).ok()))
        .bind(body.get("issueId").and_then(Value::as_str).and_then(|value| Uuid::parse_str(value).ok()))
        .bind(body.get("approvalIssueId").and_then(Value::as_str).and_then(|value| Uuid::parse_str(value).ok()))
        .bind(body.get("authConfig"))
        .bind(body.get("headerPolicy"))
        .bind(body.get("metadataPolicy"))
        .bind(body.get("onDemandToolsConfig"))
        .bind(metadata)
        .fetch_optional(&state.pool).await;
    match row {
        Ok(Some(row)) => {
            let next_profile_id = row.get::<Option<Uuid>, _>("profile_id");
            if previous_profile_id != Some(next_profile_id) {
                if let Some(previous_profile_id) = previous_profile_id.flatten() {
                    if let Err(error) = sqlx::query(
                        "DELETE FROM tool_profile_bindings
                          WHERE company_id = $1 AND target_type = 'gateway'
                            AND target_id = $2::text AND profile_id = $3",
                    )
                    .bind(company_id)
                    .bind(gateway_id)
                    .bind(previous_profile_id)
                    .execute(&state.pool)
                    .await
                    {
                        tracing::error!(%error, %gateway_id, "Failed to remove stale gateway profile binding");
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error":"Gateway profile binding update failed"})),
                        );
                    }
                }
                if let Some(next_profile_id) = next_profile_id {
                    if let Err(error) = sqlx::query(
                        "INSERT INTO tool_profile_bindings
                            (company_id, profile_id, target_type, target_id, priority)
                         VALUES ($1, $2, 'gateway', $3::text, 100)
                         ON CONFLICT (company_id, target_type, target_id, profile_id)
                         DO UPDATE SET updated_at = NOW()",
                    )
                    .bind(company_id)
                    .bind(next_profile_id)
                    .bind(gateway_id)
                    .execute(&state.pool)
                    .await
                    {
                        tracing::error!(%error, %gateway_id, "Failed to create gateway profile binding");
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error":"Gateway profile binding update failed"})),
                        );
                    }
                }
            }
            let gateway_public_id = row.get::<String, _>("gateway_public_id");
            let gateway_name = row.get::<String, _>("name");
            let endpoint_path = named_gateway_endpoint_path(&gateway_public_id);
            let client_snippets =
                named_gateway_client_snippets(&gateway_name, &gateway_public_id);
            (
                StatusCode::OK,
                Json(
                    serde_json::json!({"id":row.get::<Uuid,_>("id"),"companyId":company_id,"gatewayPublicId":gateway_public_id,"endpointPath":endpoint_path,"name":gateway_name,"slug":row.get::<String,_>("slug"),"displaySlug":row.get::<String,_>("display_slug"),"description":row.get::<Option<String>,_>("description"),"status":row.get::<String,_>("status"),"profileId":row.get::<Option<Uuid>,_>("profile_id"),"defaultProfileMode":row.get::<String,_>("default_profile_mode"),"contextScopeType":row.get::<String,_>("context_scope_type"),"contextScopeId":row.get::<Option<String>,_>("context_scope_id"),"agentId":row.get::<Option<Uuid>,_>("agent_id"),"projectId":row.get::<Option<Uuid>,_>("project_id"),"issueId":row.get::<Option<Uuid>,_>("issue_id"),"approvalIssueId":row.get::<Option<Uuid>,_>("approval_issue_id"),"authConfig":row.get::<Value,_>("auth_config"),"headerPolicy":row.get::<Value,_>("header_policy"),"metadataPolicy":row.get::<Value,_>("metadata_policy"),"onDemandToolsConfig":row.get::<Value,_>("on_demand_tools_config"),"metadata":row.get::<Value,_>("metadata"),"createdByAgentId":row.get::<Option<Uuid>,_>("created_by_agent_id"),"createdByUserId":row.get::<Option<String>,_>("created_by_user_id"),"archivedAt":row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("archived_at"),"tokens":[],"clientSnippets":client_snippets,"createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")}),
                ),
            )
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"Gateway not found"})),
        ),
        Err(error) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error":error.to_string()})),
        ),
    }
}

async fn create_named_gateway_token(
    Path(gateway_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let company_id = body
        .get("companyId")
        .and_then(Value::as_str)
        .and_then(|v| Uuid::parse_str(v).ok());
    let Some(company_id) = company_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"companyId is required"})),
        );
    };
    if let Err(response) = require_named_gateway_admin(&state, &actor, company_id).await {
        return response;
    }
    if let Some(response) = mcp_token_request_rate_limit_response(
        &state,
        company_id,
        &format!("gateway:{gateway_id}"),
    )
    .await
    {
        return response;
    }
    let token_id = Uuid::new_v4();
    let token_secret = random_named_gateway_secret();
    let token = format!("pcgw_{}.{}", token_id, token_secret);
    let token_prefix = format!("pcgw_{}", &token_id.simple().to_string()[..8]);
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Gateway token");
    if name.len() > 160 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"name must be at most 160 characters"})),
        );
    }
    let client_label = body
        .get("clientLabel")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(name);
    let owner_note = body
        .get("ownerNote")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Created through the Parrot MCP gateway API.");
    if client_label.len() > 160 || owner_note.len() > 1000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"clientLabel or ownerNote is too long"})),
        );
    }
    let subject_type = body
        .get("subjectType")
        .and_then(Value::as_str)
        .unwrap_or("gateway_client");
    if subject_type != "gateway_client" {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error":"public named gateway tokens only support gateway_client subjects"})),
        );
    }
    let subject_id = body.get("subjectId").and_then(Value::as_str).map(str::trim);
    if subject_id.is_some_and(|value| value.is_empty() || value.len() > 240) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"subjectId must contain 1 to 240 characters"})),
        );
    }
    let allowed_actions = match body.get("allowedActions") {
        None => json!(["tools/list", "tools/call"]),
        Some(value) => {
            let Some(actions) = value.as_array() else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error":"allowedActions must be an array"})),
                );
            };
            if actions.is_empty()
                || actions.len() > 2
                || actions.iter().any(|action| {
                    !matches!(action.as_str(), Some("tools/list") | Some("tools/call"))
                })
            {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error":"allowedActions must contain tools/list and/or tools/call"})),
                );
            }
            let mut unique = Vec::new();
            for action in actions.iter().filter_map(Value::as_str) {
                if !unique.iter().any(|existing| *existing == action) {
                    unique.push(action);
                }
            }
            if unique.len() != actions.len() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error":"allowedActions must not contain duplicates"})),
                );
            }
            Value::Array(unique.into_iter().map(|value| json!(value)).collect())
        }
    };
    let expires_at = match body.get("expiresAt") {
        None => None,
        Some(Value::Null) => None,
        Some(Value::String(value)) => match chrono::DateTime::parse_from_rfc3339(value) {
            Ok(value) => Some(value.with_timezone(&chrono::Utc)),
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error":"expiresAt must be RFC3339"})),
                )
            }
        },
        Some(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"expiresAt must be RFC3339 or null"})),
            )
        }
    };
    let expiry_override_reason = body
        .get("expiryOverrideReason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if body.get("expiresAt").is_some_and(Value::is_null) && expiry_override_reason.is_none() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error":"non-expiring gateway tokens require expiryOverrideReason"})),
        );
    }
    if expiry_override_reason.is_some_and(|value| value.len() > 1000) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"expiryOverrideReason is too long"})),
        );
    }
    let actor_user_id = actor.principal_id().map(|value| value.to_string());
    let row = sqlx::query("INSERT INTO tool_mcp_gateway_tokens (id,company_id,gateway_id,name,token_hash,token_prefix,subject_type,subject_id,client_label,owner_note,allowed_actions,expires_at,expiry_override_reason,expiry_override_by_user_id,expiry_override_at,created_by_user_id) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,CASE WHEN $13 IS NULL THEN NULL ELSE NOW() END,$14 WHERE EXISTS (SELECT 1 FROM tool_mcp_gateways WHERE id=$3 AND company_id=$2) RETURNING id,gateway_id,token_prefix,subject_type,subject_id,client_label,owner_note,created_at,updated_at,expires_at,expiry_override_reason,expiry_override_by_user_id,expiry_override_at,allowed_actions")
        .bind(token_id).bind(company_id).bind(gateway_id).bind(name).bind(hash_gateway_token(&token)).bind(&token_prefix)
        .bind(subject_type).bind(subject_id).bind(client_label).bind(owner_note).bind(&allowed_actions).bind(expires_at)
        .bind(expiry_override_reason).bind(actor_user_id.clone()).fetch_optional(&state.pool).await;
    match row {
        Ok(Some(row)) => (
            StatusCode::CREATED,
            Json(
                serde_json::json!({"id":row.get::<Uuid,_>("id"),"gatewayId":row.get::<Uuid,_>("gateway_id"),"companyId":company_id,"name":name,"token":token,"tokenPrefix":row.get::<String,_>("token_prefix"),"subjectType":row.get::<String,_>("subject_type"),"subjectId":row.get::<Option<String>,_>("subject_id"),"clientLabel":row.get::<String,_>("client_label"),"ownerNote":row.get::<String,_>("owner_note"),"allowedActions":row.get::<Value,_>("allowed_actions"),"expiresAt":row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("expires_at"),"expiryOverrideReason":row.get::<Option<String>,_>("expiry_override_reason"),"expiryOverrideByUserId":row.get::<Option<String>,_>("expiry_override_by_user_id"),"expiryOverrideAt":row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("expiry_override_at"),"createdByUserId":actor_user_id,"createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")}),
            ),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"Gateway not found"})),
        ),
        Err(error) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error":error.to_string()})),
        ),
    }
}

async fn revoke_named_gateway_token(
    Path(token_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let company_id = body
        .get("companyId")
        .and_then(Value::as_str)
            .and_then(|v| Uuid::parse_str(v).ok());
    let Some(company_id) = company_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"companyId is required"})),
        );
    };
    if let Err(response) = require_named_gateway_admin(&state, &actor, company_id).await {
        return response;
    }
    let updated = sqlx::query("UPDATE tool_mcp_gateway_tokens SET revoked_at=NOW(),updated_at=NOW() WHERE id=$1 AND company_id=$2 AND revoked_at IS NULL RETURNING id,revoked_at").bind(token_id).bind(company_id).fetch_optional(&state.pool).await.unwrap_or(None);
    match updated {
        Some(row) => (
            StatusCode::OK,
            Json(
                serde_json::json!({"id":row.get::<Uuid,_>("id"),"revokedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("revoked_at")}),
            ),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"Token not found"})),
        ),
    }
}

async fn list_connections(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> impl IntoResponse {
    if crate::routes::assert_board(&actor).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Board access required",
                "reasonCode": "board_access_required"
            })),
        );
    }
    if crate::routes::assert_company_access(&actor, company_id, true).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"Company access denied"})),
        );
    }
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
            'id', id, 'companyId', company_id, 'applicationId', application_id,\
            'name', name, 'uid', uid, 'connectionKind', connection_kind,\
            'ownership', ownership, 'transport', transport, 'authKind', auth_kind,\
            'status', status, 'transportConfig', transport_config,\
            'credentialSecretRefs', credential_secret_refs, 'enabled', enabled,\
            'createdByAgentId', created_by_agent_id, 'createdByUserId', created_by_user_id,\
            'createdAt', created_at, 'updatedAt', updated_at) ORDER BY name), '[]'::jsonb)\
         FROM tool_connections WHERE company_id = $1",
    )
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(Value::Array(vec![]));
    (
        StatusCode::OK,
        Json(serde_json::json!({ "connections": rows })),
    )
}

async fn list_policies(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> impl IntoResponse {
    if crate::routes::assert_board(&actor).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Board access required",
                "reasonCode": "board_access_required"
            })),
        );
    }
    if crate::routes::assert_company_access(&actor, company_id, true).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"Company access denied"})),
        );
    }
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
            'id', id, 'companyId', company_id, 'name', name, 'description', description,\
            'policyType', policy_type, 'priority', priority, 'enabled', enabled,\
            'selectors', selectors, 'conditions', conditions, 'config', config,\
            'createdByAgentId', created_by_agent_id, 'createdByUserId', created_by_user_id,\
            'createdAt', created_at, 'updatedAt', updated_at) ORDER BY priority, name), '[]'::jsonb)\
         FROM tool_policies WHERE company_id = $1 AND policy_type <> 'trust_rule'",
    ).bind(company_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    (
        StatusCode::OK,
        Json(serde_json::json!({ "policies": rows })),
    )
}

async fn create_policy(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if crate::routes::assert_company_access(&actor, company_id, false).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"Company access denied"})),
        );
    }
    let Some(name) = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"name is required"})),
        );
    };
    let policy_type = body
        .get("policyType")
        .or_else(|| body.get("policy_type"))
        .and_then(Value::as_str)
        .unwrap_or("allow");
    if !matches!(
        policy_type,
        "allow" | "block" | "require_approval" | "rate_limit"
    ) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"policyType is invalid"})),
        );
    }
    let selectors = body
        .get("selectors")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if !selectors.is_object() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"selectors must be an object"})),
        );
    }
    let priority = body
        .get("priority")
        .and_then(Value::as_i64)
        .unwrap_or(100)
        .clamp(0, 10_000) as i32;
    let enabled = body.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let description = body.get("description").and_then(Value::as_str);
    let config = match body.get("config") {
        Some(Value::Null) | None => None,
        Some(value) if value.is_object() => Some(value.clone()),
        Some(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"config must be an object or null"})),
            );
        }
    };
    let row = sqlx::query(
        "INSERT INTO tool_policies (company_id,name,description,policy_type,priority,enabled,selectors,conditions,config,created_by_user_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         RETURNING id,created_at,updated_at",
    )
    .bind(company_id).bind(name).bind(description).bind(policy_type).bind(priority).bind(enabled)
    .bind(&selectors).bind(body.get("conditions")).bind(&config)
    .bind(match actor { AuthorizationActor::Board { user_id, .. } => Some(user_id.to_string()), _ => None })
    .fetch_one(&state.pool).await;
    match row {
        Ok(row) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": row.get::<Uuid,_>("id"), "companyId": company_id, "name": name,
                "description": description, "policyType": policy_type, "priority": priority,
                "enabled": enabled, "selectors": selectors, "conditions": body.get("conditions"),
                "config": config, "createdAt": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
                "updatedAt": row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")
            })),
        ),
        Err(error) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": error.to_string()})),
        ),
    }
}

async fn delete_policy(
    Path((company_id, policy_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> impl IntoResponse {
    if crate::routes::assert_company_access(&actor, company_id, false).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"Company access denied"})),
        );
    }
    let deleted =
        sqlx::query("DELETE FROM tool_policies WHERE id=$1 AND company_id=$2 AND policy_type <> 'trust_rule' RETURNING id")
            .bind(policy_id)
            .bind(company_id)
            .fetch_optional(&state.pool)
            .await;
    match deleted {
        Ok(Some(_)) => (
            StatusCode::OK,
            Json(serde_json::json!({"id": policy_id, "deleted": true})),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"Policy not found"})),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error.to_string()})),
        ),
    }
}

async fn effective_profiles_for_agent(
    Path((company_id, agent_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> impl IntoResponse {
    if crate::routes::assert_board(&actor).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Board access required",
                "reasonCode": "board_access_required"
            })),
        );
    }
    if crate::routes::assert_company_access(&actor, company_id, true).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"Company access denied"})),
        );
    }
    let agent_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM agents WHERE id = $1 AND company_id = $2)",
    )
    .bind(agent_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await;
    match agent_exists {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error":"Agent not found"})),
            );
        }
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":error.to_string()})),
            );
        }
    }
    let profiles = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT COALESCE(
            jsonb_agg(
                to_jsonb(p) || jsonb_build_object('profileKey', p.profile_key)
                ORDER BY p.name
            ),
            '[]'::jsonb
        )
          FROM tool_profiles AS p
          JOIN tool_profile_bindings AS b
            ON b.profile_id = p.id
           AND b.company_id = p.company_id
         WHERE p.company_id = $1
           AND b.target_type = 'agent'
           AND b.target_id = $2::text
        "#,
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(Value::Array(vec![]));
    let bindings = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT COALESCE(jsonb_agg(to_jsonb(b) ORDER BY b.created_at), '[]'::jsonb)
          FROM tool_profile_bindings AS b
         WHERE b.company_id = $1
           AND b.target_type = 'agent'
           AND b.target_id = $2::text
        "#,
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(Value::Array(vec![]));
    let entries = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT COALESCE(
            jsonb_agg(
                jsonb_build_object(
                    'id', e.id,
                    'profileId', e.profile_id,
                    'selectorType', e.selector_type,
                    'effect', e.effect,
                    'connectionId', e.connection_id,
                    'toolName', e.tool_name,
                    'createdAt', e.created_at,
                    'updatedAt', e.updated_at
                ) ORDER BY e.created_at
            ),
            '[]'::jsonb
        )
          FROM tool_profile_entries AS e
          JOIN tool_profiles AS p
            ON p.id = e.profile_id
           AND p.company_id = e.company_id
         WHERE e.company_id = $1
           AND e.profile_id IN (
                SELECT b.profile_id
                  FROM tool_profile_bindings AS b
                 WHERE b.company_id = $1
                   AND b.target_type = 'agent'
                   AND b.target_id = $2::text
           )
        "#,
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(Value::Array(vec![]));
    let allowed_names = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT COALESCE(
            jsonb_agg(DISTINCT e.tool_name)
                FILTER (WHERE e.effect IN ('include', 'allow') AND e.tool_name IS NOT NULL),
            '[]'::jsonb
        )
          FROM tool_profile_entries AS e
          JOIN tool_profiles AS p
            ON p.id = e.profile_id
           AND p.company_id = e.company_id
         WHERE e.company_id = $1
           AND e.profile_id IN (
                SELECT b.profile_id
                  FROM tool_profile_bindings AS b
                 WHERE b.company_id = $1
                   AND b.target_type = 'agent'
                   AND b.target_id = $2::text
           )
        "#,
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(Value::Array(vec![]));
    let installed_connections = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT COALESCE(
            jsonb_agg(
                DISTINCT jsonb_build_object(
                    'id', c.id,
                    'companyId', c.company_id,
                    'applicationId', c.application_id,
                    'name', c.name,
                    'uid', c.uid,
                    'connectionKind', c.connection_kind,
                    'ownership', c.ownership,
                    'transport', c.transport,
                    'authKind', c.auth_kind,
                    'status', c.status,
                    'transportConfig', c.transport_config,
                    'credentialSecretRefs', c.credential_secret_refs,
                    'enabled', c.enabled,
                    'createdAt', c.created_at,
                    'updatedAt', c.updated_at
                )
            ),
            '[]'::jsonb
        )
          FROM tool_connections AS c
          JOIN tool_profile_entries AS e
            ON e.connection_id = c.id
           AND e.company_id = c.company_id
          JOIN tool_profiles AS p
            ON p.id = e.profile_id
           AND p.company_id = e.company_id
         WHERE c.company_id = $1
           AND e.profile_id IN (
                SELECT b.profile_id
                  FROM tool_profile_bindings AS b
                 WHERE b.company_id = $1
                   AND b.target_type = 'agent'
                   AND b.target_id = $2::text
           )
        "#,
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(Value::Array(vec![]));
    (
        StatusCode::OK,
        Json(
            serde_json::json!({"agentId": agent_id, "profiles": profiles, "entries": entries, "bindings": bindings, "allowedTools": [], "allowedToolNames": allowed_names, "installedConnections": installed_connections}),
        ),
    )
}

/// Paperclip UI run-detail contract: return persisted tool decisions associated
/// with a heartbeat run. The route is Board-only, company-scoped, and mirrors
/// Paperclip's not-found behavior for a run outside the requested company.
async fn get_run_decisions(
    Path((company_id, run_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> impl IntoResponse {
    if crate::routes::assert_board(&actor).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Board access required",
                "reasonCode": "board_access_required"
            })),
        );
    }
    if crate::routes::assert_company_access(&actor, company_id, true).is_err() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Company access denied",
                "reasonCode": "company_access_denied"
            })),
        );
    }

    let run_exists = match sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM heartbeat_runs WHERE id = $1 AND company_id = $2
         )",
    )
    .bind(run_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    {
        Ok(exists) => exists,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": error.to_string()})),
            );
        }
    };
    if !run_exists {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Run not found"})),
        );
    }

    let invocations = match sqlx::query(
        "SELECT id, idempotency_key, actor_type, actor_id, agent_id, issue_id, run_id,
                application_id, connection_id, catalog_entry_id, tool_name,
                arguments_hash, arguments_summary, policy_decision, matched_policy_ids,
                approval_state, status, upstream_request_id, result_hash, result_summary,
                result_size_bytes, result_artifact_id, error_code, error_message,
                started_at, completed_at, created_at, updated_at
           FROM tool_invocations
          WHERE company_id = $1 AND run_id = $2
          ORDER BY created_at DESC",
    )
    .bind(company_id)
    .bind(run_id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": error.to_string()})),
            );
        }
    };

    let mut decisions = Vec::with_capacity(invocations.len());
    for invocation in invocations {
        let invocation_id: Uuid = invocation.get("id");
        let action = match sqlx::query(
            "SELECT id, issue_id, interaction_id, approval_id, status,
                    canonical_arguments_hash, canonical_arguments_summary, signed_arguments,
                    preview_markdown, requested_by_agent_id, requested_by_user_id,
                    resolved_by_agent_id, resolved_by_user_id, decided_by_agent_id,
                    decided_by_user_id, decided_at, expires_at, resolved_at, created_at, updated_at
               FROM tool_action_requests WHERE company_id = $1 AND invocation_id = $2
               ORDER BY created_at DESC LIMIT 1",
        )
        .bind(company_id)
        .bind(invocation_id)
        .fetch_optional(&state.pool)
        .await
        {
            Ok(row) => row,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": error.to_string()})),
                );
            }
        };

        let events = match sqlx::query(
            "SELECT id, event_type, actor_type, actor_id, agent_id, run_id, issue_id,
                    application_id, connection_id, catalog_entry_id, invocation_id,
                    action_request_id, runtime_slot_id, tool_name, decision,
                    matched_policy_ids, reason_code, outcome, latency_ms, arguments_summary,
                    request_hash, request_summary, result_hash, result_summary,
                    result_size_bytes, redaction_plan, rate_limit_state, metadata,
                    error_code, error_message, created_at
               FROM tool_call_events
              WHERE company_id = $1 AND invocation_id = $2
              ORDER BY created_at DESC",
        )
        .bind(company_id)
        .bind(invocation_id)
        .fetch_all(&state.pool)
        .await
        {
            Ok(rows) => rows,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": error.to_string()})),
                );
            }
        };

        let event_values: Vec<Value> = events
            .iter()
            .map(|event| {
                serde_json::json!({
                    "id": event.get::<Uuid, _>("id"),
                    "companyId": company_id,
                    "eventType": event.get::<String, _>("event_type"),
                    "actorType": event.get::<String, _>("actor_type"),
                    "actorId": event.get::<Option<String>, _>("actor_id"),
                    "agentId": event.get::<Option<Uuid>, _>("agent_id"),
                    "runId": event.get::<Option<Uuid>, _>("run_id"),
                    "issueId": event.get::<Option<Uuid>, _>("issue_id"),
                    "invocationId": event.get::<Option<Uuid>, _>("invocation_id"),
                    "actionRequestId": event.get::<Option<Uuid>, _>("action_request_id"),
                    "toolName": event.get::<Option<String>, _>("tool_name"),
                    "decision": event.get::<Option<String>, _>("decision"),
                    "matchedPolicyIds": event.get::<Value, _>("matched_policy_ids"),
                    "reasonCode": event.get::<Option<String>, _>("reason_code"),
                    "outcome": event.get::<String, _>("outcome"),
                    "latencyMs": event.get::<Option<i32>, _>("latency_ms"),
                    "argumentsSummary": event.get::<Option<Value>, _>("arguments_summary"),
                    "requestHash": event.get::<Option<String>, _>("request_hash"),
                    "requestSummary": event.get::<Option<Value>, _>("request_summary"),
                    "resultHash": event.get::<Option<String>, _>("result_hash"),
                    "resultSummary": event.get::<Option<Value>, _>("result_summary"),
                    "resultSizeBytes": event.get::<Option<i32>, _>("result_size_bytes"),
                    "redactionPlan": event.get::<Option<Value>, _>("redaction_plan"),
                    "rateLimitState": event.get::<Option<Value>, _>("rate_limit_state"),
                    "metadata": event.get::<Option<Value>, _>("metadata"),
                    "errorCode": event.get::<Option<String>, _>("error_code"),
                    "errorMessage": event.get::<Option<String>, _>("error_message"),
                    "createdAt": event.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect();
        let latest_event = event_values.first().cloned().unwrap_or(Value::Null);
        let policy_decision: Option<String> = invocation.get("policy_decision");
        let pending_action = action.as_ref().and_then(|row| {
            (row.get::<String, _>("status") == "pending").then(|| {
                serde_json::json!({
                    "actionRequestId": row.get::<Uuid, _>("id"),
                    "issueId": row.get::<Option<Uuid>, _>("issue_id"),
                    "interactionId": row.get::<Option<Uuid>, _>("interaction_id"),
                    "approvalId": row.get::<Option<Uuid>, _>("approval_id"),
                    "status": row.get::<String, _>("status"),
                    "previewMarkdown": row.get::<Option<String>, _>("preview_markdown"),
                })
            })
        });
        let latest_decision = event_values
            .first()
            .and_then(|value| value.get("decision"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(policy_decision.clone());
        let latest_outcome = event_values
            .first()
            .and_then(|value| value.get("outcome"))
            .cloned()
            .unwrap_or(Value::Null);
        let latest_reason_code = event_values
            .first()
            .and_then(|value| value.get("reasonCode"))
            .cloned()
            .unwrap_or(Value::Null);

        decisions.push(serde_json::json!({
            "invocation": {
                "id": invocation_id,
                "companyId": company_id,
                "idempotencyKey": invocation.get::<Option<String>, _>("idempotency_key"),
                "actorType": invocation.get::<String, _>("actor_type"),
                "actorId": invocation.get::<Option<String>, _>("actor_id"),
                "agentId": invocation.get::<Option<Uuid>, _>("agent_id"),
                "issueId": invocation.get::<Option<Uuid>, _>("issue_id"),
                "runId": invocation.get::<Option<Uuid>, _>("run_id"),
                "toolName": invocation.get::<String, _>("tool_name"),
                "argumentsHash": invocation.get::<Option<String>, _>("arguments_hash"),
                "argumentsSummary": invocation.get::<Option<Value>, _>("arguments_summary"),
                "policyDecision": policy_decision,
                "matchedPolicyIds": invocation.get::<Value, _>("matched_policy_ids"),
                "approvalState": invocation.get::<String, _>("approval_state"),
                "status": invocation.get::<String, _>("status"),
                "upstreamRequestId": invocation.get::<Option<String>, _>("upstream_request_id"),
                "resultHash": invocation.get::<Option<String>, _>("result_hash"),
                "resultSummary": invocation.get::<Option<Value>, _>("result_summary"),
                "resultSizeBytes": invocation.get::<Option<i32>, _>("result_size_bytes"),
                "resultArtifactId": invocation.get::<Option<Uuid>, _>("result_artifact_id"),
                "errorCode": invocation.get::<Option<String>, _>("error_code"),
                "errorMessage": invocation.get::<Option<String>, _>("error_message"),
                "startedAt": invocation.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"),
                "completedAt": invocation.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at"),
                "createdAt": invocation.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": invocation.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            },
            "actionRequest": action.as_ref().map(|row| serde_json::json!({
                "id": row.get::<Uuid, _>("id"), "companyId": company_id,
                "invocationId": invocation_id, "issueId": row.get::<Option<Uuid>, _>("issue_id"),
                "interactionId": row.get::<Option<Uuid>, _>("interaction_id"),
                "approvalId": row.get::<Option<Uuid>, _>("approval_id"), "status": row.get::<String, _>("status"),
                "canonicalArgumentsHash": row.get::<String, _>("canonical_arguments_hash"),
                "canonicalArgumentsSummary": row.get::<Value, _>("canonical_arguments_summary"),
                "signedArguments": row.get::<Option<String>, _>("signed_arguments"),
                "previewMarkdown": row.get::<Option<String>, _>("preview_markdown"),
                "requestedByAgentId": row.get::<Option<Uuid>, _>("requested_by_agent_id"),
                "requestedByUserId": row.get::<Option<String>, _>("requested_by_user_id"),
                "resolvedByAgentId": row.get::<Option<Uuid>, _>("resolved_by_agent_id"),
                "resolvedByUserId": row.get::<Option<String>, _>("resolved_by_user_id"),
                "decidedByAgentId": row.get::<Option<Uuid>, _>("decided_by_agent_id"),
                "decidedByUserId": row.get::<Option<String>, _>("decided_by_user_id"),
                "decidedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("decided_at"),
                "expiresAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at"),
                "resolvedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at"),
                "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            })),
            "auditEvents": event_values.clone(),
            "latestAuditEvent": latest_event,
            "decision": latest_decision,
            "outcome": latest_outcome,
            "reasonCode": latest_reason_code,
            "denialReason": Value::Null,
            "pendingAction": pending_action,
        }));
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({"runId": run_id, "decisions": decisions})),
    )
}

pub fn tool_routes() -> Router<AppState> {
    Router::new()
        .route("/tool-gateway/sessions", post(create_gateway_session))
        .route(
            "/tool-gateway/sessions/:session_id/revoke",
            post(revoke_gateway_session),
        )
        .route("/tool-gateway/tools", get(list_gateway_tools))
        .route("/tool-gateway/tools/call", post(call_gateway_tool))
        .route(
            "/tool-gateway/mcp",
            get(mcp_session_info)
                .post(mcp_session_protocol)
                .delete(close_mcp_session),
        )
        .route(
            "/mcp/gateways/:gateway_public_id",
            get(mcp_session_info_named)
                .post(mcp_session_protocol_named)
                .delete(close_mcp_session_named),
        )
        .route(
            "/tool-gateway/gateways/:gateway_id/mcp",
            get(mcp_session_info_named)
                .post(mcp_session_protocol_named)
                .delete(close_mcp_session_named),
        )
        .route(
            "/companies/:company_id/tools/gateways",
            get(list_named_gateways).post(create_named_gateway),
        )
        .route(
            "/tool-gateway/gateways/:gateway_id",
            axum::routing::patch(update_named_gateway),
        )
        .route(
            "/tool-gateway/gateways/:gateway_id/tokens",
            post(create_named_gateway_token),
        )
        .route(
            "/tool-gateway/gateway-tokens/:token_id/revoke",
            post(revoke_named_gateway_token),
        )
        .route(
            "/tool-gateway/action-requests/:action_id/approve",
            post(approve_gateway_action),
        )
        .route(
            "/tool-gateway/action-requests/:action_id/decline",
            post(decline_gateway_action),
        )
        .route(
            "/companies/:company_id/tools/connections",
            get(list_connections),
        )
        .route(
            "/companies/:company_id/tools/policies",
            get(list_policies).post(create_policy),
        )
        .route(
            "/companies/:company_id/tools/policies/:policy_id",
            axum::routing::delete(delete_policy),
        )
        .route(
            "/companies/:company_id/tools/runs/:run_id/decisions",
            get(get_run_decisions),
        )
        .route(
            "/companies/:company_id/tools/profiles/effective/agents/:agent_id",
            get(effective_profiles_for_agent),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paperclip `packages/mcp-server/src/tools.ts` 暴露的内置工具数量。
    const PAPERCLIP_PARITY_TOOL_COUNT: usize = 41;
    /// Parrot 在 Paperclip 基础上额外提供的工具：`paperclipHireAgent`（走 approval 流程）。
    const PARROT_EXTRA_TOOL_COUNT: usize = 1;
    /// Parrot 已有 API dispatcher 覆盖、此前漏注册到 MCP tools/list 的扩展工具。
    const PARROT_EXTENDED_TOOL_COUNT: usize = 55;

    #[test]
    fn paperclip_builtin_registry_contains_core_tools() {
        let tools = paperclip_builtin_tools();
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<std::collections::HashSet<_>>();
        for required in [
            "paperclipMe",
            "paperclipListIssues",
            "paperclipGetIssue",
            "paperclipCreateIssue",
            "paperclipUpdateIssue",
            "paperclipAddComment",
            "paperclipGetDocument",
            "paperclipUpsertIssueDocument",
            "paperclipApprovalDecision",
            "paperclipApiRequest",
        ] {
            assert!(names.contains(required), "missing MCP tool {required}");
        }
        assert_eq!(names.len(), tools.len(), "MCP tool names must be unique");
        assert_eq!(
            tools.len(),
            PAPERCLIP_PARITY_TOOL_COUNT + PARROT_EXTRA_TOOL_COUNT + PARROT_EXTENDED_TOOL_COUNT,
            "MCP registry size drifted from the Paperclip reference and Parrot extensions"
        );
        assert!(names.contains("paperclipHireAgent"));
        for extended in [
            "paperclipCreateCase", "paperclipGetCase", "paperclipUpdateCase",
            "paperclipListCases", "paperclipGetCaseChildren", "paperclipGetCaseEvents",
            "paperclipGetIssueCases", "paperclipGetCaseDocument", "paperclipListCaseDocuments",
            "paperclipUpsertCaseDocument", "paperclipDeleteCaseDocument",
            "paperclipRestoreCaseDocumentRevision", "paperclipLockCaseDocument",
            "paperclipUnlockCaseDocument", "paperclipListCaseDocumentRevisions",
            "paperclipListCaseDocumentAnnotations", "paperclipCreateCaseDocumentAnnotation",
            "paperclipGetCaseDocumentAnnotationThread", "paperclipReplyCaseDocumentAnnotation",
            "paperclipUpdateCaseDocumentAnnotation", "paperclipCreateCaseLink",
            "paperclipListIssueAttachments", "paperclipCreateIssueAttachment",
            "paperclipDeleteAttachment", "paperclipGetAttachmentContent",
            "paperclipListIssueDocumentAnnotations", "paperclipCreateIssueDocumentAnnotation",
            "paperclipGetIssueDocumentAnnotationThread", "paperclipReplyIssueDocumentAnnotation",
            "paperclipUpdateIssueDocumentAnnotation", "paperclipListIssueExternalObjects",
            "paperclipRefreshIssueExternalObjects", "paperclipListIssueFileResources",
            "paperclipGetIssueFileResourceContent", "paperclipResolveIssueFileResource",
            "paperclipListLabels", "paperclipCreateLabel", "paperclipDeleteLabel",
            "paperclipListRoutines", "paperclipGetRoutine", "paperclipCreateRoutine",
            "paperclipUpdateRoutine", "paperclipListRoutineRevisions",
            "paperclipRestoreRoutineRevision", "paperclipListRoutineDescriptionAnnotations",
            "paperclipCreateRoutineDescriptionAnnotation",
            "paperclipGetRoutineDescriptionAnnotationThread",
            "paperclipReplyRoutineDescriptionAnnotation",
            "paperclipUpdateRoutineDescriptionAnnotation", "paperclipListRoutineRuns",
            "paperclipRunRoutine", "paperclipCreateRoutineTrigger", "paperclipUpdateRoutineTrigger",
            "paperclipDeleteRoutineTrigger", "paperclipRotateRoutineTriggerSecret",
        ] {
            assert!(names.contains(extended), "missing Parrot MCP extension {extended}");
        }
        assert!(tools.iter().all(|tool| tool
            .get("inputSchema")
            .and_then(|schema| schema.get("type"))
            .is_some()));
    }

    #[test]
    fn paperclip_argument_validation_covers_required_fields_and_sensitive_wrappers() {
        assert!(validate_paperclip_arguments("paperclipGetIssue", &serde_json::json!({})).is_err());
        assert!(validate_paperclip_arguments(
            "paperclipGetIssue",
            &serde_json::json!({"issueId": "ABC-1"})
        )
        .is_ok());
        assert!(validate_paperclip_arguments(
            "paperclipUpsertIssueDocument",
            &serde_json::json!({
                "issueId": "ABC-1", "key": "Bad Key", "body": "content"
            })
        )
        .is_err());
        assert!(validate_paperclip_arguments(
            "paperclipApiRequest",
            &serde_json::json!({
                "method": "POST", "path": "/issues/1", "jsonBody": "not-json"
            })
        )
        .is_err());
        assert!(validate_paperclip_arguments(
            "paperclipCreateIssue",
            &serde_json::json!({"title":"x", "priority":"invalid"})
        )
        .is_err());
        assert!(validate_paperclip_arguments(
            "paperclipCreateIssue",
            &serde_json::json!({"title":"x", "parentId":"not-a-uuid"})
        )
        .is_err());
        assert!(validate_paperclip_arguments(
            "paperclipCreateIssue",
            &serde_json::json!({"title":"x", "unexpected":true})
        )
        .is_err());
        assert!(validate_paperclip_arguments(
            "paperclipListComments",
            &serde_json::json!({"issueId":"ABC-1", "limit":501})
        )
        .is_err());
        assert!(validate_paperclip_arguments(
            "paperclipListComments",
            &serde_json::json!({"issueId":"ABC-1", "order":"sideways"})
        )
        .is_err());

        let source_issue_id = Uuid::new_v4();
        assert!(validate_paperclip_arguments(
            "paperclipHireAgent",
            &serde_json::json!({
                "name": "CTO",
                "role": "cto",
                "adapterType": "claude_local",
                "reportsTo": Uuid::new_v4(),
                "sourceIssueId": source_issue_id,
            })
        )
        .is_ok());
        assert!(validate_paperclip_arguments(
            "paperclipHireAgent",
            &serde_json::json!({
                "name": "Engineer",
                "role": "not-a-role",
                "adapterType": "claude_local",
            })
        )
        .is_err());
    }

    #[test]
    fn hire_agent_request_body_uses_canonical_endpoint_fields() {
        let issue_id = Uuid::new_v4();
        let body = build_hire_agent_request_body(&serde_json::json!({
            "companyId": Uuid::new_v4(),
            "name": "Engineer",
            "role": "engineer",
            "adapterType": "claude_local",
            "issueIds": [issue_id],
        }))
        .expect("hire body should be built");

        assert!(body.get("companyId").is_none());
        assert!(body.get("issueIds").is_none());
        assert_eq!(body["sourceIssueIds"], serde_json::json!([issue_id]));
        assert_eq!(body["adapterConfig"], serde_json::json!({}));
        assert_eq!(body["runtimeConfig"], serde_json::json!({}));
    }

    #[test]
    fn first_party_tools_are_default_allowed_but_explicit_denies_remain() {
        assert!(allow_first_party_tool_on_default_deny(
            "paperclipHireAgent",
            "deny_default",
            false,
        ));
        assert!(!allow_first_party_tool_on_default_deny(
            "paperclipHireAgent",
            "deny_policy_block",
            false,
        ));
        assert!(!allow_first_party_tool_on_default_deny(
            "paperclipHireAgent",
            "deny_default",
            true,
        ));
        assert!(!allow_first_party_tool_on_default_deny(
            "mcp.example:dangerous_tool",
            "deny_default",
            false,
        ));
    }

    #[test]
    fn gateway_policy_queries_are_valid_sql_literals() {
        assert!(!TOOL_POLICY_QUERY.contains('\\'));
        assert!(TOOL_POLICY_QUERY.contains("FROM tool_policies"));
        assert!(!TRUST_RULE_HIT_UPDATE.contains('\\'));
        assert!(TRUST_RULE_HIT_UPDATE.contains("UPDATE tool_policies"));
    }

    #[test]
    fn every_paperclip_schema_is_closed_and_runtime_validated() {
        for definition in paperclip_builtin_tool_definitions() {
            assert_eq!(
                definition.input_schema.get("type").and_then(Value::as_str),
                Some("object"),
                "{} must be an object schema",
                definition.name
            );
            assert_eq!(
                definition
                    .input_schema
                    .get("additionalProperties")
                    .and_then(Value::as_bool),
                Some(false),
                "{} must reject unknown fields",
                definition.name
            );
            assert!(
                definition
                    .input_schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .is_some(),
                "{} must expose properties",
                definition.name
            );
        }
        assert!(validate_paperclip_arguments(
            "paperclipCreateIssue",
            &serde_json::json!({"title":"valid", "status":"todo", "priority":"medium"})
        )
        .is_ok());
        assert!(validate_paperclip_arguments(
            "paperclipCreateIssue",
            &serde_json::json!({"title":"valid", "status":"not-a-status"})
        )
        .is_err());
        assert!(validate_paperclip_arguments(
            "paperclipGetGoal",
            &serde_json::json!({"goalId":"not-a-uuid"})
        )
        .is_err());
        assert!(validate_paperclip_arguments(
            "paperclipUpsertIssueDocument",
            &serde_json::json!({"issueId":"ABC-1", "key":"ok", "body":"x", "format":"html"})
        )
        .is_err());
    }

    #[test]
    fn paperclip_comment_contract_validates_presentation_and_metadata_rows() {
        let valid = serde_json::json!({
            "issueId": "ABC-1",
            "body": "details",
            "presentation": {"kind": "system_notice", "tone": "warning"},
            "metadata": {
                "version": 1,
                "sections": [{"title": "Run", "rows": [
                    {"type": "text", "text": "completed"},
                    {"type": "run_link", "runId": "00000000-0000-0000-0000-000000000001"}
                ]}]
            }
        });
        assert!(validate_paperclip_arguments("paperclipAddComment", &valid).is_ok());
        assert!(validate_paperclip_arguments(
            "paperclipAddComment",
            &serde_json::json!({
                "issueId": "ABC-1", "body": "details", "presentation": {"tone": "loud"}
            })
        )
        .is_err());
        assert!(validate_paperclip_arguments(
            "paperclipAddComment",
            &serde_json::json!({
                "issueId": "ABC-1", "body": "details", "metadata": {"version": 2, "sections": []}
            })
        )
        .is_err());
    }

    #[test]
    fn paperclip_interaction_contract_validates_versioned_payloads() {
        let suggest = serde_json::json!({
            "issueId": "ABC-1",
            "payload": {"version": 1, "tasks": [{"clientKey": "task-1", "title": "Do work"}]}
        });
        assert!(validate_paperclip_arguments("paperclipSuggestTasks", &suggest).is_ok());
        assert!(validate_paperclip_arguments("paperclipSuggestTasks", &serde_json::json!({
            "issueId": "ABC-1", "payload": {"version": 1, "tasks": [{"clientKey": "dup", "title": "a"}, {"clientKey": "dup", "title": "b"}]}
        })).is_err());
        let questions = serde_json::json!({
            "issueId": "ABC-1",
            "payload": {"version": 1, "questions": [{"id": "choice", "prompt": "Choose", "selectionMode": "single", "options": [{"id": "yes", "label": "Yes"}]}]}
        });
        assert!(validate_paperclip_arguments("paperclipAskUserQuestions", &questions).is_ok());
        assert!(validate_paperclip_arguments(
            "paperclipRequestConfirmation",
            &serde_json::json!({
                "issueId": "ABC-1", "payload": {"version": 1}
            })
        )
        .is_err());
    }

    #[test]
    fn mcp_accept_negotiation_is_fail_closed_for_unsupported_media() {
        let mut headers = HeaderMap::new();
        assert!(mcp_accepts_json_or_sse(&headers));
        headers.insert(
            "accept",
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        assert!(mcp_accepts_json_or_sse(&headers));
        headers.insert("accept", HeaderValue::from_static("text/plain"));
        assert!(!mcp_accepts_json_or_sse(&headers));
    }

    #[test]
    fn mcp_prefers_json_when_client_accepts_json_and_sse() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "accept",
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        assert!(!mcp_wants_sse(&headers));
        headers.insert("accept", HeaderValue::from_static("text/event-stream"));
        assert!(mcp_wants_sse(&headers));
    }

    #[test]
    fn mcp_registry_uses_typed_definitions() {
        let definitions = paperclip_builtin_tool_definitions();
        assert_eq!(
            definitions.len(),
            PAPERCLIP_PARITY_TOOL_COUNT + PARROT_EXTRA_TOOL_COUNT + PARROT_EXTENDED_TOOL_COUNT
        );
        assert!(definitions.iter().all(|definition| {
            definition.name.starts_with("paperclip")
            && definition.input_schema.get("type").is_some()
        }));
    }

    #[test]
    fn mcp_wire_result_preserves_content_and_structured_content() {
        let result = normalize_mcp_wire_result(serde_json::json!({
            "content": [{"type":"text", "text":"hello"}],
            "structuredContent": {"ok": true},
            "isError": false
        }))
        .expect("valid MCP content");
        assert_eq!(result["content"][0]["text"], "hello");
        assert_eq!(result["structuredContent"]["ok"], true);
        assert_eq!(result["isError"], false);
    }

    #[test]
    fn mcp_wire_result_rejects_malformed_content() {
        assert!(normalize_mcp_wire_result(serde_json::json!({
            "content": [{"type":"text"}]
        }))
        .is_err());
    }

    #[test]
    fn named_gateway_tokens_use_paperclip_wire_shape() {
        let token_id = Uuid::new_v4();
        let token = format!("pcgw_{}.{}", token_id, random_named_gateway_secret());
        assert!(token.starts_with("pcgw_"));
        assert_eq!(token.split('.').count(), 2);
        assert_eq!(token.split('.').next(), Some(format!("pcgw_{token_id}").as_str()));
    }

    #[test]
    fn oauth_secret_references_project_to_bearer_authorization() {
        let reference = serde_json::json!({"configPath": "oauth.access_token"});
        assert_eq!(mcp_secret_header_name(&reference).as_deref(), Some("Authorization"));
        assert_eq!(mcp_secret_header_prefix(&reference), "Bearer ");
        assert_eq!(
            mcp_secret_header_name(&serde_json::json!({"configPath": "headers.X-Trace"}))
                .as_deref(),
            Some("X-Trace")
        );
    }

    #[test]
    fn synthetic_fixture_preserves_state_across_tool_calls() {
        let template_id = "paperclip.synthetic-todo-kv";
        let key = format!("parity-{}", Uuid::new_v4());
        let title = format!("parity-item-{}", Uuid::new_v4());

        builtin_mcp_request(
            template_id,
            "tools/call",
            &serde_json::json!({
                "name": "set_value",
                "arguments": {"key": key.clone(), "value": {"ok": true}}
            }),
        )
        .expect("synthetic set_value should succeed");
        let value = builtin_mcp_request(
            template_id,
            "tools/call",
            &serde_json::json!({
                "name": "get_value",
                "arguments": {"key": key}
            }),
        )
        .expect("synthetic get_value should succeed");
        assert!(value["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("\"ok\":true")));

        let created = builtin_mcp_request(
            template_id,
            "tools/call",
            &serde_json::json!({
                "name": "create_item",
                "arguments": {"title": title}
            }),
        )
        .expect("synthetic create_item should succeed");
        let item_id = created["content"][0]["text"]
            .as_str()
            .and_then(|text| text.strip_prefix("created:"))
            .expect("synthetic create_item should return an item id")
            .to_string();
        builtin_mcp_request(
            template_id,
            "tools/call",
            &serde_json::json!({
                "name": "mark_done",
                "arguments": {"id": item_id.clone()}
            }),
        )
        .expect("synthetic mark_done should succeed");
        let listed = builtin_mcp_request(
            template_id,
            "tools/call",
            &serde_json::json!({"name": "list_items", "arguments": {}}),
        )
        .expect("synthetic list_items should succeed");
        let listed_text = listed["content"][0]["text"]
            .as_str()
            .expect("synthetic list_items should return text");
        assert!(listed_text.contains(&title));
        assert!(listed_text.contains("\"done\":true"));

        builtin_mcp_request(
            template_id,
            "tools/call",
            &serde_json::json!({
                "name": "delete_item",
                "arguments": {"id": item_id}
            }),
        )
        .expect("synthetic delete_item should succeed");
    }

    #[test]
    fn object_without_removes_context_fields_before_rest_forwarding() {
        assert_eq!(
            object_without(
                &serde_json::json!({"issueId": "i", "title": "t"}),
                &["issueId"]
            ),
            serde_json::json!({"title": "t"})
        );
    }

    #[test]
    fn gateway_session_revoke_scope_requires_board_company_and_uses_agent_context() {
        let company_id = Uuid::new_v4();
        let board = AuthorizationActor::board(Uuid::new_v4(), company_id);
        let board_scope = gateway_session_revoke_scope(
            &board,
            Some(&serde_json::json!({"companyId": company_id})),
        )
        .expect("board company should resolve");
        assert_eq!(
            board_scope,
            GatewaySessionRevokeScope {
                company_id,
                agent_id: None,
                run_id: None,
            }
        );

        let agent_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));
        let agent_scope = gateway_session_revoke_scope(&agent, None)
            .expect("agent scope should come from authentication context");
        assert_eq!(
            agent_scope,
            GatewaySessionRevokeScope {
                company_id,
                agent_id: Some(agent_id),
                run_id: Some(run_id),
            }
        );

        let (status, Json(error)) = gateway_session_revoke_scope(&board, None).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            error["reasonCode"],
            Value::String("company_required".to_string())
        );
    }

    #[test]
    fn anonymous_gateway_session_revoke_is_forbidden() {
        let (status, Json(error)) =
            gateway_session_revoke_scope(&AuthorizationActor::none(), None).unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            error["reasonCode"],
            Value::String("authentication_required".to_string())
        );
    }

    #[test]
    fn paperclip_tool_errors_preserve_upstream_business_status() {
        let error = PaperclipBuiltinToolError::upstream(
            StatusCode::FORBIDDEN,
            "missing agents:create".to_string(),
        );
        assert_eq!(error.response_status(), StatusCode::FORBIDDEN);
        assert_eq!(error.reason_code(), "paperclip_tool_upstream_failed");
        assert_eq!(error.upstream_status, Some(StatusCode::FORBIDDEN));

        let error = PaperclipBuiltinToolError::from("gateway unavailable");
        assert_eq!(error.response_status(), StatusCode::BAD_GATEWAY);
        assert_eq!(error.reason_code(), "paperclip_tool_call_failed");
        assert_eq!(error.upstream_status, None);
    }
}
