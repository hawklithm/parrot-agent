//! Local fake sandbox driver — a deterministic, dependency-free implementation
//! of the [`EnvironmentDriverTrait`] sandbox contract.
//!
//! This is the Parrot-local equivalent of Paperclip's built-in/plugin sandbox
//! driver seam: it does not shell out to Kubernetes/E2B/Daytona, but it exercises
//! the full lease lifecycle (acquire → ready → release) against an in-memory
//! store and resolves its capabilities through the shared
//! [`crate::environment_driver::sandbox_capabilities`] normalizer. Real cloud
//! providers are registered as plugin drivers; this fake lets local code and
//! tests verify the driver contract without external infrastructure.
//!
//! Cloud-provider verification is `ENVIRONMENT-GATED`; the fake provider covers
//! the local lifecycle and error-injection paths.

use std::collections::HashMap;

use async_trait::async_trait;
use models::{EnvironmentDriver, ExecutionEnvironment};
use parking_lot::Mutex;
use serde_json::json;
use uuid::Uuid;

use super::sandbox_capabilities::{
    builtin_sandbox_provider_verified_methods, resolve_effective_sandbox_capabilities,
    BuiltinSandboxProvider, ResolveSandboxCapabilitiesInput, SandboxCapabilityDeclaration,
};
use super::{DriverError, EnvironmentDriverTrait, EnvironmentProbeResult, LeaseAcquisitionResult};

/// Verified worker verbs the local fake provider advertises. It supports
/// reusable leases (resume/release/destroy), execution, and incremental
/// session output, but no native file sync (it mirrors a built-in provider).
fn fake_verified_methods() -> Vec<String> {
    builtin_sandbox_provider_verified_methods(Some(&BuiltinSandboxProvider {
        supports_reusable_leases: true,
        execute: true,
    }))
    .into_iter()
    .collect()
}

/// Capability declaration for the local fake provider.
fn fake_declaration() -> SandboxCapabilityDeclaration {
    SandboxCapabilityDeclaration {
        reusable_leases: Some(true),
        persistent_process_sessions: Some(true),
        independent_control_commands: Some(true),
        incremental_session_output: Some(true),
        native_sync_in: None,
        native_sync_out: None,
    }
}

/// In-memory lease record for the fake provider. Fields are written on acquire
/// and used by tests/diagnostics; the lib build keeps them for parity with the
/// real driver's persisted lease shape.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct FakeLease {
    id: Uuid,
    environment_id: Uuid,
    provider: String,
}

/// Local fake sandbox driver.
pub struct LocalFakeSandboxDriver {
    provider: String,
    leases: Mutex<HashMap<Uuid, FakeLease>>,
}

impl LocalFakeSandboxDriver {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            leases: Mutex::new(HashMap::new()),
        }
    }

    /// Resolve this provider's effective capabilities (worker-property baseline
    /// + declaration, no per-lease narrowing).
    pub fn capabilities(&self) -> super::sandbox_capabilities::EffectiveSandboxCapabilities {
        resolve_effective_sandbox_capabilities(&ResolveSandboxCapabilitiesInput {
            verified_methods: Some(fake_verified_methods()),
            declared: Some(fake_declaration()),
            narrowing: None,
        })
    }
}

#[async_trait]
impl EnvironmentDriverTrait for LocalFakeSandboxDriver {
    async fn probe(&self, _environment: &ExecutionEnvironment) -> Result<EnvironmentProbeResult, DriverError> {
        let caps = self.capabilities();
        let summary = format!(
            "local fake sandbox provider '{}' ready: reusable_leases={}, persistent_process_sessions={}, incremental_session_output={}",
            self.provider, caps.reusable_leases, caps.persistent_process_sessions, caps.incremental_session_output
        );
        Ok(EnvironmentProbeResult {
            ok: true,
            driver: EnvironmentDriver::Sandbox,
            summary,
        })
    }

    async fn acquire_lease(
        &self,
        environment: &ExecutionEnvironment,
        _workspace_id: Option<String>,
        _metadata: Option<serde_json::Value>,
    ) -> Result<LeaseAcquisitionResult, DriverError> {
        let caps = self.capabilities();
        if !caps.reusable_leases {
            return Err(DriverError::LeaseAcquisitionFailed(
                "local fake sandbox provider does not support reusable leases".to_string(),
            ));
        }

        let lease_id = Uuid::new_v4();
        let lease = FakeLease {
            id: lease_id,
            environment_id: environment.id,
            provider: self.provider.clone(),
        };
        self.leases.lock().insert(lease_id, lease);

        Ok(LeaseAcquisitionResult {
            lease_id,
            provider: self.provider.clone(),
            connection_info: json!({
                "provider": self.provider,
                "environmentId": environment.id.to_string(),
                "persistentProcessSessions": caps.persistent_process_sessions,
                "incrementalSessionOutput": caps.incremental_session_output,
            }),
            expires_at: None,
        })
    }

    async fn release_lease(
        &self,
        _environment: &ExecutionEnvironment,
        lease_id: Uuid,
    ) -> Result<(), DriverError> {
        let removed = self.leases.lock().remove(&lease_id).is_some();
        if removed {
            Ok(())
        } else {
            Err(DriverError::ConnectionError(format!(
                "lease {} not found for local fake sandbox provider '{}'",
                lease_id, self.provider
            )))
        }
    }

    async fn ensure_ready(&self, environment: &ExecutionEnvironment) -> Result<(), DriverError> {
        self.probe(environment).await.map(|_| ())
    }

    fn driver_type(&self) -> EnvironmentDriver {
        EnvironmentDriver::Sandbox
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use models::EnvironmentStatus;

    fn env() -> ExecutionEnvironment {
        ExecutionEnvironment {
            id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            name: "test-env".to_string(),
            description: None,
            driver: EnvironmentDriver::Sandbox,
            status: EnvironmentStatus::Active,
            config: serde_json::json!({"driver":"sandbox","provider":"local_fake","image":"alpine:3"}),
            env_vars: serde_json::json!({}),
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn fake_provider_capabilities_match_contract() {
        let driver = LocalFakeSandboxDriver::new("local_fake");
        let caps = driver.capabilities();
        assert!(caps.reusable_leases);
        assert!(caps.persistent_process_sessions);
        assert!(caps.incremental_session_output);
        // A built-in/local provider has no native sync hooks.
        assert!(!caps.native_sync_in);
        assert!(!caps.native_sync_out);
    }

    #[tokio::test]
    async fn lifecycle_acquire_probe_release() {
        let driver = LocalFakeSandboxDriver::new("local_fake");
        let environment = env();

        let probe = driver.probe(&environment).await.unwrap();
        assert!(probe.ok);
        assert_eq!(probe.driver, EnvironmentDriver::Sandbox);

        let lease = driver.acquire_lease(&environment, None, None).await.unwrap();
        assert_eq!(lease.provider, "local_fake");
        assert!(lease.connection_info.get("persistentProcessSessions").is_some());

        // Releasing the same lease succeeds.
        driver.release_lease(&environment, lease.lease_id).await.unwrap();

        // Releasing again fails (orphan cleanup is the caller's job).
        let err = driver.release_lease(&environment, lease.lease_id).await.unwrap_err();
        assert!(matches!(err, DriverError::ConnectionError(_)));
    }

    #[tokio::test]
    async fn ensure_ready_resolves_when_probe_ok() {
        let driver = LocalFakeSandboxDriver::new("local_fake");
        let environment = env();
        assert!(driver.ensure_ready(&environment).await.is_ok());
    }
}
