use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};
use models::routine::{Routine, RoutineRun, RoutineTriggerConfig, RoutineStatus};
use models::goal::{Goal, GoalPriority, GoalStatus};
use repositories::routine_repository::RoutineRepository;
use repositories::goal_repository::GoalRepository;

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

#[async_trait]
pub trait GoalService: Send + Sync {
    async fn create_goal(
        &self,
        company_id: Uuid,
        name: String,
        description: Option<String>,
        priority: GoalPriority,
        created_by_user_id: Uuid,
    ) -> ServiceResult<Goal>;

    async fn get_goal(&self, goal_id: Uuid) -> ServiceResult<Option<Goal>>;
    async fn list_goals(&self, company_id: Uuid) -> ServiceResult<Vec<Goal>>;
    async fn list_goals_by_agent(&self, agent_id: Uuid) -> ServiceResult<Vec<Goal>>;
    async fn list_child_goals(&self, parent_goal_id: Uuid) -> ServiceResult<Vec<Goal>>;
    async fn update_goal(&self, goal_id: Uuid, name: Option<String>, description: Option<String>, priority: Option<GoalPriority>) -> ServiceResult<Goal>;
    async fn complete_goal(&self, goal_id: Uuid) -> ServiceResult<Goal>;
    async fn abandon_goal(&self, goal_id: Uuid) -> ServiceResult<Goal>;
    async fn delete_goal(&self, goal_id: Uuid) -> ServiceResult<()>;
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
        let routine = Routine::new(company_id, agent_id, name, description, trigger_config, created_by_user_id);
        self.repository.create(routine).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn get_routine(&self, routine_id: Uuid) -> ServiceResult<Option<Routine>> {
        self.repository.get(routine_id).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn list_routines(&self, company_id: Uuid) -> ServiceResult<Vec<Routine>> {
        self.repository.list_by_company(company_id).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn list_routines_by_agent(&self, agent_id: Uuid) -> ServiceResult<Vec<Routine>> {
        self.repository.list_by_agent(agent_id).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn update_routine(&self, routine_id: Uuid, name: Option<String>, description: Option<String>) -> ServiceResult<Routine> {
        let mut routine = self.repository.get(routine_id).await
            .map_err(|e| ServiceError::Database(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {} not found", routine_id)))?;

        if let Some(n) = name {
            routine.name = n;
        }
        if let Some(d) = description {
            routine.description = Some(d);
        }

        self.repository.update(routine).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn delete_routine(&self, routine_id: Uuid) -> ServiceResult<()> {
        self.repository.delete(routine_id).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn pause_routine(&self, routine_id: Uuid) -> ServiceResult<Routine> {
        let mut routine = self.repository.get(routine_id).await
            .map_err(|e| ServiceError::Database(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {} not found", routine_id)))?;

        routine.status = RoutineStatus::Paused;
        self.repository.update(routine).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn resume_routine(&self, routine_id: Uuid) -> ServiceResult<Routine> {
        let mut routine = self.repository.get(routine_id).await
            .map_err(|e| ServiceError::Database(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {} not found", routine_id)))?;

        routine.status = RoutineStatus::Active;
        self.repository.update(routine).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn trigger_routine(&self, routine_id: Uuid, trigger_source: String) -> ServiceResult<RoutineRun> {
        let run = RoutineRun::new(routine_id, trigger_source);
        self.repository.create_run(run).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn list_runs(&self, routine_id: Uuid, limit: i64) -> ServiceResult<Vec<RoutineRun>> {
        self.repository.list_runs(routine_id, limit).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn get_run(&self, run_id: Uuid) -> ServiceResult<Option<RoutineRun>> {
        self.repository.get_run(run_id).await.map_err(|e| ServiceError::Database(e.to_string()))
    }
}

pub struct GoalServiceImpl {
    repository: Arc<dyn GoalRepository>,
}

impl GoalServiceImpl {
    pub fn new(repository: Arc<dyn GoalRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl GoalService for GoalServiceImpl {
    async fn create_goal(
        &self,
        company_id: Uuid,
        name: String,
        description: Option<String>,
        priority: GoalPriority,
        created_by_user_id: Uuid,
    ) -> ServiceResult<Goal> {
        let goal = Goal::new(company_id, name, description, priority, created_by_user_id);
        self.repository.create(goal).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn get_goal(&self, goal_id: Uuid) -> ServiceResult<Option<Goal>> {
        self.repository.get(goal_id).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn list_goals(&self, company_id: Uuid) -> ServiceResult<Vec<Goal>> {
        self.repository.list_by_company(company_id).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn list_goals_by_agent(&self, agent_id: Uuid) -> ServiceResult<Vec<Goal>> {
        self.repository.list_by_agent(agent_id).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn list_child_goals(&self, parent_goal_id: Uuid) -> ServiceResult<Vec<Goal>> {
        self.repository.list_children(parent_goal_id).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn update_goal(&self, goal_id: Uuid, name: Option<String>, description: Option<String>, priority: Option<GoalPriority>) -> ServiceResult<Goal> {
        let mut goal = self.repository.get(goal_id).await
            .map_err(|e| ServiceError::Database(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Goal {} not found", goal_id)))?;

        if let Some(n) = name {
            goal.name = n;
        }
        if let Some(d) = description {
            goal.description = Some(d);
        }
        if let Some(p) = priority {
            goal.priority = p;
        }

        self.repository.update(goal).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn complete_goal(&self, goal_id: Uuid) -> ServiceResult<Goal> {
        let mut goal = self.repository.get(goal_id).await
            .map_err(|e| ServiceError::Database(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Goal {} not found", goal_id)))?;

        goal.complete();
        self.repository.update(goal).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn abandon_goal(&self, goal_id: Uuid) -> ServiceResult<Goal> {
        let mut goal = self.repository.get(goal_id).await
            .map_err(|e| ServiceError::Database(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Goal {} not found", goal_id)))?;

        goal.abandon();
        self.repository.update(goal).await.map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn delete_goal(&self, goal_id: Uuid) -> ServiceResult<()> {
        self.repository.delete(goal_id).await.map_err(|e| ServiceError::Database(e.to_string()))
    }
}
