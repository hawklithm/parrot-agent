use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::goal::{Goal, GoalStatus, GoalLevel};

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
               (id, company_id, title, description, level, status, parent_id,
                owner_agent_id, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#
        )
        .bind(goal.id)
        .bind(goal.company_id)
        .bind(&goal.title)
        .bind(&goal.description)
        .bind(&goal.level)
        .bind(&goal.status)
        .bind(goal.parent_id)
        .bind(goal.owner_agent_id)
        .bind(goal.created_at)
        .bind(goal.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(goal)
    }

    async fn get(&self, goal_id: Uuid) -> RepositoryResult<Option<Goal>> {
        let goal = sqlx::query_as::<_, Goal>(
            r#"SELECT id, company_id, title, description, level, status, parent_id,
                      owner_agent_id, created_at, updated_at
               FROM goals WHERE id = $1"#
        )
        .bind(goal_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(goal)
    }

    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<Goal>> {
        let goals = sqlx::query_as::<_, Goal>(
            r#"SELECT id, company_id, title, description, level, status, parent_id,
                      owner_agent_id, created_at, updated_at
               FROM goals WHERE company_id = $1 ORDER BY created_at DESC"#
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(goals)
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> RepositoryResult<Vec<Goal>> {
        let goals = sqlx::query_as::<_, Goal>(
            r#"SELECT id, company_id, title, description, level, status, parent_id,
                      owner_agent_id, created_at, updated_at
               FROM goals WHERE owner_agent_id = $1 ORDER BY created_at DESC"#
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(goals)
    }

    async fn list_children(&self, parent_goal_id: Uuid) -> RepositoryResult<Vec<Goal>> {
        let goals = sqlx::query_as::<_, Goal>(
            r#"SELECT id, company_id, title, description, level, status, parent_id,
                      owner_agent_id, created_at, updated_at
               FROM goals WHERE parent_id = $1 ORDER BY created_at DESC"#
        )
        .bind(parent_goal_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(goals)
    }

    async fn update(&self, goal: Goal) -> RepositoryResult<Goal> {
        sqlx::query(
            r#"UPDATE goals
               SET title = $2, description = $3, status = $4, level = $5,
                   owner_agent_id = $6, updated_at = $7
               WHERE id = $1"#
        )
        .bind(goal.id)
        .bind(&goal.title)
        .bind(&goal.description)
        .bind(&goal.status)
        .bind(&goal.level)
        .bind(goal.owner_agent_id)
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
