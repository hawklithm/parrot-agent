use async_trait::async_trait;
use chrono::{DateTime, Utc};
use repositories::{RoutineTriggerRepository, RoutineRepository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use models::{RoutineTrigger, TriggerType, TriggerStatus};
use crate::errors::ServiceError;

/// Input for creating a routine trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTriggerInput {
    pub routine_id: Uuid,
    pub trigger_type: TriggerType,
    pub config: serde_json::Value,
    pub enabled: bool,
}

/// Input for updating a routine trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTriggerInput {
    pub enabled: Option<bool>,
    pub config: Option<serde_json::Value>,
}

/// Trigger execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerExecutionResult {
    pub trigger_id: Uuid,
    pub routine_id: Uuid,
    pub executed_at: DateTime<Utc>,
    pub success: bool,
    pub run_id: Option<Uuid>,
    pub error_message: Option<String>,
}

/// Routine Trigger Service trait
#[async_trait]
pub trait RoutineTriggerService: Send + Sync {
    /// Create a new trigger
    async fn create(&self, input: CreateTriggerInput) -> Result<RoutineTrigger, ServiceError>;

    /// Get trigger by ID
    async fn get(&self, id: Uuid) -> Result<RoutineTrigger, ServiceError>;

    /// Update trigger
    async fn update(&self, id: Uuid, input: UpdateTriggerInput) -> Result<RoutineTrigger, ServiceError>;

    /// Delete trigger
    async fn delete(&self, id: Uuid) -> Result<(), ServiceError>;

    /// Enable trigger
    async fn enable(&self, id: Uuid) -> Result<RoutineTrigger, ServiceError>;

    /// Disable trigger
    async fn disable(&self, id: Uuid) -> Result<RoutineTrigger, ServiceError>;

    /// List triggers by routine
    async fn list_by_routine(&self, routine_id: Uuid) -> Result<Vec<RoutineTrigger>, ServiceError>;

    /// List triggers by type
    async fn list_by_type(&self, trigger_type: TriggerType) -> Result<Vec<RoutineTrigger>, ServiceError>;

    /// Execute a trigger (fire the routine)
    async fn execute(&self, trigger_id: Uuid) -> Result<TriggerExecutionResult, ServiceError>;

    /// Validate trigger config based on type
    fn validate_trigger_config(&self, trigger_type: TriggerType, config: &serde_json::Value) -> Result<(), ServiceError>;

    /// Get triggers ready for execution (for cron scheduler)
    async fn get_ready_triggers(&self) -> Result<Vec<RoutineTrigger>, ServiceError>;

    /// Record trigger execution
    async fn record_execution(&self, trigger_id: Uuid, success: bool, run_id: Option<Uuid>, error_message: Option<String>) -> Result<(), ServiceError>;
}

/// Default Routine Trigger Service Implementation
pub struct DefaultRoutineTriggerService {
    trigger_repo: Arc<dyn RoutineTriggerRepository>,
    routine_repo: Arc<dyn RoutineRepository>,
}

impl DefaultRoutineTriggerService {
    pub fn new(
        trigger_repo: Arc<dyn RoutineTriggerRepository>,
        routine_repo: Arc<dyn RoutineRepository>,
    ) -> Self {
        Self {
            trigger_repo,
            routine_repo,
        }
    }

    /// Parse cron expression
    fn validate_cron_expression(&self, cron_expr: &str) -> Result<(), ServiceError> {
        // Basic validation - check format (5 or 6 fields)
        let parts: Vec<&str> = cron_expr.split_whitespace().collect();
        if parts.len() != 5 && parts.len() != 6 {
            return Err(ServiceError::InvalidInput(format!(
                "Invalid cron expression format: expected 5 or 6 fields, got {}",
                parts.len()
            )));
        }

        // TODO: Use a proper cron parser library for full validation
        Ok(())
    }

    /// Validate webhook config
    fn validate_webhook_config(&self, config: &serde_json::Value) -> Result<(), ServiceError> {
        // Webhook requires secret for signature verification
        if !config.get("secret").and_then(|v| v.as_str()).is_some() {
            return Err(ServiceError::InvalidInput("Webhook trigger requires 'secret' in config".to_string()));
        }

        // Optional: allowed_sources
        Ok(())
    }

    /// Validate manual trigger config
    fn validate_manual_config(&self, _config: &serde_json::Value) -> Result<(), ServiceError> {
        // Manual triggers have minimal config requirements
        Ok(())
    }
}

#[async_trait]
impl RoutineTriggerService for DefaultRoutineTriggerService {
    async fn create(&self, input: CreateTriggerInput) -> Result<RoutineTrigger, ServiceError> {
        // Verify routine exists
        let routine = self.routine_repo
            .get(input.routine_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to verify routine: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {} not found", input.routine_id)))?;

        // Validate trigger config
        self.validate_trigger_config(input.trigger_type, &input.config)?;

        let trigger = RoutineTrigger {
            id: Uuid::new_v4(),
            company_id: routine.company_id,
            routine_id: input.routine_id,
            kind: match input.trigger_type {
                TriggerType::Schedule => models::TriggerKind::Schedule,
                TriggerType::Webhook => models::TriggerKind::Webhook,
                TriggerType::Manual => models::TriggerKind::Manual,
                TriggerType::Event => models::TriggerKind::Manual,
                TriggerType::Cron => models::TriggerKind::Schedule,
            },
            label: None,
            enabled: input.enabled,
            trigger_type: input.trigger_type,
            config: input.config,
            status: TriggerStatus::Active,
            next_trigger_at: None,
            last_triggered_at: None,
            cron_expression: None,
            timezone: None,
            next_run_at: None,
            last_fired_at: None,
            public_id: None,
            secret_id: None,
            signing_mode: None,
            replay_window_sec: None,
            last_rotated_at: None,
            last_result: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        self.trigger_repo
            .create(trigger)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create trigger: {}", e)))
    }

    async fn get(&self, id: Uuid) -> Result<RoutineTrigger, ServiceError> {
        self.trigger_repo
            .find_by_id(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get trigger: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Trigger {} not found", id)))
    }

    async fn update(&self, id: Uuid, input: UpdateTriggerInput) -> Result<RoutineTrigger, ServiceError> {
        let mut trigger = self.get(id).await?;

        if let Some(enabled) = input.enabled {
            trigger.enabled = enabled;
        }

        if let Some(config) = input.config {
            // Validate new config
            self.validate_trigger_config(trigger.trigger_type, &config)?;
            trigger.config = config;
        }

        trigger.updated_at = Utc::now();

        self.trigger_repo
            .update(trigger)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update trigger: {}", e)))
    }

    async fn delete(&self, id: Uuid) -> Result<(), ServiceError> {
        self.trigger_repo
            .delete(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to delete trigger: {}", e)))
    }

    async fn enable(&self, id: Uuid) -> Result<RoutineTrigger, ServiceError> {
        let mut trigger = self.get(id).await?;
        trigger.enabled = true;
        trigger.updated_at = Utc::now();

        self.trigger_repo
            .update(trigger)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to enable trigger: {}", e)))
    }

    async fn disable(&self, id: Uuid) -> Result<RoutineTrigger, ServiceError> {
        let mut trigger = self.get(id).await?;
        trigger.enabled = false;
        trigger.updated_at = Utc::now();

        self.trigger_repo
            .update(trigger)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to disable trigger: {}", e)))
    }

    async fn list_by_routine(&self, routine_id: Uuid) -> Result<Vec<RoutineTrigger>, ServiceError> {
        self.trigger_repo
            .find_by_routine_id(routine_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to list triggers by routine: {}", e)))
    }

    async fn list_by_type(&self, trigger_type: TriggerType) -> Result<Vec<RoutineTrigger>, ServiceError> {
        let kind = match trigger_type {
            TriggerType::Schedule => models::TriggerKind::Schedule,
            TriggerType::Webhook => models::TriggerKind::Webhook,
            TriggerType::Manual => models::TriggerKind::Manual,
            TriggerType::Event => models::TriggerKind::Manual,
            TriggerType::Cron => models::TriggerKind::Schedule,
        };
        self.trigger_repo
            .find_by_type(kind)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to list triggers by type: {}", e)))
    }

    async fn execute(&self, trigger_id: Uuid) -> Result<TriggerExecutionResult, ServiceError> {
        let trigger = self.get(trigger_id).await?;

        if !trigger.enabled {
            return Err(ServiceError::InvalidInput(format!("Trigger {} is disabled", trigger_id)));
        }

        let routine = self.routine_repo
            .get(trigger.routine_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get routine: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {} not found", trigger.routine_id)))?;

        // TODO: Integrate with RoutineService to fire the routine
        // For now, return a mock result
        let executed_at = Utc::now();

        Ok(TriggerExecutionResult {
            trigger_id,
            routine_id: routine.id,
            executed_at,
            success: true,
            run_id: Some(Uuid::new_v4()),
            error_message: None,
        })
    }

    fn validate_trigger_config(&self, trigger_type: TriggerType, config: &serde_json::Value) -> Result<(), ServiceError> {
        match trigger_type {
            TriggerType::Schedule => {
                // Schedule triggers use cron_expression
                let cron_expr = config.get("cron_expression")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ServiceError::InvalidInput("Schedule trigger requires 'cron_expression' in config".to_string()))?;

                self.validate_cron_expression(cron_expr)?;
                Ok(())
            }
            TriggerType::Cron => {
                // Cron triggers need cron_expression
                let cron_expr = config.get("cron_expression")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ServiceError::InvalidInput("Cron trigger requires 'cron_expression' in config".to_string()))?;

                self.validate_cron_expression(cron_expr)?;
                Ok(())
            }
            TriggerType::Event => {
                // Event triggers - minimal validation
                Ok(())
            }
            TriggerType::Webhook => {
                self.validate_webhook_config(config)
            }
            TriggerType::Manual => {
                self.validate_manual_config(config)
            }
        }
    }

    async fn get_ready_triggers(&self) -> Result<Vec<RoutineTrigger>, ServiceError> {
        // Get all enabled triggers
        let all_triggers = self.trigger_repo
            .find_enabled()
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get enabled triggers: {}", e)))?;

        // Filter triggers that are due for execution
        let now = Utc::now();
        let ready_triggers: Vec<RoutineTrigger> = all_triggers
            .into_iter()
            .filter(|trigger| {
                if let Some(next_trigger_at) = trigger.next_trigger_at {
                    next_trigger_at <= now
                } else {
                    // No next_trigger_at means it's ready (first run for cron triggers)
                    trigger.trigger_type == TriggerType::Cron
                }
            })
            .collect();

        Ok(ready_triggers)
    }

    async fn record_execution(&self, trigger_id: Uuid, success: bool, run_id: Option<Uuid>, error_message: Option<String>) -> Result<(), ServiceError> {
        let mut trigger = self.get(trigger_id).await?;

        trigger.last_triggered_at = Some(Utc::now());

        // Calculate next_trigger_at for cron triggers
        if trigger.trigger_type == TriggerType::Cron {
            if let Some(cron_expr) = trigger.config.get("cron_expression").and_then(|v| v.as_str()) {
                // TODO: Use cron parser to calculate next execution time
                // For now, set it to 1 hour from now as a placeholder
                trigger.next_trigger_at = Some(Utc::now() + chrono::Duration::hours(1));
            }
        }

        if !success {
            trigger.status = TriggerStatus::Failed;
        }

        trigger.updated_at = Utc::now();

        self.trigger_repo
            .update(trigger)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to record execution: {}", e)))?;

        // TODO: Store execution history in a separate table
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_cron_expression() {
        let service = DefaultRoutineTriggerService::new(
            Arc::new(MockRoutineTriggerRepository::new()),
            Arc::new(MockRoutineRepository::new()),
        );

        // Valid 5-field cron
        assert!(service.validate_cron_expression("0 9 * * *").is_ok());

        // Valid 6-field cron
        assert!(service.validate_cron_expression("0 0 9 * * *").is_ok());

        // Invalid - too few fields
        assert!(service.validate_cron_expression("0 9 *").is_err());

        // Invalid - too many fields
        assert!(service.validate_cron_expression("0 0 9 * * * * extra").is_err());
    }

    #[test]
    fn test_validate_webhook_config() {
        let service = DefaultRoutineTriggerService::new(
            Arc::new(MockRoutineTriggerRepository::new()),
            Arc::new(MockRoutineRepository::new()),
        );

        // Valid webhook config
        let valid_config = serde_json::json!({
            "secret": "webhook-secret-123",
            "allowed_sources": ["192.168.1.0/24"]
        });
        assert!(service.validate_webhook_config(&valid_config).is_ok());

        // Missing secret
        let invalid_config = serde_json::json!({
            "allowed_sources": ["192.168.1.0/24"]
        });
        assert!(service.validate_webhook_config(&invalid_config).is_err());
    }
}
