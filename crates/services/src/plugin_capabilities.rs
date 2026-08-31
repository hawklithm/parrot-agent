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
    /// Suggested company-scoped agents this plugin can provision/resolve.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<PluginManagedAgentDeclaration>,
    /// Suggested company-scoped projects this plugin can provision/resolve.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<PluginManagedProjectDeclaration>,
    /// Suggested company-scoped routines this plugin can provision/resolve.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routines: Vec<PluginManagedRoutineDeclaration>,
    /// Suggested company skills this plugin can install/resolve.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<PluginManagedSkillDeclaration>,
    /// Trusted local folders this plugin can configure and access.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_folders: Vec<PluginLocalFolderDeclaration>,
    /// Declarative launcher metadata for host-mounted plugin entry points.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launchers: Vec<PluginLauncherDeclaration>,
    /// Minimum host version required (semver lower bound).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_host_version: Option<String>,
    /// One or more categories classifying this plugin (§6.2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// UI bundle declarations (§19); `ui.slots`/`ui.launchers` are preferred
    /// over the flat legacy fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<PluginUiDeclaration>,
    /// Legacy alias for `minimumHostVersion`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_paperclip_version: Option<String>,
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
    for entity in &slot.entity_types {
        if !PLUGIN_UI_SLOT_ENTITY_TYPES.contains(&entity.as_str()) {
            return Err(CapabilityError::InvalidManifest(format!(
                "unsupported ui slot entityType: {entity}"
            )));
        }
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
    /// Fine-grained sandbox capability declaration (declaration ∩ verified ∩ narrowing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_capabilities: Option<SandboxProviderCapabilities>,
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

/// Maximum length for a managed-resource stable key (Paperclip validators).
pub const MANAGED_KEY_MAX_LEN: usize = 100;

/// Validate a Paperclip managed-resource stable key.
///
/// Paperclip rule (`validators/plugin.ts`): `^[a-z0-9][a-z0-9._:-]*$`, max 100
/// chars — must start with a lowercase alphanumeric and contain only lowercase
/// letters, digits, dots, colons, underscores, or hyphens. Note this set
/// includes `:` unlike the plugin id pattern.
pub fn validate_managed_key(kind: &str, key: &str) -> Result<(), CapabilityError> {
    if key.is_empty() || key.len() > MANAGED_KEY_MAX_LEN {
        return Err(CapabilityError::InvalidManifest(format!(
            "{kind} must be 1-{MANAGED_KEY_MAX_LEN} chars"
        )));
    }
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => {
            return Err(CapabilityError::InvalidManifest(format!(
                "{kind} '{key}' must start with a lowercase alphanumeric"
            )))
        }
    }
    if !chars.all(|c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | ':' | '-')
    }) {
        return Err(CapabilityError::InvalidManifest(format!(
            "{kind} '{key}' may only contain lowercase letters, digits, dots, colons, underscores, or hyphens"
        )));
    }
    Ok(())
}

/// Declares a company-scoped agent a plugin can provision and resolve by key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManagedAgentDeclaration {
    /// Stable identifier for this managed agent, unique within the plugin.
    pub agent_key: String,
    /// Suggested visible agent name.
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapter_preference: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_monthly_cents: Option<i64>,
}

/// Declares a company-scoped project a plugin can provision and resolve by key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManagedProjectDeclaration {
    pub project_key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Declares a company skill a plugin can install and resolve by key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManagedSkillDeclaration {
    pub skill_key: String,
    pub display_name: String,
    /// Suggested skill slug. Defaults to `skillKey`. Must match the key pattern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Declares a company routine a plugin can provision and resolve by key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManagedRoutineDeclaration {
    pub routine_key: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Folder access level requested by a trusted plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginLocalFolderAccess {
    Read,
    ReadWrite,
}

/// Declares a company-scoped local folder a trusted plugin wants configured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginLocalFolderDeclaration {
    /// Stable identifier for this folder, unique within the plugin.
    pub folder_key: String,
    /// Human-readable name shown in plugin settings.
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Access level requested by the plugin. Defaults to `readWrite`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<PluginLocalFolderAccess>,
    /// Relative directories expected to exist under the configured root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_directories: Vec<String>,
    /// Relative files expected to exist under the configured root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_files: Vec<String>,
}

/// Validate a relative path expected under a local folder root.
///
/// Paperclip rejects absolute paths, `..` traversal, and paths over 500 chars.
pub fn validate_local_folder_relative_path(path: &str) -> Result<(), CapabilityError> {
    if path.is_empty() || path.len() > 500 {
        return Err(CapabilityError::InvalidManifest(format!(
            "local folder path must be 1-500 chars: '{path}'"
        )));
    }
    if path.starts_with('/') {
        return Err(CapabilityError::InvalidManifest(format!(
            "local folder path must be relative: '{path}'"
        )));
    }
    if path.split('/').any(|seg| seg == "..") {
        return Err(CapabilityError::InvalidManifest(format!(
            "local folder path must not contain '..': '{path}'"
        )));
    }
    Ok(())
}

/// Validate a local folder declaration.
pub fn validate_local_folder(
    folder: &PluginLocalFolderDeclaration,
) -> Result<(), CapabilityError> {
    validate_managed_key("folderKey", &folder.folder_key)?;
    if folder.display_name.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "localFolders.displayName is required".into(),
        ));
    }
    for dir in &folder.required_directories {
        validate_local_folder_relative_path(dir)?;
    }
    for file in &folder.required_files {
        validate_local_folder_relative_path(file)?;
    }
    Ok(())
}

/// Where in the host UI a launcher should be placed.
pub const PLUGIN_LAUNCHER_PLACEMENT_ZONES: &[&str] = &[
    "page",
    "detailTab",
    "taskDetailView",
    "dashboardWidget",
    "sidebar",
    "sidebarPanel",
    "projectSidebarItem",
    "globalToolbarButton",
    "toolbarButton",
    "contextMenuItem",
    "commentAnnotation",
    "commentContextMenuItem",
    "settingsPage",
];

/// What a launcher does when activated.
pub const PLUGIN_LAUNCHER_ACTIONS: &[&str] = &[
    "navigate",
    "openModal",
    "openDrawer",
    "openPopover",
    "performAction",
    "deepLink",
];

/// Size hints for plugin-owned launcher destinations.
pub const PLUGIN_LAUNCHER_BOUNDS: &[&str] = &["inline", "compact", "default", "wide", "full"];

/// Containers a launcher destination may render in.
pub const PLUGIN_LAUNCHER_RENDER_ENVIRONMENTS: &[&str] = &[
    "hostInline",
    "hostOverlay",
    "hostRoute",
    "external",
    "iframe",
];

/// What should happen when a launcher is activated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginLauncherActionDeclaration {
    /// What kind of launch behavior the host should perform.
    #[serde(rename = "type")]
    pub action_type: String,
    /// Stable target identifier or URL; meaning depends on `type`.
    pub target: String,
    /// Optional arbitrary parameters passed along to the target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// Optional render metadata for the destination opened by a launcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginLauncherRenderDeclaration {
    /// High-level container the launcher expects the host to use.
    pub environment: String,
    /// Optional size hint for the destination surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<String>,
}

/// Declares a plugin launcher surface independent of the slot that mounts it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginLauncherDeclaration {
    /// Stable identifier for this launcher, unique within the plugin.
    pub id: String,
    /// Human-readable label shown for the launcher.
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Where in the host UI this launcher should be placed.
    pub placement_zone: String,
    /// Optional export name in the UI bundle when the launcher has custom UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_name: Option<String>,
    /// Optional entity targeting for context-sensitive launcher zones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_types: Vec<String>,
    /// Optional ordering hint within the placement zone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    /// What should happen when the launcher is activated.
    pub action: PluginLauncherActionDeclaration,
    /// Optional render/container hints for the launched destination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<PluginLauncherRenderDeclaration>,
}

/// Validate a launcher declaration.
///
/// - `id` and `displayName` are required, `id` unique (checked by caller)
/// - `placementZone` must be canonical
/// - `action.type` must be canonical and `action.target` non-empty
/// - `render.environment` must be canonical; `render.bounds` must be canonical
pub fn validate_launcher(launcher: &PluginLauncherDeclaration) -> Result<(), CapabilityError> {
    if launcher.id.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "launchers.id is required".into(),
        ));
    }
    if launcher.display_name.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "launchers.displayName is required".into(),
        ));
    }
    if !PLUGIN_LAUNCHER_PLACEMENT_ZONES.contains(&launcher.placement_zone.as_str()) {
        return Err(CapabilityError::InvalidManifest(format!(
            "unsupported launcher placementZone: {}",
            launcher.placement_zone
        )));
    }
    if !PLUGIN_LAUNCHER_ACTIONS.contains(&launcher.action.action_type.as_str()) {
        return Err(CapabilityError::InvalidManifest(format!(
            "unsupported launcher action type: {}",
            launcher.action.action_type
        )));
    }
    if launcher.action.target.trim().is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "launchers.action.target is required".into(),
        ));
    }
    if let Some(render) = &launcher.render {
        if !PLUGIN_LAUNCHER_RENDER_ENVIRONMENTS.contains(&render.environment.as_str()) {
            return Err(CapabilityError::InvalidManifest(format!(
                "unsupported launcher render environment: {}",
                render.environment
            )));
        }
        if let Some(bounds) = &render.bounds {
            if !PLUGIN_LAUNCHER_BOUNDS.contains(&bounds.as_str()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "unsupported launcher render bounds: {bounds}"
                )));
            }
        }
    }
    Ok(())
}

/// Validate a `minimumHostVersion` / `minimumPaperclipVersion` value.
///
/// Paperclip requires a semver lower bound:
/// `^\d+\.\d+\.\d+(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$`
pub fn validate_minimum_host_version(version: &str) -> Result<(), CapabilityError> {
    fn is_ident(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    }
    let (core, rest) = match version.split_once('-') {
        Some((core, rest)) => (core, Some(rest)),
        None => (version, None),
    };
    // Split build metadata off the pre-release (or the core).
    let (core, pre, build) = match rest {
        Some(rest) => match rest.split_once('+') {
            Some((pre, build)) => (core, Some(pre), Some(build)),
            None => (core, Some(rest), None),
        },
        None => match core.split_once('+') {
            Some((c, build)) => (c, None, Some(build)),
            None => (core, None, None),
        },
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 || !parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(CapabilityError::InvalidManifest(format!(
            "minimumHostVersion must be semver (major.minor.patch): {version}"
        )));
    }
    if let Some(pre) = pre {
        if !pre.split('.').all(is_ident) {
            return Err(CapabilityError::InvalidManifest(format!(
                "minimumHostVersion has invalid pre-release: {version}"
            )));
        }
    }
    if let Some(build) = build {
        if !build.split('.').all(is_ident) {
            return Err(CapabilityError::InvalidManifest(format!(
                "minimumHostVersion has invalid build metadata: {version}"
            )));
        }
    }
    Ok(())
}

/// Plugin classification categories (PLUGIN_SPEC §6.2).
pub const PLUGIN_CATEGORIES: &[&str] = &["connector", "workspace", "automation", "ui", "environment"];

/// Fine-grained sandbox capability declaration for a plugin environment driver.
///
/// Every flag is optional and partial: the host resolves the effective
/// capability as `declaration ∩ verified ∩ narrowing`. A declared flag never
/// grants a capability the live worker did not verify.
///
/// Port of `@paperclipai/shared` `SandboxProviderCapabilities`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxProviderCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reusable_leases: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_sync_in: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_sync_out: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_process_sessions: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub independent_control_commands: Option<bool>,
    /// Selects the session-output streaming path; every other provider keeps
    /// the output-file poll path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incremental_session_output: Option<bool>,
}

/// Groups plugin UI declarations served from the shared UI bundle root.
///
/// Port of `@paperclipai/shared` `PluginUiDeclaration` (PLUGIN_SPEC §19).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PluginUiDeclaration {
    /// UI extension slots this plugin fills.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<PluginUiSlotDeclaration>,
    /// Declarative launcher metadata for host-mounted plugin entry points.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launchers: Vec<PluginLauncherDeclaration>,
}

/// Validate a category list (PLUGIN_SPEC §6.2).
///
/// Paperclip requires at least one category and every value must be canonical.
pub fn validate_categories(categories: &[String]) -> Result<(), CapabilityError> {
    if categories.is_empty() {
        return Err(CapabilityError::InvalidManifest(
            "at least one category is required".into(),
        ));
    }
    for category in categories {
        if !PLUGIN_CATEGORIES.contains(&category.as_str()) {
            return Err(CapabilityError::InvalidManifest(format!(
                "unsupported plugin category: {category}"
            )));
        }
    }
    Ok(())
}

/// Entity types a context-sensitive UI slot can attach to (PLUGIN_SPEC §19.3).
pub const PLUGIN_UI_SLOT_ENTITY_TYPES: &[&str] = &[
    "project",
    "issue",
    "agent",
    "goal",
    "run",
    "comment",
    "execution_workspace",
    "project_workspace",
];

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
    // Paperclip: /^[a-z0-9][a-z0-9._-]*$/ — must start with a lowercase
    // alphanumeric. No trailing colon (unlike managed-resource keys).
    {
        let id = &manifest.id;
        let first_ok = id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
        let rest_ok = id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'));
        if id.is_empty() || !first_ok || !rest_ok {
            return Err(CapabilityError::InvalidManifest(format!(
                "invalid plugin id: {id}"
            )));
        }
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

    // still accepted and merged so existing manifests keep validating.
    let mut ui_slots = manifest.ui_slots.clone();
    let mut launchers = manifest.launchers.clone();
    if let Some(ui) = &manifest.ui {
        ui_slots.extend(ui.slots.iter().cloned());
        launchers.extend(ui.launchers.iter().cloned());
    }
    let declares_ui = manifest.declares_ui || !ui_slots.is_empty() || manifest.ui.is_some();

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
        for slot in &ui_slots {
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

    {
        let mut seen = std::collections::HashSet::new();
        for agent in &manifest.agents {
            validate_managed_key("agentKey", &agent.agent_key)?;
            if !seen.insert(agent.agent_key.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate agentKey: {}",
                    agent.agent_key
                )));
            }
        }
        if !manifest.agents.is_empty() && !has_capability(&manifest.capabilities, "agents.managed")
        {
            return Err(CapabilityError::MissingCapability(
                "agents".into(),
                "agents.managed".into(),
            ));
        }
    }

    {
        let mut seen = std::collections::HashSet::new();
        for project in &manifest.projects {
            validate_managed_key("projectKey", &project.project_key)?;
            if !seen.insert(project.project_key.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate projectKey: {}",
                    project.project_key
                )));
            }
        }
        if !manifest.projects.is_empty()
            && !has_capability(&manifest.capabilities, "projects.managed")
        {
            return Err(CapabilityError::MissingCapability(
                "projects".into(),
                "projects.managed".into(),
            ));
        }
    }

    {
        let mut seen = std::collections::HashSet::new();
        for routine in &manifest.routines {
            validate_managed_key("routineKey", &routine.routine_key)?;
            if !seen.insert(routine.routine_key.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate routineKey: {}",
                    routine.routine_key
                )));
            }
        }
        if !manifest.routines.is_empty()
            && !has_capability(&manifest.capabilities, "routines.managed")
        {
            return Err(CapabilityError::MissingCapability(
                "routines".into(),
                "routines.managed".into(),
            ));
        }
    }

    {
        let mut seen = std::collections::HashSet::new();
        for skill in &manifest.skills {
            validate_managed_key("skillKey", &skill.skill_key)?;
            if let Some(slug) = &skill.slug {
                validate_managed_key("slug", slug)?;
            }
            if !seen.insert(skill.skill_key.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate skillKey: {}",
                    skill.skill_key
                )));
            }
        }
        if !manifest.skills.is_empty() && !has_capability(&manifest.capabilities, "skills.managed")
        {
            return Err(CapabilityError::MissingCapability(
                "skills".into(),
                "skills.managed".into(),
            ));
        }
    }

    {
        let mut seen = std::collections::HashSet::new();
        for folder in &manifest.local_folders {
            validate_local_folder(folder)?;
            if !seen.insert(folder.folder_key.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate folderKey: {}",
                    folder.folder_key
                )));
            }
        }
        if !manifest.local_folders.is_empty()
            && !has_capability(&manifest.capabilities, "local.folders")
        {
            return Err(CapabilityError::MissingCapability(
                "localFolders".into(),
                "local.folders".into(),
            ));
        }
    }

    validate_categories(&manifest.categories)?;

    // Paperclip nests UI declarations under `ui`; the flat legacy fields are
    if let Some(v) = &manifest.minimum_host_version {
        validate_minimum_host_version(v)?;
    }
    if let Some(v) = &manifest.minimum_paperclip_version {
        validate_minimum_host_version(v)?;
    }

    {
        let mut seen = std::collections::HashSet::new();
        for launcher in &launchers {
            validate_launcher(launcher)?;
            if !seen.insert(launcher.id.clone()) {
                return Err(CapabilityError::InvalidManifest(format!(
                    "duplicate launcher id: {}",
                    launcher.id
                )));
            }
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
            agents: vec![],
            projects: vec![],
            routines: vec![],
            skills: vec![],
            local_folders: vec![],
            launchers: vec![],
            categories: vec!["connector".into()],
            ui: None,
            minimum_host_version: None,
            minimum_paperclip_version: None,
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
            sandbox_capabilities: None,
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

    fn agent(key: &str) -> PluginManagedAgentDeclaration {
        PluginManagedAgentDeclaration {
            agent_key: key.into(),
            display_name: "Agent".into(),
            role: None,
            title: None,
            icon: None,
            adapter_type: None,
            adapter_preference: vec![],
            budget_monthly_cents: None,
        }
    }

    fn folder(key: &str) -> PluginLocalFolderDeclaration {
        PluginLocalFolderDeclaration {
            folder_key: key.into(),
            display_name: "Folder".into(),
            description: None,
            access: None,
            required_directories: vec![],
            required_files: vec![],
        }
    }

    #[test]
    fn managed_key_must_start_alphanumeric_and_allow_colon() {
        // Colons are allowed in managed keys (unlike plugin ids).
        assert!(validate_managed_key("agentKey", "ns:agent").is_ok());
        assert!(validate_managed_key("agentKey", "agent.1-a_b").is_ok());
        // Leading dot/hyphen/underscore is rejected.
        assert!(validate_managed_key("agentKey", ".agent").is_err());
        assert!(validate_managed_key("agentKey", "-agent").is_err());
        // Uppercase and empty rejected.
        assert!(validate_managed_key("agentKey", "Agent").is_err());
        assert!(validate_managed_key("agentKey", "").is_err());
        // Over 100 chars rejected.
        let long = "a".repeat(101);
        assert!(validate_managed_key("agentKey", &long).is_err());
    }

    #[test]
    fn plugin_id_rejects_leading_punctuation() {
        // Paperclip: /^[a-z0-9][a-z0-9._-]*$/ — no leading punctuation, no colon.
        for bad in [".acme", "-acme", "_acme", "acme:thing"] {
            let mut m = manifest(&[]);
            m.id = bad.into();
            assert!(validate_manifest(&m).is_err(), "id {bad:?} must be rejected");
        }
        for good in ["acme.thing", "a1-b_c.d"] {
            let mut m = manifest(&[]);
            m.id = good.into();
            assert!(validate_manifest(&m).is_ok(), "id {good:?} must be accepted");
        }
    }

    #[test]
    fn managed_agents_require_agents_managed_capability() {
        let mut m = manifest(&[]);
        m.agents = vec![agent("bot")];
        assert!(validate_manifest(&m).is_err());
        m.capabilities.push("agents.managed".into());
        assert!(validate_manifest(&m).is_ok());
        m.agents = vec![agent("bot"), agent("bot")];
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn managed_projects_routines_skills_require_their_capabilities() {
        let mut m = manifest(&[]);
        m.projects = vec![PluginManagedProjectDeclaration {
            project_key: "p".into(),
            display_name: "P".into(),
            description: None,
            status: None,
            color: None,
        }];
        assert!(validate_manifest(&m).is_err());
        m.capabilities.push("projects.managed".into());
        assert!(validate_manifest(&m).is_ok());

        let mut m2 = manifest(&[]);
        m2.routines = vec![PluginManagedRoutineDeclaration {
            routine_key: "r".into(),
            title: "R".into(),
            description: None,
        }];
        assert!(validate_manifest(&m2).is_err());
        m2.capabilities.push("routines.managed".into());
        assert!(validate_manifest(&m2).is_ok());

        let mut m3 = manifest(&[]);
        m3.skills = vec![PluginManagedSkillDeclaration {
            skill_key: "s".into(),
            display_name: "S".into(),
            slug: Some("custom-slug".into()),
            description: None,
        }];
        assert!(validate_manifest(&m3).is_err());
        m3.capabilities.push("skills.managed".into());
        assert!(validate_manifest(&m3).is_ok());
    }

    #[test]
    fn skill_slug_must_match_key_pattern() {
        let mut m = manifest(&["skills.managed"]);
        m.skills = vec![PluginManagedSkillDeclaration {
            skill_key: "s".into(),
            display_name: "S".into(),
            slug: Some("Bad Slug".into()),
            description: None,
        }];
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn local_folders_require_capability_and_safe_paths() {
        let mut m = manifest(&[]);
        m.local_folders = vec![folder("docs")];
        assert!(validate_manifest(&m).is_err());
        m.capabilities.push("local.folders".into());
        assert!(validate_manifest(&m).is_ok());

        // Traversal and absolute paths are rejected.
        let mut bad = folder("docs");
        bad.required_directories = vec!["../etc".into()];
        m.local_folders = vec![bad];
        assert!(validate_manifest(&m).is_err());

        let mut bad2 = folder("docs");
        bad2.required_files = vec!["/etc/passwd".into()];
        m.local_folders = vec![bad2];
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn local_folder_access_serializes_camel_case() {
        assert_eq!(
            serde_json::to_string(&PluginLocalFolderAccess::ReadWrite).unwrap(),
            "\"readWrite\""
        );
    }

    fn launcher(zone: &str, action: &str) -> PluginLauncherDeclaration {
        PluginLauncherDeclaration {
            id: "l1".into(),
            display_name: "Launcher".into(),
            description: None,
            placement_zone: zone.into(),
            export_name: None,
            entity_types: vec![],
            order: None,
            action: PluginLauncherActionDeclaration {
                action_type: action.into(),
                target: "/somewhere".into(),
                params: None,
            },
            render: None,
        }
    }

    #[test]
    fn launcher_placement_zone_must_be_canonical() {
        assert!(validate_launcher(&launcher("sidebar", "navigate")).is_ok());
        assert!(validate_launcher(&launcher("nowhere", "navigate")).is_err());
    }

    #[test]
    fn launcher_action_type_must_be_canonical_with_target() {
        assert!(validate_launcher(&launcher("page", "deepLink")).is_ok());
        assert!(validate_launcher(&launcher("page", "nuke")).is_err());
        let mut l = launcher("page", "navigate");
        l.action.target = " ".into();
        assert!(validate_launcher(&l).is_err());
    }

    #[test]
    fn launcher_render_must_be_canonical() {
        let mut l = launcher("page", "navigate");
        l.render = Some(PluginLauncherRenderDeclaration {
            environment: "iframe".into(),
            bounds: Some("wide".into()),
        });
        assert!(validate_launcher(&l).is_ok());

        let mut bad_env = l.clone();
        bad_env.render = Some(PluginLauncherRenderDeclaration {
            environment: "shadowRealm".into(),
            bounds: None,
        });
        assert!(validate_launcher(&bad_env).is_err());

        let mut bad_bounds = l.clone();
        bad_bounds.render = Some(PluginLauncherRenderDeclaration {
            environment: "iframe".into(),
            bounds: Some("enormous".into()),
        });
        assert!(validate_launcher(&bad_bounds).is_err());
    }

    #[test]
    fn launchers_require_unique_ids() {
        let mut m = manifest(&[]);
        m.launchers = vec![launcher("page", "navigate")];
        assert!(validate_manifest(&m).is_ok());
        m.launchers = vec![launcher("page", "navigate"), launcher("page", "navigate")];
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn minimum_host_version_must_be_semver() {
        for good in [
            "1.2.3",
            "0.10.0",
            "1.0.0-beta.1",
            "1.0.0-rc.1+build.5",
            "1.0.0+build.5",
        ] {
            assert!(
                validate_minimum_host_version(good).is_ok(),
                "{good} must be accepted"
            );
        }
        for bad in ["1.2", "1.2.x", "v1.2.3", "1..3", "", "1.2.3-"] {
            assert!(
                validate_minimum_host_version(bad).is_err(),
                "{bad} must be rejected"
            );
        }
    }

    #[test]
    fn manifest_validates_both_minimum_version_aliases() {
        let mut m = manifest(&[]);
        m.minimum_host_version = Some("1.2.3".into());
        assert!(validate_manifest(&m).is_ok());
        m.minimum_host_version = Some("1.2".into());
        assert!(validate_manifest(&m).is_err());

        let mut m2 = manifest(&[]);
        m2.minimum_paperclip_version = Some("not-semver".into());
        assert!(validate_manifest(&m2).is_err());
    }

    #[test]
    fn canonical_launcher_lists_match_paperclip() {
        assert_eq!(PLUGIN_LAUNCHER_PLACEMENT_ZONES.len(), 13);
        assert_eq!(PLUGIN_LAUNCHER_ACTIONS.len(), 6);
        assert_eq!(PLUGIN_LAUNCHER_BOUNDS.len(), 5);
        assert_eq!(PLUGIN_LAUNCHER_RENDER_ENVIRONMENTS.len(), 5);
    }

    #[test]
    fn categories_must_be_non_empty_and_canonical() {
        assert!(validate_categories(&["connector".into()]).is_ok());
        assert!(validate_categories(&["connector".into(), "ui".into()]).is_ok());
        assert!(validate_categories(&[]).is_err());
        assert!(validate_categories(&["nope".into()]).is_err());
    }

    #[test]
    fn manifest_requires_at_least_one_category() {
        let mut m = manifest(&[]);
        assert!(validate_manifest(&m).is_ok());
        m.categories = vec![];
        assert!(validate_manifest(&m).is_err());
        m.categories = vec!["bogus".into()];
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn nested_ui_slots_and_launchers_are_validated_and_merged() {
        let mut m = manifest(&[]);
        m.ui = Some(PluginUiDeclaration {
            slots: vec![slot("page")],
            launchers: vec![launcher("sidebar", "navigate")],
        });
        // Nested UI still requires entrypoints.ui.
        assert!(validate_manifest(&m).is_err());
        m.entrypoints.ui = Some("dist/ui".into());
        assert!(validate_manifest(&m).is_ok());

        // A nested launcher with a bogus placement zone must be rejected.
        let mut bad = m.clone();
        bad.ui = Some(PluginUiDeclaration {
            slots: vec![],
            launchers: vec![launcher("nowhere", "navigate")],
        });
        assert!(validate_manifest(&bad).is_err());
    }

    #[test]
    fn nested_and_flat_ui_slots_merge_for_duplicate_detection() {
        let mut m = manifest(&[]);
        m.entrypoints.ui = Some("dist/ui".into());
        m.ui_slots = vec![slot("page")];
        m.ui = Some(PluginUiDeclaration {
            slots: vec![slot("page")],
            launchers: vec![],
        });
        // Same slot id declared both flat and nested -> duplicate.
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn sandbox_capabilities_deserialize_as_partial_flags() {
        let caps: SandboxProviderCapabilities = serde_json::from_value(serde_json::json!({
            "reusableLeases": true,
            "incrementalSessionOutput": true
        }))
        .unwrap();
        assert_eq!(caps.reusable_leases, Some(true));
        assert_eq!(caps.incremental_session_output, Some(true));
        // Absent flags stay None (defer to the verified baseline).
        assert_eq!(caps.native_sync_in, None);
        assert_eq!(caps.persistent_process_sessions, None);
    }

    #[test]
    fn driver_sandbox_capabilities_round_trip() {
        let mut d = driver("e2b");
        d.sandbox_capabilities = Some(SandboxProviderCapabilities {
            reusable_leases: Some(true),
            native_sync_in: Some(false),
            ..Default::default()
        });
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"sandboxCapabilities\""), "got: {json}");
        let back: PluginEnvironmentDriverDeclaration = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn canonical_category_list_matches_paperclip() {
        assert_eq!(PLUGIN_CATEGORIES.len(), 5);
        assert_eq!(PLUGIN_CATEGORIES.first().copied(), Some("connector"));
        assert_eq!(PLUGIN_CATEGORIES.last().copied(), Some("environment"));
    }

    #[test]
    fn ui_slot_entity_types_must_be_canonical() {
        let mut s = slot("detailTab");
        s.entity_types = vec!["issue".into()];
        assert!(validate_ui_slot(&s).is_ok());

        s.entity_types = vec!["issue".into(), "widget".into()];
        assert!(matches!(
            validate_ui_slot(&s),
            Err(CapabilityError::InvalidManifest(_))
        ));
    }

    #[test]
    fn canonical_entity_type_list_matches_paperclip() {
        assert_eq!(PLUGIN_UI_SLOT_ENTITY_TYPES.len(), 8);
        assert_eq!(PLUGIN_UI_SLOT_ENTITY_TYPES.first().copied(), Some("project"));
        assert_eq!(
            PLUGIN_UI_SLOT_ENTITY_TYPES.last().copied(),
            Some("project_workspace")
        );
    }
}
