use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::goal::{Goal, GoalStatus};

#[async_trait]
pub trait GoalRepository: Send + Sync {
    async fn create(&self, goal: Goal) -> RepositoryResult<Goal>;
    async fn get(&self, goal_id: Uuid) -> RepositoryResult<Option<Goal>>;
    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<Goal>>;
    async fn list_by_agent(&self, agent_id: Uuid) -> RepositoryResult<Vec<Goal>>;
    async fn list_children(&self, parent_goal_id: Uuid) -> RepositoryResult<Vec<Goal>>;
    async fn update(&self, goal: Goal) -> RepositoryResult<Goal>;
    async fn delete(&self, goal_id: Uuid) -> RepositoryResult<()>;
}

pub struct PostgresGoalRepository {
    pool: PgPool,
}

impl PostgresGoalRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GoalRepository for PostgresGoalRepository {
    async fn create(&self, goal: Goal) -> RepositoryResult<Goal> {
        sqlx::query(
            r#"INSERT INTO goals
               (id, company_id, parent_goal_id, agent_id, name, description, status, priority,
                target_completion_date, completed_at, created_at, updated_at, created_by_user_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#
        )
        .bind(goal.id)
        .bind(goal.company_id)
        .bind(goal.parent_goal_id)
        .bind(goal.agent_id)
        .bind(&goal.name)
        .bind(&goal.description)
        .bind(&goal.status)
        .bind(&goal.priority)
        .bind(goal.target_completion_date)
        .bind(goal.completed_at)
        .bind(goal.created_at)
        .bind(goal.updated_at)
        .bind(goal.created_by_user_id)
        .execute(&self.pool)
        .await?;
        Ok(goal)
    }

    async fn get(&self, goal_id: Uuid) -> RepositoryResult<Option<Goal>> {
        let goal = sqlx::query_as::<_, Goal>(
            r#"SELECT id, company_id, parent_goal_id, agent_id, name, description, status, priority,
                      target_completion_date, completed_at, created_at, updated_at, created_by_user_id
               FROM goals WHERE id = $1"#
        )
        .bind(goal_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(goal)
    }

    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<Goal>> {
        let goals = sqlx::query_as::<_, Goal>(
            r#"SELECT id, company_id, parent_goal_id, agent_id, name, description, status, priority,
                      target_completion_date, completed_at, created_at, updated_at, created_by_user_id
               FROM goals WHERE company_id = $1 ORDER BY created_at DESC"#
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(goals)
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> RepositoryResult<Vec<Goal>> {
        let goals = sqlx::query_as::<_, Goal>(
            r#"SELECT id, company_id, parent_goal_id, agent_id, name, description, status, priority,
                      target_completion_date, completed_at, created_at, updated_at, created_by_user_id
               FROM goals WHERE agent_id = $1 ORDER BY created_at DESC"#
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(goals)
    }

    async fn list_children(&self, parent_goal_id: Uuid) -> RepositoryResult<Vec<Goal>> {
        let goals = sqlx::query_as::<_, Goal>(
            r#"SELECT id, company_id, parent_goal_id, agent_id, name, description, status, priority,
                      target_completion_date, completed_at, created_at, updated_at, created_by_user_id
               FROM goals WHERE parent_goal_id = $1 ORDER BY created_at DESC"#
        )
        .bind(parent_goal_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(goals)
    }

    async fn update(&self, goal: Goal) -> RepositoryResult<Goal> {
        sqlx::query(
            r#"UPDATE goals
               SET name = $2, description = $3, status = $4, priority = $5,
                   target_completion_date = $6, completed_at = $7, updated_at = $8
               WHERE id = $1"#
        )
        .bind(goal.id)
        .bind(&goal.name)
        .bind(&goal.description)
        .bind(&goal.status)
        .bind(&goal.priority)
        .bind(goal.target_completion_date)
        .bind(goal.completed_at)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(goal)
    }

    async fn delete(&self, goal_id: Uuid) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM goals WHERE id = $1")
            .bind(goal_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
