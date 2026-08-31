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
use uuid::Uuid;

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

/// Canonical JSON key order: collect every recursively nested key name.
///
/// Port of Paperclip `flattenKeys`.
fn flatten_keys(value: &serde_json::Value, keys: &mut std::collections::BTreeSet<String>) {
    if let serde_json::Value::Object(map) = value {
        for (key, nested) in map {
            keys.insert(key.clone());
            flatten_keys(nested, keys);
        }
    }
}

/// Deterministic content hash: `sha256(canonical JSON)`, where canonical JSON
/// serializes the value with every object key at every nesting level sorted.
///
/// Port of Paperclip `stableHash` (`createHash("sha256")
/// .update(JSON.stringify(value, Object.keys(flattenKeys(value)).sort()))`).
pub fn stable_hash(value: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    let mut keys = std::collections::BTreeSet::new();
    flatten_keys(value, &mut keys);
    let key_names: Vec<&str> = keys.iter().map(String::as_str).collect();
    // serde_json with preserve_order off sorts map keys only when using
    // BTreeMap-backed maps; serialize through a deterministic re-writer that
    // sorts object keys at every depth (matching JS sorted-key stringify).
    let canonical = canonicalize(value, &key_names);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex_encode(&hasher.finalize())
}

/// Re-serialize `value` with every object's keys sorted lexicographically.
fn canonicalize(value: &serde_json::Value, key_names: &[&str]) -> String {
    match value {
        serde_json::Value::Object(map) => {
            // JS `JSON.stringify(value, keysArray)` re-includes ONLY the listed
            // top-level keys... but for nested objects the replacer receives
            // every object, so all depths are filtered to the same key list.
            // Paperclip passes the flattened key names, so nested objects that
            // contain keys outside the list are serialized as `{}` by JS.
            let mut parts: Vec<String> = Vec::with_capacity(map.len());
            for (key, val) in map {
                // JS replacer semantics: keys not in the whitelist are skipped.
                if !key_names.contains(&key.as_str()) && !key_names.is_empty() {
                    continue;
                }
                parts.push(format!("{}:{}", serde_json::to_string(key).unwrap(), canonicalize(val, key_names)));
            }
            parts.sort(); // JS object key order from the replacer list is sorted upstream
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(|item| canonicalize(item, key_names)).collect();
            format!("[{}]", parts.join(","))
        }
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".to_string()),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Risk classification for a tool descriptor.
///
/// Port of Paperclip `classifyRisk` (descriptor annotations + name heuristics).
pub fn classify_risk(name: &str, annotations: &serde_json::Value) -> &'static str {
    let annotation = |key: &str| {
        annotations
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };
    let destructive = annotation("destructiveHint");
    let read_only = annotation("readOnlyHint");
    let idempotent = annotation("idempotentHint");
    let open_world = annotation("openWorldHint");
    if destructive {
        return "destructive";
    }
    if read_only && (idempotent || !open_world) {
        return "read";
    }
    if read_only {
        return "low";
    }
    let name = name.to_ascii_lowercase();
    if name.contains("delete")
        || name.contains("remove")
        || name.contains("destroy")
        || name.contains("drop")
    {
        return "destructive";
    }
    if name.contains("create")
        || name.contains("update")
        || name.contains("write")
        || name.contains("send")
        || name.contains("publish")
    {
        return "write";
    }
    "medium"
}

/// Content hash of a tool descriptor: Paperclip `descriptorHash`.
pub fn descriptor_hash(
    name: &str,
    title: Option<&str>,
    description: Option<&str>,
    input_schema: &serde_json::Value,
    annotations: &serde_json::Value,
    risk_level: &str,
) -> String {
    stable_hash(&serde_json::json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": input_schema,
        "annotations": annotations,
        "riskLevel": risk_level,
    }))
}

/// Schema-only content hash: Paperclip `stableHash(descriptor.inputSchema ?? {})`.
pub fn schema_hash(input_schema: &serde_json::Value) -> String {
    stable_hash(input_schema)
}

/// A policy row simplified to the fields Paperclip's decision ladder reads.
///
/// Faithful to `server/src/services/tool-access-policy.ts` `decide()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySpec {
    pub id: String,
    /// One of `TOOL_POLICY_TYPES`.
    pub policy_type: String,
    /// Selector: only apply this policy to this tool name (None = all tools).
    pub selector_tool_name: Option<String>,
    pub description: Option<String>,
    /// trust_rule only: the raw `config.trustRule` object (liveness, review
    /// hashes, argument filters). None means the rule is not configured.
    pub trust_rule_config: Option<serde_json::Value>,
    /// rate_limit only: whether the caller pre-evaluated the limit as exceeded.
    pub rate_limit_exceeded: bool,
}

impl PolicySpec {
    /// Convenience constructor for tests/simple callers.
    pub fn simple(id: &str, policy_type: &str) -> Self {
        Self {
            id: id.into(),
            policy_type: policy_type.into(),
            selector_tool_name: None,
            description: None,
            trust_rule_config: None,
            rate_limit_exceeded: false,
        }
    }

    /// Derive the trust-rule flags Paperclip computes before deciding.
    fn trust_rule_state(&self, ctx: &EvaluationContext, now: chrono::DateTime<chrono::Utc>) -> (bool, bool) {
        let active = trust_rule_is_active(self.trust_rule_config.as_ref(), now);
        let needs_review = trust_rule_needs_review(
            self.trust_rule_config.as_ref(),
            ctx.catalog_status.as_deref(),
            ctx.catalog_version_hash.as_deref(),
            ctx.catalog_schema_hash.as_deref(),
        );
        (active, needs_review)
    }
}

/// Static inputs the ladder reads outside the policy table.
#[derive(Debug, Clone, Default)]
pub struct EvaluationContext {
    pub tool_name: String,
    /// Paperclip `explicitGrant(ctx)`.
    pub explicit_grant: bool,
    /// Paperclip profile fallback: an effective profile allows the tool
    /// (defaultAction=allow or an include entry matched, with no excludes).
    pub profile_allows: bool,
    /// Raw invocation arguments (evaluated against trust-rule filters).
    pub arguments: Option<serde_json::Value>,
    /// Pre-computed `arguments_hash`; derived from `arguments` when None.
    pub arguments_hash: Option<String>,
    /// Catalog entry status of the invoked tool, when known.
    pub catalog_status: Option<String>,
    pub catalog_version_hash: Option<String>,
    pub catalog_schema_hash: Option<String>,
    /// Observed rate-limit state from the enforcing caller; attached to the
    /// outcome when the ladder decides rate_limited.
    pub last_rate_limit_state: Option<RateLimitState>,
}

fn rate_bucket_key_for(policy: &PolicySpec, _ctx: &EvaluationContext) -> String {
    policy.id.clone()
}

impl EvaluationContext {
    /// Effective arguments hash, computed on demand.
    pub fn effective_arguments_hash(&self) -> String {
        if let Some(hash) = &self.arguments_hash {
            return hash.clone();
        }
        match &self.arguments {
            Some(args) => arguments_hash(args),
            None => arguments_hash(&serde_json::Value::Null),
        }
    }
}

/// The outcome of the decision ladder: decision + reason code pair exactly as
/// Paperclip emits them.
#[derive(Debug, Clone)]
pub struct DecisionOutcome {
    /// One of `TOOL_POLICY_DECISIONS`.
    pub decision: &'static str,
    pub reason_code: &'static str,
    pub message: String,
    /// The policy that decided, when one did.
    pub policy_id: Option<String>,
    /// Populated when the decision is `rate_limited`: Paperclip's
    /// rateLimitState for the 429 body. Serialize via
    /// [`RateLimitState`]; carried as JSON for struct-field simplicity.
    pub rate_limit_state: Option<RateLimitState>,
}

fn self_filters(policy: &PolicySpec) -> Option<TrustRuleArgumentFilters> {
    let config = policy.trust_rule_config.as_ref()?;
    let filters = config.get("argumentFilters")?;
    serde_json::from_value(filters.clone()).ok()
}

fn matched(policy: &PolicySpec, ctx: &EvaluationContext) -> bool {
    match &policy.selector_tool_name {
        Some(name) => name == &ctx.tool_name,
        None => true,
    }
}

/// Paperclip's `decide()` precedence ladder over an ordered policy list.
///
/// Order (fail-closed): block → rate_limit (if exceeded) → trust_rule
/// (needs_review → require_approval, else allow) → require_approval → allow →
/// explicit grant → effective profile → **deny by default**.
pub fn decide_tool_access(
    policies: &[PolicySpec],
    ctx: &EvaluationContext,
) -> DecisionOutcome {
    decide_tool_access_at(policies, ctx, chrono::Utc::now())
}

/// As [`decide_tool_access`], with an explicit clock for deterministic tests.
pub fn decide_tool_access_at(
    policies: &[PolicySpec],
    ctx: &EvaluationContext,
    now: chrono::DateTime<chrono::Utc>,
) -> DecisionOutcome {
    for policy in policies {
        if !matched(policy, ctx) {
            continue;
        }
        match policy.policy_type.as_str() {
            "block" => {
                return DecisionOutcome {
                    decision: "deny",
                    reason_code: "deny_policy_block",
                    message: policy.description.clone().unwrap_or_else(|| "Tool access is blocked by policy.".into()),
                    policy_id: Some(policy.id.clone()),
                    rate_limit_state: None,
                };
            }
            "rate_limit" if policy.rate_limit_exceeded => {
                // The enforcing caller attaches the observed RateLimitState
                // (count/limit/window) to the context before the ladder runs.
                let state = ctx
                    .last_rate_limit_state
                    .as_ref()
                    .filter(|s| s.bucket_key == rate_bucket_key_for(policy, ctx))
                    .cloned();
                return DecisionOutcome {
                    decision: "rate_limited",
                    reason_code: "rate_limited",
                    message: "Tool access rate limit exceeded.".into(),
                    policy_id: Some(policy.id.clone()),
                    rate_limit_state: state,
                };
            }
            "trust_rule" => {
                let (active, needs_review) = policy.trust_rule_state(ctx, now);
                if !active {
                    // Paperclip: inactive trust rules fall through.
                    continue;
                }
                // Argument filters must match for the rule to fire.
                let filters = self_filters(policy);
                let hash = ctx.effective_arguments_hash();
                let arguments = ctx.arguments.clone().unwrap_or(serde_json::Value::Null);
                if !argument_filters_match(filters.as_ref(), &arguments, &hash) {
                    continue;
                }
                if needs_review {
                    return DecisionOutcome {
                        decision: "require_approval",
                        reason_code: "requires_review_changed_tool",
                        message: "Tool definition changed or was quarantined after this trust rule was created; review is required.".into(),
                        policy_id: Some(policy.id.clone()),
                        rate_limit_state: None,
                    };
                }
                return DecisionOutcome {
                    decision: "allow",
                    reason_code: "allow_trust_rule",
                    message: policy.description.clone().unwrap_or_else(|| "Tool access allowed by trust rule.".into()),
                    policy_id: Some(policy.id.clone()),
                    rate_limit_state: None,
                };
            }
            "require_approval" => {
                return DecisionOutcome {
                    decision: "require_approval",
                    reason_code: "requires_approval_policy",
                    message: policy.description.clone().unwrap_or_else(|| "Tool access requires approval.".into()),
                    policy_id: Some(policy.id.clone()),
                    rate_limit_state: None,
                };
            }
            "allow" => {
                return DecisionOutcome {
                    decision: "allow",
                    reason_code: "allow_policy",
                    message: "Tool access allowed by policy.".into(),
                    policy_id: Some(policy.id.clone()),
                    rate_limit_state: None,
                };
            }
            // rate_limit not exceeded and inactive trust rules fall through,
            // exactly like Paperclip's `continue`.
            _ => {}
        }
    }
    if ctx.explicit_grant {
        return DecisionOutcome {
            decision: "allow",
            reason_code: "allow_explicit_grant",
            message: "Tool access allowed by explicit grant.".into(),
            policy_id: None,
            rate_limit_state: None,
        };
    }
    if ctx.profile_allows {
        return DecisionOutcome {
            decision: "allow",
            reason_code: "allow_profile",
            message: "Tool access allowed by effective profile.".into(),
            policy_id: None,
            rate_limit_state: None,
        };
    }
    DecisionOutcome {
        decision: "deny",
        reason_code: "deny_default",
        message: "No effective tool profile, grant, or allow policy permits this call.".into(),
        policy_id: None,
        rate_limit_state: None,
    }
}

/// Trust-rule argument filters - port of Paperclip
/// `ToolTrustRuleArgumentFilters`.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustRuleArgumentFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_any: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_hashes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_equals: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_not_equals: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_in: Option<std::collections::BTreeMap<String, Vec<serde_json::Value>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_matches: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_exists: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_absent: Option<Vec<String>>,
}

/// Stable stringify: objects sorted by key at every depth, arrays in order.
///
/// Port of Paperclip `stableStringify`.
pub fn stable_stringify(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_default(),
                        stable_stringify(&map[key])
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(stable_stringify).collect();
            format!("[{}]", parts.join(","))
        }
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".to_string()),
    }
}

/// sha256 over the stable stringify of a value - Paperclip trust-rule
/// argument hashing.
pub fn arguments_hash(arguments: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(stable_stringify(arguments).as_bytes());
    hex_encode(&hasher.finalize())
}

/// Read a dot-separated path from JSON, supporting numeric array indices.
///
/// Port of Paperclip `readPath`.
pub fn read_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    if path.is_empty() {
        return None;
    }
    let mut current = value;
    for segment in path.split('.') {
        match current {
            serde_json::Value::Object(map) => {
                current = map.get(segment)?;
            }
            serde_json::Value::Array(items) => {
                let index: usize = segment.parse().ok()?;
                current = items.get(index)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

/// Port of Paperclip `argumentFiltersMatch`.
///
/// Returns false unless at least one constraint is configured; `allowAny`
/// short-circuits to true. Mirrors the filter matrix exactly.
pub fn argument_filters_match(
    filters: Option<&TrustRuleArgumentFilters>,
    arguments: &serde_json::Value,
    arguments_hash: &str,
) -> bool {
    let Some(filters) = filters else {
        return false;
    };
    if filters.allow_any == Some(true) {
        return true;
    }
    if let Some(exact) = &filters.exact_hash {
        if exact != arguments_hash {
            return false;
        }
    }
    if let Some(allowed) = &filters.allowed_hashes {
        if !allowed.is_empty() && !allowed.iter().any(|h| h == arguments_hash) {
            return false;
        }
    }
    if let Some(equals) = &filters.field_equals {
        for (path, expected) in equals {
            let actual = read_path(arguments, path);
            let actual_str = actual.map(stable_stringify).unwrap_or_else(|| "undefined".into());
            if actual_str != stable_stringify(expected) {
                return false;
            }
        }
    }
    if let Some(not_equals) = &filters.field_not_equals {
        for (path, expected) in not_equals {
            let actual = read_path(arguments, path);
            let actual_str = actual.map(stable_stringify).unwrap_or_else(|| "undefined".into());
            if actual_str == stable_stringify(expected) {
                return false;
            }
        }
    }
    if let Some(field_in) = &filters.field_in {
        for (path, allowed_values) in field_in {
            let actual = read_path(arguments, path);
            let actual_str = actual.map(stable_stringify).unwrap_or_else(|| "undefined".into());
            if !allowed_values.iter().any(|expected| stable_stringify(expected) == actual_str) {
                return false;
            }
        }
    }
    if let Some(matches) = &filters.field_matches {
        for (path, pattern) in matches {
            let Some(actual) = read_path(arguments, path).and_then(serde_json::Value::as_str) else {
                return false;
            };
            let Ok(re) = regex::Regex::new(pattern) else {
                return false;
            };
            if !re.is_match(actual) {
                return false;
            }
        }
    }
    if let Some(exists) = &filters.field_exists {
        if exists.iter().any(|path| read_path(arguments, path).is_none()) {
            return false;
        }
    }
    if let Some(absent) = &filters.field_absent {
        if absent.iter().any(|path| read_path(arguments, path).is_some()) {
            return false;
        }
    }
    filters.exact_hash.is_some()
        || filters.allowed_hashes.as_ref().is_some_and(|h| !h.is_empty())
        || filters.field_equals.is_some()
        || filters.field_not_equals.is_some()
        || filters.field_in.is_some()
        || filters.field_matches.is_some()
        || filters.field_exists.as_ref().is_some_and(|f| !f.is_empty())
        || filters.field_absent.as_ref().is_some_and(|f| !f.is_empty())
}

/// Trust-rule liveness: config present, not revoked, not expired.
///
/// Port of Paperclip `trustRuleIsActive` (evaluated against `now`).
pub fn trust_rule_is_active(
    config: Option<&serde_json::Value>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    let Some(config) = config else {
        return false;
    };
    if let Some(revoked_at) = config.get("revokedAt").and_then(serde_json::Value::as_str) {
        if let Ok(at) = chrono::DateTime::parse_from_rfc3339(revoked_at) {
            if at.with_timezone(&chrono::Utc) <= now {
                return false;
            }
        }
    }
    if let Some(expires_at) = config.get("expiresAt").and_then(serde_json::Value::as_str) {
        if let Ok(at) = chrono::DateTime::parse_from_rfc3339(expires_at) {
            if at.with_timezone(&chrono::Utc) <= now {
                return false;
            }
        }
    }
    true
}

/// Whether the trust rule needs re-review: catalog quarantined/removed, or the
/// configured version/schema hash no longer matches the live catalog entry.
///
/// Port of Paperclip `trustRuleNeedsReview`.
pub fn trust_rule_needs_review(
    config: Option<&serde_json::Value>,
    catalog_status: Option<&str>,
    catalog_version_hash: Option<&str>,
    catalog_schema_hash: Option<&str>,
) -> bool {
    let Some(config) = config else {
        return false;
    };
    let rule_version = config.get("catalogVersionHash").and_then(serde_json::Value::as_str);
    let rule_schema = config.get("schemaHash").and_then(serde_json::Value::as_str);
    matches!(catalog_status, Some("quarantined") | Some("removed"))
        || rule_version.is_some_and(|rv| catalog_version_hash.is_some_and(|cv| rv != cv))
        || rule_schema.is_some_and(|rs| catalog_schema_hash.is_some_and(|cs| rs != cs))
}

/// Rate-limit rule parsed from a policy's `config.rateLimit` (or the config
/// itself). Port of Paperclip `rateLimitRule`: positive integer limit and
/// windowSeconds are required; keyBy strings are passed through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitRule {
    pub limit: i64,
    pub window_seconds: i64,
    pub key_by: Option<Vec<String>>,
}

/// Parse a rate-limit rule from a policy config JSON. Returns `None` for
/// invalid rules (Paperclip treats those as deny_policy_block at runtime).
pub fn rate_limit_rule(config: &serde_json::Value) -> Option<RateLimitRule> {
    let raw = config
        .get("rateLimit")
        .filter(|value| value.is_object())
        .unwrap_or(config);
    let limit = raw.get("limit").and_then(serde_json::Value::as_i64)?;
    let window_seconds = raw.get("windowSeconds").and_then(serde_json::Value::as_i64)?;
    if limit <= 0 || window_seconds <= 0 {
        return None;
    }
    let key_by = raw
        .get("keyBy")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        });
    Some(RateLimitRule {
        limit,
        window_seconds,
        key_by,
    })
}

/// Window bucket classification: minute/hour/day/month.
///
/// Port of Paperclip `windowKind`.
pub fn window_kind(window_seconds: i64) -> &'static str {
    if window_seconds <= 60 {
        "minute"
    } else if window_seconds <= 3600 {
        "hour"
    } else if window_seconds <= 86400 {
        "day"
    } else {
        "month"
    }
}

/// Fixed-window start instant.
///
/// Port of Paperclip `windowStart`.
pub fn window_start(now_ms: i64, window_seconds: i64) -> chrono::DateTime<chrono::Utc> {
    let window_ms = window_seconds * 1000;
    let start_ms = (now_ms / window_ms) * window_ms;
    chrono::DateTime::from_timestamp_millis(start_ms).unwrap_or_else(|| chrono::Utc::now())
}

/// Bucket identity components for a rate-limit counter.
#[derive(Debug, Clone, Default)]
pub struct RateLimitContext {
    pub company_id: String,
    pub agent_id: Option<String>,
    pub application_id: Option<String>,
    pub connection_id: Option<String>,
    pub tool_name: String,
}

/// Bucket key: the keyBy dimensions joined with `|`, defaulting to
/// company/agent/connection/tool. Port of Paperclip `rateBucket`.
pub fn rate_bucket(rule: &RateLimitRule, ctx: &RateLimitContext) -> String {
    let default_parts = ["company", "agent", "connection", "tool"];
    let parts: &[String] = rule.key_by.as_deref().unwrap_or(&[]);
    let owned_default: Vec<String> = default_parts.iter().map(|s| s.to_string()).collect();
    let parts: Vec<String> = if parts.is_empty() {
        owned_default
    } else {
        parts.to_vec()
    };
    parts
        .iter()
        .map(|part| match part.as_str() {
            "company" => format!("company:{}", ctx.company_id),
            "agent" => format!("agent:{}", ctx.agent_id.as_deref().unwrap_or("none")),
            "application" => {
                format!("application:{}", ctx.application_id.as_deref().unwrap_or("none"))
            }
            "connection" => {
                format!("connection:{}", ctx.connection_id.as_deref().unwrap_or("none"))
            }
            _ => format!("tool:{}", ctx.tool_name),
        })
        .collect::<Vec<_>>()
        .join("|")
}
/// Atomic fixed-window rate-limit consumption over tool_rate_limit_counters.
///
/// Port of Paperclip `enforceRateLimit` (consume=true branch): INSERT ... ON
/// CONFLICT decrements remaining, guarded by remaining > 0; no row returned
/// means the bucket is exhausted.
/// Paperclip `enforceRateLimit` state, serialized for the 429 response body.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RateLimitState {
    pub limited: bool,
    pub count: i64,
    pub limit: i64,
    pub window_seconds: i64,
    pub bucket_key: String,
}

/// Paperclip `enforceRateLimit`: `consume=false` observes without taking a
/// token; `consume=true` takes a token atomically. Returns the full state.
pub async fn enforce_rate_limit_full(
    pool: &sqlx::PgPool,
    company_id: Uuid,
    policy_id: &Uuid,
    bucket_key: &str,
    rule: &RateLimitRule,
    now: chrono::DateTime<chrono::Utc>,
    consume: bool,
) -> Result<RateLimitState, sqlx::Error> {
    if !consume {
        let counter_key = format!("{policy_id}:{bucket_key}");
        let row: Option<(i32, i32)> = sqlx::query_as(
            "SELECT \"limit\", remaining FROM tool_rate_limit_counters \
             WHERE company_id = $1 AND policy_id = $2 AND counter_key = $3 \
               AND window_kind = $4 AND window_start_at = $5",
        )
        .bind(company_id)
        .bind(policy_id)
        .bind(&counter_key)
        .bind(window_kind(rule.window_seconds))
        .bind(window_start(now.timestamp_millis(), rule.window_seconds))
        .fetch_optional(pool)
        .await?;
        let (limit, remaining) = row.unwrap_or((rule.limit as i32, rule.limit as i32));
        let count = (limit as i64 - remaining as i64).max(0);
        return Ok(RateLimitState {
            limited: count >= rule.limit,
            count,
            limit: rule.limit,
            window_seconds: rule.window_seconds,
            bucket_key: bucket_key.to_string(),
        });
    }
    let kind = window_kind(rule.window_seconds);
    let start = window_start(
        now.timestamp_millis(),
        rule.window_seconds,
    );
    let reset_at = start + chrono::Duration::seconds(rule.window_seconds);
    let counter_key = format!("{policy_id}:{bucket_key}");
    let updated: Option<i32> = sqlx::query_scalar(
        "INSERT INTO tool_rate_limit_counters
            (id, company_id, policy_id, counter_key, scope_type, scope_id, window_kind, window_start_at, \"limit\", remaining, reset_at)
         VALUES (gen_random_uuid(), $1, $2, $3, 'policy', $4, $5, $6, $7, $8, $9)
         ON CONFLICT (company_id, policy_id, counter_key, window_kind, window_start_at)
         DO UPDATE SET
            remaining = GREATEST(0, LEAST(tool_rate_limit_counters.remaining, $7) - 1),
            reset_at = $9,
            updated_at = NOW()
         WHERE tool_rate_limit_counters.remaining > 0
         RETURNING remaining",
    )
    .bind(company_id)
    .bind(policy_id)
    .bind(&counter_key)
    .bind(policy_id.to_string())
    .bind(kind)
    .bind(start)
    .bind(rule.limit as i32)
    .bind((rule.limit - 1) as i32)
    .bind(reset_at)
    .fetch_optional(pool)
    .await?;
    match updated {
        None => Ok(RateLimitState {
            limited: true,
            count: rule.limit,
            limit: rule.limit,
            window_seconds: rule.window_seconds,
            bucket_key: bucket_key.to_string(),
        }),
        Some(remaining) => Ok(RateLimitState {
            limited: false,
            count: (rule.limit - remaining as i64).max(0),
            limit: rule.limit,
            window_seconds: rule.window_seconds,
            bucket_key: bucket_key.to_string(),
        }),
    }
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

    #[test]
    fn gateway_issuance_outcomes_are_canonical() {
        // Outcomes produced by ConnectionTokenExchangeError constructors and
        // direct call sites in api/src/routes/tool_access.rs.
        for outcome in [
            "success",
            "denied",
            "use_env_lease",
            "upstream_error",
            "failure",
            "rate_limited", // canonical, reserved for the limited path
        ] {
            assert!(
                CONNECTION_TOKEN_ISSUANCE_OUTCOMES.contains(&outcome),
                "{outcome} must be canonical"
            );
        }
        // Paths emitted by connection_token_path are exactly the canonical set.
        for path in ["exchange", "oauth_access", "static"] {
            assert!(CONNECTION_TOKEN_ISSUANCE_PATHS.contains(&path));
        }
    }

/// Canonical JSON key order: collect every recursively nested key name.
///
/// Port of Paperclip `flattenKeys`.

    #[test]
    fn stable_hash_is_key_order_independent() {
        let a = serde_json::json!({"b": 1, "a": {"y": 2, "x": 3}});
        let b = serde_json::json!({"a": {"x": 3, "y": 2}, "b": 1});
        assert_eq!(stable_hash(&a), stable_hash(&b));
        let c = serde_json::json!({"b": 1, "a": {"y": 2, "x": 4}});
        assert_ne!(stable_hash(&a), stable_hash(&c));
    }

    #[test]
    fn descriptor_hash_changes_when_schema_changes() {
        let schema1 = serde_json::json!({"type": "object", "properties": {"key": {"type": "string"}}});
        let schema2 = serde_json::json!({"type": "object", "properties": {"key": {"type": "number"}}});
        let h1 = descriptor_hash("kv_get", Some("Get"), None, &schema1, &serde_json::json!({}), "low");
        let h2 = descriptor_hash("kv_get", Some("Get"), None, &schema2, &serde_json::json!({}), "low");
        assert_ne!(h1, h2, "schema change must change version hash");
        // Identical descriptor (key order shuffled inside schema) keeps the hash.
        let schema3 = serde_json::json!({"properties": {"key": {"type": "string"}}, "type": "object"});
        let h3 = descriptor_hash("kv_get", Some("Get"), None, &schema3, &serde_json::json!({}), "low");
        assert_eq!(h1, h3, "key-order-only change must NOT change the hash");
        assert_eq!(h1, descriptor_hash("kv_get", Some("Get"), None, &schema1, &serde_json::json!({}), "low"));
    }

    #[test]
    fn schema_hash_tracks_schema_only() {
        let schema = serde_json::json!({"type": "object"});
        assert_eq!(schema_hash(&schema), schema_hash(&serde_json::json!({"type": "object"})));
        assert_ne!(schema_hash(&schema), schema_hash(&serde_json::json!({"type": "string"})));
        assert_eq!(schema_hash(&serde_json::json!({})).len(), 64);
    }

    #[test]
    fn risk_classification_matches_annotations_then_names() {
        let ann = |s: &str, d: bool| serde_json::json!({s: d});
        assert_eq!(classify_risk("delete_thing", &ann("destructiveHint", true)), "destructive");
        assert_eq!(classify_risk("get_thing", &ann("readOnlyHint", true)), "read");
        assert_eq!(classify_risk("create_thing", &serde_json::json!({})), "write");
        assert_eq!(classify_risk("remove_thing", &serde_json::json!({})), "destructive");
        assert_eq!(classify_risk("do_thing", &serde_json::json!({})), "medium");
    }

    #[test]
    fn decision_ladder_follows_paperclip_precedence() {
        let policy = |id: &str, kind: &str| PolicySpec {
            id: id.into(),
            policy_type: kind.into(),
            selector_tool_name: None,
            description: None,
            trust_rule_config: if kind == "trust_rule" {
                // A live rule always carries argumentFilters (Paperclip rules
                // use allowAny when unrestricted); without them the ladder
                // falls through, matching argumentFiltersMatch(undefined)=false.
                Some(serde_json::json!({"argumentFilters": {"allowAny": true}}))
            } else {
                None
            },
            rate_limit_exceeded: false,
        };
        let ctx = EvaluationContext::default();
        // block beats a later allow.
        let out = decide_tool_access(&[policy("p1", "block"), policy("p2", "allow")], &ctx);
        assert_eq!(out.decision, "deny");
        assert_eq!(out.reason_code, "deny_policy_block");
        // allow beats a later require_approval.
        let out = decide_tool_access(&[policy("p1", "allow"), policy("p2", "require_approval")], &ctx);
        assert_eq!(out.decision, "allow");
        assert_eq!(out.reason_code, "allow_policy");
        // require_approval beats a later allow.
        let out = decide_tool_access(&[policy("p1", "require_approval"), policy("p2", "allow")], &ctx);
        assert_eq!(out.decision, "require_approval");
        assert_eq!(out.reason_code, "requires_approval_policy");
        // rate_limit only limits when exceeded; otherwise falls through.
        let mut limited = policy("p1", "rate_limit");
        limited.rate_limit_exceeded = true;
        let out = decide_tool_access(&[limited, policy("p2", "allow")], &ctx);
        assert_eq!(out.decision, "rate_limited");
        assert_eq!(out.reason_code, "rate_limited");
        // trust rule needs review -> require_approval.
        let mut review = policy("p1", "trust_rule");
        review.trust_rule_config = Some(serde_json::json!({
            "argumentFilters": {"allowAny": true},
            "catalogVersionHash": "v1"
        }));
        let ctx_review = EvaluationContext {
            catalog_status: Some("active".into()),
            catalog_version_hash: Some("v2".into()),
            ..EvaluationContext::default()
        };
        let out = decide_tool_access(&[review], &ctx_review);
        assert_eq!(out.decision, "require_approval");
        assert_eq!(out.reason_code, "requires_review_changed_tool");
        // active trust rule -> allow.
        let out = decide_tool_access(&[policy("p1", "trust_rule")], &ctx);
        assert_eq!(out.decision, "allow");
        assert_eq!(out.reason_code, "allow_trust_rule");
        // inactive trust rule falls through to default deny.
        let mut inactive = policy("p1", "trust_rule");
        inactive.trust_rule_config = None;
        let out = decide_tool_access(&[inactive], &ctx);
        assert_eq!(out.decision, "deny");
        assert_eq!(out.reason_code, "deny_default");
    }

    #[test]
    fn grant_profile_and_default_deny_fallbacks() {
        let ctx = EvaluationContext { explicit_grant: true, profile_allows: true, ..Default::default() };
        // Grant outranks profile.
        let out = decide_tool_access(&[], &ctx);
        assert_eq!(out.reason_code, "allow_explicit_grant");
        let out = decide_tool_access(&[], &EvaluationContext { profile_allows: true, ..Default::default() });
        assert_eq!(out.reason_code, "allow_profile");
        let out = decide_tool_access(&[], &EvaluationContext::default());
        assert_eq!((out.decision, out.reason_code), ("deny", "deny_default"));
    }

    #[test]
    fn selectors_narrow_policy_scope() {
        let mut scoped = PolicySpec::simple("scoped", "block");
        scoped.selector_tool_name = Some("other_tool".into());
        let ctx = EvaluationContext { tool_name: "my_tool".into(), ..Default::default() };
        // Selector does not match -> policy skipped -> default deny.
        let out = decide_tool_access(&[scoped], &ctx);
        assert_eq!(out.reason_code, "deny_default");
    }

    #[test]
    fn stable_stringify_sorts_keys_recursively() {
        let a = serde_json::json!({"b": 1, "a": {"x": 3, "y": [1, {"z": 2}]}});
        let b = serde_json::json!({"a": {"y": [1, {"z": 2}], "x": 3}, "b": 1});
        assert_eq!(stable_stringify(&a), stable_stringify(&b));
        assert_eq!(stable_stringify(&serde_json::json!(null)), "null");
        assert_eq!(stable_stringify(&serde_json::json!(true)), "true");
    }

    #[test]
    fn arguments_hash_is_stable_and_hex() {
        let args = serde_json::json!({"key": "demo/launch", "limit": 5});
        let h1 = arguments_hash(&args);
        let h2 = arguments_hash(&serde_json::json!({"limit": 5, "key": "demo/launch"}));
        assert_eq!(h1, h2, "key order must not change the hash");
        assert_eq!(h1.len(), 64);
        assert_ne!(h1, arguments_hash(&serde_json::json!({"key": "other"})));
    }

    #[test]
    fn read_path_supports_nested_and_array_segments() {
        let v = serde_json::json!({"a": {"b": [{"c": 1}, {"c": 2}]}});
        assert_eq!(read_path(&v, "a.b.1.c"), Some(&serde_json::json!(2)));
        assert_eq!(read_path(&v, "a.b.5.c"), None);
        assert_eq!(read_path(&v, ""), None);
    }

    #[test]
    fn argument_filters_full_matrix() {
        let args = serde_json::json!({"env": "prod", "limit": 10, "note": "deploy on friday"});
        let h = arguments_hash(&args);

        let f = TrustRuleArgumentFilters { allow_any: Some(true), ..Default::default() };
        assert!(argument_filters_match(Some(&f), &args, &h));
        assert!(!argument_filters_match(None, &args, &h));
        assert!(!argument_filters_match(Some(&TrustRuleArgumentFilters::default()), &args, &h));

        let f = TrustRuleArgumentFilters { exact_hash: Some(h.clone()), ..Default::default() };
        assert!(argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { exact_hash: Some("deadbeef".into()), ..Default::default() };
        assert!(!argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { allowed_hashes: Some(vec!["x".into(), h.clone()]), ..Default::default() };
        assert!(argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { allowed_hashes: Some(vec!["x".into()]), ..Default::default() };
        assert!(!argument_filters_match(Some(&f), &args, &h));

        let f = TrustRuleArgumentFilters { field_equals: Some([("env".into(), serde_json::json!("prod"))].into_iter().collect()), ..Default::default() };
        assert!(argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { field_equals: Some([("env".into(), serde_json::json!("dev"))].into_iter().collect()), ..Default::default() };
        assert!(!argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { field_not_equals: Some([("env".into(), serde_json::json!("dev"))].into_iter().collect()), ..Default::default() };
        assert!(argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { field_in: Some([("limit".into(), vec![serde_json::json!(5), serde_json::json!(10)])].into_iter().collect()), ..Default::default() };
        assert!(argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { field_in: Some([("limit".into(), vec![serde_json::json!(5)])].into_iter().collect()), ..Default::default() };
        assert!(!argument_filters_match(Some(&f), &args, &h));

        let f = TrustRuleArgumentFilters { field_matches: Some([("note".into(), "deploy".into())].into_iter().collect()), ..Default::default() };
        assert!(argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { field_matches: Some([("note".into(), "^deploy on (monday|friday)$".into())].into_iter().collect()), ..Default::default() };
        assert!(argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { field_matches: Some([("note".into(), "^deploy on monday$".into())].into_iter().collect()), ..Default::default() };
        assert!(!argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { field_matches: Some([("note".into(), "((".into())].into_iter().collect()), ..Default::default() };
        assert!(!argument_filters_match(Some(&f), &args, &h), "invalid regex must fail closed");
        let f = TrustRuleArgumentFilters { field_matches: Some([("limit".into(), "10".into())].into_iter().collect()), ..Default::default() };
        assert!(!argument_filters_match(Some(&f), &args, &h), "non-string target must fail");

        let f = TrustRuleArgumentFilters { field_exists: Some(vec!["env".into()]), ..Default::default() };
        assert!(argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { field_exists: Some(vec!["env.deep".into()]), ..Default::default() };
        assert!(!argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { field_absent: Some(vec!["missing".into()]), ..Default::default() };
        assert!(argument_filters_match(Some(&f), &args, &h));
        let f = TrustRuleArgumentFilters { field_absent: Some(vec!["env".into()]), ..Default::default() };
        assert!(!argument_filters_match(Some(&f), &args, &h));

        let f = TrustRuleArgumentFilters {
            allow_any: Some(true),
            exact_hash: Some("mismatch".into()),
            ..Default::default()
        };
        assert!(argument_filters_match(Some(&f), &args, &h), "allowAny overrides");
    }

    #[test]
    fn trust_rule_liveness_and_review() {
        let active = serde_json::json!({"catalogVersionHash": "v1"});
        let now = chrono::Utc::now();
        assert!(trust_rule_is_active(Some(&active), now));
        let revoked = serde_json::json!({"revokedAt": "2020-01-01T00:00:00Z"});
        assert!(!trust_rule_is_active(Some(&revoked), now));
        let expired = serde_json::json!({"expiresAt": "2020-01-01T00:00:00Z"});
        assert!(!trust_rule_is_active(Some(&expired), now));
        let future = serde_json::json!({"expiresAt": "2999-01-01T00:00:00Z"});
        assert!(trust_rule_is_active(Some(&future), now));
        assert!(!trust_rule_is_active(None, now));

        assert!(trust_rule_needs_review(Some(&active), Some("quarantined"), Some("v1"), None));
        assert!(trust_rule_needs_review(Some(&active), Some("removed"), Some("v1"), None));
        assert!(trust_rule_needs_review(Some(&active), Some("active"), Some("v2"), None));
        assert!(!trust_rule_needs_review(Some(&active), Some("active"), Some("v1"), None));
        let with_schema = serde_json::json!({"schemaHash": "s1"});
        assert!(trust_rule_needs_review(Some(&with_schema), Some("active"), Some("v1"), Some("s2")));
        assert!(!trust_rule_needs_review(Some(&with_schema), Some("active"), Some("v1"), Some("s1")));
        assert!(!trust_rule_needs_review(None, Some("quarantined"), None, None));
    }

    #[test]
    fn rate_limit_rule_parses_and_rejects_invalid() {
        let config = serde_json::json!({"rateLimit": {"limit": 100, "windowSeconds": 3600, "keyBy": ["company", "tool"]}});
        let rule = rate_limit_rule(&config).expect("valid rule");
        assert_eq!(rule.limit, 100);
        assert_eq!(rule.window_seconds, 3600);
        assert_eq!(rule.key_by.as_deref().map(<[String]>::len), Some(2));

        // Top-level config also accepted (no rateLimit wrapper).
        let flat = serde_json::json!({"limit": 5, "windowSeconds": 60});
        assert!(rate_limit_rule(&flat).is_some());
        // Missing/zero/negative fields -> None (fail closed at runtime).
        assert!(rate_limit_rule(&serde_json::json!({"windowSeconds": 60})).is_none());
        assert!(rate_limit_rule(&serde_json::json!({"limit": 0, "windowSeconds": 60})).is_none());
        assert!(rate_limit_rule(&serde_json::json!({"limit": 5, "windowSeconds": -1})).is_none());
        assert!(rate_limit_rule(&serde_json::json!({})).is_none());
    }

    #[test]
    fn window_kind_classification_matches_paperclip() {
        assert_eq!(window_kind(60), "minute");
        assert_eq!(window_kind(61), "hour");
        assert_eq!(window_kind(3600), "hour");
        assert_eq!(window_kind(3601), "day");
        assert_eq!(window_kind(86400), "day");
        assert_eq!(window_kind(86401), "month");
    }

    #[test]
    fn window_start_aligns_to_epoch_buckets() {
        // 90s window: t=125s -> bucket starts at 90s.
        let start = window_start(125_000, 90);
        assert_eq!(start.timestamp_millis(), 90_000);
        // Fixed windows are deterministic.
        assert_eq!(window_start(179_999, 90), window_start(90_000, 90));
        assert_ne!(window_start(180_000, 90), window_start(179_999, 90));
    }

    #[test]
    fn rate_bucket_uses_keyby_or_default_dimensions() {
        let ctx = RateLimitContext {
            company_id: "co".into(),
            agent_id: Some("ag".into()),
            application_id: Some("app".into()),
            connection_id: Some("conn".into()),
            tool_name: "kv_get".into(),
        };
        let rule = RateLimitRule { limit: 1, window_seconds: 60, key_by: Some(vec!["company".into(), "tool".into()]) };
        assert_eq!(rate_bucket(&rule, &ctx), "company:co|tool:kv_get");
        // Default: company|agent|connection|tool.
        let rule = RateLimitRule { limit: 1, window_seconds: 60, key_by: None };
        assert_eq!(
            rate_bucket(&rule, &ctx),
            "company:co|agent:ag|connection:conn|tool:kv_get"
        );
        // Unknown dimension falls to tool.
        let rule = RateLimitRule { limit: 1, window_seconds: 60, key_by: Some(vec!["weird".into()]) };
        assert_eq!(rate_bucket(&rule, &ctx), "tool:kv_get");
    }
}
