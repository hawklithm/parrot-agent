use models::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::RoutineService;
use models::routine::{RunSource, RoutineRun};

/// Routine run source
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutineRunSource {
    Schedule,
    Manual,
    Api,
    Webhook,
}

impl RoutineRunSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            RoutineRunSource::Schedule => "schedule",
            RoutineRunSource::Manual => "manual",
            RoutineRunSource::Api => "api",
            RoutineRunSource::Webhook => "webhook",
        }
    }
}

/// Input for dispatching a routine run
#[derive(Debug, Clone)]
pub struct DispatchRoutineRunInput {
    pub routine_id: Uuid,
    pub trigger_id: Option<Uuid>,
    pub source: RoutineRunSource,
    pub payload: Option<serde_json::Value>,
    pub variables: Option<std::collections::HashMap<String, String>>,
    pub idempotency_key: Option<String>,
    pub project_id: Option<Uuid>,
    pub assignee_agent_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub actor_agent_id: Option<Uuid>,
}



/// Routine Execution Service
///
/// Dispatches routine runs on behalf of the scheduler. It delegates to
/// `RoutineService::fire_routine`, the single source of truth for run creation,
/// so scheduled triggers honor the same concurrency policy, idempotency, and
/// dispatch-fingerprint model as manual triggers (§4B.3 alignment).
pub struct RoutineExecutionService {
    routine_service: Arc<dyn RoutineService>,
}

impl RoutineExecutionService {
    pub fn new(routine_service: Arc<dyn RoutineService>) -> Self {
        Self { routine_service }
    }

    /// Dispatch a routine run via the shared `RoutineService` path.
    pub async fn dispatch_routine_run(
        &self,
        input: DispatchRoutineRunInput,
    ) -> Result<RoutineRun, AppError> {
        let source = match input.source {
            RoutineRunSource::Schedule => RunSource::Schedule,
            RoutineRunSource::Manual => RunSource::Manual,
            RoutineRunSource::Api => RunSource::Manual,
            RoutineRunSource::Webhook => RunSource::Webhook,
        };
        let trigger_id = input.trigger_id.unwrap_or_else(Uuid::nil);
        self.routine_service
            .fire_routine(input.routine_id, trigger_id, source)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to dispatch routine run: {}", e)))
    }
}
