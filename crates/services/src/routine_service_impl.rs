use async_trait::async_trait;
use chrono::Utc;
use repositories::{RoutineRepository, RoutineTriggerRepository, RoutineRevisionRepository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use models::{
    Routine, RoutineRun, RoutineTrigger, RoutineRevision, RoutineStatus,
    ConcurrencyPolicy, CatchUpPolicy, RunSource, RunStatus, TriggerKind,
    RoutineVariable, RoutineVariableType
};
use crate::ServiceError;

/// Create Routine Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoutineInput {
    pub company_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub project_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub assignee_agent_id: Uuid,
    pub priority: i32,
    pub status: RoutineStatus,
    pub concurrency_policy: ConcurrencyPolicy,
    pub catch_up_policy: CatchUpPolicy,
    pub variables: Vec<RoutineVariable>,
    pub env: serde_json::Value,
    pub responsible_user_id: Option<Uuid>,
}

/// Update Routine Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRoutineInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<RoutineStatus>,
    pub priority: Option<i32>,
    pub assignee_agent_id: Option<Uuid>,
    pub concurrency_policy: Option<ConcurrencyPolicy>,
    pub catch_up_policy: Option<CatchUpPolicy>,
    pub variables: Option<Vec<RoutineVariable>>,
    pub env: Option<serde_json::Value>,
}

/// Routine Service trait
#[async_trait]
pub trait RoutineService: Send + Sync {
    /// Create a new routine with initial revision
    async fn create(&self, input: CreateRoutineInput) -> Result<Routine, ServiceError>;

    /// Get routine by id
    async fn get_by_id(&self, id: Uuid) -> Result<Routine, ServiceError>;

    /// Update routine and create new revision if needed
    async fn update(&self, id: Uuid, input: UpdateRoutineInput) -> Result<Routine, ServiceError>;

    /// Delete routine
    async fn delete(&self, id: Uuid) -> Result<(), ServiceError>;

    /// List routines by company
    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<Routine>, ServiceError>;

    /// List routines by agent
    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<Routine>, ServiceError>;

    /// Trigger routilly
    async fn trigger_manual(&self, routine_id: Uuid, triggered_by: Uuid) -> Result<RoutineRun, ServiceError>;

    /// Fire routine (internal execution)
    async fn fire_routine(&self, routine_id: Uuid, trigger_id: Uuid, source: RunSource) -> Result<RoutineRun, ServiceError>;

    /// Get routine revisions
    async fn get_revisions(&self, routine_id: Uuid) -> Result<Vec<RoutineRevision>, ServiceError>;

    /// Restore routine to a specific revision
    async fn restore_revision(&self, routine_id: Uuid, revision_id: Uuid) -> Result<Routine, ServiceError>;

    /// Validate routine variables
    fn validate_variables(&self, variables: &[RoutineVariable]) -> Result<(), ServiceError>;
}

/// Default Routine Service Implementation
pub struct DefaultRoutineService {
    routine_repo: Arc<dyn RoutineRepository>,
    trigger_repo: Arc<dyn RoutineTriggerRepository>,
    revision_repo: Arc<dyn RoutineRevisionRepository>,
}

impl DefaultRoutineService {
    pub fn new(
        routine_repo: Arc<dyn RoutineRepository>,
        trigger_repo: Arc<dyn RoutineTriggerRepository>,
        revision_repo: Arc<dyn RoutineRevisionRepository>,
    ) -> Self {
        Self {
            routine_repo,
            trigger_repo,
            revision_repo,
        }
    }

    /// Create initial revision for routine
    async fn create_initial_revision(&self, routine: &Routine) -> Result<RoutineRevision, ServiceError> {
        let snapshot = serde_json::json!({
            "version": 1,
            "routine": {
                "id": routine.id,
                "title": routine.title,
                "description": routine.description,
                "assignee_agent_id": routine.assignee_agent_id,
                "priority": routine.priority,
                "status": routine.status,
                "concurrency_policy": routine.concurrency_policy,
                "catch_up_policy": routine.catch_up_policy,
                "variables": routine.variables,
                "env": routine.env,
            },
            "triggers": []
        });

        let revision = RoutineRevision {
            id: Uuid::new_v4(),
            company_id: routine.company_id,
            routine_id: routine.id,
            revision_number: 1,
            title: Some(routine.title.clone()),
            description: routine.description.clone(),
            snapshot,
            change_summary: Some("Initial revision".to_string()),
            restored_from_revision_id: None,
            created_by_agent_id: None,
            created_by_user_id: routine.responsible_user_id,
            created_at: Utc::now(),
        };

        self.revision_repo
            .create(revision.clone())
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create revision: {}", e)))?;

        Ok(revision)
    }

    /// Check if core fields changed (requires new revision)
    fn should_create_revision(&self, old: &Routine, new: &UpdateRoutineInput) -> bool {
        if new.assignee_agent_id.is_some() && new.assignee_agent_id != Some(old.assignee_agent_id) {
            return true;
        }
        if new.concurrency_policy.is_some() && new.concurrency_policy.as_ref() != Some(&old.concurrency_policy) {
            return true;
        }
        if new.catch_up_policy.is_some() && new.catch_up_policy.as_ref() != Some(&old.catch_up_policy) {
            return true;
        }
        if new.variables.is_some() {
            return true;
        }
        if new.env.is_some() {
            return true;
        }
        false
    }

    /// Check concurrency policy before creating run
    async fn check_concurrency(&self, routine_id: Uuid, policy: ConcurrencyPolicy) -> Result<Option<Uuid>, ServiceError> {
        let active_runs = self.routine_repo
            .find_active_runs(routine_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find active runs: {}", e)))?;

        if active_runs.is_empty() {
            return Ok(None);
        }

        match policy {
            ConcurrencyPolicy::CoalesceIfActive => {
                // Return the first active run to coalesce into
                Ok(Some(active_runs[0].id))
            }
            ConcurrencyPolicy::SkipIfActive => {
                // Signal to skip this run
                Err(ServiceError::InvalidInput("Routine has active run, skipping".to_string()))
            }
            ConcurrencyPolicy::Parallel => {
                // Allow parallel execution
                Ok(None)
            }
        }
    }
}

#[async_trait]
impl RoutineService for DefaultRoutineService {
    async fn create(&self, input: CreateRoutineInput) -> Result<Routine, ServiceError> {
        // Validate variables
        self.validate_variables(&input.variables)?;

        let now = Utc::now();
        let routine = Routine {
            id: Uuid::new_v4(),
            company_id: input.company_id,
            project_id: input.project_id,
            goal_id: input.goal_id,
            parent_issue_id: None,
            title: input.title,
            description: input.description,
            assignee_agent_id: input.assignee_agent_id,
            priority: input.priority,
            status: input.status,
            concurrency_policy: input.concurrency_policy,
            catch_up_policy: input.catch_up_policy,
            variables: serde_json::to_value(&input.variables)
                .map_err(|e| ServiceError::Internal(format!("Failed to serialize variables: {}", e)))?,
            env: input.env,
            latest_revision_id: None,
            latest_revision_number: 0,
            responsible_user_id: input.responsible_user_id,
            last_triggered_at: None,
            last_enqueued_at: None,
            created_at: now,
            updated_at: now,
        };

        // Create routine
        let mut created_routine = self.routine_repo
            .create(routine.clone())
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create routine: {}", e)))?;

        // Create initial revision
        let revision = self.create_initial_revision(&created_routine).await?;

        // Update routine with revision reference
        created_routine.latest_revision_id = Some(revision.id);
        created_routine.latest_revision_number = 1;

        let updated_routine = self.routine_repo
            .update(created_routine)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update routine with revision: {}", e)))?;

        Ok(updated_routine)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Routine, ServiceError> {
        self.routine_repo
            .find_by_id(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find routine: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Routine not found".to_string()))
    }

    async fn update(&self, id: Uuid, input: UpdateRoutineInput) -> Result<Routine, ServiceError> {
        let mut routine = self.get_by_id(id).await?;

        // Validate variables if provided
        if let Some(ref variables) = input.variables {
            self.validate_variables(variables)?;
        }

        // Check if we need a new revision
        let needs_revision = self.should_create_revision(&routine, &input);

        // Apply updates
        if let Some(title) = input.title {
            routine.title = title;
        }
        if let Some(description) = input.description {
            routine.description = Some(description);
        }
        if let Some(status) = input.status {
            routine.status = status;
        }
        if let Some(priority) = input.priority {
            routine.priority = priority;
        }
        if let Some(assignee_agent_id) = input.assignee_agent_id {
            routine.assignee_agent_id = assignee_agent_id;
        }
        if let Some(concurrency_policy) = input.concurrency_policy {
            routine.concurrency_policy = concurrency_policy;
        }
        if let Some(catch_up_policy) = input.catch_up_policy {
            routine.catch_up_policy = catch_up_policy;
        }
        if let Some(variables) = input.variables {
            routine.variables = serde_json::to_value(&variables)
                .map_err(|e| ServiceError::Internal(format!("Failed to serialize variables: {}", e)))?;
        }
        if let Some(env) = input.env {
            routine.env = env;
        }

        routine.updated_at = Utc::now();

        // Create new revision if needed
        if needs_revision {
            let new_revision_number = routine.latest_revision_number + 1;
            let snapshot = serde_json::json!({
                "version": 1,
                "routine": {
                    "id": routine.id,
                    "title": routine.title,
                    "description": routine.description,
                    "assignee_agent_id": routine.assignee_agent_id,
                    "priority": routine.priority,
                    "status": routine.status,
                    "concurrency_policy": routine.concurrency_policy,
                    "catch_up_policy": routine.catch_up_policy,
                    "variables": routine.variables,
                    "env": routine.env,
                }
            });

            let revision = RoutineRevision {
                id: Uuid::new_v4(),
                company_id: routine.company_id,
                routine_id: routine.id,
                revision_number: new_revision_number,
                title: Some(routine.title.clone()),
                description: routine.description.clone(),
                snapshot,
                change_summary: Some("Configuration updated".to_string()),
                restored_from_revision_id: None,
                created_by_agent_id: None,
                created_by_user_id: routine.responsible_user_id,
                created_at: Utc::now(),
            };

            self.revision_repo
                .create(revision.clone())
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to create revision: {}", e)))?;

            routine.latest_revision_id = Some(revision.id);
            routine.latest_revision_number = new_revision_number;
        }

        // Update routine
        let updated_routine = self.routine_repo
            .update(routine)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update routine: {}", e)))?;

        Ok(updated_routine)
    }

    async fn delete(&self, id: Uuid) -> Result<(), ServiceError> {
        self.routine_repo
            .delete(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to delete routine: {}", e)))
    }

    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<Routine>, ServiceError> {
        self.routine_repo
            .find_by_company_id(company_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to list routines: {}", e)))
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<Routine>, ServiceError> {
        self.routine_repo
            .find_by_assignee_agent_id(agent_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to list routines: {}", e)))
    }

    async fn trigger_manual(&self, routine_id: Uuid, triggered_by: Uuid) -> Result<RoutineRun, ServiceError> {
        let routine = self.get_by_id(routine_id).await?;

        // Check if routine is active
        if routine.status != RoutineStatus::Active {
            return Err(ServiceError::InvalidInput("Routine is not active".to_string()));
        }

        self.fire_routine(routine_id, Uuid::nil(), RunSource::Manual).await
    }

    async fn fire_routine(&self, routine_id: Uuid, trigger_id: Uuid, source: RunSource) -> Result<RoutineRun, ServiceError> {
        let routine = self.get_by_id(routine_id).await?;

        // Check concurrency policy
        let coalesced_into = match self.check_concurrency(routine_id, routine.concurrency_policy).await {
            Ok(Some(run_id)) => {
                // Coalesce into existing run
                let mut run = RoutineRun {
                    id: Uuid::new_v4(),
                    company_id: routine.company_id,
                    routine_id,
                    trigger_id: if trigger_id.is_nil() { None } else { Some(trigger_id) },
                    source,
                    status: RunStatus::Coalesced,
                    triggered_at: Utc::now(),
                    routine_revision_id: routine.latest_revision_id,
                    idempotency_key: None,
                    trigger_payload: serde_json::json!({}),
                    dispatch_fingerprint: None,
                    linked_issue_id: None,
                    coalesced_into_run_id: Some(run_id),
                    failure_reason: None,
                    completed_at: None,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };

                self.routine_repo
                    .create_run(run.clone())
                    .await
                    .map_err(|e| ServiceError::Internal(format!("Failed to create coalesced run: {}", e)))?;

                return Ok(run);
            }
            Ok(None) => None,
            Err(e) if e.to_string().contains("skipping") => {
                // Create skipped run
                let mut run = RoutineRun {
                    id: Uuid::new_v4(),
                    company_id: routine.company_id,
                    routine_id,
                    trigger_id: if trigger_id.is_nil() { None } else { Some(trigger_id) },
                    source,
                    status: RunStatus::Skipped,
                    triggered_at: Utc::now(),
                    routine_revision_id: routine.latest_revision_id,
                    idempotency_key: None,
                    trigger_payload: serde_json::json!({}),
                    dispatch_fingerprint: None,
                    linked_issue_id: None,
                    coalesced_into_run_id: None,
                    failure_reason: Some("Active run exists".to_string()),
                    completed_at: Some(Utc::now()),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };

                self.routine_repo
                    .create_run(run.clone())
                    .await
                    .map_err(|e| ServiceError::Internal(format!("Failed to create skipped run: {}", e)))?;

                return Ok(run);
            }
            Err(e) => return Err(e),
        };

        // Create new run
        let run = RoutineRun {
            id: Uuid::new_v4(),
            company_id: routine.company_id,
            routine_id,
            trigger_id: if trigger_id.is_nil() { None } else { Some(trigger_id) },
            source,
            status: RunStatus::Queued,
            triggered_at: Utc::now(),
            routine_revision_id: routine.latest_revision_id,
            idempotency_key: None,
            trigger_payload: serde_json::json!({}),
            dispatch_fingerprint: Some(format!("{}:{}", routine_id, Uuid::new_v4())),
            linked_issue_id: None,
            coalesced_into_run_id: None,
            failure_reason: None,
            completed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let created_run = self.routine_repo
            .create_run(run)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create run: {}", e)))?;

        // Update routine last_enqueued_at
        let mut updated_routine = routine.clone();
        updated_routine.last_enqueued_at = Some(Utc::now());
        self.routine_repo
            .update(updated_routine)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update routine: {}", e)))?;

        Ok(created_run)
    }

    async fn get_revisions(&self, routine_id: Uuid) -> Result<Vec<RoutineRevision>, ServiceError> {
        self.revision_repo
            .find_by_routine_id(routine_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get revisions: {}", e)))
    }

    async fn restore_revision(&self, routine_id: Uuid, revision_id: Uuid) -> Result<Routine, ServiceError> {
        let revision = self.revision_repo
            .find_by_id(revision_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find revision: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Revision not found".to_string()))?;

        if revision.routine_id != routine_id {
            return Err(ServiceError::InvalidInput("Revision does not belong to this routine".to_string()));
        }

        // Extract routine data from snapshot
        let snapshot_routine = revision.snapshot.get("routine")
            .ok_or_else(|| ServiceError::Internal("Invalid revision snapshot".to_string()))?;

        let mut routine = self.get_by_id(routine_id).await?;

        // Restore fields from snapshot
        if let Some(title) = snapshot_routine.get("title").and_then(|v| v.as_str()) {
            routine.title = title.to_string();
        }
        if let Some(description) = snapshot_routine.get("description").and_then(|v| v.as_str()) {
            routine.description = Some(description.to_string());
        }
        if let Some(variables) = snapshot_routine.get("variables") {
            routine.variables = variables.clone();
        }
        if let Some(env) = snapshot_routine.get("env") {
            routine.env = env.clone();
        }

        routine.updated_at = Utc::now();

        // Create restoration revision
        let new_revision_number = routine.latest_revision_number + 1;
        let new_revision = RoutineRevision {
            id: Uuid::new_v4(),
            company_id: routine.company_id,
            routine_id: routine.id,
            revision_number: new_revision_number,
            title: Some(routine.title.clone()),
            description: routine.description.clone(),
            snapshot: revision.snapshot.clone(),
            change_summary: Some(format!("Restored from revision {}", revision.revision_number)),
            restored_from_revision_id: Some(revision_id),
            created_by_agent_id: None,
            created_by_user_id: routine.responsible_user_id,
            created_at: Utc::now(),
        };

        self.revision_repo
            .create(new_revision.clone())
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create restoration revision: {}", e)))?;

        routine.latest_revision_id = Some(new_revision.id);
        routine.latest_revision_number = new_revision_number;

        let updated_routine = self.routine_repo
            .update(routine)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update routine: {}", e)))?;

        Ok(updated_routine)
    }

    fn validate_variables(&self, variables: &[RoutineVariable]) -> Result<(), ServiceError> {
        let mut names = std::collections::HashSet::new();

        for var in variables {
            // Check name uniqueness
            if !names.insert(&var.name) {
                return Err(ServiceError::InvalidInput(format!("Duplicate variable name: {}", var.name)));
            }

            // Validate name format (alphanumeric + underscore)
            if !var.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Err(ServiceError::InvalidInput(format!("Invalid variable name: {}", var.name)));
            }

            // Validate default value type matches variable type
            if let Some(ref default_value) = var.default_value {
                let value_matches_type = match var.var_type {
                    RoutineVariableType::Text => default_value.is_string(),
                    RoutineVariableType::Number => default_value.is_number(),
                    RoutineVariableType::Boolean => default_value.is_boolean(),
                    RoutineVariableType::Select => {
                        if let Some(ref options) = var.options {
                            default_value.is_string() && options.iter().any(|opt| {
                                opt.as_str() == default_value.as_str()
                            })
                        } else {
                            false
                        }
                    }
                    RoutineVariableType::Secret => default_value.is_string(),
                };

                if !value_matches_type {
                    return Err(ServiceError::InvalidInput(format!(
                        "Variable '{}' default value type doesn't match declared type",
                        var.name
                    )));
                }
            }

            // Validate required variables have default values or will be provided at runtime
            if var.required && var.default_value.is_none() {
                // This is OK - required variables can be provided at runtime
            }

            // Validate select type has options
            if var.var_type == RoutineVariableType::Select && var.options.is_none() {
                return Err(ServiceError::InvalidInput(format!(
                    "Variable '{}' is select type but has no options",
                    var.name
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_variables() {
        let service = DefaultRoutineService::new(
            Arc::new(MockRoutineRepository::new()),
            Arc::new(MockTriggerRepository::new()),
            Arc::new(MockRevisionRepository::new()),
        );

        // Valid variables
        let vars = vec![
            RoutineVariable {
                name: "api_key".to_string(),
                label: "API Key".to_string(),
                var_type: RoutineVariableType::Secret,
                default_value: None,
                required: true,
                options: None,
            },
            RoutineVariable {
                name: "retry_count".to_string(),
                label: "Retry Count".to_string(),
                var_type: RoutineVariableType::Number,
                default_value: Some(serde_json::json!(3)),
                required: false,
                options: None,
            },
        ];

        assert!(service.validate_variables(&vars).is_ok());

        // Duplicate names
        let vars_dup = vec![
            RoutineVariable {
                name: "key".to_string(),
                label: "Key 1".to_string(),
                var_type: RoutineVariableType::Text,
                default_value: None,
                required: false,
                options: None,
            },
            RoutineVariable {
                name: "key".to_string(),
                label: "Key 2".to_string(),
                var_type: RoutineVariableType::Text,
                default_value: None,
                required: false,
                options: None,
            },
        ];

        assert!(service.validate_variables(&vars_dup).is_err());
    }
}
