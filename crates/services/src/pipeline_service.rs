use async_trait::async_trait;
use chrono::Utc;
use repositories::{PipelineRepository, PipelineCaseRepository, PipelineStageRepository, PipelineTransitionRepository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use models::{
    Pipeline, PipelineStage, PipelineCase, PipelineTransition, CaseEvent,
    PipelineStageKind, TerminalKind, PipelineStageConfig, CreatePipelineInput, CreateStageInput
};
use crate::ServiceError;

/// Case advancement input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceCaseInput {
    pub case_id: Uuid,
    pub to_stage_id: Uuid,
    pub actor_type: Option<String>,
    pub actor_id: Option<Uuid>,
    pub note: Option<String>,
}

/// Case creation input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCaseInput {
    pub pipeline_id: Uuid,
    pub stage_id: Uuid,
    pub case_key: String,
    pub title: String,
    pub summary: Option<String>,
    pub fields: serde_json::Value,
}

/// Pipeline Service trait
#[async_trait]
pub trait PipelineService: Send + Sync {
    /// Create pipeline with stages and transitions
    async fn create_pipeline(&self, input: CreatePipelineInput) -> Result<Pipeline, ServiceError>;

    /// Get pipeline by ID
    async fn get_pipeline(&self, id: Uuid) -> Result<Pipeline, ServiceError>;

    /// Create a case in pipeline
    async fn create_case(&self, input: CreateCaseInput) -> Result<PipelineCase, ServiceError>;

    /// Advance case to next stage
    async fn advance_case(&self, input: AdvanceCaseInput) -> Result<PipelineCase, ServiceError>;

    /// Get case by ID
    async fn get_case(&self, id: Uuid) -> Result<PipelineCase, ServiceError>;

    /// List cases in pipeline
    async fn list_cases(&self, pipeline_id: Uuid, stage_id: Option<Uuid>) -> Result<Vec<PipelineCase>, ServiceError>;

    /// Mark case as terminal (done/cancelled)
    async fn mark_terminal(&self, case_id: Uuid, kind: TerminalKind) -> Result<PipelineCase, ServiceError>;

    /// Validate transition is allowed
    async fn validate_transition(&self, case_id: Uuid, to_stage_id: Uuid) -> Result<bool, ServiceError>;

    /// Get case history (events)
    async fn get_case_events(&self, case_id: Uuid) -> Result<Vec<CaseEvent>, ServiceError>;
}

/// Default Pipeline Service Implementation
pub struct DefaultPipelineService {
    pipeline_repo: Arc<dyn PipelineRepository>,
    case_repo: Arc<dyn PipelineCaseRepository>,
    stage_repo: Arc<dyn PipelineStageRepository>,
    transition_repo: Arc<dyn PipelineTransitionRepository>,
}

impl DefaultPipelineService {
    pub fn new(
        pipeline_repo: Arc<dyn PipelineRepository>,
        case_repo: Arc<dyn PipelineCaseRepository>,
        stage_repo: Arc<dyn PipelineStageRepository>,
        transition_repo: Arc<dyn PipelineTransitionRepository>,
    ) -> Self {
        Self {
            pipeline_repo,
            case_repo,
            stage_repo,
            transition_repo,
        }
    }

    /// Record case event
    async fn record_event(
        &self,
        case_id: Uuid,
        event_type: String,
        payload: serde_json::Value,
        actor_type: Option<String>,
        actor_id: Option<Uuid>,
    ) -> Result<(), ServiceError> {
        let event = CaseEvent {
            id: Uuid::new_v4(),
            case_id,
            event_type,
            payload,
            actor_type,
            actor_id,
            created_at: Utc::now(),
        };

        self.case_repo
            .create_event(event)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to record event: {}", e)))?;

        Ok(())
    }

    /// Check if stage is terminal
    fn is_terminal_stage(&self, stage_kind: PipelineStageKind) -> bool {
        matches!(stage_kind, PipelineStageKind::Done | PipelineStageKind::Cancelled)
    }

    /// Evaluate transition conditions
    async fn evaluate_conditions(&self, case: &PipelineCase, conditions: &serde_json::Value) -> Result<bool, ServiceError> {
        // Simplified condition evaluation
        // Full implementation would parse conditions and evaluate against case state

        if conditions.is_null() || conditions.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            return Ok(true); // No conditions = always allowed
        }

        // TODO: Implement proper condition evaluation
        // Examples: field_equals, field_not_empty, has_approvals, etc.

        Ok(true)
    }
}

#[async_trait]
impl PipelineService for DefaultPipelineService {
    async fn create_pipeline(&self, input: CreatePipelineInput) -> Result<Pipeline, ServiceError> {
        let now = Utc::now();

        // Create pipeline
        let pipeline = Pipeline {
            id: Uuid::new_v4(),
            company_id: input.company_id,
            key: input.key,
            name: input.name,
            description: input.description,
            project_id: input.project_id,
            enforce_transitions: input.enforce_transitions,
            created_at: now,
            updated_at: now,
        };

        let created_pipeline = self.pipeline_repo
            .create(pipeline.clone())
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create pipeline: {}", e)))?;

        // Create stages
        let mut stage_ids = std::collections::HashMap::new();
        for stage_input in &input.stages {
            let stage = PipelineStage {
                id: Uuid::new_v4(),
                pipeline_id: created_pipeline.id,
                key: stage_input.key.clone(),
                name: stage_input.name.clone(),
                kind: stage_input.kind,
                position: stage_input.position,
                config: serde_json::to_value(&stage_input.config)
                    .map_err(|e| ServiceError::Internal(format!("Failed to serialize config: {}", e)))?,
                created_at: now,
                updated_at: now,
            };

            let created_stage = self.stage_repo
                .create(stage)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to create stage: {}", e)))?;

            stage_ids.insert(stage_input.key.clone(), created_stage.id);
        }

        // Create default transitions (sequential flow)
        let mut sorted_stages = input.stages.clone();
        sorted_stages.sort_by_key(|s| s.position);

        for i in 0..sorted_stages.len() - 1 {
            let from_stage_id = stage_ids[&sorted_stages[i].key];
            let to_stage_id = stage_ids[&sorted_stages[i + 1].key];

            let transition = PipelineTransition {
                id: Uuid::new_v4(),
                pipeline_id: created_pipeline.id,
                from_stage_id,
                to_stage_id,
                label: Some(format!("{} -> {}", sorted_stages[i].name, sorted_stages[i + 1].name)),
                conditions: serde_json::json!({}),
            };

            self.transition_repo
                .create(transition)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to create transition: {}", e)))?;
        }

        Ok(created_pipeline)
    }

    async fn get_pipeline(&self, id: Uuid) -> Result<Pipeline, ServiceError> {
        self.pipeline_repo
            .find_by_id(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find pipeline: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Pipeline not found".to_string()))
    }

    async fn create_case(&self, input: CreateCaseInput) -> Result<PipelineCase, ServiceError> {
        // Verify pipeline and stage exist
        let pipeline = self.get_pipeline(input.pipeline_id).await?;

        let stage = self.stage_repo
            .find_by_id(input.stage_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find stage: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Stage not found".to_string()))?;

        if stage.pipeline_id != pipeline.id {
            return Err(ServiceError::InvalidInput("Stage does not belong to pipeline".to_string()));
        }

        let now = Utc::now();
        let case = PipelineCase {
            id: Uuid::new_v4(),
            company_id: pipeline.company_id,
            pipeline_id: input.pipeline_id,
            stage_id: input.stage_id,
            case_key: input.case_key,
            title: input.title,
            summary: input.summary,
            fields: input.fields.clone(),
            terminal_kind: None,
            version: 1,
            pending_suggestion: None,
            created_at: now,
            updated_at: now,
        };

        let created_case = self.case_repo
            .create(case)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create case: {}", e)))?;

        // Record creation event
        self.record_event(
            created_case.id,
            "case.created".to_string(),
            serde_json::json!({
                "case_id": created_case.id,
                "stage_id": input.stage_id,
                "title": created_case.title,
            }),
            None,
            None,
        ).await?;

        Ok(created_case)
    }

    async fn advance_case(&self, input: AdvanceCaseInput) -> Result<PipelineCase, ServiceError> {
        // Get current case
        let mut case = self.get_case(input.case_id).await?;

        // Check if case is terminal
        if case.terminal_kind.is_some() {
            return Err(ServiceError::InvalidInput("Cannot advance terminal case".to_string()));
        }

        // Get target stage
        let to_stage = self.stage_repo
            .find_by_id(input.to_stage_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find target stage: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Target stage not found".to_string()))?;

        // Get pipeline to check enforce_transitions
        let pipeline = self.get_pipeline(case.pipeline_id).await?;

        // Validate transition if pipeline enforces it
        if pipeline.enforce_transitions {
            let valid = self.validate_transition(case.id, input.to_stage_id).await?;
            if !valid {
                return Err(ServiceError::InvalidInput("Invalid transition".to_string()));
            }
        }

        // Record previous stage for event
        let from_stage_id = case.stage_id;

        // Update case with optimistic locking
        case.stage_id = input.to_stage_id;
        case.version += 1;
        case.updated_at = Utc::now();

        // Check if moving to terminal stage
        if self.is_terminal_stage(to_stage.kind) {
            case.terminal_kind = match to_stage.kind {
                PipelineStageKind::Done => Some(TerminalKind::Done),
                PipelineStageKind::Cancelled => Some(TerminalKind::Cancelled),
                _ => None,
            };
        }

        // Update case
        let updated_case = self.case_repo
            .update(case)
            .await
            .map_err(|e| {
                if e.to_string().contains("version") {
                    ServiceError::Conflict("Case was modified by another operation".to_string())
                } else {
                    ServiceError::Internal(format!("Failed to update case: {}", e))
                }
            })?;

        // Record advancement event
        self.record_event(
            updated_case.id,
            "case.advanced".to_string(),
            serde_json::json!({
                "from_stage_id": from_stage_id,
                "to_stage_id": input.to_stage_id,
                "note": input.note,
            }),
            input.actor_type,
            input.actor_id,
        ).await?;

        Ok(updated_case)
    }

    async fn get_case(&self, id: Uuid) -> Result<PipelineCase, ServiceError> {
        self.case_repo
            .find_by_id(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find case: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Case not found".to_string()))
    }

    async fn list_cases(&self, pipeline_id: Uuid, stage_id: Option<Uuid>) -> Result<Vec<PipelineCase>, ServiceError> {
        if let Some(stage_id) = stage_id {
            self.case_repo
                .find_by_stage_id(stage_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to list cases: {}", e)))
        } else {
            self.case_repo
                .find_by_pipeline_id(pipeline_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to list cases: {}", e)))
        }
    }

    async fn mark_terminal(&self, case_id: Uuid, kind: TerminalKind) -> Result<PipelineCase, ServiceError> {
        let mut case = self.get_case(case_id).await?;

        if case.terminal_kind.is_some() {
            return Err(ServiceError::InvalidInput("Case is already terminal".to_string()));
        }

        case.terminal_kind = Some(kind);
        case.version += 1;
        case.updated_at = Utc::now();

        let updated_case = self.case_repo
            .update(case)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update case: {}", e)))?;

        // Record terminal event
        self.record_event(
            updated_case.id,
            "case.terminal".to_string(),
            serde_json::json!({
                "terminal_kind": kind,
            }),
            None,
            None,
        ).await?;

        Ok(updated_case)
    }

    async fn validate_transition(&self, case_id: Uuid, to_stage_id: Uuid) -> Result<bool, ServiceError> {
        let case = self.get_case(case_id).await?;

        // Find valid transitions from current stage
        let transitions = self.transition_repo
            .find_by_from_stage_id(case.stage_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find transitions: {}", e)))?;

        // Check if target stage is reachable
        let valid_transition = transitions.iter().find(|t| t.to_stage_id == to_stage_id);

        match valid_transition {
            Some(transition) => {
                // Evaluate transition conditions
                self.evaluate_conditions(&case, &transition.conditions).await
            }
            None => Ok(false),
        }
    }

    async fn get_case_events(&self, case_id: Uuid) -> Result<Vec<CaseEvent>, ServiceError> {
        self.case_repo
            .find_events_by_case_id(case_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get case events: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_terminal_stage() {
        let service = DefaultPipelineService::new(
            Arc::new(MockPipelineRepository::new()),
            Arc::new(MockCaseRepository::new()),
            Arc::new(MockStageRepository::new()),
            Arc::new(MockTransitionRepository::new()),
        );

        assert!(service.is_terminal_stage(PipelineStageKind::Done));
        assert!(service.is_terminal_stage(PipelineStageKind::Cancelled));
        assert!(!service.is_terminal_stage(PipelineStageKind::Open));
        assert!(!service.is_terminal_stage(PipelineStageKind::Working));
    }
}
