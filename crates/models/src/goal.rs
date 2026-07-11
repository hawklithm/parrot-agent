use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "goal_status", rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Completed,
    Abandoned,
    Archived,
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
    pub parent_goal_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub status: GoalStatus,
    pub priority: GoalPriority,
    pub target_completion_date: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by_user_id: Uuid,
}

impl Goal {
    pub fn new(
        company_id: Uuid,
    name: String,
        description: Option<String>,
        priority: GoalPriority,
        created_by_user_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            parent_goal_id: None,
            agent_id: None,
            name,
            description,
            status: GoalStatus::Active,
            priority,
            target_completion_date: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
            created_by_user_id,
        }
    }

    pub fn complete(&mut self) {
        self.status = GoalStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn abandon(&mut self) {
        self.status = GoalStatus::Abandoned;
        self.updated_at = Utc::now();
    }
}
