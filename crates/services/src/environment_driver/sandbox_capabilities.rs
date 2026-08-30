//! Sandbox capability contract — one normalizer for both the local/fake and
//! plugin sandbox driver branches.
//!
//! This is a faithful Rust port of Paperclip's
//! `server/src/services/environment-runtime.ts`
//! (`SANDBOX_CAPABILITY_KEYS`, `resolveEffectiveSandboxCapabilities`,
//! `buildSandboxCapabilityNarrowing`, `builtinSandboxProviderVerifiedMethods`)
//! and the behaviors pinned by `sandbox-capability-contract.test.ts`.
//!
//! Capabilities are resolved from two inputs:
//! - `verified_methods`: the worker verbs the provider actually advertised/supported.
//! - `declared`: the provider's optional capability declaration.
//!
//! A capability is effective only when the provider verified every prerequisite
//! worker verb AND (for opt-in capabilities) explicitly declared it. A narrowing
//! (per-lease policy) can only restrict a capability, never grant one.

use std::collections::HashMap;
use std::collections::HashSet;

/// Capability keys in the sandbox contract (order-stable, mirrors Paperclip).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SandboxCapabilityKey {
    /// Provider supports reusable leases (resume/release/destroy).
    ReusableLeases,
    /// Native file sync-in (worker property; defers to verified baseline).
    NativeSyncIn,
    /// Native file sync-out (worker property; defers to verified baseline).
    NativeSyncOut,
    /// Persistent process sessions (long-lived ACP/process sessions).
    PersistentProcessSessions,
    /// Independent control commands (interrupt/stop without killing the lease).
    IndependentControlCommands,
    /// Incremental session output streaming (opt-in only).
    IncrementalSessionOutput,
}

/// Canonical ordering of the capability keys. Mirrors `SANDBOX_CAPABILITY_KEYS`.
pub const SANDBOX_CAPABILITY_KEYS: [SandboxCapabilityKey; 6] = [
    SandboxCapabilityKey::ReusableLeases,
    SandboxCapabilityKey::NativeSyncIn,
    SandboxCapabilityKey::NativeSyncOut,
    SandboxCapabilityKey::PersistentProcessSessions,
    SandboxCapabilityKey::IndependentControlCommands,
    SandboxCapabilityKey::IncrementalSessionOutput,
];

/// Opt-in capabilities never resolve true without an explicit declaration.
const SANDBOX_CAPABILITY_OPT_IN_KEYS: &[SandboxCapabilityKey] =
    &[SandboxCapabilityKey::IncrementalSessionOutput];

/// A capability is a worker property (defers to the verified baseline when
/// undeclared) unless it is opt-in.
fn is_worker_property(key: SandboxCapabilityKey) -> bool {
    !SANDBOX_CAPABILITY_OPT_IN_KEYS.contains(&key)
}

/// Worker verbs a capability requires before it can be effective.
fn capability_prerequisite_methods(key: SandboxCapabilityKey) -> &'static [&'static str] {
    match key {
        SandboxCapabilityKey::ReusableLeases => &[
            "environmentResumeLease",
            "environmentReleaseLease",
            "environmentDestroyLease",
        ],
        SandboxCapabilityKey::NativeSyncIn => &["environmentSyncIn"],
        SandboxCapabilityKey::NativeSyncOut => &["environmentSyncOut"],
        SandboxCapabilityKey::PersistentProcessSessions => &["environmentExecute"],
        SandboxCapabilityKey::IndependentControlCommands => &["environmentExecute"],
        SandboxCapabilityKey::IncrementalSessionOutput => &["environmentExecute"],
    }
}

/// Resolved effective capability set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectiveSandboxCapabilities {
    pub reusable_leases: bool,
    pub native_sync_in: bool,
    pub native_sync_out: bool,
    pub persistent_process_sessions: bool,
    pub independent_control_commands: bool,
    pub incremental_session_output: bool,
}

impl EffectiveSandboxCapabilities {
    fn get_mut(&mut self, key: SandboxCapabilityKey) -> &mut bool {
        match key {
            SandboxCapabilityKey::ReusableLeases => &mut self.reusable_leases,
            SandboxCapabilityKey::NativeSyncIn => &mut self.native_sync_in,
            SandboxCapabilityKey::NativeSyncOut => &mut self.native_sync_out,
            SandboxCapabilityKey::PersistentProcessSessions => &mut self.persistent_process_sessions,
            SandboxCapabilityKey::IndependentControlCommands => &mut self.independent_control_commands,
            SandboxCapabilityKey::IncrementalSessionOutput => &mut self.incremental_session_output,
        }
    }

    /// Read an effective capability by key (call-site convenience).
    pub fn get(&self, key: SandboxCapabilityKey) -> bool {
        match key {
            SandboxCapabilityKey::ReusableLeases => self.reusable_leases,
            SandboxCapabilityKey::NativeSyncIn => self.native_sync_in,
            SandboxCapabilityKey::NativeSyncOut => self.native_sync_out,
            SandboxCapabilityKey::PersistentProcessSessions => self.persistent_process_sessions,
            SandboxCapabilityKey::IndependentControlCommands => self.independent_control_commands,
            SandboxCapabilityKey::IncrementalSessionOutput => self.incremental_session_output,
        }
    }
}

/// A provider capability declaration (mirrors the TS `declared` record).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SandboxCapabilityDeclaration {
    pub reusable_leases: Option<bool>,
    pub native_sync_in: Option<bool>,
    pub native_sync_out: Option<bool>,
    pub persistent_process_sessions: Option<bool>,
    pub independent_control_commands: Option<bool>,
    pub incremental_session_output: Option<bool>,
}

impl SandboxCapabilityDeclaration {
    fn declared_value(&self, key: SandboxCapabilityKey) -> Option<bool> {
        match key {
            SandboxCapabilityKey::ReusableLeases => self.reusable_leases,
            SandboxCapabilityKey::NativeSyncIn => self.native_sync_in,
            SandboxCapabilityKey::NativeSyncOut => self.native_sync_out,
            SandboxCapabilityKey::PersistentProcessSessions => self.persistent_process_sessions,
            SandboxCapabilityKey::IndependentControlCommands => self.independent_control_commands,
            SandboxCapabilityKey::IncrementalSessionOutput => self.incremental_session_output,
        }
    }

    fn declared_value_mut(&mut self, key: SandboxCapabilityKey) -> &mut Option<bool> {
        match key {
            SandboxCapabilityKey::ReusableLeases => &mut self.reusable_leases,
            SandboxCapabilityKey::NativeSyncIn => &mut self.native_sync_in,
            SandboxCapabilityKey::NativeSyncOut => &mut self.native_sync_out,
            SandboxCapabilityKey::PersistentProcessSessions => &mut self.persistent_process_sessions,
            SandboxCapabilityKey::IndependentControlCommands => &mut self.independent_control_commands,
            SandboxCapabilityKey::IncrementalSessionOutput => &mut self.incremental_session_output,
        }
    }
}

/// Per-lease narrowing that can only restrict capabilities (fail-closed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SandboxCapabilityNarrowing {
    pub reusable_leases: Option<bool>,
    pub native_sync_in: Option<bool>,
    pub native_sync_out: Option<bool>,
    pub persistent_process_sessions: Option<bool>,
    pub independent_control_commands: Option<bool>,
    pub incremental_session_output: Option<bool>,
}

impl SandboxCapabilityNarrowing {
    fn restriction(&self, key: SandboxCapabilityKey) -> Option<bool> {
        match key {
            SandboxCapabilityKey::ReusableLeases => self.reusable_leases,
            SandboxCapabilityKey::NativeSyncIn => self.native_sync_in,
            SandboxCapabilityKey::NativeSyncOut => self.native_sync_out,
            SandboxCapabilityKey::PersistentProcessSessions => self.persistent_process_sessions,
            SandboxCapabilityKey::IndependentControlCommands => self.independent_control_commands,
            SandboxCapabilityKey::IncrementalSessionOutput => self.incremental_session_output,
        }
    }
}

/// Build the per-target narrowing for a sandbox lease. A job backend or the
/// `nativeFileSyncUnsupported` flag disables native sync; a config-resolution
/// failure fails closed on persistent sessions and incremental session output.
pub struct SandboxCapabilityNarrowingInput {
    pub lease_policy: String,
    pub lease_metadata: HashMap<String, serde_json::Value>,
    pub config_resolution_failed: bool,
}

pub fn build_sandbox_capability_narrowing(
    input: &SandboxCapabilityNarrowingInput,
) -> SandboxCapabilityNarrowing {
    let backend = input
        .lease_metadata
        .get("backend")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let native_file_sync_unsupported = input
        .lease_metadata
        .get("nativeFileSyncUnsupported")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let disables_native_sync = backend == "job" || native_file_sync_unsupported;

    SandboxCapabilityNarrowing {
        // A reuse-by-environment policy keeps the lease reusable even on a job
        // backend; narrowing only ever restricts, so this stays true.
        reusable_leases: if input.lease_policy == "reuse_by_environment" {
            Some(true)
        } else {
            None
        },
        native_sync_in: if disables_native_sync { Some(false) } else { None },
        native_sync_out: if disables_native_sync { Some(false) } else { None },
        persistent_process_sessions: if input.config_resolution_failed {
            Some(false)
        } else {
            None
        },
        incremental_session_output: if input.config_resolution_failed {
            Some(false)
        } else {
            None
        },
        ..Default::default()
    }
}

/// Map a built-in sandbox provider's own methods to the worker verb names the
/// plugin branch uses. A built-in provider has no native sync hooks.
pub fn builtin_sandbox_provider_verified_methods(
    provider: Option<&BuiltinSandboxProvider>,
) -> HashSet<String> {
    let mut methods = HashSet::new();
    let Some(provider) = provider else {
        return methods;
    };
    if provider.supports_reusable_leases {
        methods.insert("environmentResumeLease".to_string());
        methods.insert("environmentReleaseLease".to_string());
        methods.insert("environmentDestroyLease".to_string());
    }
    if provider.execute {
        methods.insert("environmentExecute".to_string());
    }
    methods
}

/// Minimal built-in provider descriptor (mirrors the TS `{ supportsReusableLeases?, execute? }`).
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltinSandboxProvider {
    pub supports_reusable_leases: bool,
    pub execute: bool,
}

/// Input for [`resolve_effective_sandbox_capabilities`].
pub struct ResolveSandboxCapabilitiesInput {
    pub verified_methods: Option<Vec<String>>,
    pub declared: Option<SandboxCapabilityDeclaration>,
    pub narrowing: Option<SandboxCapabilityNarrowing>,
}

/// The one normalizer for the sandbox capability contract.
pub fn resolve_effective_sandbox_capabilities(
    input: &ResolveSandboxCapabilitiesInput,
) -> EffectiveSandboxCapabilities {
    let verified: HashSet<String> = match &input.verified_methods {
        Some(methods) => methods.iter().cloned().collect(),
        None => HashSet::new(),
    };
    let declared = input.declared.as_ref();

    let mut effective = EffectiveSandboxCapabilities::default();

    for key in SANDBOX_CAPABILITY_KEYS {
        let prereqs = capability_prerequisite_methods(key);
        let verified_ok = prereqs.iter().all(|m| verified.contains(*m));

        let declared_value = declared.and_then(|d| d.declared_value(key));
        let worker_property = is_worker_property(key);

        // Baseline before narrowing:
        // - explicit true  -> verified_ok (declared capability requires verification)
        // - explicit false -> removed
        // - undeclared: a worker property defers to the verified baseline;
        //   an opt-in capability stays false until explicitly declared.
        let base = if verified_ok {
            match declared_value {
                Some(true) => true,
                Some(false) => false,
                None => worker_property,
            }
        } else {
            false
        };

        let narrowed = match input
            .narrowing
            .as_ref()
            .and_then(|n| n.restriction(key))
        {
            Some(false) => false,
            _ => base,
        };

        *effective.get_mut(key) = narrowed;
    }

    effective
}

/// Helper: build a declaration from a full field set (every key explicit).
#[allow(clippy::too_many_arguments)]
pub fn declare(
    reusable_leases: bool,
    native_sync_in: bool,
    native_sync_out: bool,
    persistent_process_sessions: bool,
    independent_control_commands: bool,
    incremental_session_output: bool,
) -> SandboxCapabilityDeclaration {
    SandboxCapabilityDeclaration {
        reusable_leases: Some(reusable_leases),
        native_sync_in: Some(native_sync_in),
        native_sync_out: Some(native_sync_out),
        persistent_process_sessions: Some(persistent_process_sessions),
        independent_control_commands: Some(independent_control_commands),
        incremental_session_output: Some(incremental_session_output),
    }
}

/// Field name for debugging/contract parity (matches Paperclip key names).
pub fn capability_key_name(key: SandboxCapabilityKey) -> &'static str {
    match key {
        SandboxCapabilityKey::ReusableLeases => "reusableLeases",
        SandboxCapabilityKey::NativeSyncIn => "nativeSyncIn",
        SandboxCapabilityKey::NativeSyncOut => "nativeSyncOut",
        SandboxCapabilityKey::PersistentProcessSessions => "persistentProcessSessions",
        SandboxCapabilityKey::IndependentControlCommands => "independentControlCommands",
        SandboxCapabilityKey::IncrementalSessionOutput => "incrementalSessionOutput",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm(methods: &[&str]) -> Option<Vec<String>> {
        Some(methods.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn absent_declaration_defers_to_worker_supported_methods() {
        let effective = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&["environmentSyncIn", "environmentSyncOut"]),
            declared: None,
            narrowing: None,
        });
        assert!(effective.native_sync_in);
        assert!(effective.native_sync_out);
        assert!(!effective.persistent_process_sessions);
        assert!(!effective.reusable_leases);
    }

    #[test]
    fn effective_capabilities_are_subset_of_verified_and_declared() {
        let declared = declare(false, false, false, true, false, false);
        let effective = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&["environmentExecute"]),
            declared: Some(declared),
            narrowing: None,
        });
        assert!(effective.persistent_process_sessions);
        assert!(!effective.independent_control_commands);
        assert!(!effective.native_sync_in);

        let verified_only = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&["environmentExecute"]),
            declared: None,
            narrowing: None,
        });
        for key in SANDBOX_CAPABILITY_KEYS {
            if effective.get(key) {
                assert!(verified_only.get(key), "{} leaked beyond verified", capability_key_name(key));
            }
        }
    }

    #[test]
    fn kubernetes_job_lease_disables_native_sync() {
        let narrowing = build_sandbox_capability_narrowing(&SandboxCapabilityNarrowingInput {
            lease_policy: "ephemeral".into(),
            lease_metadata: {
                let mut m = HashMap::new();
                m.insert("backend".to_string(), serde_json::json!("job"));
                m
            },
            config_resolution_failed: false,
        });
        let effective = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&[
                "environmentAcquireLease",
                "environmentResumeLease",
                "environmentReleaseLease",
                "environmentDestroyLease",
                "environmentExecute",
                "environmentSyncIn",
                "environmentSyncOut",
            ]),
            declared: Some(SandboxCapabilityDeclaration {
                native_sync_in: Some(true),
                native_sync_out: Some(true),
                ..Default::default()
            }),
            narrowing: Some(narrowing),
        });
        assert!(!effective.native_sync_in);
        assert!(!effective.native_sync_out);
        assert!(effective.persistent_process_sessions);

        let flagged = build_sandbox_capability_narrowing(&SandboxCapabilityNarrowingInput {
            lease_policy: "ephemeral".into(),
            lease_metadata: {
                let mut m = HashMap::new();
                m.insert("nativeFileSyncUnsupported".to_string(), serde_json::json!(true));
                m
            },
            config_resolution_failed: false,
        });
        assert_eq!(flagged.native_sync_in, Some(false));
        assert_eq!(flagged.native_sync_out, Some(false));
    }

    #[test]
    fn persistent_process_sessions_follow_verified_and_declared() {
        let narrowing = build_sandbox_capability_narrowing(&SandboxCapabilityNarrowingInput {
            lease_policy: "ephemeral".into(),
            lease_metadata: HashMap::new(),
            config_resolution_failed: false,
        });
        let effective = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&["environmentExecute"]),
            declared: Some(declare(false, false, false, true, false, false)),
            narrowing: Some(narrowing),
        });
        assert!(effective.persistent_process_sessions);
    }

    #[test]
    fn config_resolution_failure_fails_closed_on_persistent_process_sessions() {
        let narrowing = build_sandbox_capability_narrowing(&SandboxCapabilityNarrowingInput {
            lease_policy: "ephemeral".into(),
            lease_metadata: HashMap::new(),
            config_resolution_failed: true,
        });
        let effective = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&["environmentExecute"]),
            declared: Some(declare(false, false, false, true, false, false)),
            narrowing: Some(narrowing),
        });
        assert!(!effective.persistent_process_sessions);

        let sync_narrowing = build_sandbox_capability_narrowing(&SandboxCapabilityNarrowingInput {
            lease_policy: "reuse_by_environment".into(),
            lease_metadata: {
                let mut m = HashMap::new();
                m.insert("backend".to_string(), serde_json::json!("job"));
                m
            },
            config_resolution_failed: true,
        });
        assert_eq!(sync_narrowing.reusable_leases, Some(true));
        assert_eq!(sync_narrowing.native_sync_in, Some(false));
        assert_eq!(sync_narrowing.native_sync_out, Some(false));
    }

    #[test]
    fn builtin_provider_branch_uses_same_normalizer_as_plugin_branch() {
        let declared = SandboxCapabilityDeclaration {
            reusable_leases: Some(true),
            persistent_process_sessions: Some(true),
            ..Default::default()
        };

        let builtin_methods =
            builtin_sandbox_provider_verified_methods(Some(&BuiltinSandboxProvider {
                supports_reusable_leases: true,
                execute: true,
            }));
        let builtin_effective = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: Some(builtin_methods.into_iter().collect()),
            declared: Some(declared.clone()),
            narrowing: None,
        });

        let plugin_effective = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&[
                "environmentResumeLease",
                "environmentReleaseLease",
                "environmentDestroyLease",
                "environmentExecute",
            ]),
            declared: Some(declared),
            narrowing: None,
        });

        assert_eq!(builtin_effective, plugin_effective);
        assert!(builtin_effective.reusable_leases);
        assert!(builtin_effective.persistent_process_sessions);
        assert!(!builtin_effective.native_sync_in);

        let no_exec = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: Some(
                builtin_sandbox_provider_verified_methods(Some(&BuiltinSandboxProvider {
                    supports_reusable_leases: false,
                    execute: false,
                }))
                .into_iter()
                .collect(),
            ),
            declared: Some(SandboxCapabilityDeclaration {
                persistent_process_sessions: Some(true),
                ..Default::default()
            }),
            narrowing: None,
        });
        assert!(!no_exec.persistent_process_sessions);
    }

    #[test]
    fn present_declaration_never_grants_beyond_verified() {
        for key in SANDBOX_CAPABILITY_KEYS {
            let mut d = SandboxCapabilityDeclaration::default();
            *d.declared_value_mut(key) = Some(true);
            let effective = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
                verified_methods: Some(vec![]),
                declared: Some(d),
                narrowing: None,
            });
            assert!(!effective.get(key), "{} leaked without verified verb", capability_key_name(key));
        }

        let resume_only = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&["environmentResumeLease"]),
            declared: Some(declare(true, false, false, false, false, false)),
            narrowing: None,
        });
        assert!(!resume_only.reusable_leases);
    }

    #[test]
    fn reusable_provider_without_destroy_support_resolves_false() {
        let resume_release = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&["environmentResumeLease", "environmentReleaseLease"]),
            declared: Some(declare(true, false, false, false, false, false)),
            narrowing: None,
        });
        assert!(!resume_release.reusable_leases);

        let all = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&[
                "environmentResumeLease",
                "environmentReleaseLease",
                "environmentDestroyLease",
            ]),
            declared: Some(declare(true, false, false, false, false, false)),
            narrowing: None,
        });
        assert!(all.reusable_leases);
    }

    #[test]
    fn generic_one_shot_provider_does_not_get_session_output_streaming() {
        let effective = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&["environmentExecute"]),
            declared: Some(declare(false, false, false, true, true, false)),
            narrowing: None,
        });
        assert!(effective.persistent_process_sessions);
        assert!(effective.independent_control_commands);
        assert!(!effective.incremental_session_output);
    }

    #[test]
    fn incremental_session_output_is_opt_in() {
        let undeclared = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&["environmentExecute"]),
            declared: None,
            narrowing: None,
        });
        assert!(!undeclared.incremental_session_output);

        let declared = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&["environmentExecute"]),
            declared: Some(declare(false, false, false, false, false, true)),
            narrowing: None,
        });
        assert!(declared.incremental_session_output);

        let declared_unverified = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: Some(vec![]),
            declared: Some(declare(false, false, false, false, false, true)),
            narrowing: None,
        });
        assert!(!declared_unverified.incremental_session_output);
    }

    #[test]
    fn config_resolution_failure_fails_closed_on_incremental_session_output() {
        let narrowing = build_sandbox_capability_narrowing(&SandboxCapabilityNarrowingInput {
            lease_policy: "ephemeral".into(),
            lease_metadata: HashMap::new(),
            config_resolution_failed: true,
        });
        let effective = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: vm(&["environmentExecute"]),
            declared: Some(declare(false, false, false, false, false, true)),
            narrowing: Some(narrowing),
        });
        assert!(!effective.incremental_session_output);
    }

    #[test]
    fn unknown_or_unavailable_verification_resolves_false() {
        let declared_all = declare(true, true, true, true, true, true);
        for verified in [None, Some(vec![])] {
            let effective = resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
                verified_methods: verified,
                declared: Some(declared_all.clone()),
                narrowing: None,
            });
            for key in SANDBOX_CAPABILITY_KEYS {
                assert!(!effective.get(key), "{} leaked with no verification", capability_key_name(key));
            }
        }
    }
}
