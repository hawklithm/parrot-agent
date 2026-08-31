//! Connected Apps / Tool Gateway contract — Paperclip `@paperclipai/shared`
//! `constants.ts` + `types/tool-access.ts`.
//!
//! This module ports the canonical enums the Tool Gateway and Connected Apps
//! surfaces share, plus the *derived predicates* Paperclip defines over them
//! (`isToolConnectionAttentionHealth`, terminal invocation/action-request
//! states). Keeping the predicates next to the tables means the needs-attention
//! aggregation, the Apps surfaces, and the audit feed cannot drift apart.
//!
//! Parrot already has runtime code for these surfaces (see
//! `crates/services/src/tool_access*.rs` and
//! `crates/api/src/routes/tool_access.rs`); this module supplies the canonical
//! vocabulary and shared semantics those paths must agree on.

/// Health states for a tool connection.
pub const TOOL_CONNECTION_HEALTH_STATUSES: &[&str] = &[
    "unknown",
    "healthy",
    "degraded",
    "failed",
    "unchecked",
    "ok",
    "error",
    "missing_secret",
];

/// Health states that mean an app needs the user's attention (bad/missing key
/// or a degraded connection). Single source of truth shared by the
/// needs-attention aggregation and the Apps surfaces so their counts agree.
pub const TOOL_CONNECTION_ATTENTION_HEALTH_STATUSES: &[&str] =
    &["degraded", "failed", "error", "missing_secret"];

/// Whether a connection health status requires user attention.
///
/// Port of Paperclip `isToolConnectionAttentionHealth`.
pub fn is_tool_connection_attention_health(status: &str) -> bool {
    TOOL_CONNECTION_ATTENTION_HEALTH_STATUSES.contains(&status)
}

/// How a connection obtains an upstream credential.
pub const CONNECTION_TOKEN_ISSUANCE_PATHS: &[&str] = &["exchange", "oauth_access", "static"];

/// Outcome of a token issuance attempt.
pub const CONNECTION_TOKEN_ISSUANCE_OUTCOMES: &[&str] = &[
    "success",
    "denied",
    "rate_limited",
    "use_env_lease",
    "upstream_error",
    "failure",
];

/// Tool Gateway policy kinds.
pub const TOOL_POLICY_TYPES: &[&str] =
    &["allow", "block", "require_approval", "trust_rule", "rate_limit"];

/// Result of a policy evaluation.
pub const TOOL_POLICY_DECISIONS: &[&str] =
    &["allow", "deny", "require_approval", "rate_limited", "defer_runtime"];

/// Lifecycle states of a tool invocation.
pub const TOOL_INVOCATION_STATUSES: &[&str] = &[
    "pending",
    "authorized",
    "denied",
    "awaiting_approval",
    "executing",
    "succeeded",
    "failed",
    "cancelled",
    "timed_out",
    "rate_limited",
];

/// Invocation statuses that are final — no further transitions occur.
pub const TOOL_INVOCATION_TERMINAL_STATUSES: &[&str] = &[
    "denied",
    "succeeded",
    "failed",
    "cancelled",
    "timed_out",
    "rate_limited",
];

/// Whether an invocation status is terminal.
pub fn is_terminal_invocation_status(status: &str) -> bool {
    TOOL_INVOCATION_TERMINAL_STATUSES.contains(&status)
}

/// Approval states of a tool invocation.
pub const TOOL_INVOCATION_APPROVAL_STATES: &[&str] = &[
    "not_required",
    "required",
    "pending",
    "approved",
    "rejected",
    "expired",
];

/// Lifecycle states of an action request (approval workflow).
pub const TOOL_ACTION_REQUEST_STATUSES: &[&str] = &[
    "pending",
    "approved",
    "executing",
    "rejected",
    "expired",
    "cancelled",
    "executed",
    "failed",
];

/// Action-request statuses that are final.
pub const TOOL_ACTION_REQUEST_TERMINAL_STATUSES: &[&str] =
    &["rejected", "expired", "cancelled", "executed", "failed"];

/// Whether an action-request status is terminal.
pub fn is_terminal_action_request_status(status: &str) -> bool {
    TOOL_ACTION_REQUEST_TERMINAL_STATUSES.contains(&status)
}

/// Catalog entry kinds and statuses.
pub const TOOL_CATALOG_ENTRY_KINDS: &[&str] = &["tool", "resource", "prompt"];
pub const TOOL_CATALOG_ENTRY_STATUSES: &[&str] = &["active", "disabled", "quarantined", "removed"];

/// Risk levels attached to catalog entries.
pub const TOOL_RISK_LEVELS: &[&str] =
    &["low", "medium", "high", "critical", "read", "write", "destructive"];

/// Profile statuses, default actions, selector types, and entry effects.
pub const TOOL_PROFILE_STATUSES: &[&str] = &["draft", "active", "disabled", "archived"];
pub const TOOL_PROFILE_DEFAULT_ACTIONS: &[&str] = &["deny", "allow"];
pub const TOOL_PROFILE_ENTRY_SELECTOR_TYPES: &[&str] =
    &["application", "connection", "catalog_entry", "tool_name", "risk_level"];
pub const TOOL_PROFILE_ENTRY_EFFECTS: &[&str] = &["include", "exclude"];
pub const TOOL_PROFILE_BINDING_TARGET_TYPES: &[&str] =
    &["company", "agent", "project", "routine", "issue", "gateway"];

/// MCP gateway statuses and default-profile resolution modes.
pub const TOOL_MCP_GATEWAY_STATUSES: &[&str] = &["draft", "active", "disabled", "archived"];
pub const TOOL_MCP_GATEWAY_DEFAULT_PROFILE_MODES: &[&str] = &[
    "gateway_only",
    "inherit_context_then_gateway",
    "gateway_then_context",
];

/// Runtime kinds and slot statuses.
pub const TOOL_RUNTIME_KINDS: &[&str] = &["remote_session", "local_stdio"];
pub const TOOL_RUNTIME_SLOT_STATUSES: &[&str] =
    &["starting", "running", "idle", "stopped", "failed", "disabled", "error"];

/// Rate-limit window kinds.
pub const TOOL_RATE_LIMIT_WINDOW_KINDS: &[&str] = &["minute", "hour", "day", "month"];

/// Audit event types emitted by the Tool Gateway.
pub const TOOL_AUDIT_EVENT_TYPES: &[&str] = &[
    "discovery",
    "policy_decision",
    "invocation_created",
    "call_started",
    "call_completed",
    "call_failed",
    "call_denied",
    "approval_requested",
    "approval_resolved",
    "session_revoked",
    "trust_rule_created",
    "trust_rule_revoked",
    "trust_rule_used",
    "runtime_started",
    "runtime_stopped",
    "rate_limited",
];

/// Connection lifecycle event types.
pub const TOOL_CONNECTION_LIFECYCLE_EVENT_TYPES: &[&str] = &[
    "app_connected",
    "app_paused",
    "app_resumed",
    "allowlist_changed",
    "reconnected",
    "disconnected",
    "actions_quarantined",
];

/// Recoverable connection error codes — states a user can fix by
/// (re)authorizing rather than reconfiguring the connection.
pub const CONNECTION_RECOVERABLE_ERROR_CODES: &[&str] = &[
    "user_authorization_required",
    "grant_revoked",
    "needs_reauthorization",
    "missing_secret",
];

/// Validate that a status string belongs to a canonical list.
fn require_member(kind: &str, list: &[&str], value: &str) -> Result<(), ToolAccessContractError> {
    if list.contains(&value) {
        Ok(())
    } else {
        Err(ToolAccessContractError::UnknownValue {
            kind: kind.to_string(),
            value: value.to_string(),
        })
    }
}

/// Reject a status that is not a canonical Tool Gateway enum member.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ToolAccessContractError {
    #[error("unknown {kind}: {value}")]
    UnknownValue { kind: String, value: String },
}

/// Validate a connection health status.
pub fn validate_connection_health(status: &str) -> Result<(), ToolAccessContractError> {
    require_member("connection health status", TOOL_CONNECTION_HEALTH_STATUSES, status)
}

/// Validate a token issuance path.
pub fn validate_token_issuance_path(path: &str) -> Result<(), ToolAccessContractError> {
    require_member("token issuance path", CONNECTION_TOKEN_ISSUANCE_PATHS, path)
}

/// Validate a token issuance outcome.
pub fn validate_token_issuance_outcome(outcome: &str) -> Result<(), ToolAccessContractError> {
    require_member(
        "token issuance outcome",
        CONNECTION_TOKEN_ISSUANCE_OUTCOMES,
        outcome,
    )
}

/// Validate an invocation status.
pub fn validate_invocation_status(status: &str) -> Result<(), ToolAccessContractError> {
    require_member("invocation status", TOOL_INVOCATION_STATUSES, status)
}

/// Validate an action-request status.
pub fn validate_action_request_status(status: &str) -> Result<(), ToolAccessContractError> {
    require_member("action request status", TOOL_ACTION_REQUEST_STATUSES, status)
}

/// Validate a policy decision.
pub fn validate_policy_decision(decision: &str) -> Result<(), ToolAccessContractError> {
    require_member("policy decision", TOOL_POLICY_DECISIONS, decision)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attention_health_is_single_source_of_truth() {
        for status in ["degraded", "failed", "error", "missing_secret"] {
            assert!(
                is_tool_connection_attention_health(status),
                "{status} must need attention"
            );
        }
        for status in ["unknown", "healthy", "unchecked", "ok"] {
            assert!(
                !is_tool_connection_attention_health(status),
                "{status} must not need attention"
            );
        }
    }

    #[test]
    fn attention_statuses_are_a_subset_of_health_statuses() {
        for status in TOOL_CONNECTION_ATTENTION_HEALTH_STATUSES {
            assert!(
                TOOL_CONNECTION_HEALTH_STATUSES.contains(status),
                "{status} must be a valid health status"
            );
        }
    }

    #[test]
    fn terminal_invocation_and_action_request_states() {
        for status in ["denied", "succeeded", "failed", "cancelled", "timed_out", "rate_limited"] {
            assert!(
                is_terminal_invocation_status(status),
                "{status} must be terminal"
            );
        }
        for status in ["pending", "authorized", "awaiting_approval", "executing"] {
            assert!(
                !is_terminal_invocation_status(status),
                "{status} must not be terminal"
            );
        }
        for status in ["rejected", "expired", "cancelled", "executed", "failed"] {
            assert!(
                is_terminal_action_request_status(status),
                "{status} must be terminal"
            );
        }
        for status in ["pending", "approved", "executing"] {
            assert!(
                !is_terminal_action_request_status(status),
                "{status} must not be terminal"
            );
        }
    }

    #[test]
    fn validators_accept_canonical_and_reject_unknown() {
        assert!(validate_connection_health("healthy").is_ok());
        assert!(validate_connection_health("fantastic").is_err());
        assert!(validate_token_issuance_path("oauth_access").is_ok());
        assert!(validate_token_issuance_path("magic").is_err());
        assert!(validate_token_issuance_outcome("use_env_lease").is_ok());
        assert!(validate_token_issuance_outcome("maybe").is_err());
        assert!(validate_invocation_status("awaiting_approval").is_ok());
        assert!(validate_invocation_status("wandering").is_err());
        assert!(validate_action_request_status("executed").is_ok());
        assert!(validate_action_request_status("pending_forever").is_err());
        assert!(validate_policy_decision("defer_runtime").is_ok());
        assert!(validate_policy_decision("shrug").is_err());
    }

    #[test]
    fn canonical_list_sizes_match_paperclip() {
        assert_eq!(TOOL_CONNECTION_HEALTH_STATUSES.len(), 8);
        assert_eq!(TOOL_CONNECTION_ATTENTION_HEALTH_STATUSES.len(), 4);
        assert_eq!(CONNECTION_TOKEN_ISSUANCE_PATHS.len(), 3);
        assert_eq!(CONNECTION_TOKEN_ISSUANCE_OUTCOMES.len(), 6);
        assert_eq!(TOOL_POLICY_TYPES.len(), 5);
        assert_eq!(TOOL_POLICY_DECISIONS.len(), 5);
        assert_eq!(TOOL_INVOCATION_STATUSES.len(), 10);
        assert_eq!(TOOL_INVOCATION_APPROVAL_STATES.len(), 6);
        assert_eq!(TOOL_ACTION_REQUEST_STATUSES.len(), 8);
        assert_eq!(TOOL_CATALOG_ENTRY_KINDS.len(), 3);
        assert_eq!(TOOL_CATALOG_ENTRY_STATUSES.len(), 4);
        assert_eq!(TOOL_RISK_LEVELS.len(), 7);
        assert_eq!(TOOL_PROFILE_STATUSES.len(), 4);
        assert_eq!(TOOL_PROFILE_DEFAULT_ACTIONS.len(), 2);
        assert_eq!(TOOL_PROFILE_ENTRY_SELECTOR_TYPES.len(), 5);
        assert_eq!(TOOL_PROFILE_ENTRY_EFFECTS.len(), 2);
        assert_eq!(TOOL_PROFILE_BINDING_TARGET_TYPES.len(), 6);
        assert_eq!(TOOL_MCP_GATEWAY_STATUSES.len(), 4);
        assert_eq!(TOOL_MCP_GATEWAY_DEFAULT_PROFILE_MODES.len(), 3);
        assert_eq!(TOOL_RUNTIME_KINDS.len(), 2);
        assert_eq!(TOOL_RUNTIME_SLOT_STATUSES.len(), 7);
        assert_eq!(TOOL_RATE_LIMIT_WINDOW_KINDS.len(), 4);
        assert_eq!(TOOL_AUDIT_EVENT_TYPES.len(), 16);
        assert_eq!(TOOL_CONNECTION_LIFECYCLE_EVENT_TYPES.len(), 7);
    }

    #[test]
    fn attention_set_covers_every_recoverable_health_failure() {
        // Every health status that blocks token issuance in the gateway route
        // must be an attention status, and vice versa.
        for status in TOOL_CONNECTION_ATTENTION_HEALTH_STATUSES {
            assert!(
                is_tool_connection_attention_health(status),
                "{status} must be an attention status"
            );
        }
        // The non-canonical 'unhealthy' value previously used by Parrot is not
        // a health status at all and must not be treated as one.
        assert!(!TOOL_CONNECTION_HEALTH_STATUSES.contains(&"unhealthy"));
        assert!(!is_tool_connection_attention_health("unhealthy"));
    }

    #[test]
    fn terminal_sets_exclude_in_flight_states() {
        for status in TOOL_INVOCATION_STATUSES {
            let terminal = is_terminal_invocation_status(status);
            let in_flight = matches!(*status, "pending" | "authorized" | "awaiting_approval" | "executing");
            assert!(
                terminal != in_flight,
                "{status} must be exactly one of terminal/in-flight"
            );
        }
        for status in TOOL_ACTION_REQUEST_STATUSES {
            let terminal = is_terminal_action_request_status(status);
            let in_flight = matches!(*status, "pending" | "approved" | "executing");
            assert!(
                terminal != in_flight,
                "{status} must be exactly one of terminal/in-flight"
            );
        }
    }

/// Map a connection-scoped activity-log action + details to a canonical
/// lifecycle event type, or `None` when the row is not an operator-visible
/// lifecycle change.
///
/// Port of Paperclip `activityLogActionToLifecycleType`
/// (`server/src/services/tool-access.ts`), extended to also recognize the
/// action spellings Parrot already writes (`tool_connection.created`,
/// `tool_connection.oauth_connected`) so the two systems' feeds line up. A
/// `tool_connection.updated` row only surfaces when its details carry a
/// `lifecycle` discriminator (`paused`/`resumed`/`allowlist_changed`);
/// plain settings edits stay out of the feed.
pub fn activity_log_action_to_lifecycle_type(
    action: &str,
    details: Option<&serde_json::Value>,
) -> Option<&'static str> {
    let lifecycle = details
        .and_then(|d| d.get("lifecycle"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match action {
        "tool_app.connected" | "tool_app.oauth_connected" | "tool_example.installed" => {
            Some("app_connected")
        }
        // Parrot spellings for the same operator-visible moments.
        "tool_connection.created" | "tool_connection.oauth_connected" => Some("app_connected"),
        "tool_app.reconnected" => Some("reconnected"),
        "tool_connection.archived" | "tool_connection.deleted" => Some("disconnected"),
        "tool_connection.updated" => match lifecycle {
            "paused" => Some("app_paused"),
            "resumed" => Some("app_resumed"),
            "allowlist_changed" => Some("allowlist_changed"),
            _ => None,
        },
        _ => None,
    }
}

    #[test]
    fn activity_actions_map_to_lifecycle_types() {
        let d = serde_json::json!({"lifecycle": "paused"});
        assert_eq!(
            activity_log_action_to_lifecycle_type("tool_connection.updated", Some(&d)),
            Some("app_paused")
        );
        let d = serde_json::json!({"lifecycle": "allowlist_changed", "added": 2});
        assert_eq!(
            activity_log_action_to_lifecycle_type("tool_connection.updated", Some(&d)),
            Some("allowlist_changed")
        );
        // Plain settings edits produce no lifecycle event.
        let d = serde_json::json!({"name": "renamed"});
        assert_eq!(
            activity_log_action_to_lifecycle_type("tool_connection.updated", Some(&d)),
            None
        );
        // Paperclip spellings.
        assert_eq!(
            activity_log_action_to_lifecycle_type("tool_app.connected", None),
            Some("app_connected")
        );
        assert_eq!(
            activity_log_action_to_lifecycle_type("tool_connection.archived", None),
            Some("disconnected")
        );
        // Parrot spellings.
        assert_eq!(
            activity_log_action_to_lifecycle_type("tool_connection.oauth_connected", None),
            Some("app_connected")
        );
        assert_eq!(
            activity_log_action_to_lifecycle_type("tool_connection.created", None),
            Some("app_connected")
        );
        // Unknown actions are not lifecycle events.
        assert_eq!(activity_log_action_to_lifecycle_type("company.updated", None), None);
    }

    #[test]
    fn every_lifecycle_type_is_canonical() {
        for action in [
            "tool_app.connected",
            "tool_app.oauth_connected",
            "tool_example.installed",
            "tool_connection.created",
            "tool_connection.oauth_connected",
        ] {
            let t = activity_log_action_to_lifecycle_type(action, None).unwrap();
            assert!(
                TOOL_CONNECTION_LIFECYCLE_EVENT_TYPES.contains(&t),
                "{t} must be canonical"
            );
        }
    }
}
