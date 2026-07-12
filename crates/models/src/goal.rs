use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "goal_level", rename_all = "snake_case")]
pub enum GoalLevel {
    Company,
    Project,
    Task,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "goal_status", rename_all = "snake_case")]
pub enum GoalStatus {
    Planned,
    Active,
    Completed,
    Archived,
    Achieved,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "goal_priority", rename_all = "snake_case")]
pub enum GoalPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalMetrics {
    pub target_completion_date: Option<DateTime<Utc>>,
    pub progress_percentage: Option<f64>,
    pub total_routines: i64,
    pub completed_routines: i64,
    pub total_issues: i64,
    pub completed_issues: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Goal {
    pub id: Uuid,
    pub company_id: Uuid,
    pub title: String,
    pub name: String,
    pub description: Option<String>,
    pub level: GoalLevel,
    pub status: GoalStatus,
    pub priority: GoalPriority,
    pub parent_id: Option<Uuid>,
    pub owner_agent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGoalInput {
    pub company_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub level: GoalLevel,
    pub parent_id: Option<Uuid>,
    pub owner_agent_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateGoalInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<GoalStatus>,
    pub owner_agent_id: Option<Uuid>,
}

impl Goal {
    pub fn new(input: CreateGoalInput) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id: input.company_id,
            title: input.title.clone(),
            name: input.title,
            description: input.description,
            level: input.level,
            status: GoalStatus::Planned,
            priority: GoalPriority::Medium,
            parent_id: input.parent_id,
            owner_agent_id: input.owner_agent_id,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn mark_active(&mut self) {
        self.status = GoalStatus::Active;
        self.updated_at = Utc::now();
    }

    pub fn mark_completed(&mut self) {
        self.status = GoalStatus::Completed;
        self.updated_at = Utc::now();
    }

    pub fn mark_archived(&mut self) {
        self.status = GoalStatus::Archived;
        self.updated_at = Utc::now();
    }
}
