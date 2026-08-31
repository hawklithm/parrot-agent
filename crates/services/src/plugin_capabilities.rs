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
    /// Restricted plugin-owned database namespace declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<PluginDatabaseDeclaration>,
    /// UI slot contributions (requires `entrypoints.ui`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ui_slots: Vec<PluginUiSlotDeclaration>,
    /// Scoped JSON API routes mounted under `/api/plugins/:pluginId/api/*` (§20).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_routes: Vec<PluginApiRouteDeclaration>,
    /// External object reference providers this plugin contributes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_references: Vec<PluginObjectReferenceProviderDeclaration>,
    /// Environment drivers this plugin contributes (§: runtime drivers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment_drivers: Vec<PluginEnvironmentDriverDeclaration>,
    /// JSON Schema for operator-editable instance configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_config_schema: Option<serde_json::Value>,
    /// Whether the manifest declares UI (legacy flag; `ui_slots` is preferred).
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

/// Core tables a plugin may read or join at runtime.
///
/// Port of `@paperclipai/shared` `PLUGIN_DATABASE_CORE_READ_TABLES`.
pub const PLUGIN_DATABASE_CORE_READ_TABLES: &[&str] = &[
    "companies",
    "projects",
    "goals",
    "agents",
    "issues",
    "issue_documents",
    "issue_relations",
    "issue_comments",
    "heartbeat_runs",
    "cost_events",
    "approvals",
    "issue_approvals",
    "budget_incidents",
];

/// UI extension slot types (PLUGIN_SPEC §19).
pub const PLUGIN_UI_SLOT_TYPES: &[&str] = &[
    "page",
    "detailTab",
    "taskDetailView",
    "dashboardWidget",
    "sidebar",
    "routeSidebar",
    "sidebarPanel",
    "projectSidebarItem",
    "globalToolbarButton",
    "toolbarButton",
    "contextMenuItem",
    "commentAnnotation",
    "commentContextMenuItem",
    "settingsPage",
    "companySettingsPage",
];

/// Slot types that require `entityTypes` to be declared.
pub const ENTITY_SCOPED_UI_SLOT_TYPES: &[&str] = &["detailTab", "taskDetailView", "contextMenuItem"];

/// Restricted plugin-owned database namespace declaration.
///
/// Port of `@paperclipai/shared` `PluginDatabaseDeclaration` (PLUGIN_SPEC §21.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PluginDatabaseDeclaration {
    /// Optional stable human-readable slug included in the host-derived namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace_slug: Option<String>,
    /// SQL migration directory relative to the plugin package root.
    pub migrations_dir: String,
    /// Public core tables this plugin may read or join at runtime.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub core_read_tables: Vec<String>,
}

/// A UI mount point contributed by the plugin.
///
/// Port of `@paperclipai/shared` `PluginUiSlotDeclaration` (PLUGIN_SPEC §19).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginUiSlotDeclaration {
    /// The type of UI mount point.
    #[serde(rename = "type")]
    pub slot_type: String,
    /// Unique slot identifier within the plugin.
    pub id: String,
    /// Human-readable name shown in navigation or tab labels.
    pub display_name: String,
    /// Which export name in the UI bundle provides this component.
    pub export_name: String,
    /// Entity targets for context-sensitive slots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_types: Vec<String>,
    /// Optional company-scoped route segment for page/routeSidebar/companySettingsPage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_path: Option<String>,
    /// Optional ordering hint; lower numbers appear first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}

/// Validate a database namespace declaration (PLUGIN_SPEC §21.3).
///
/// - `migrationsDir` is required
/// - every `coreReadTables` entry must be a canonical core table
pub fn validate_database_declaration(
    db: &PluginDatabaseDeclaration,
) -> Result<(), CapabilityError> {
    if db.migrations_dir.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "database.migrationsDir is required".into(),
        ));
    }
    for table in &db.core_read_tables {
        if !PLUGIN_DATABASE_CORE_READ_TABLES.contains(&table.as_str()) {
            return Err(CapabilityError::InvalidManifest(format!(
                "unsupported core read table: {table}"
            )));
        }
    }
    Ok(())
}

/// Validate a UI slot declaration (PLUGIN_SPEC §19).
///
/// - `type` must be a canonical slot type
/// - `id`, `displayName`, and `exportName` are required
/// - context-sensitive slots require `entityTypes`
pub fn validate_ui_slot(slot: &PluginUiSlotDeclaration) -> Result<(), CapabilityError> {
    if !PLUGIN_UI_SLOT_TYPES.contains(&slot.slot_type.as_str()) {
        return Err(CapabilityError::InvalidManifest(format!(
            "unsupported UI slot type: {}",
            slot.slot_type
        )));
    }
    if slot.id.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest("ui slot id is required".into()));
    }
    if slot.display_name.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "ui slot displayName is required".into(),
        ));
    }
    if slot.export_name.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "ui slot exportName is required".into(),
        ));
    }
    if ENTITY_SCOPED_UI_SLOT_TYPES.contains(&slot.slot_type.as_str())
        && slot.entity_types.is_empty()
    {
        return Err(CapabilityError::InvalidManifest(format!(
            "ui slot type '{}' requires entityTypes",
            slot.slot_type
        )));
    }
    Ok(())
}

/// HTTP methods a plugin API route may accept (PLUGIN_SPEC §20).
pub const PLUGIN_API_ROUTE_METHODS: &[&str] = &["GET", "POST", "PATCH", "DELETE"];

/// Actor classes allowed to call a plugin API route (PLUGIN_SPEC §20).
pub const PLUGIN_API_ROUTE_AUTH_MODES: &[&str] = &["board", "agent", "board-or-agent", "webhook"];

/// Checkout policies the host enforces before worker dispatch (PLUGIN_SPEC §20).
pub const PLUGIN_API_ROUTE_CHECKOUT_POLICIES: &[&str] = &[
    "none",
    "required-for-agent-in-progress",
    "always-for-agent",
];

/// The capability required to expose a plugin API route (PLUGIN_SPEC §20).
pub const API_ROUTE_CAPABILITY: &str = "api.routes.register";

/// How the host resolves company access for a route (PLUGIN_SPEC §20).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "camelCase")]
pub enum PluginApiRouteCompanyResolution {
    /// Read the company id from a body field.
    Body { key: String },
    /// Read the company id from a query parameter.
    Query { key: String },
    /// Resolve the company from an issue path parameter.
    Issue { param: String },
}

/// Declares a scoped JSON API route mounted under
/// `/api/plugins/:pluginId/api/*`.
///
/// Port of `@paperclipai/shared` `PluginApiRouteDeclaration` (PLUGIN_SPEC §20).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginApiRouteDeclaration {
    /// Stable plugin-defined route key passed to the worker.
    pub route_key: String,
    /// HTTP method accepted by this route.
    pub method: String,
    /// Plugin-local path under `/api/plugins/:pluginId/api`.
    pub path: String,
    /// Actor class allowed to call the route.
    pub auth: String,
    /// Capability required to expose the route.
    pub capability: String,
    /// Optional checkout policy enforced by the host before worker dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkout_policy: Option<String>,
    /// How the host resolves company access for this route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company_resolution: Option<PluginApiRouteCompanyResolution>,
}

/// Validate a plugin API route declaration (PLUGIN_SPEC §20).
///
/// - `routeKey` non-empty and unique (uniqueness checked by the caller)
/// - `method` must be a canonical HTTP method
/// - `path` must be absolute (`/...`)
/// - `auth` must be a canonical auth mode
/// - `capability` must be `api.routes.register`
/// - `checkoutPolicy`, when present, must be canonical
pub fn validate_api_route(route: &PluginApiRouteDeclaration) -> Result<(), CapabilityError> {
    if route.route_key.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "api route routeKey is required".into(),
        ));
    }
    if !PLUGIN_API_ROUTE_METHODS.contains(&route.method.as_str()) {
        return Err(CapabilityError::InvalidManifest(format!(
            "unsupported api route method: {}",
            route.method
        )));
    }
    if !route.path.starts_with('/') {
        return Err(CapabilityError::InvalidManifest(format!(
            "api route path must start with '/': {}",
            route.path
        )));
    }
    if !PLUGIN_API_ROUTE_AUTH_MODES.contains(&route.auth.as_str()) {
        return Err(CapabilityError::InvalidManifest(format!(
            "unsupported api route auth mode: {}",
            route.auth
        )));
    }
    if route.capability != API_ROUTE_CAPABILITY {
        return Err(CapabilityError::InvalidManifest(format!(
            "api route capability must be '{}'",
            API_ROUTE_CAPABILITY
        )));
    }
    if let Some(policy) = &route.checkout_policy {
        if !PLUGIN_API_ROUTE_CHECKOUT_POLICIES.contains(&policy.as_str()) {
            return Err(CapabilityError::InvalidManifest(format!(
                "unsupported api route checkoutPolicy: {policy}"
            )));
        }
    }
    Ok(())
}

/// Optional default refresh behavior for an external object provider.
///
/// Port of `@paperclipai/shared` `PluginObjectReferenceRefreshPolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PluginObjectReferenceRefreshPolicy {
    /// Default freshness window for resolved objects from this provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_ttl_seconds: Option<i64>,
    /// UI-visible staleness window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_after_seconds: Option<i64>,
}

/// Declares an external object reference provider contributed by the plugin.
///
/// Port of `@paperclipai/shared` `PluginObjectReferenceProviderDeclaration`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginObjectReferenceProviderDeclaration {
    /// Stable provider key such as "github", "linear", or "mocktracker".
    pub provider_key: String,
    /// Human-readable provider name shown in operator-facing surfaces.
    pub display_name: String,
    /// Provider object types this plugin can detect and resolve.
    pub object_types: Vec<String>,
    /// Human-readable URL patterns this provider recognizes (metadata only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub url_patterns: Vec<String>,
    /// Optional default refresh behavior for this provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_policy: Option<PluginObjectReferenceRefreshPolicy>,
    /// Webhook endpoint keys declared under `webhooks` that can refresh objects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub webhook_endpoint_keys: Vec<String>,
}

/// Driver classification for a plugin environment driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginEnvironmentDriverKind {
    /// Used by core `driver: "plugin"` environments.
    EnvironmentDriver,
    /// Used by core `driver: "sandbox"` environments whose provider is a plugin.
    SandboxProvider,
}

/// Declares an environment runtime driver contributed by the plugin.
///
/// Port of `@paperclipai/shared` `PluginEnvironmentDriverDeclaration`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEnvironmentDriverDeclaration {
    /// Stable driver key, unique within the plugin.
    pub driver_key: String,
    /// Driver classification; defaults to `environment_driver`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<PluginEnvironmentDriverKind>,
    /// Human-readable name shown in environment configuration UI.
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Sandbox providers must opt in before the host retains/resumes leases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reusable_leases: Option<bool>,
    /// Provider can keep a temporary setup sandbox alive for customization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_interactive_setup: Option<bool>,
    /// Connection types the setup sandbox can expose (initially `ssh`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interactive_setup_connection_types: Vec<String>,
    /// Provider can capture a reusable template from a live setup sandbox.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_template_capture: Option<bool>,
    /// Kind of template reference returned by the provider's capture hook.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_ref_kind: Option<String>,
}

/// Validate an external object reference provider declaration.
///
/// - `providerKey` and `displayName` are required
/// - `objectTypes` must be non-empty (a provider that detects nothing is invalid)
/// - `webhookEndpointKeys` must reference endpoints declared under `webhooks`
pub fn validate_object_reference_provider(
    provider: &PluginObjectReferenceProviderDeclaration,
    declared_webhook_keys: &[String],
) -> Result<(), CapabilityError> {
    if provider.provider_key.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "objectReferences.providerKey is required".into(),
        ));
    }
    if provider.display_name.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "objectReferences.displayName is required".into(),
        ));
    }
    if provider.object_types.is_empty() {
        return Err(CapabilityError::InvalidManifest(format!(
            "objectReferences provider '{}' must declare objectTypes",
            provider.provider_key
        )));
    }
    for key in &provider.webhook_endpoint_keys {
        if !declared_webhook_keys.contains(key) {
            return Err(CapabilityError::InvalidManifest(format!(
                "objectReferences provider '{}' references undeclared webhook endpointKey '{}'",
                provider.provider_key, key
            )));
        }
    }
    Ok(())
}

/// Validate an environment driver declaration.
///
/// - `driverKey` and `displayName` are required
/// - `templateRefKind` requires `supportsTemplateCapture`
/// - `interactiveSetupConnectionTypes` requires `supportsInteractiveSetup`
pub fn validate_environment_driver(
    driver: &PluginEnvironmentDriverDeclaration,
) -> Result<(), CapabilityError> {
    if driver.driver_key.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "environmentDrivers.driverKey is required".into(),
        ));
    }
    if driver.display_name.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "environmentDrivers.displayName is required".into(),
        ));
    }
    if driver.template_ref_kind.is_some() && driver.supports_template_capture != Some(true) {
        return Err(CapabilityError::InvalidManifest(format!(
            "environmentDrivers driver '{}' declares templateRefKind without supportsTemplateCapture",
            driver.driver_key
        )));
    }
    if !driver.interactive_setup_connection_types.is_empty()
        && driver.supports_interactive_setup != Some(true)
    {
        return Err(CapabilityError::InvalidManifest(format!(
            "environmentDrivers driver '{}' declares interactiveSetupConnectionTypes without supportsInteractiveSetup",
            driver.driver_key
        )));
    }
    Ok(())
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

    let declares_ui = manifest.declares_ui || !manifest.ui_slots.is_empty();
    if declares_ui && manifest.entrypoints.ui.is_none() {
        return Err(CapabilityError::InvalidManifest(
            "ui declared without entrypoints.ui".into(),
        ));
    }

    if let Some(db) = &manifest.database {
        validate_database_declaration(db)?;
        // A plugin that reaches into core tables must be able to read them.
        if !db.core_read_tables.is_empty()
            && !has_capability(&manifest.capabilities, "database.namespace.read")
        {
            return Err(CapabilityError::MissingCapability(
                "database.coreReadTables".into(),
                "database.namespace.read".into(),
            ));
        }
    }

    {
        let mut seen = std::collections::HashSet::new();
        for slot in &manifest.ui_slots {
            validate_ui_slot(slot)?;
            if !seen.insert(slot.id.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate ui slot id: {}",
                    slot.id
                )));
            }
        }
    }

    {
        let mut seen = std::collections::HashSet::new();
        for route in &manifest.api_routes {
            validate_api_route(route)?;
            if !seen.insert(route.route_key.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate api route routeKey: {}",
                    route.route_key
                )));
            }
        }
        if !manifest.api_routes.is_empty()
            && !has_capability(&manifest.capabilities, API_ROUTE_CAPABILITY)
        {
            return Err(CapabilityError::MissingCapability(
                "apiRoutes".into(),
                API_ROUTE_CAPABILITY.into(),
            ));
        }
    }

    let declared_webhook_keys: Vec<String> = manifest
        .webhooks
        .as_ref()
        .map(|hooks| hooks.iter().map(|h| h.endpoint_key.clone()).collect())
        .unwrap_or_default();
    {
        let mut seen = std::collections::HashSet::new();
        for provider in &manifest.object_references {
            validate_object_reference_provider(provider, &declared_webhook_keys)?;
            if !seen.insert(provider.provider_key.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate objectReferences providerKey: {}",
                    provider.provider_key
                )));
            }
        }
        if !manifest.object_references.is_empty()
            && !has_capability(&manifest.capabilities, "external.objects.read")
        {
            return Err(CapabilityError::MissingCapability(
                "objectReferences".into(),
                "external.objects.read".into(),
            ));
        }
    }

    {
        let mut seen = std::collections::HashSet::new();
        for driver in &manifest.environment_drivers {
            validate_environment_driver(driver)?;
            if !seen.insert(driver.driver_key.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate environmentDrivers driverKey: {}",
                    driver.driver_key
                )));
            }
        }
        if !manifest.environment_drivers.is_empty()
            && !has_capability(&manifest.capabilities, "environment.drivers.register")
        {
            return Err(CapabilityError::MissingCapability(
                "environmentDrivers".into(),
                "environment.drivers.register".into(),
            ));
        }
    }

    if let Some(schema) = &manifest.instance_config_schema {
        // A config schema must be an object-shaped JSON Schema.
        if !schema.is_object() {
            return Err(CapabilityError::InvalidManifest(
                "instanceConfigSchema must be a JSON object".into(),
            ));
        }
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
            database: None,
            ui_slots: vec![],
            api_routes: vec![],
            object_references: vec![],
            environment_drivers: vec![],
            instance_config_schema: None,
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

    fn slot(slot_type: &str) -> PluginUiSlotDeclaration {
        PluginUiSlotDeclaration {
            slot_type: slot_type.into(),
            id: "s1".into(),
            display_name: "Slot".into(),
            export_name: "Slot".into(),
            entity_types: vec![],
            route_path: None,
            order: None,
        }
    }

    #[test]
    fn database_declaration_requires_migrations_dir() {
        let mut db = PluginDatabaseDeclaration::default();
        assert!(validate_database_declaration(&db).is_err());
        db.migrations_dir = "drizzle".into();
        assert!(validate_database_declaration(&db).is_ok());
    }

    #[test]
    fn core_read_tables_must_be_canonical() {
        let mut db = PluginDatabaseDeclaration {
            migrations_dir: "drizzle".into(),
            ..Default::default()
        };
        db.core_read_tables = vec!["issues".into(), "secrets".into()];
        assert!(validate_database_declaration(&db).is_err());
        db.core_read_tables = vec!["issues".into(), "issue_comments".into()];
        assert!(validate_database_declaration(&db).is_ok());
    }

    #[test]
    fn core_read_tables_require_namespace_read_capability() {
        let mut m = manifest(&[]);
        m.database = Some(PluginDatabaseDeclaration {
            migrations_dir: "drizzle".into(),
            core_read_tables: vec!["issues".into()],
            namespace_slug: None,
        });
        assert!(validate_manifest(&m).is_err());
        m.capabilities.push("database.namespace.read".into());
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn ui_slot_type_must_be_canonical() {
        assert!(validate_ui_slot(&slot("page")).is_ok());
        let bad = slot("madeUpSlot");
        assert!(matches!(
            validate_ui_slot(&bad),
            Err(CapabilityError::InvalidManifest(_))
        ));
    }

    #[test]
    fn context_sensitive_ui_slots_require_entity_types() {
        // detailTab is context-sensitive.
        let s = slot("detailTab");
        assert!(validate_ui_slot(&s).is_err());
        let mut ok = s;
        ok.entity_types = vec!["issue".into()];
        assert!(validate_ui_slot(&ok).is_ok());
    }

    #[test]
    fn ui_slots_require_entrypoints_ui_and_unique_ids() {
        let mut m = manifest(&[]);
        m.ui_slots = vec![slot("page")];
        assert!(validate_manifest(&m).is_err());
        m.entrypoints.ui = Some("dist/ui".into());
        assert!(validate_manifest(&m).is_ok());

        m.ui_slots = vec![slot("page"), slot("page")];
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn instance_config_schema_must_be_object() {
        let mut m = manifest(&[]);
        m.instance_config_schema = Some(serde_json::json!("not-an-object"));
        assert!(validate_manifest(&m).is_err());
        m.instance_config_schema =
            Some(serde_json::json!({"type": "object", "properties": {}}));
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn canonical_table_and_slot_lists_match_paperclip() {
        assert_eq!(PLUGIN_DATABASE_CORE_READ_TABLES.len(), 13);
        assert_eq!(PLUGIN_DATABASE_CORE_READ_TABLES.first().copied(), Some("companies"));
        assert_eq!(
            PLUGIN_DATABASE_CORE_READ_TABLES.last().copied(),
            Some("budget_incidents")
        );
        assert_eq!(PLUGIN_UI_SLOT_TYPES.len(), 15);
        assert_eq!(PLUGIN_UI_SLOT_TYPES.first().copied(), Some("page"));
        assert_eq!(PLUGIN_UI_SLOT_TYPES.last().copied(), Some("companySettingsPage"));
    }

    fn route(method: &str, auth: &str) -> PluginApiRouteDeclaration {
        PluginApiRouteDeclaration {
            route_key: "r1".into(),
            method: method.into(),
            path: "/issues/:issueId/smoke".into(),
            auth: auth.into(),
            capability: API_ROUTE_CAPABILITY.into(),
            checkout_policy: None,
            company_resolution: None,
        }
    }

    #[test]
    fn api_route_method_must_be_canonical() {
        assert!(validate_api_route(&route("POST", "board")).is_ok());
        assert!(validate_api_route(&route("PUT", "board")).is_err());
    }

    #[test]
    fn api_route_path_must_be_absolute() {
        let mut r = route("GET", "board");
        r.path = "issues".into();
        assert!(validate_api_route(&r).is_err());
        r.path = "/issues".into();
        assert!(validate_api_route(&r).is_ok());
    }

    #[test]
    fn api_route_auth_must_be_canonical() {
        assert!(validate_api_route(&route("GET", "board-or-agent")).is_ok());
        assert!(validate_api_route(&route("GET", "anyone")).is_err());
    }

    #[test]
    fn api_route_capability_must_be_api_routes_register() {
        let mut r = route("GET", "board");
        r.capability = "issues.read".into();
        assert!(validate_api_route(&r).is_err());
    }

    #[test]
    fn api_route_checkout_policy_must_be_canonical() {
        let mut r = route("POST", "agent");
        r.checkout_policy = Some("sometimes".into());
        assert!(validate_api_route(&r).is_err());
        r.checkout_policy = Some("required-for-agent-in-progress".into());
        assert!(validate_api_route(&r).is_ok());
    }

    #[test]
    fn api_routes_require_capability_and_unique_keys() {
        let mut m = manifest(&[]);
        m.api_routes = vec![route("GET", "board")];
        assert!(validate_manifest(&m).is_err());
        m.capabilities.push(API_ROUTE_CAPABILITY.into());
        assert!(validate_manifest(&m).is_ok());

        m.api_routes = vec![route("GET", "board"), route("POST", "agent")];
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn company_resolution_deserializes_tagged_union() {
        let body: PluginApiRouteCompanyResolution =
            serde_json::from_value(serde_json::json!({"from": "body", "key": "companyId"})).unwrap();
        assert_eq!(
            body,
            PluginApiRouteCompanyResolution::Body { key: "companyId".into() }
        );
        let issue: PluginApiRouteCompanyResolution =
            serde_json::from_value(serde_json::json!({"from": "issue", "param": "issueId"})).unwrap();
        assert_eq!(
            issue,
            PluginApiRouteCompanyResolution::Issue { param: "issueId".into() }
        );
    }

    #[test]
    fn canonical_route_lists_match_paperclip() {
        assert_eq!(PLUGIN_API_ROUTE_METHODS.len(), 4);
        assert_eq!(PLUGIN_API_ROUTE_AUTH_MODES.len(), 4);
        assert_eq!(PLUGIN_API_ROUTE_CHECKOUT_POLICIES.len(), 3);
    }

    fn provider(key: &str) -> PluginObjectReferenceProviderDeclaration {
        PluginObjectReferenceProviderDeclaration {
            provider_key: key.into(),
            display_name: "Provider".into(),
            object_types: vec!["pull_request".into()],
            url_patterns: vec![],
            refresh_policy: None,
            webhook_endpoint_keys: vec![],
        }
    }

    fn driver(key: &str) -> PluginEnvironmentDriverDeclaration {
        PluginEnvironmentDriverDeclaration {
            driver_key: key.into(),
            kind: None,
            display_name: "Driver".into(),
            description: None,
            supports_reusable_leases: None,
            supports_interactive_setup: None,
            interactive_setup_connection_types: vec![],
            supports_template_capture: None,
            template_ref_kind: None,
        }
    }

    #[test]
    fn object_reference_provider_requires_key_name_and_types() {
        let mut p = provider("github");
        assert!(validate_object_reference_provider(&p, &[]).is_ok());

        let mut no_types = p.clone();
        no_types.object_types = vec![];
        assert!(validate_object_reference_provider(&no_types, &[]).is_err());

        p.display_name = " ".into();
        assert!(validate_object_reference_provider(&p, &[]).is_err());
    }

    #[test]
    fn object_reference_webhook_keys_must_be_declared() {
        let mut m = manifest(&["external.objects.read"]);
        let mut p = provider("github");
        p.webhook_endpoint_keys = vec!["missing".into()];
        m.object_references = vec![p.clone()];
        assert!(validate_manifest(&m).is_err());

        m.webhooks = Some(vec![PluginWebhookDeclaration {
            endpoint_key: "missing".into(),
            display_name: "M".into(),
            description: None,
        }]);
        m.capabilities.push("webhooks.receive".into());
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn object_references_require_external_objects_read() {
        let mut m = manifest(&[]);
        m.object_references = vec![provider("github")];
        assert!(validate_manifest(&m).is_err());
        m.capabilities.push("external.objects.read".into());
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn environment_driver_requires_key_and_name() {
        assert!(validate_environment_driver(&driver("k8s")).is_ok());
        let mut d = driver("k8s");
        d.driver_key = " ".into();
        assert!(validate_environment_driver(&d).is_err());
    }

    #[test]
    fn template_ref_kind_requires_template_capture() {
        let mut d = driver("k8s");
        d.template_ref_kind = Some("image".into());
        assert!(validate_environment_driver(&d).is_err());
        d.supports_template_capture = Some(true);
        assert!(validate_environment_driver(&d).is_ok());
    }

    #[test]
    fn interactive_connection_types_require_interactive_setup() {
        let mut d = driver("k8s");
        d.interactive_setup_connection_types = vec!["ssh".into()];
        assert!(validate_environment_driver(&d).is_err());
        d.supports_interactive_setup = Some(true);
        assert!(validate_environment_driver(&d).is_ok());
    }

    #[test]
    fn environment_drivers_require_register_capability_and_unique_keys() {
        let mut m = manifest(&[]);
        m.environment_drivers = vec![driver("e2b")];
        assert!(validate_manifest(&m).is_err());
        m.capabilities.push("environment.drivers.register".into());
        assert!(validate_manifest(&m).is_ok());
        m.environment_drivers = vec![driver("e2b"), driver("e2b")];
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn driver_kind_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&PluginEnvironmentDriverKind::SandboxProvider).unwrap(),
            "\"sandbox_provider\""
        );
    }
}
