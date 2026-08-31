//! Tool access read endpoints.
//!
//! The tool-access persistence/service layer has not been migrated yet, but
//! Paperclip's UI expects these company-scoped read contracts to exist. Return
//! the same empty, typed envelopes until tool connections, profiles and
//! policies are backed by their repositories.

use axum::{
    body::to_bytes,
    extract::{Extension, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures::StreamExt;
use models::{CommentActorType, CreateIssueInput, UpdateIssueInput};
use serde_json::Value;
use services::issue_service::{
    CheckoutInput, IssueQueryFilter, Pagination as IssuePagination, ReleaseInput,
};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::convert::Infallible;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::mcp::{request_kind, McpInvocationContext, McpRequestKind, McpToolDefinition};
use crate::paperclip_internal::PaperclipInternalClient;
use services::auth::{AuthorizationAction, AuthorizationActor, AuthorizationService, PermissionKey};

fn hash_gateway_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
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
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": Uuid::new_v4(),
            "method": method,
            "params": params,
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("MCP server returned HTTP {}", status));
    }
    if let Some(error) = body.get("error") {
        return Err(error.to_string());
    }
    Ok(body.get("result").cloned().unwrap_or(body))
}

fn paperclip_builtin_tool_definitions() -> Vec<McpToolDefinition> {
    const TOOLS: &[(&str, &str)] = &[
        ("paperclipMe", "Get the current authenticated Paperclip actor details"),
        ("paperclipInboxLite", "Get the current authenticated agent inbox-lite assignment list"),
        ("paperclipHireAgent", "Request to hire a new agent (creates an approval request that requires board approval)"),
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
    TOOLS
        .iter()
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
                        "role": {"type": "string", "enum": ["ceo", "vp", "manager", "researcher", "general"], "description": "Agent role"},
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
                        "issueIds": {"type": ["array", "null"], "items": {"type": "string", "format": "uuid"}, "description": "Related issue IDs"}
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
    for key in ["blockedByIssueIds", "labelIds", "issueIds"] {
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
    run_id: Uuid,
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
                        checkout_run_id: run_id,
                    },
                )
                .await
                .map_err(|error| error.to_string())?;
            sqlx::query(
                "UPDATE issues SET assignee_agent_id = $2, checkout_run_id = $3, execution_run_id = $3, updated_at = NOW() WHERE id = $1 AND company_id = $4",
            )
            .bind(issue_id)
            .bind(agent_id)
            .bind(run_id)
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
            state
                .issue_service
                .release(
                    issue_id,
                    company_id,
                    ReleaseInput {
                        release_run_id: run_id,
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
                            Some(run_id),
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

pub(crate) async fn mcp_stdio_request(
    command: &str,
    args: &[String],
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let request =
        serde_json::json!({"jsonrpc":"2.0","id":Uuid::new_v4(),"method":method,"params":params})
            .to_string()
            + "\n";
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(request.as_bytes())
            .await
            .map_err(|error| error.to_string())?;
    }
    let mut stdout = child.stdout.take().ok_or("MCP stdio stdout unavailable")?;
    let mut bytes = Vec::new();
    stdout
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    let _ = child.kill().await;
    let text = String::from_utf8_lossy(&bytes).to_string();
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with('{'))
        .ok_or("MCP stdio returned no JSON")?;
    let body: Value = serde_json::from_str(line).map_err(|error| error.to_string())?;
    if let Some(error) = body.get("error") {
        return Err(error.to_string());
    }
    Ok(body.get("result").cloned().unwrap_or(body))
}

fn connection_url(config: &Value) -> Option<String> {
    config
        .get("url")
        .or_else(|| config.get("endpoint"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

async fn execute_mcp_connection(
    state: &AppState,
    company_id: Uuid,
    tool_name: &str,
    parameters: Value,
) -> Result<Value, String> {
    let raw = tool_name
        .strip_prefix("mcp.")
        .ok_or("MCP tool name must start with mcp.")?;
    let (uid, upstream_name) = raw
        .split_once(':')
        .ok_or("MCP tool name must be mcp.<connection>:<tool>")?;
    let connection = sqlx::query("SELECT transport, transport_config FROM tool_connections WHERE company_id=$1 AND uid=$2 AND enabled=true")
        .bind(company_id).bind(uid).fetch_optional(&state.pool).await.map_err(|error| error.to_string())?
        .ok_or("MCP connection not found or disabled")?;
    let transport: String = connection.get("transport");
    let config: Value = connection.get("transport_config");
    if transport == "mcp_remote" {
        match connection_url(&config) {
            Some(url) => {
                mcp_http_request(
                    &url,
                    "tools/call",
                    serde_json::json!({"name": upstream_name, "arguments": parameters}),
                )
                .await
            }
            None => Err("MCP connection has no remote URL".to_string()),
        }
    } else {
        match config.get("command").and_then(Value::as_str) {
            Some(command) => {
                let args = config
                    .get("args")
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                mcp_stdio_request(
                    command,
                    &args,
                    "tools/call",
                    serde_json::json!({"name": upstream_name, "arguments": parameters}),
                )
                .await
            }
            None => Err("MCP connection has no executable transport configuration".to_string()),
        }
    }
}

async fn load_gateway_session(
    state: &AppState,
    token: &str,
) -> Result<sqlx::postgres::PgRow, (StatusCode, Json<Value>)> {
    let row = sqlx::query(
        "SELECT s.id, s.company_id, s.agent_id, s.run_id, s.issue_id, s.expires_at, s.revoked_at,
                r.status::text AS run_status
           FROM tool_gateway_sessions s
           JOIN heartbeat_runs r ON r.id = s.run_id
          WHERE s.token_hash = $1",
    )
    .bind(hash_gateway_token(token))
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
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Tool gateway session is invalid"})),
        ));
    };
    let revoked_at: Option<chrono::DateTime<chrono::Utc>> = row.get("revoked_at");
    let expires_at: chrono::DateTime<chrono::Utc> = row.get("expires_at");
    if revoked_at.is_some() || expires_at <= chrono::Utc::now() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Tool gateway session is expired or revoked"})),
        ));
    }
    let run_status: String = row.get("run_status");
    if !matches!(run_status.as_str(), "queued" | "running") {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Tool gateway run is no longer active"})),
        ));
    }
    let _ = sqlx::query(
        "UPDATE tool_gateway_sessions SET last_used_at = NOW(), updated_at = NOW() WHERE id = $1",
    )
    .bind(row.get::<Uuid, _>("id"))
    .execute(&state.pool)
    .await;
    Ok(row)
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
        let existing_agent_id: Uuid = existing.get("agent_id");
        let existing_run_id: Uuid = existing.get("run_id");
        if existing_agent_id != agent_id
            || scope
                .run_id
                .is_some_and(|run_id| existing_run_id != run_id)
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
    let matches = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
           SELECT 1 FROM tool_mcp_gateways g
           JOIN tool_gateway_sessions s ON s.company_id = g.company_id
          WHERE s.token_hash = $1
            AND (g.gateway_public_id = $2 OR g.id::text = $2)
            AND g.status <> 'archived'
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
            "runId": session.get::<Uuid, _>("run_id"),
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
            // Paperclip's MCP server returns text content. Keep structuredContent
            // only for object-shaped values: Claude Code validates structured
            // content as a record, while list tools legitimately return arrays.
            // Arrays remain fully available as JSON text and match Paperclip's
            // wire contract instead of causing an SDK schema error.
            let mut tool_result = serde_json::json!({
                "content": [{"type":"text","text":result.to_string()}],
                "isError":false
            });
            if result.is_object() {
                tool_result["structuredContent"] = result;
            }
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
    let connections = sqlx::query("SELECT id, uid, transport, transport_config FROM tool_connections WHERE company_id = $1 AND enabled = true")
        .bind(session.get::<Uuid, _>("company_id")).fetch_all(&state.pool).await.unwrap_or_default();
    for connection in connections {
        let connection_id: Uuid = connection.get("id");
        let uid: String = connection.get("uid");
        let transport: String = connection.get("transport");
        let config: Value = connection.get("transport_config");
        let result = if transport == "mcp_remote" {
            if let Some(url) = connection_url(&config) {
                mcp_http_request(&url, "tools/list", serde_json::json!({})).await
            } else {
                Err("MCP remote connection has no URL".to_string())
            }
        } else {
            if let Some(command) = config.get("command").and_then(Value::as_str) {
                let args = config
                    .get("args")
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                mcp_stdio_request(command, &args, "tools/list", serde_json::json!({})).await
            } else {
                Err("MCP stdio connection has no command".to_string())
            }
        };
        if let Ok(result) = result {
            for tool in result
                .get("tools")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                if let Some(upstream_name) = tool.get("name").and_then(Value::as_str) {
                    tools.push(serde_json::json!({
                        "name": format!("mcp.{}:{}", uid, upstream_name),
                        "description": tool.get("description").and_then(Value::as_str).unwrap_or(""),
                        "inputSchema": tool.get("inputSchema").cloned().unwrap_or_else(|| serde_json::json!({"type":"object","properties":{}})),
                        "connectionId": connection_id,
                        "upstreamToolName": upstream_name,
                    }));
                }
            }
        }
    }
    tools.extend(paperclip_builtin_tools());
    let company_id: Uuid = session.get("company_id");
    let agent_id: Uuid = session.get("agent_id");
    let mut visible = Vec::with_capacity(tools.len());
    for tool in tools.drain(..) {
        let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        if gateway_decision(&state, company_id, agent_id, name).await != "deny" {
            visible.push(tool);
        }
    }
    (StatusCode::OK, Json(Value::Array(visible)))
}

async fn gateway_decision(
    state: &AppState,
    company_id: Uuid,
    agent_id: Uuid,
    tool_name: &str,
) -> String {
    let profile_effect: Option<String> = sqlx::query_scalar(
        "SELECT e.effect FROM tool_profile_entries e
           JOIN tool_profile_bindings b
             ON b.profile_id = e.profile_id AND b.company_id = e.company_id
          WHERE b.company_id = $1 AND e.company_id = $1
            AND b.target_type = 'agent' AND b.target_id = $2
            AND (e.tool_name = $3 OR e.tool_name = '*')
          ORDER BY CASE WHEN e.tool_name = $3 THEN 0 ELSE 1 END,
                   CASE WHEN e.effect IN ('exclude', 'deny') THEN 0 ELSE 1 END
          LIMIT 1",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(tool_name)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    if let Some(effect) = profile_effect {
        if matches!(effect.as_str(), "exclude" | "deny") {
            return "deny".to_string();
        }
        if matches!(effect.as_str(), "include" | "allow") {
            return "allow".to_string();
        }
    }
    let policy_type: Option<String> = sqlx::query_scalar(
        "SELECT policy_type FROM tool_policies
          WHERE company_id = $1 AND enabled = true
            AND (selectors->>'toolName' = $2 OR selectors->>'tool_name' = $2 OR selectors->>'tool' = $2)
          ORDER BY priority DESC LIMIT 1",
    )
    .bind(company_id).bind(tool_name)
    .fetch_optional(&state.pool).await.unwrap_or(None);
    match policy_type.as_deref() {
        Some("deny") | Some("block") => "deny".to_string(),
        Some("require_approval") | Some("approval") | Some("ask_first") => {
            "require_approval".to_string()
        }
        Some("allow") => "allow".to_string(),
        _ if tool_name.starts_with("paperclip") => "allow".to_string(),
        _ => "deny".to_string(),
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

async fn call_paperclip_builtin_tool(
    state: &AppState,
    token: &str,
    company_id: Uuid,
    agent_id: Uuid,
    run_id: Uuid,
    tool_name: &str,
    parameters: &Value,
) -> Result<Value, String> {
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
        "paperclipHireAgent" => (
            "POST",
            format!("/companies/{company_id}/approvals"),
            Some({
                // 构建hire_agent的payload
                let mut hire_payload = object_without(&parameters, &["companyId", "issueIds"]);

                // 如果没有adapterConfig，添加默认值
                if let Some(obj) = hire_payload.as_object_mut() {
                    if !obj.contains_key("adapterConfig") {
                        obj.insert("adapterConfig".to_string(), serde_json::json!({}));
                    }
                }

                // 构建approval请求
                serde_json::json!({
                    "type": "hire_agent",
                    "requestedByAgentId": agent_id,
                    "payload": hire_payload,
                    "issueIds": parameters.get("issueIds").cloned().unwrap_or_else(|| serde_json::json!([]))
                })
            }),
        ),
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
                _ => return Err(format!("unsupported approval action: {action}")),
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
                let requested_agent = parameters
                    .get("agentId")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| agent_id.to_string());
                Some(serde_json::json!({
                    "agentId": requested_agent,
                    "expectedStatuses": parameters.get("expectedStatuses").cloned().unwrap_or_else(|| serde_json::json!(["todo", "backlog", "blocked"])),
                    "checkoutRunId": run_id
                }))
            },
        ),
        "paperclipReleaseIssue" => (
            "POST",
            format!(
                "/issues/{}/release",
                path_part(parameters.get("issueId"), "issueId")?
            ),
            Some(serde_json::json!({
                "releaseRunId": run_id,
                "result": parameters.get("result"),
                "targetStatus": parameters.get("targetStatus")
            })),
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
                    object.insert(
                        "actor_run_id".to_string(),
                        Value::String(run_id.to_string()),
                    );
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
            let workspace_id = path_part(parameters.get("workspaceId"), "workspaceId")?;
            let action = path_part(parameters.get("action"), "action")?;
            (
                "POST",
                format!("/execution-workspaces/{workspace_id}/runtime-services/{action}"),
                Some(object_without(
                    &parameters,
                    &["issueId", "workspaceId", "action"],
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
                "/issues/{}/documents/{}/annotations/{}/reply",
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
                "/cases/{}/documents/{}/annotations/{}/reply",
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
                "/routines/{}/description/annotations/{}/reply",
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
                return Err(format!("unsupported HTTP method: {method}"));
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
        _ => return Err(format!("Unknown Paperclip tool: {tool_name}")),
    };
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|error| format!("invalid HTTP method: {error}"))?;
    let client = PaperclipInternalClient::new(token, run_id);
    let (status, value) = client.request(method.clone(), &path, body).await?;
    if !status.is_success() {
        return Err(format!(
            "{} {} failed with {}: {}",
            method, path, status, value
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
    agent_id: Uuid,
    run_id: Uuid,
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
         VALUES ($1,$2,$3,'agent',$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
         ON CONFLICT (company_id, idempotency_key) DO NOTHING
         RETURNING id",
    )
    .bind(invocation_id)
    .bind(company_id)
    .bind(idempotency_key)
    .bind(agent_id.to_string())
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
    agent_id: Uuid,
    run_id: Uuid,
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
         VALUES ($1,$2,$3,'agent',$4,$5,$6,$7,$8,$9,$10,$11,'pending','pending')
         ON CONFLICT (company_id, idempotency_key) DO NOTHING
         RETURNING id",
    )
    .bind(invocation_id)
    .bind(company_id)
    .bind(idempotency_key)
    .bind(agent_id.to_string())
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
    let agent_id: Uuid = session.get("agent_id");
    let run_id: Uuid = session.get("run_id");
    let _invocation_context = McpInvocationContext {
        session_id: session.get("id"),
        company_id,
        agent_id,
        run_id,
        issue_id: session.get("issue_id"),
    };
    if tool_name.starts_with("paperclip") {
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
        let decision = gateway_decision(&state, company_id, agent_id, tool_name).await;
        if decision == "deny" {
            let invocation_id = match reserve_gateway_invocation(
                &state,
                company_id,
                agent_id,
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
                agent_id,
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
            agent_id,
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
                let _ = sqlx::query(
                    "UPDATE tool_invocations SET status='failed', error_message=$2,
                     completed_at=NOW(), updated_at=NOW() WHERE id=$1",
                )
                .bind(invocation_id)
                .bind(&error)
                .execute(&state.pool)
                .await;
                (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({
                        "error": error,
                        "reasonCode": "paperclip_tool_call_failed",
                        "invocationId": invocation_id
                    })),
                )
            }
        };
    }
    let plugin = sqlx::query("SELECT id, manifest FROM plugins WHERE status = 'ready' AND EXISTS (SELECT 1 FROM jsonb_array_elements(manifest->'tools') item WHERE item->>'name' = $1)")
        .bind(tool_name).fetch_optional(&state.pool).await.unwrap_or(None);
    if plugin.is_none() && tool_name.starts_with("mcp.") {
        let raw = &tool_name[4..];
        if let Some((uid, upstream_name)) = raw.split_once(':') {
            let connection = sqlx::query("SELECT id, transport, transport_config FROM tool_connections WHERE company_id=$1 AND uid=$2 AND enabled=true")
                .bind(company_id).bind(uid).fetch_optional(&state.pool).await.unwrap_or(None);
            if let Some(connection) = connection {
                let connection_id: Uuid = connection.get("id");
                let transport: String = connection.get("transport");
                let config: Value = connection.get("transport_config");
                let parameters = body
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let decision = gateway_decision(&state, company_id, agent_id, tool_name).await;
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
                let result = if transport == "mcp_remote" {
                    match connection_url(&config) {
                        Some(url) => {
                            mcp_http_request(
                                &url,
                                "tools/call",
                                serde_json::json!({"name": upstream_name, "arguments": parameters}),
                            )
                            .await
                        }
                        None => Err("MCP connection has no remote URL".to_string()),
                    }
                } else {
                    match config.get("command").and_then(Value::as_str) {
                        Some(command) => {
                            let args = config
                                .get("args")
                                .and_then(Value::as_array)
                                .map(|values| {
                                    values
                                        .iter()
                                        .filter_map(Value::as_str)
                                        .map(ToOwned::to_owned)
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            mcp_stdio_request(
                                command,
                                &args,
                                "tools/call",
                                serde_json::json!({"name": upstream_name, "arguments": parameters}),
                            )
                            .await
                        }
                        None => {
                            Err("MCP connection has no executable transport configuration"
                                .to_string())
                        }
                    }
                };
                return match result {
                    Ok(value) => {
                        let _ = sqlx::query("UPDATE tool_invocations SET status='succeeded',result_summary=$2,completed_at=NOW(),updated_at=NOW() WHERE id=$1").bind(invocation_id).bind(serde_json::json!({"valueType":"json"})).execute(&state.pool).await;
                        let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,connection_id,tool_name,decision,outcome,invocation_id) VALUES ($1,'call_completed','agent',$2,$3,$4,$5,$6,$7,'success',$8)").bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(connection_id).bind(tool_name).bind(&decision).bind(invocation_id).execute(&state.pool).await;
                        (
                            StatusCode::OK,
                            Json(
                                serde_json::json!({"decision":"allowed","invocationId":invocation_id,"result":value}),
                            ),
                        )
                    }
                    Err(error) => {
                        let _ = sqlx::query("UPDATE tool_invocations SET status='failed',error_message=$2,completed_at=NOW(),updated_at=NOW() WHERE id=$1").bind(invocation_id).bind(&error).execute(&state.pool).await;
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
    }
    let Some(plugin) = plugin else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Tool not found", "reasonCode": "tool_not_found"})),
        );
    };
    let plugin_id: Uuid = plugin.get("id");
    let parameters = body
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let decision = gateway_decision(&state, company_id, agent_id, tool_name).await;
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
            .bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).execute(&state.pool).await;
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
            .bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).bind(action_id).execute(&state.pool).await;
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
        .bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(&args_summary).bind(invocation_id).execute(&state.pool).await;
    let result = state
        .plugin_service
        .dispatch_tool(plugin_id, tool_name, parameters)
        .await;
    match result {
        Ok(value) => {
            let _ = sqlx::query("UPDATE tool_invocations SET status='succeeded', result_summary=$2, completed_at=NOW(), updated_at=NOW() WHERE id=$1")
                .bind(invocation_id).bind(serde_json::json!({"valueType":"json"})).execute(&state.pool).await;
            let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,invocation_id,result_summary) VALUES ($1,'call_completed','agent',$2,$3,$4,$5,'allow','success',$6,$7)")
                .bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).bind(serde_json::json!({"valueType":"json"})).execute(&state.pool).await;
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
                .bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).bind(&message).execute(&state.pool).await;
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
       RETURNING ar.invocation_id, ar.signed_arguments, i.tool_name, i.agent_id, i.run_id",
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
    let agent_id: Uuid = row.get("agent_id");
    let run_id: Uuid = row.get("run_id");
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
        execute_mcp_connection(&state, company_id, &tool_name, parameters).await
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
          'name',g.name,'slug',g.slug,'description',g.description,'status',g.status,
          'profileId',g.profile_id,'agentId',g.agent_id,'issueId',g.issue_id,
          'metadata',g.metadata,'createdAt',g.created_at,'updatedAt',g.updated_at,
          'tokens',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',t.id,'gatewayId',t.gateway_id,'name',t.name,'tokenPrefix',t.token_prefix,'allowedActions',t.allowed_actions,'expiresAt',t.expires_at,'lastUsedAt',t.last_used_at,'revokedAt',t.revoked_at,'createdAt',t.created_at,'updatedAt',t.updated_at) ORDER BY t.created_at DESC) FROM tool_mcp_gateway_tokens t WHERE t.gateway_id=g.id),'[]'::jsonb)
        ) ORDER BY g.name),'[]'::jsonb) FROM tool_mcp_gateways g WHERE g.company_id=$1 AND g.status <> 'archived'",
    ).bind(company_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
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
        .and_then(Value::as_str)
        .unwrap_or(name)
        .trim()
        .to_lowercase()
        .replace(' ', "-");
    let row = sqlx::query("INSERT INTO tool_mcp_gateways (company_id,name,slug,description,agent_id,issue_id,metadata) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id,gateway_public_id,created_at,updated_at")
        .bind(company_id).bind(name).bind(&slug).bind(body.get("description").and_then(Value::as_str))
        .bind(body.get("agentId").and_then(Value::as_str).and_then(|v| Uuid::parse_str(v).ok()))
        .bind(body.get("issueId").and_then(Value::as_str).and_then(|v| Uuid::parse_str(v).ok()))
        .bind(body.get("metadata").cloned().unwrap_or_else(|| serde_json::json!({}))).fetch_one(&state.pool).await;
    match row {
        Ok(row) => (
            StatusCode::CREATED,
            Json(
                serde_json::json!({"id":row.get::<Uuid,_>("id"),"companyId":company_id,"gatewayPublicId":row.get::<String,_>("gateway_public_id"),"name":name,"slug":slug,"description":body.get("description"),"status":"active","agentId":body.get("agentId"),"issueId":body.get("issueId"),"metadata":body.get("metadata").cloned().unwrap_or_else(||serde_json::json!({})),"tokens":[],"createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")}),
            ),
        ),
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
    let row = sqlx::query("UPDATE tool_mcp_gateways SET name=COALESCE($3,name), description=COALESCE($4,description), status=COALESCE($5,status), updated_at=NOW() WHERE id=$1 AND company_id=$2 RETURNING id,gateway_public_id,name,slug,description,status,agent_id,issue_id,metadata,created_at,updated_at")
        .bind(gateway_id).bind(company_id).bind(body.get("name").and_then(Value::as_str)).bind(body.get("description").and_then(Value::as_str)).bind(body.get("status").and_then(Value::as_str)).fetch_optional(&state.pool).await;
    match row {
        Ok(Some(row)) => (
            StatusCode::OK,
            Json(
                serde_json::json!({"id":row.get::<Uuid,_>("id"),"companyId":company_id,"gatewayPublicId":row.get::<String,_>("gateway_public_id"),"name":row.get::<String,_>("name"),"slug":row.get::<String,_>("slug"),"description":row.get::<Option<String>,_>("description"),"status":row.get::<String,_>("status"),"agentId":row.get::<Option<Uuid>,_>("agent_id"),"issueId":row.get::<Option<Uuid>,_>("issue_id"),"metadata":row.get::<Value,_>("metadata"),"tokens":[],"createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")}),
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
    let token = format!("pcgw_{}", Uuid::new_v4().simple());
    let token_id = Uuid::new_v4();
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Gateway token");
    let row = sqlx::query("INSERT INTO tool_mcp_gateway_tokens (id,company_id,gateway_id,name,token_hash,token_prefix,allowed_actions,expires_at) SELECT $1,$2,$3,$4,$5,$6,COALESCE($7,'[\"tools/list\",\"tools/call\"]'::jsonb),$8 WHERE EXISTS (SELECT 1 FROM tool_mcp_gateways WHERE id=$3 AND company_id=$2) RETURNING id,gateway_id,token_prefix,created_at,updated_at,expires_at,allowed_actions")
        .bind(token_id).bind(company_id).bind(gateway_id).bind(name).bind(hash_gateway_token(&token)).bind(&token[..12.min(token.len())]).bind(body.get("allowedActions")).bind(body.get("expiresAt").and_then(Value::as_str).and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok()).map(|v|v.with_timezone(&chrono::Utc))).fetch_optional(&state.pool).await;
    match row {
        Ok(Some(row)) => (
            StatusCode::CREATED,
            Json(
                serde_json::json!({"id":row.get::<Uuid,_>("id"),"gatewayId":row.get::<Uuid,_>("gateway_id"),"companyId":company_id,"name":name,"token":token,"tokenPrefix":row.get::<String,_>("token_prefix"),"allowedActions":row.get::<Value,_>("allowed_actions"),"expiresAt":row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("expires_at"),"createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")}),
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
         FROM tool_policies WHERE company_id = $1",
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
        "allow" | "deny" | "block" | "require_approval" | "approval" | "ask_first"
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
        .unwrap_or(0)
        .clamp(-1_000_000, 1_000_000) as i32;
    let enabled = body.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let description = body.get("description").and_then(Value::as_str);
    let config = body.get("config").cloned().filter(Value::is_object);
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
        sqlx::query("DELETE FROM tool_policies WHERE id=$1 AND company_id=$2 RETURNING id")
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
           AND b.target_id = $2
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
           AND b.target_id = $2
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
                   AND b.target_id = $2
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
                   AND b.target_id = $2
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
                   AND b.target_id = $2
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
            PAPERCLIP_PARITY_TOOL_COUNT + PARROT_EXTRA_TOOL_COUNT,
            "MCP registry size drifted from the Paperclip reference"
        );
        assert!(names.contains("paperclipHireAgent"));
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
            PAPERCLIP_PARITY_TOOL_COUNT + PARROT_EXTRA_TOOL_COUNT
        );
        assert!(definitions.iter().all(|definition| {
            definition.name.starts_with("paperclip")
                && definition.input_schema.get("type").is_some()
        }));
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
}
