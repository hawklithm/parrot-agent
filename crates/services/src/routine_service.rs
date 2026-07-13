use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};
use chrono::Utc;
use models::routine::{Routine, RoutineRun, RoutineTriggerConfig, RoutineStatus};
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
    async fn list_runs(&self, routine_id: Uuid, limit: i64) -> ServiceResult<Vec<RoutineRun>>;
    async fn get_run(&self, run_id: Uuid) -> ServiceResult<Option<RoutineRun>>;
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
        trigger_config: RoutineTriggerConfig,
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
        let routine = self.repository.get(routine_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {} not found", routine_id)))?;
        let source = match trigger_source.as_str() {
            "schedule" => models::routine::RunSource::Schedule,
            "webhook" => models::routine::RunSource::Webhook,
            _ => models::routine::RunSource::Manual,
        };
        let run = RoutineRun::new(routine.company_id, routine_id, source);
        self.repository.create_run(run).await.map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn list_runs(&self, routine_id: Uuid, limit: i64) -> ServiceResult<Vec<RoutineRun>> {
        self.repository.list_runs(routine_id, limit).await.map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn get_run(&self, run_id: Uuid) -> ServiceResult<Option<RoutineRun>> {
        self.repository.get_run(run_id).await.map_err(|e| ServiceError::Repository(e.to_string()))
    }
}
