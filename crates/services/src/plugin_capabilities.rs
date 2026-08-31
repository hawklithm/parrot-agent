//! Canonical plugin capability model — Paperclip `PLUGIN_SPEC` §15 / `@paperclipai/shared`
//! `PLUGIN_CAPABILITIES` (`packages/shared/src/constants.ts`).
//!
//! Paperclip grants plugins *named permissions*, not filesystem/network/command
//! allowlists. Plugins declare the capabilities they need in their manifest and
//! the host enforces them at runtime. This module is the Parrot-local port of
//! that contract so plugin manifests, the capability validator, and the host
//! bridge all agree on one vocabulary.
//!
//! Groups: Data Read, Data Write, Plugin State, Runtime/Integration, Agent
//! Tools, UI.
//!
//! Unknown capability strings from newer Paperclip plugins are preserved via
//! `PluginCapability::Other` so a manifest declaring a capability this host
//! does not know is rejected as *unknown* rather than silently accepted.

use serde::{Deserialize, Serialize};

/// The canonical capability strings, in Paperclip's declaration order.
pub const PLUGIN_CAPABILITIES: &[&str] = &[
    // Data Read
    "companies.read",
    "projects.read",
    "project.workspaces.read",
    "execution.workspaces.read",
    "issues.read",
    "issue.relations.read",
    "issue.subtree.read",
    "issue.comments.read",
    "issue.interactions.read",
    "issue.attachments.read",
    "approvals.read",
    "issue.documents.read",
    "agents.read",
    "goals.read",
    "goals.create",
    "goals.update",
    "activity.read",
    "costs.read",
    "issues.orchestration.read",
    "access.members.read",
    "access.invites.read",
    "authorization.grants.read",
    "authorization.policies.read",
    "authorization.audit.read",
    "database.namespace.read",
    // Data Write
    "issues.create",
    "issues.update",
    "issue.relations.write",
    "issues.checkout",
    "issues.wakeup",
    "issue.comments.create",
    "issue.comments.create_human_attributed",
    "issue.interactions.create",
    "issue.interactions.respond",
    "approvals.respond",
    "issue.documents.write",
    "projects.managed",
    "routines.managed",
    "skills.managed",
    "agents.pause",
    "agents.resume",
    "agents.invoke",
    "agents.managed",
    "access.members.write",
    "access.invites.write",
    "authorization.grants.write",
    "authorization.policies.write",
    "agent.sessions.create",
    "agent.sessions.list",
    "agent.sessions.send",
    "agent.sessions.close",
    "activity.log.write",
    "metrics.write",
    "telemetry.track",
    "database.namespace.migrate",
    "database.namespace.write",
    "external.objects.detect",
    "external.objects.read",
    "external.objects.write",
    "external.objects.refresh",
    // Plugin State
    "plugin.state.read",
    "plugin.state.write",
    // Runtime / Integration
    "events.subscribe",
    "events.emit",
    "jobs.schedule",
    "webhooks.receive",
    "api.routes.register",
    "http.outbound",
    "secrets.read-ref",
    "environment.drivers.register",
    "local.folders",
    // Agent Tools
    "agent.tools.register",
    // UI
    "instance.settings.register",
    "ui.sidebar.register",
    "ui.page.register",
    "ui.detailTab.register",
    "ui.dashboardWidget.register",
    "ui.commentAnnotation.register",
    "ui.action.register",
];

/// Errors from capability resolution/validation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CapabilityError {
    #[error("unknown plugin capability: {0}")]
    UnknownCapability(String),
    #[error("operation '{0}' requires undeclared capability '{1}'")]
    MissingCapability(String, String),
    #[error("invalid tool declaration: {0}")]
    InvalidTool(String),
}

/// Parse a capability string into its canonical form, rejecting unknown values.
pub fn parse_capability(raw: &str) -> Result<String, CapabilityError> {
    if PLUGIN_CAPABILITIES.contains(&raw) {
        Ok(raw.to_string())
    } else {
        Err(CapabilityError::UnknownCapability(raw.to_string()))
    }
}

/// Whether `granted` contains the capability required by `op`.
pub fn has_capability(granted: &[String], required: &str) -> bool {
    granted.iter().any(|c| c == required)
}

/// Declares an agent tool contributed by the plugin. Tools are namespaced by
/// plugin ID at runtime (e.g. `linear:search-issues`).
///
/// Port of `@paperclipai/shared` `PluginToolDeclaration` (PLUGIN_SPEC §11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginToolDeclaration {
    /// Tool name, unique within the plugin. Namespaced by plugin ID at runtime.
    pub name: String,
    /// Human-readable name shown to agents and in the UI.
    pub display_name: String,
    /// Description provided to the agent so it knows when to use this tool.
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    pub parameters_schema: serde_json::Value,
}

/// Validate a tool declaration against a plugin's granted capabilities.
///
/// Paperclip requires `agent.tools.register` for any tool contribution
/// (PLUGIN_SPEC §11); a plugin that declares tools without it is invalid.
pub fn validate_tool_declaration(
    tool: &PluginToolDeclaration,
    granted: &[String],
) -> Result<(), CapabilityError> {
    if tool.name.trim().is_empty() {
        return Err(CapabilityError::InvalidTool("name is required".into()));
    }
    if !tool
        .name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
    {
        return Err(CapabilityError::InvalidTool(format!(
            "invalid tool name: {}",
            tool.name
        )));
    }
    if tool.display_name.trim().is_empty() {
        return Err(CapabilityError::InvalidTool(
            "displayName is required".into(),
        ));
    }
    if tool.description.trim().is_empty() {
        return Err(CapabilityError::InvalidTool("description is required".into()));
    }
    if !has_capability(granted, "agent.tools.register") {
        return Err(CapabilityError::MissingCapability(
            format!("tools.register:{}", tool.name),
            "agent.tools.register".into(),
        ));
    }
    Ok(())
}

/// The capability a host-bridge operation requires.
///
/// Centralizes the operation→capability mapping so the bridge and the
/// capability validator cannot drift apart.
pub fn required_capability(op: &str) -> Option<&'static str> {
    let cap = match op {
        "issues.list" | "issues.get" => "issues.read",
        "issues.create" => "issues.create",
        "issues.update" => "issues.update",
        "issues.checkout" => "issues.checkout",
        "issues.wakeup" => "issues.wakeup",
        "issue.comments.list" => "issue.comments.read",
        "issue.comments.create" => "issue.comments.create",
        "issue.documents.read" => "issue.documents.read",
        "issue.documents.write" => "issue.documents.write",
        "issue.attachments.read" => "issue.attachments.read",
        "issue.interactions.read" => "issue.interactions.read",
        "issue.interactions.create" => "issue.interactions.create",
        "issue.interactions.respond" => "issue.interactions.respond",
        "issue.relations.read" => "issue.relations.read",
        "issue.relations.write" => "issue.relations.write",
        "issue.subtree.read" => "issue.subtree.read",
        "approvals.read" => "approvals.read",
        "approvals.respond" => "approvals.respond",
        "agents.read" => "agents.read",
        "agents.invoke" => "agents.invoke",
        "agents.pause" => "agents.pause",
        "agents.resume" => "agents.resume",
        "goals.read" => "goals.read",
        "goals.create" => "goals.create",
        "goals.update" => "goals.update",
        "activity.read" => "activity.read",
        "activity.log.write" => "activity.log.write",
        "costs.read" => "costs.read",
        "plugin.state.read" => "plugin.state.read",
        "plugin.state.write" => "plugin.state.write",
        "events.emit" => "events.emit",
        "http.outbound" => "http.outbound",
        "secrets.read-ref" => "secrets.read-ref",
        "database.namespace.read" => "database.namespace.read",
        "database.namespace.write" => "database.namespace.write",
        "database.namespace.migrate" => "database.namespace.migrate",
        "external.objects.read" => "external.objects.read",
        "external.objects.write" => "external.objects.write",
        "external.objects.detect" => "external.objects.detect",
        "external.objects.refresh" => "external.objects.refresh",
        _ => return None,
    };
    Some(cap)
}

/// Authorize a host-bridge operation against the plugin's granted capabilities.
pub fn authorize_operation(op: &str, granted: &[String]) -> Result<(), CapabilityError> {
    match required_capability(op) {
        Some(cap) if has_capability(granted, cap) => Ok(()),
        Some(cap) => Err(CapabilityError::MissingCapability(op.into(), cap.into())),
        // Unknown host operations are denied rather than allowed by omission.
        None => Err(CapabilityError::UnknownCapability(op.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> PluginToolDeclaration {
        PluginToolDeclaration {
            name: name.into(),
            display_name: "D".into(),
            description: "desc".into(),
            parameters_schema: serde_json::json!({"type": "object"}),
        }
    }

    #[test]
    fn capability_list_matches_paperclip_ordering() {
        // 79 canonical capabilities, in Paperclip's declaration order
        // (packages/shared/src/constants.ts PLUGIN_CAPABILITIES).
        assert_eq!(PLUGIN_CAPABILITIES.len(), 79);
        assert_eq!(PLUGIN_CAPABILITIES.first().copied(), Some("companies.read"));
        assert_eq!(PLUGIN_CAPABILITIES.last().copied(), Some("ui.action.register"));
    }

    #[test]
    fn capability_set_is_exact_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for cap in PLUGIN_CAPABILITIES {
            assert!(seen.insert(*cap), "duplicate capability: {cap}");
            // Every entry must be a known capability for the parser.
            assert_eq!(parse_capability(cap).unwrap(), *cap);
        }
        // Spelled exactly like Paperclip, including the non-`[a-z.]` names.
        assert!(PLUGIN_CAPABILITIES.contains(&"issue.comments.create_human_attributed"));
        assert!(PLUGIN_CAPABILITIES.contains(&"secrets.read-ref"));
        assert!(PLUGIN_CAPABILITIES.contains(&"ui.detailTab.register"));
        // A near-miss that is NOT a real capability must be rejected.
        assert!(parse_capability("issue.comments.create_human").is_err());
    }

    #[test]
    fn parse_capability_accepts_known_and_rejects_unknown() {
        assert_eq!(parse_capability("issues.read").unwrap(), "issues.read");
        assert_eq!(
            parse_capability("nope.thing"),
            Err(CapabilityError::UnknownCapability("nope.thing".into()))
        );
    }

    #[test]
    fn tool_declaration_requires_agent_tools_register() {
        let t = tool("search");
        assert_eq!(
            validate_tool_declaration(&t, &[]),
            Err(CapabilityError::MissingCapability(
                "tools.register:search".into(),
                "agent.tools.register".into()
            ))
        );
        assert!(validate_tool_declaration(&t, &["agent.tools.register".into()]).is_ok());
    }

    #[test]
    fn tool_declaration_rejects_blank_fields() {
        let mut t = tool("search");
        t.description = "  ".into();
        assert!(validate_tool_declaration(&t, &["agent.tools.register".into()]).is_err());
        let mut t2 = tool("bad name");
        t2.display_name = "D".into();
        assert!(validate_tool_declaration(&t2, &["agent.tools.register".into()]).is_err());
    }

    #[test]
    fn authorize_operation_maps_and_enforces() {
        let granted = vec!["issues.read".to_string()];
        assert!(authorize_operation("issues.list", &granted).is_ok());
        assert_eq!(
            authorize_operation("issues.create", &granted),
            Err(CapabilityError::MissingCapability(
                "issues.create".into(),
                "issues.create".into()
            ))
        );
    }

    #[test]
    fn unknown_operation_is_denied_not_allowed() {
        assert!(authorize_operation("totally.unknown", &["issues.read".into()]).is_err());
    }

    #[test]
    fn tool_declaration_round_trips() {
        let t = tool("linear:search");
        let back: PluginToolDeclaration =
            serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(back, t);
        assert_eq!(back.name, "linear:search");
    }
}
