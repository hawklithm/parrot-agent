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
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
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

/// Declares a scheduled job contributed by the plugin.
///
/// Port of `@paperclipai/shared` `PluginJobDeclaration` (PLUGIN_SPEC §13.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginJobDeclaration {
    /// Stable identifier for this job, unique within the plugin.
    pub job_key: String,
    /// Human-readable name shown in the operator UI.
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Cron expression for the schedule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
}

/// Declares a webhook endpoint the plugin can receive.
/// Route: `POST /api/plugins/:pluginId/webhooks/:endpointKey`
///
/// Port of `@paperclipai/shared` `PluginWebhookDeclaration` (PLUGIN_SPEC §18).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWebhookDeclaration {
    /// Stable identifier for this endpoint, unique within the plugin.
    pub endpoint_key: String,
    /// Human-readable name shown in the operator UI.
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The subset of `PaperclipPluginManifestV1` (PLUGIN_SPEC §6) this host
/// validates. Unknown/extra manifest keys are ignored so newer Paperclip
/// manifests still load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifestV1 {
    /// Globally unique plugin identifier (e.g. `"acme.linear-sync"`).
    /// Lowercase alphanumeric with dots, hyphens, or underscores.
    pub id: String,
    /// Plugin API version. Must be `1` for the current spec.
    pub api_version: i64,
    /// Semver version of the plugin package (e.g. `"1.2.0"`).
    pub version: String,
    /// Human-readable name (max 100 chars).
    pub display_name: String,
    /// Short description (max 500 chars).
    pub description: String,
    /// Author name (max 200 chars).
    pub author: String,
    /// Capabilities this plugin requires from the host. Enforced at runtime.
    pub capabilities: Vec<String>,
    /// Entrypoint paths relative to the package root.
    pub entrypoints: PluginManifestEntrypoints,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jobs: Option<Vec<PluginJobDeclaration>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhooks: Option<Vec<PluginWebhookDeclaration>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<PluginToolDeclaration>>,
    /// Whether the manifest declares UI slots (requires `entrypoints.ui`).
    #[serde(default)]
    pub declares_ui: bool,
}

/// Entrypoint paths relative to the package root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifestEntrypoints {
    /// Path to the worker entrypoint (required).
    pub worker: String,
    /// Path to the UI bundle directory (required when UI is declared).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<String>,
}

/// Validate a manifest against Paperclip's declaration rules.
///
/// Enforced (PLUGIN_SPEC §6/§11/§13.6/§18):
/// - `apiVersion` must be `1`
/// - `id` must be lowercase alphanumeric with `.`/`-`/`_`
/// - required text fields must be non-empty and within length caps
/// - every capability must be canonical
/// - `tools` requires `agent.tools.register`
/// - `jobs` requires `jobs.schedule`
/// - `webhooks` requires `webhooks.receive`
/// - declared UI requires `entrypoints.ui`
/// - duplicate job keys / webhook keys / tool names are rejected
pub fn validate_manifest(manifest: &PluginManifestV1) -> Result<(), CapabilityError> {
    if manifest.api_version != 1 {
        return Err(CapabilityError::InvalidManifest(format!(
            "unsupported apiVersion: {}",
            manifest.api_version
        )));
    }
    if manifest.id.is_empty()
        || !manifest
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'))
    {
        return Err(CapabilityError::InvalidManifest(format!(
            "invalid plugin id: {}",
            manifest.id
        )));
    }
    if manifest.display_name.is_empty() || manifest.display_name.len() > 100 {
        return Err(CapabilityError::InvalidManifest(
            "displayName must be 1-100 chars".into(),
        ));
    }
    if manifest.description.is_empty() || manifest.description.len() > 500 {
        return Err(CapabilityError::InvalidManifest(
            "description must be 1-500 chars".into(),
        ));
    }
    if manifest.author.is_empty() || manifest.author.len() > 200 {
        return Err(CapabilityError::InvalidManifest(
            "author must be 1-200 chars".into(),
        ));
    }
    if manifest.entrypoints.worker.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "entrypoints.worker is required".into(),
        ));
    }

    for cap in &manifest.capabilities {
        parse_capability(cap)?;
    }

    if let Some(jobs) = &manifest.jobs {
        if !jobs.is_empty() && !has_capability(&manifest.capabilities, "jobs.schedule") {
            return Err(CapabilityError::MissingCapability(
                "jobs".into(),
                "jobs.schedule".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for job in jobs {
            if job.job_key.trim().is_empty() {
                return Err(CapabilityError::InvalidManifest("jobKey is required".into()));
            }
            if !seen.insert(job.job_key.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate jobKey: {}",
                    job.job_key
                )));
            }
        }
    }

    if let Some(webhooks) = &manifest.webhooks {
        if !webhooks.is_empty() && !has_capability(&manifest.capabilities, "webhooks.receive") {
            return Err(CapabilityError::MissingCapability(
                "webhooks".into(),
                "webhooks.receive".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for hook in webhooks {
            if hook.endpoint_key.trim().is_empty() {
                return Err(CapabilityError::InvalidManifest(
                    "endpointKey is required".into(),
                ));
            }
            if !seen.insert(hook.endpoint_key.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate endpointKey: {}",
                    hook.endpoint_key
                )));
            }
        }
    }

    if let Some(tools) = &manifest.tools {
        if !tools.is_empty() && !has_capability(&manifest.capabilities, "agent.tools.register") {
            return Err(CapabilityError::MissingCapability(
                "tools".into(),
                "agent.tools.register".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for tool in tools {
            validate_tool_declaration(tool, &manifest.capabilities)?;
            if !seen.insert(tool.name.clone()) {
                return Err(CapabilityError::InvalidTool(format!(
                    "duplicate tool name: {}",
                    tool.name
                )));
            }
        }
    }

    if manifest.declares_ui && manifest.entrypoints.ui.is_none() {
        return Err(CapabilityError::InvalidManifest(
            "ui declared without entrypoints.ui".into(),
        ));
    }

    Ok(())
}

/// Declares an agent tool contributed by the plugin. Tools are namespaced by
/// plugin ID at runtime (e.g. `linear:search-issues`).
///
/// Port of `@paperclipai/shared` `PluginToolDeclaration` (PLUGIN_SPEC §11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[cfg(test)]
mod manifest_tests {
    use super::*;

    fn manifest(caps: &[&str]) -> PluginManifestV1 {
        PluginManifestV1 {
            id: "acme.linear-sync".into(),
            api_version: 1,
            version: "1.2.0".into(),
            display_name: "Linear Sync".into(),
            description: "Syncs issues".into(),
            author: "Jane Doe".into(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            entrypoints: PluginManifestEntrypoints {
                worker: "dist/worker.js".into(),
                ui: None,
            },
            jobs: None,
            webhooks: None,
            tools: None,
            declares_ui: false,
        }
    }

    #[test]
    fn minimal_manifest_is_valid() {
        assert!(validate_manifest(&manifest(&["issues.read"])).is_ok());
    }

    #[test]
    fn api_version_must_be_one() {
        let mut m = manifest(&[]);
        m.api_version = 2;
        assert!(matches!(
            validate_manifest(&m),
            Err(CapabilityError::InvalidManifest(_))
        ));
    }

    #[test]
    fn plugin_id_must_be_lowercase_alphanumeric() {
        for bad in ["Acme.Thing", "acme thing", "acme/thing", ""] {
            let mut m = manifest(&[]);
            m.id = bad.into();
            assert!(validate_manifest(&m).is_err(), "id {bad:?} must be rejected");
        }
        for good in ["acme.linear-sync", "acme_thing", "acme.thing2"] {
            let mut m = manifest(&[]);
            m.id = good.into();
            assert!(validate_manifest(&m).is_ok(), "id {good:?} must be accepted");
        }
    }

    #[test]
    fn jobs_require_jobs_schedule_capability() {
        let mut m = manifest(&["issues.read"]);
        m.jobs = Some(vec![PluginJobDeclaration {
            job_key: "sync".into(),
            display_name: "Sync".into(),
            description: None,
            schedule: Some("0 * * * *".into()),
        }]);
        assert_eq!(
            validate_manifest(&m),
            Err(CapabilityError::MissingCapability(
                "jobs".into(),
                "jobs.schedule".into()
            ))
        );
        m.capabilities.push("jobs.schedule".into());
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn webhooks_require_webhooks_receive_capability() {
        let mut m = manifest(&[]);
        m.webhooks = Some(vec![PluginWebhookDeclaration {
            endpoint_key: "linear".into(),
            display_name: "Linear".into(),
            description: None,
        }]);
        assert!(validate_manifest(&m).is_err());
        m.capabilities.push("webhooks.receive".into());
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn tools_require_agent_tools_register_in_manifest() {
        let mut m = manifest(&["issues.read"]);
        m.tools = Some(vec![PluginToolDeclaration {
            name: "search".into(),
            display_name: "Search".into(),
            description: "d".into(),
            parameters_schema: serde_json::json!({"type": "object"}),
        }]);
        assert!(validate_manifest(&m).is_err());
        m.capabilities.push("agent.tools.register".into());
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn duplicate_job_and_webhook_keys_are_rejected() {
        let job = PluginJobDeclaration {
            job_key: "sync".into(),
            display_name: "S".into(),
            description: None,
            schedule: None,
        };
        let mut m = manifest(&["jobs.schedule"]);
        m.jobs = Some(vec![job.clone(), job]);
        assert!(validate_manifest(&m).is_err());

        let hook = PluginWebhookDeclaration {
            endpoint_key: "h".into(),
            display_name: "H".into(),
            description: None,
        };
        let mut m2 = manifest(&["webhooks.receive"]);
        m2.webhooks = Some(vec![hook.clone(), hook]);
        assert!(validate_manifest(&m2).is_err());
    }

    #[test]
    fn declared_ui_requires_entrypoints_ui() {
        let mut m = manifest(&[]);
        m.declares_ui = true;
        assert!(validate_manifest(&m).is_err());
        m.entrypoints.ui = Some("dist/ui".into());
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn unknown_manifest_capability_is_rejected() {
        let mut m = manifest(&[]);
        m.capabilities.push("made.up".into());
        assert!(matches!(
            validate_manifest(&m),
            Err(CapabilityError::UnknownCapability(_))
        ));
    }

    #[test]
    fn manifest_round_trips_camel_case() {
        let m = manifest(&["issues.read"]);
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"apiVersion\""), "got: {json}");
        assert!(json.contains("\"displayName\""));
        let back: PluginManifestV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }
}
