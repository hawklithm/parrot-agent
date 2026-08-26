use async_trait::async_trait;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};
use models::routine::{
    ConcurrencyPolicy, Routine, RoutineRun, RoutineStatus, RoutineTriggerConfig, RunSource, RunStatus,
};
use repositories::routine_repository::RoutineRepository;

#[async_trait]
pub trait RoutineService: Send + Sync {
    async fn create_routine(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
        name: String,
        description: Option<String>,
        trigger_config: RoutineTriggerConfig,
        created_by_user_id: Uuid,
    ) -> ServiceResult<Routine>;

    async fn get_routine(&self, routine_id: Uuid) -> ServiceResult<Option<Routine>>;
    async fn get_by_id(&self, id: Uuid) -> Result<Routine, ServiceError>;
    async fn list_routines(&self, company_id: Uuid) -> ServiceResult<Vec<Routine>>;
    async fn list_routines_by_agent(&self, agent_id: Uuid) -> ServiceResult<Vec<Routine>>;
    async fn update_routine(&self, routine_id: Uuid, name: Option<String>, description: Option<String>) -> ServiceResult<Routine>;
    async fn delete_routine(&self, routine_id: Uuid) -> ServiceResult<()>;
    async fn pause_routine(&self, routine_id: Uuid) -> ServiceResult<Routine>;
    async fn resume_routine(&self, routine_id: Uuid) -> ServiceResult<Routine>;

    async fn trigger_routine(&self, routine_id: Uuid, trigger_source: String) -> ServiceResult<RoutineRun>;
    /// Fire a routine run with full lifecycle: concurrency-policy enforcement
    /// (coalesce / skip / parallel), dispatch fingerprint, and trigger linkage.
    /// Used by scheduled triggers (§4B.3) and manual triggers alike.
    async fn fire_routine(
        &self,
        routine_id: Uuid,
        trigger_id: Uuid,
        source: models::routine::RunSource,
    ) -> ServiceResult<RoutineRun>;
    async fn fire_routine_with_options(
        &self,
        routine_id: Uuid,
        options: RoutineFireOptions,
    ) -> ServiceResult<RoutineRun>;
    async fn list_runs(&self, routine_id: Uuid, limit: i64) -> ServiceResult<Vec<RoutineRun>>;
    async fn get_run(&self, run_id: Uuid) -> ServiceResult<Option<RoutineRun>>;
}

#[derive(Debug, Clone)]
pub struct RoutineFireOptions {
    pub trigger_id: Option<Uuid>,
    pub source: RunSource,
    pub payload: Option<serde_json::Value>,
    pub variables: Option<std::collections::HashMap<String, String>>,
    pub idempotency_key: Option<String>,
    pub project_id: Option<Uuid>,
    pub assignee_agent_id: Option<Uuid>,
}

pub struct RoutineServiceImpl {
    repository: Arc<dyn RoutineRepository>,
}

impl RoutineServiceImpl {
    pub fn new(repository: Arc<dyn RoutineRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl RoutineService for RoutineServiceImpl {
    async fn create_routine(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
        name: String,
        description: Option<String>,
        _trigger_config: RoutineTriggerConfig,
        created_by_user_id: Uuid,
    ) -> ServiceResult<Routine> {
        let input = models::routine::CreateRoutineInput {
            company_id,
            title: name,
            description,
            project_id: None,
            goal_id: None,
            assignee_agent_id: agent_id,
            priority: 0,
            status: models::routine::RoutineStatus::Active,
            concurrency_policy: models::routine::ConcurrencyPolicy::Parallel,
            catch_up_policy: models::routine::CatchUpPolicy::SkipMissed,
            variables: Vec::new(),
            env: serde_json::Value::Object(serde_json::Map::new()),
            responsible_user_id: Some(created_by_user_id),
        };
        let routine = Routine::new(input);
        self.repository.create(routine).await.map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn get_routine(&self, routine_id: Uuid) -> ServiceResult<Option<Routine>> {
        self.repository.get(routine_id).await.map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Routine, ServiceError> {
        self.repository.get(id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {} not found", id)))
    }

    async fn list_routines(&self, company_id: Uuid) -> ServiceResult<Vec<Routine>> {
        self.repository.list_by_company(company_id).await.map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn list_routines_by_agent(&self, agent_id: Uuid) -> ServiceResult<Vec<Routine>> {
        self.repository.list_by_agent(agent_id).await.map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn update_routine(&self, routine_id: Uuid, name: Option<String>, description: Option<String>) -> ServiceResult<Routine> {
        let mut routine = self.repository.get(routine_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {} not found", routine_id)))?;

        if let Some(n) = name {
            routine.name = n;
        }
        if let Some(d) = description {
            routine.description = Some(d);
        }

        self.repository.update(routine).await.map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn delete_routine(&self, routine_id: Uuid) -> ServiceResult<()> {
        self.repository.delete(routine_id).await.map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn pause_routine(&self, routine_id: Uuid) -> ServiceResult<Routine> {
        let mut routine = self.repository.get(routine_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {} not found", routine_id)))?;

        routine.status = RoutineStatus::Paused;
        self.repository.update(routine).await.map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn resume_routine(&self, routine_id: Uuid) -> ServiceResult<Routine> {
        let mut routine = self.repository.get(routine_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {} not found", routine_id)))?;

        routine.status = RoutineStatus::Active;
        self.repository.update(routine).await.map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn trigger_routine(&self, routine_id: Uuid, trigger_source: String) -> ServiceResult<RoutineRun> {
        let source = match trigger_source.as_str() {
            "schedule" => RunSource::Schedule,
            "webhook" => RunSource::Webhook,
            _ => RunSource::Manual,
        };
        self.fire_routine(routine_id, Uuid::nil(), source).await
    }

    async fn fire_routine(
        &self,
        routine_id: Uuid,
        trigger_id: Uuid,
        source: RunSource,
    ) -> ServiceResult<RoutineRun> {
        self.fire_routine_with_options(
            routine_id,
            RoutineFireOptions {
                trigger_id: (!trigger_id.is_nil()).then_some(trigger_id),
                source,
                payload: None,
                variables: None,
                idempotency_key: None,
                project_id: None,
                assignee_agent_id: None,
            },
        )
        .await
    }

    async fn fire_routine_with_options(
        &self,
        routine_id: Uuid,
        options: RoutineFireOptions,
    ) -> ServiceResult<RoutineRun> {
        let routine = self
            .repository
            .get(routine_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {} not found", routine_id)))?;

        if let Some(idempotency_key) = options.idempotency_key.as_deref() {
            if let Some(existing) = self
                .repository
                .find_run_by_idempotency_key(routine_id, idempotency_key)
                .await
                .map_err(|e| ServiceError::Repository(e.to_string()))?
            {
                return Ok(existing);
            }
        }

        let trigger_id = options.trigger_id;
        let trigger_payload = merge_trigger_payload(options.payload, options.variables);
        let dispatch_fingerprint = routine_dispatch_fingerprint(
            routine_id,
            trigger_id,
            options.source,
            &trigger_payload,
            options.project_id,
            options.assignee_agent_id,
            routine.latest_revision_id,
        );

        // Enforce concurrency policy (coalesce / skip / parallel).
        match self.check_concurrency(routine_id, routine.concurrency_policy).await {
            Ok(Some(run_id)) => {
                let run = RoutineRun {
                    id: Uuid::new_v4(),
                    company_id: routine.company_id,
                    routine_id,
                    trigger_id,
                    source: options.source,
                    status: RunStatus::Coalesced,
                    triggered_at: Utc::now(),
                    routine_revision_id: routine.latest_revision_id,
                    idempotency_key: options.idempotency_key.clone(),
                    trigger_payload: trigger_payload.clone(),
                    dispatch_fingerprint: Some(dispatch_fingerprint.clone()),
                    linked_issue_id: None,
                    coalesced_into_run_id: Some(run_id),
                    failure_reason: None,
                    completed_at: None,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                self.repository
                    .create_run(run.clone())
                    .await
                    .map_err(|e| ServiceError::Repository(e.to_string()))?;
                return Ok(run);
            }
            Ok(None) => {}
            Err(ServiceError::InvalidInput(message)) if message == "Routine has active run, skipping" => {
                let run = RoutineRun {
                    id: Uuid::new_v4(),
                    company_id: routine.company_id,
                    routine_id,
                    trigger_id,
                    source: options.source,
                    status: RunStatus::Skipped,
                    triggered_at: Utc::now(),
                    routine_revision_id: routine.latest_revision_id,
                    idempotency_key: options.idempotency_key.clone(),
                    trigger_payload: trigger_payload.clone(),
                    dispatch_fingerprint: Some(dispatch_fingerprint.clone()),
                    linked_issue_id: None,
                    coalesced_into_run_id: None,
                    failure_reason: Some("Active run exists".to_string()),
                    completed_at: Some(Utc::now()),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                self.repository
                    .create_run(run.clone())
                    .await
                    .map_err(|e| ServiceError::Repository(e.to_string()))?;
                return Ok(run);
            }
            Err(e) => return Err(e),
        }

        // No active run (or parallel policy): create a queued run with a dispatch fingerprint.
        let run = RoutineRun {
            id: Uuid::new_v4(),
            company_id: routine.company_id,
            routine_id,
            trigger_id,
            source: options.source,
            status: RunStatus::Queued,
            triggered_at: Utc::now(),
            routine_revision_id: routine.latest_revision_id,
            idempotency_key: options.idempotency_key,
            trigger_payload,
            dispatch_fingerprint: Some(dispatch_fingerprint),
            linked_issue_id: None,
            coalesced_into_run_id: None,
            failure_reason: None,
            completed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let created_run = self
            .repository
            .create_run(run)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        let mut updated_routine = routine.clone();
        updated_routine.last_enqueued_at = Some(Utc::now());
        self.repository
            .update(updated_routine)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        Ok(created_run)
    }
    async fn list_runs(&self, routine_id: Uuid, limit: i64) -> ServiceResult<Vec<RoutineRun>> {
        self.repository
            .list_runs(routine_id, limit)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn get_run(&self, run_id: Uuid) -> ServiceResult<Option<RoutineRun>> {
        self.repository
            .get_run(run_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }
}

impl RoutineServiceImpl {
    /// Check concurrency policy before creating a run.
    /// Returns `Some(run_id)` to coalesce into, `Ok(None)` to proceed, or an
    /// error containing "skipping" to record a skipped run.
    async fn check_concurrency(
        &self,
        routine_id: Uuid,
        policy: ConcurrencyPolicy,
    ) -> Result<Option<Uuid>, ServiceError> {
        let active_runs: Vec<RoutineRun> = self
            .repository
            .list_runs(routine_id, 10)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find runs: {}", e)))?
            .into_iter()
            .filter(|r| {
                matches!(
                    r.status,
                    RunStatus::Received | RunStatus::Queued | RunStatus::Dispatched
                )
            })
            .collect();

        if active_runs.is_empty() {
            return Ok(None);
        }

        match policy {
            ConcurrencyPolicy::CoalesceIfActive => Ok(Some(active_runs[0].id)),
            ConcurrencyPolicy::SkipIfActive => {
                Err(ServiceError::InvalidInput("Routine has active run, skipping".to_string()))
            }
            ConcurrencyPolicy::Parallel => Ok(None),
        }
    }

}

fn merge_trigger_payload(
    payload: Option<serde_json::Value>,
    variables: Option<std::collections::HashMap<String, String>>,
) -> Option<serde_json::Value> {
    let mut value = payload.unwrap_or_else(|| serde_json::json!({}));
    if let Some(variables) = variables {
        let variables = serde_json::to_value(variables).unwrap_or_else(|_| serde_json::json!({}));
        if let (Some(target), Some(source)) = (value.as_object_mut(), variables.as_object()) {
            for (key, item) in source {
                target.insert(key.clone(), item.clone());
            }
        } else if let Some(target) = value.as_object_mut() {
            target.insert("variables".to_string(), variables);
        }
    }
    Some(value)
}

fn routine_dispatch_fingerprint(
    routine_id: Uuid,
    trigger_id: Option<Uuid>,
    source: RunSource,
    payload: &Option<serde_json::Value>,
    project_id: Option<Uuid>,
    assignee_agent_id: Option<Uuid>,
    routine_revision_id: Option<Uuid>,
) -> String {
    let input = serde_json::json!({
        "routineId": routine_id,
        "triggerId": trigger_id,
        "source": source,
        "payload": payload,
        "projectId": project_id,
        "assigneeAgentId": assignee_agent_id,
        "routineRevisionId": routine_revision_id,
    });
    let digest = Sha256::digest(serde_json::to_vec(&input).unwrap_or_default());
    format!("routine:{}", hex::encode(digest))
}
