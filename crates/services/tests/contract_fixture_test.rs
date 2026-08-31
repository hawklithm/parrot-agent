
//! Rust↔TS bidirectional contract test (plan §6B.1 item 684).
//!
//! Writes this crate's canonical contract tables to a JSON fixture consumed
//! by the frontend Vitest suite
//! (`parrot-web-ui/src/lib/paperclip-shared/src/generated/contract-fixtures.json`).
//! The fixture is committed: the Rust test below fails on any drift from the
//! committed artifact, and the TS test fails if the controlled TS tables ever
//! diverge from it — so both languages are pinned to one artifact.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};

use services::plugin_capabilities::{
    PLUGIN_API_ROUTE_AUTH_MODES, PLUGIN_API_ROUTE_CHECKOUT_POLICIES, PLUGIN_API_ROUTE_METHODS,
    PLUGIN_CAPABILITIES, PLUGIN_CATEGORIES, PLUGIN_DATABASE_CORE_READ_TABLES,
    PLUGIN_UI_SLOT_ENTITY_TYPES, PLUGIN_UI_SLOT_TYPES,
};
use services::tool_access_contract::{
    CONNECTION_TOKEN_ISSUANCE_OUTCOMES, CONNECTION_TOKEN_ISSUANCE_PATHS,
    CONNECTION_RECOVERABLE_ERROR_CODES, TOOL_ACTION_REQUEST_STATUSES,
    TOOL_AUDIT_EVENT_TYPES, TOOL_CATALOG_ENTRY_KINDS, TOOL_CATALOG_ENTRY_STATUSES,
    TOOL_CONNECTION_HEALTH_STATUSES, TOOL_CONNECTION_LIFECYCLE_EVENT_TYPES,
    TOOL_INVOCATION_APPROVAL_STATES, TOOL_INVOCATION_STATUSES, TOOL_MCP_GATEWAY_STATUSES,
    TOOL_POLICY_DECISIONS, TOOL_POLICY_TYPES, TOOL_PROFILE_BINDING_TARGET_TYPES,
    TOOL_PROFILE_DEFAULT_ACTIONS, TOOL_PROFILE_ENTRY_EFFECTS,
    TOOL_PROFILE_ENTRY_SELECTOR_TYPES, TOOL_RATE_LIMIT_WINDOW_KINDS, TOOL_RISK_LEVELS,
    TOOL_RUNTIME_KINDS, TOOL_RUNTIME_SLOT_STATUSES,
};

/// Path of the committed fixture, relative to this crate.
pub const FIXTURE_RELATIVE_PATH: &str =
    "../../../parrot-web-ui/src/lib/paperclip-shared/src/generated/contract-fixtures.json";

fn expected_fixture() -> Value {
    json!({
        "plugin": {
            "PLUGIN_CAPABILITIES": PLUGIN_CAPABILITIES,
            "PLUGIN_CATEGORIES": PLUGIN_CATEGORIES,
            "PLUGIN_DATABASE_CORE_READ_TABLES": PLUGIN_DATABASE_CORE_READ_TABLES,
            "PLUGIN_UI_SLOT_TYPES": PLUGIN_UI_SLOT_TYPES,
            "PLUGIN_UI_SLOT_ENTITY_TYPES": PLUGIN_UI_SLOT_ENTITY_TYPES,
            "PLUGIN_API_ROUTE_METHODS": PLUGIN_API_ROUTE_METHODS,
            "PLUGIN_API_ROUTE_AUTH_MODES": PLUGIN_API_ROUTE_AUTH_MODES,
            "PLUGIN_API_ROUTE_CHECKOUT_POLICIES": PLUGIN_API_ROUTE_CHECKOUT_POLICIES,
        },
        "toolAccess": {
            "TOOL_CONNECTION_HEALTH_STATUSES": TOOL_CONNECTION_HEALTH_STATUSES,
            "CONNECTION_TOKEN_ISSUANCE_PATHS": CONNECTION_TOKEN_ISSUANCE_PATHS,
            "CONNECTION_TOKEN_ISSUANCE_OUTCOMES": CONNECTION_TOKEN_ISSUANCE_OUTCOMES,
            "TOOL_CATALOG_ENTRY_KINDS": TOOL_CATALOG_ENTRY_KINDS,
            "TOOL_CATALOG_ENTRY_STATUSES": TOOL_CATALOG_ENTRY_STATUSES,
            "TOOL_RISK_LEVELS": TOOL_RISK_LEVELS,
            "TOOL_POLICY_TYPES": TOOL_POLICY_TYPES,
            "TOOL_POLICY_DECISIONS": TOOL_POLICY_DECISIONS,
            "TOOL_INVOCATION_STATUSES": TOOL_INVOCATION_STATUSES,
            "TOOL_INVOCATION_APPROVAL_STATES": TOOL_INVOCATION_APPROVAL_STATES,
            "TOOL_ACTION_REQUEST_STATUSES": TOOL_ACTION_REQUEST_STATUSES,
            "TOOL_PROFILE_ENTRY_SELECTOR_TYPES": TOOL_PROFILE_ENTRY_SELECTOR_TYPES,
            "TOOL_PROFILE_ENTRY_EFFECTS": TOOL_PROFILE_ENTRY_EFFECTS,
            "TOOL_PROFILE_BINDING_TARGET_TYPES": TOOL_PROFILE_BINDING_TARGET_TYPES,
            "TOOL_MCP_GATEWAY_STATUSES": TOOL_MCP_GATEWAY_STATUSES,
            "TOOL_RUNTIME_KINDS": TOOL_RUNTIME_KINDS,
            "TOOL_RUNTIME_SLOT_STATUSES": TOOL_RUNTIME_SLOT_STATUSES,
            "TOOL_RATE_LIMIT_WINDOW_KINDS": TOOL_RATE_LIMIT_WINDOW_KINDS,
            "TOOL_AUDIT_EVENT_TYPES": TOOL_AUDIT_EVENT_TYPES,
            "TOOL_CONNECTION_LIFECYCLE_EVENT_TYPES": TOOL_CONNECTION_LIFECYCLE_EVENT_TYPES,
            "CONNECTION_RECOVERABLE_ERROR_CODES": CONNECTION_RECOVERABLE_ERROR_CODES,
        },
    })
}

fn fixture_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push(FIXTURE_RELATIVE_PATH);
    path
}

/// Contract gate: the committed fixture must match the Rust tables exactly.
/// When a table legitimately changes, re-run with
/// `REGENERATE_CONTRACT_FIXTURE=1` to rewrite the artifact, then commit it —
/// the frontend Vitest gate will force the TS side to follow.
#[test]
fn rust_contract_tables_match_committed_fixture() {
    let expected = expected_fixture().to_string();
    let path = fixture_path();

    if std::env::var("REGENERATE_CONTRACT_FIXTURE").as_deref() == Ok("1") {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("{expected}\n")).unwrap();
        return;
    }

    let committed = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "contract fixture missing at {}: {err}. Run with REGENERATE_CONTRACT_FIXTURE=1 to create it.",
            path.display()
        )
    });

    let committed_json: Value = serde_json::from_str(&committed).expect("fixture must be valid JSON");
    assert_eq!(
        committed_json, expected_fixture(),
        "Rust contract tables drifted from the committed fixture; re-run with REGENERATE_CONTRACT_FIXTURE=1 and port the change to the TS side."
    );
}
