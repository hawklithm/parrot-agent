//! Plugin SDK state-scope contract (Paperclip `@paperclipai/plugin-sdk` §21.3).
//!
//! Faithful Rust port of the SDK's `ScopeKey` type and the canonical composite
//! state-key builder. A scope key identifies exactly where plugin state lives;
//! scope is partitioned by `scopeKind` and an optional `scopeId`, with an
//! optional `namespace` sub-partition (defaulting to `"default"`) and a
//! `stateKey` within the namespace.
//!
//! This is the Parrot-local equivalent of the worker-side `ScopeKey` that the
//! host resolves into a `plugin_state` row key. Parrot's `summary_slots` table
//! already stores `scope_kind`/`scope_id`/`slot_key`; this module gives plugin
//! workers a single, canonical key shape so discovery and storage stay in
//! lockstep with Paperclip.
//!
//! @see PLUGIN_SPEC.md §21.3 `plugin_state`.

use serde::{Deserialize, Serialize};

/// What kind of Paperclip object plugin state is scoped to.
///
/// Mirrors Paperclip's `PluginStateScopeKind` (the set is open; unknown
/// variants are preserved as strings so new Paperclip scopes don't break the
/// host).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStateScopeKind {
    Instance,
    Company,
    Project,
    ProjectWorkspace,
    Issue,
    Agent,
    Goal,
    Routine,
    Skill,
    /// Forward-compatible catch-all for scopes added after this port.
    #[serde(untagged)]
    Other(String),
}

/// A scope key identifying exactly where plugin state is stored.
///
/// Port of `@paperclipai/plugin-sdk` `ScopeKey`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeKey {
    /// What kind of Paperclip object this state is scoped to.
    pub scope_kind: PluginStateScopeKind,
    /// UUID or text identifier for the scoped object. Omit for `instance` scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    /// Optional sub-namespace within the scope to avoid key collisions.
    /// Defaults to `"default"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// The state key within the namespace.
    pub state_key: String,
}

impl ScopeKey {
    /// Canonical composite state key, e.g.
    /// - `instance::default::theme` (instance scope, no scopeId)
    /// - `project:proj-uuid::default::lastView`
    /// - `issue:iss-uuid:board::pinned`
    ///
    /// `namespace` defaults to `"default"` when absent.
    pub fn composite_key(&self) -> String {
        let namespace = self.namespace.as_deref().unwrap_or("default");
        match &self.scope_id {
            Some(scope_id) => format!(
                "{}:{}:{}:{}",
                serde_json::to_value(&self.scope_kind)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "unknown".to_string()),
                scope_id,
                namespace,
                self.state_key
            ),
            None => format!(
                "{}::{}:{}",
                serde_json::to_value(&self.scope_kind)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "unknown".to_string()),
                namespace,
                self.state_key
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_scope_omits_scope_id() {
        let key = ScopeKey {
            scope_kind: PluginStateScopeKind::Instance,
            scope_id: None,
            namespace: None,
            state_key: "theme".to_string(),
        };
        assert_eq!(key.composite_key(), "instance::default:theme");
    }

    #[test]
    fn project_scope_includes_scope_id_and_default_namespace() {
        let key = ScopeKey {
            scope_kind: PluginStateScopeKind::Project,
            scope_id: Some("proj-uuid".to_string()),
            namespace: None,
            state_key: "lastView".to_string(),
        };
        assert_eq!(key.composite_key(), "project:proj-uuid:default:lastView");
    }

    #[test]
    fn issue_scope_with_explicit_namespace() {
        let key = ScopeKey {
            scope_kind: PluginStateScopeKind::Issue,
            scope_id: Some("iss-uuid".to_string()),
            namespace: Some("board".to_string()),
            state_key: "pinned".to_string(),
        };
        assert_eq!(key.composite_key(), "issue:iss-uuid:board:pinned");
    }

    #[test]
    fn unknown_scope_kind_is_preserved_via_other() {
        let key = ScopeKey {
            scope_kind: PluginStateScopeKind::Other("workspace".to_string()),
            scope_id: Some("ws-1".to_string()),
            namespace: None,
            state_key: "k".to_string(),
        };
        assert_eq!(key.composite_key(), "workspace:ws-1:default:k");
    }

    #[test]
    fn round_trips_through_json() {
        let key = ScopeKey {
            scope_kind: PluginStateScopeKind::Goal,
            scope_id: Some("goal-1".to_string()),
            namespace: Some("tab".to_string()),
            state_key: "open".to_string(),
        };
        let serialized = serde_json::to_string(&key).unwrap();
        let back: ScopeKey = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, key);
        assert_eq!(back.composite_key(), "goal:goal-1:tab:open");
    }
}

/// Optional filter applied when subscribing to an event. The host evaluates
/// the filter server-side so filtered-out events never cross the process boundary.
///
/// Port of `@paperclipai/plugin-sdk` `EventFilter` (PLUGIN_SPEC §16.1).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EventFilter {
    /// Only receive events for this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Only receive events for this company.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company_id: Option<String>,
    /// Only receive events for this agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Additional arbitrary filter fields.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Actor type for a plugin event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginEventActorType {
    User,
    Agent,
    System,
    Plugin,
}

/// Envelope wrapping every domain event delivered to a plugin worker.
///
/// Port of `@paperclipai/plugin-sdk` `PluginEvent` (PLUGIN_SPEC §16).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginEvent {
    /// Unique event identifier (UUID).
    pub event_id: String,
    /// The event type (e.g. `"issue.created"`).
    pub event_type: String,
    /// ISO 8601 timestamp when the event occurred.
    pub occurred_at: String,
    /// ID of the actor that caused the event, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    /// Type of actor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<PluginEventActorType>,
    /// Primary entity involved in the event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// Type of the primary entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    /// UUID of the company this event belongs to.
    pub company_id: String,
    /// Typed event payload.
    pub payload: serde_json::Value,
}

#[cfg(test)]
mod event_tests {
    use super::*;

    #[test]
    fn event_filter_default_is_empty() {
        let filter = EventFilter::default();
        assert!(filter.project_id.is_none());
        assert!(filter.company_id.is_none());
        assert!(filter.agent_id.is_none());
        assert!(filter.extra.is_empty());
    }

    #[test]
    fn event_envelope_round_trips() {
        let event = PluginEvent {
            event_id: "evt-1".to_string(),
            event_type: "issue.created".to_string(),
            occurred_at: "2026-08-30T00:00:00Z".to_string(),
            actor_id: Some("user-1".to_string()),
            actor_type: Some(PluginEventActorType::User),
            entity_id: Some("iss-1".to_string()),
            entity_type: Some("issue".to_string()),
            company_id: "co-1".to_string(),
            payload: serde_json::json!({"title": "x"}),
        };
        let serialized = serde_json::to_string(&event).unwrap();
        let back: PluginEvent = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, event);
        assert_eq!(back.event_type, "issue.created");
        assert_eq!(back.payload.get("title").unwrap(), "x");
    }

    #[test]
    fn event_actor_type_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&PluginEventActorType::Plugin).unwrap(),
            "\"plugin\""
        );
    }
}
