use async_trait::async_trait;
use repositories::{GoalRepository, IssueRepository};
use std::sync::Arc;
use uuid::Uuid;

use models::{Goal, GoalLevel, GoalStatus};
use crate::ServiceError;

/// Goal Service trait
#[async_trait]
pub trait GoalService: Send + Sync {
    /// Create a new goal
    async fn create(&self, input: CreateGoalInput) -> Result<Goal, ServiceError>;

    /// Get goal by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Goal, ServiceError>;

    /// Update goal
    async fn update(&self, id: Uuid, input: UpdateGoalInput) -> Result<Goal, ServiceError>;

    /// Delete goal
    async fn delete(&self, id: Uuid) -> Result<(), ServiceError>;

    /// List goals by company
    async fn list_by_company(&self, company_id: Uuid, level: Option<GoalLevel>) -> Result<Vec<Goal>, ServiceError>;

    /// Calculate goal progress based on child issues and sub-goals
    async fn calculate_progress(&self, goal_id: Uuid) -> Result<f64, ServiceError>;

    /// Mark goal as achieved
    async fn mark_achieved(&self, goal_id: Uuid) -> Result<Goal, ServiceError>;

    /// Get goal hierarchy (parent and children)
    async fn get_hierarchy(&self, goal_id: Uuid) -> Result<GoalHierarchy, ServiceError>;
}

#[derive(Debug, Clone)]
pub struct CreateGoalInput {
    pub company_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub level: GoalLevel,
    pub parent_id: Option<Uuid>,
    pub owner_agent_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct UpdateGoalInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<GoalStatus>,
    pub owner_agent_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct GoalHierarchy {
    pub goal: Goal,
    pub parent: Option<Box<Goal>>,
    pub children: Vec<Goal>,
    pub progress: f64,
}

/// Default Goal Service Implementation
pub struct DefaultGoalService {
    goal_repo: Arc<dyn GoalRepository>,
    issue_repo: Arc<dyn IssueRepository>,
}

impl DefaultGoalService {
    pub fn new(goal_repo: Arc<dyn GoalRepository>, issue_repo: Arc<dyn IssueRepository>) -> Self {
        Self { goal_repo, issue_repo }
    }

    async fn validate_hierarchy(&self, parent_id: Option<Uuid>, level: GoalLevel) -> Result<(), ServiceError> {
        if let Some(parent_id) = parent_id {
            let parent = self.goal_repo
                .find_by_id(parent_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to find parent goal: {}", e)))?
                .ok_or_else(|| ServiceError::NotFound("Parent goal not found".to_string()))?;

            // Validate level hierarchy: company -> project -> task
            match (parent.level, level) {
                (GoalLevel::Company, GoalLevel::Project) => Ok(()),
                (GoalLevel::Company, GoalLevel::Task) => Ok(()),
                (GoalLevel::Project, GoalLevel::Task) => Ok(()),
                _ => Err(ServiceError::InvalidInput("Invalid goal hierarchy".to_string())),
            }
        } else {
            Ok(())
        }
    }

    async fn detect_cycle(&self, goal_id: Uuid, parent_id: Uuid) -> Result<bool, ServiceError> {
        let mut current_id = parent_id;
        let mut visited = std::collections::HashSet::new();

        loop {
            if current_id == goal_id {
                return Ok(true); // Cycle detected
            }

            if visited.contains(&current_id) {
                return Ok(false); // Already visited, no cycle to goal_id
            }

            visited.insert(current_id);

            let current = self.goal_repo
                .find_by_id(current_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to find goal: {}", e)))?
                .ok_or_else(|| ServiceError::NotFound("Goal not found".to_string()))?;

            match current.parent_id {
                Some(pid) => current_id = pid,
                None => return Ok(false), // Reached root, no cycle
            }
        }
    }
}

#[async_trait]
impl GoalService for DefaultGoalService {
    async fn create(&self, input: CreateGoalInput) -> Result<Goal, ServiceError> {
        // Validate hierarchy
        self.validate_hierarchy(input.parent_id, input.level).await?;

        let now = chrono::Utc::now();
        let goal = Goal {
            id: Uuid::new_v4(),
            company_id: input.company_id,
            title: input.title.clone(),
            name: input.title,
            description: input.description,
            level: input.level,
            status: GoalStatus::Planned,
            priority: models::GoalPriority::Medium,
            parent_id: input.parent_id,
            owner_agent_id: input.owner_agent_id,
            created_at: now,
            updated_at: now,
        };

        self.goal_repo
            .create(goal.clone())
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create goal: {}", e)))?;

        Ok(goal)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Goal, ServiceError> {
        self.goal_repo
            .find_by_id(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find goal: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Goal not found".to_string()))
    }

    async fn update(&self, id: Uuid, input: UpdateGoalInput) -> Result<Goal, ServiceError> {
        let mut goal = self.get_by_id(id).await?;

        if let Some(title) = input.title {
            goal.title = title;
        }
        if let Some(description) = input.description {
            goal.description = Some(description);
        }
        if let Some(status) = input.status {
            goal.status = status;
        }
        if let Some(owner_agent_id) = input.owner_agent_id {
            goal.owner_agent_id = Some(owner_agent_id);
        }

        goal.updated_at = chrono::Utc::now();

        self.goal_repo
            .update(goal.clone())
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update goal: {}", e)))?;

        Ok(goal)
    }

    async fn delete(&self, id: Uuid) -> Result<(), ServiceError> {
        // Check for children
        let children = self.goal_repo
            .find_by_parent_id(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find child goals: {}", e)))?;

        if !children.is_empty() {
            return Err(ServiceError::InvalidInput("Cannot delete goal with children".to_string()));
        }

        self.goal_repo
            .delete(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to delete goal: {}", e)))?;

        Ok(())
    }

    async fn list_by_company(&self, company_id: Uuid, level: Option<GoalLevel>) -> Result<Vec<Goal>, ServiceError> {
        self.goal_repo
            .find_by_company_id(company_id, level)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to list goals: {}", e)))
    }

    async fn calculate_progress(&self, goal_id: Uuid) -> Result<f64, ServiceError> {
        // Calculate progress based on:
        // 1. Direct issues linked to this goal
        // 2. Child goals' progress (recurse)

        let issues = self.issue_repo
            .find_by_goal_id(goal_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find issues: {}", e)))?;

        let child_goals = self.goal_repo
            .find_by_parent_id(goal_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find child goals: {}", e)))?;

        let mut total_weight = 0.0;
        let mut completed_weight = 0.0;

        // Weight from issues (each issue = 1 weight)
        for issue in &issues {
            total_weight += 1.0;
            if matches!(issue.status, models::IssueStatus::Done | models::IssueStatus::Cancelled) {
                completed_weight += 1.0;
            }
        }

        // Weight from child goals (recursive, each goal = 10 weight)
        for child in &child_goals {
            total_weight += 10.0;
            let child_progress = self.calculate_progress(child.id).await?;
            completed_weight += 10.0 * (child_progress / 100.0);
        }

        let progress = if total_weight > 0.0 {
            (completed_weight / total_weight * 100.0).min(100.0)
        } else {
            0.0
        };

        Ok(progress)
    }

    async fn mark_achieved(&self, goal_id: Uuid) -> Result<Goal, ServiceError> {
        let mut goal = self.get_by_id(goal_id).await?;
        goal.status = GoalStatus::Achieved;
        goal.updated_at = chrono::Utc::now();

        self.goal_repo
            .update(goal.clone())
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update goal: {}", e)))?;

        Ok(goal)
    }

    async fn get_hierarchy(&self, goal_id: Uuid) -> Result<GoalHierarchy, ServiceError> {
        let goal = self.get_by_id(goal_id).await?;

        let parent = if let Some(parent_id) = goal.parent_id {
            Some(Box::new(self.get_by_id(parent_id).await?))
        } else {
            None
        };

        let children = self.goal_repo
            .find_by_parent_id(goal_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find children: {}", e)))?;

        let progress = self.calculate_progress(goal_id).await?;

        Ok(GoalHierarchy {
            goal,
            parent,
            children,
            progress,
        })
    }
}
