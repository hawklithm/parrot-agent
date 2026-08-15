/// Issue Goal Fallback Service
/// 
/// Issue Goal 降级策略管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum IssueGoalFallbackError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type IssueGoalFallbackResult<T> = Result<T, IssueGoalFallbackError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FallbackStrategy {
    RetryWithRelaxedConstraints,
    SimplifiedGoal,
    SubsetGoal,
    UserIntervention,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalFallback {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub original_goal: String,
    pub fallback_goal: String,
    pub strategy: FallbackStrategy,
    pub reason: String,
    pub applied_at: chrono::DateTime<chrono::Utc>,
}

pub struct IssueGoalFallbackService {
    pool: PgPool,
}

impl IssueGoalFallbackService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn apply_fallback(
        &self,
        issue_id: Uuid,
        original_goal: String,
        fallback_goal: String,
        strategy: FallbackStrategy,
        reason: String,
    ) -> IssueGoalFallbackResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO goal_fallbacks 
            (id, issue_id, original_goal, fallback_goal, strategy, reason, applied_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(issue_id)
        .bind(&original_goal)
        .bind(&fallback_goal)
        .bind(format!("{:?}", strategy))
        .bind(&reason)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        // 更新Issue的goal
        sqlx::query(
            r#"
            UPDATE issues 
            SET goal = $1, goal_fallback_applied = true
            WHERE id = $2
            "#
        )
        .bind(&fallback_goal)
        .bind(issue_id)
        .execute(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_fallback_history(&self, issue_id: Uuid) -> IssueGoalFallbackResult<Vec<GoalFallback>> {
        let rows = sqlx::query(
            r#"
            SELECT id, issue_id, original_goal, fallback_goal, strategy, reason, applied_at
            FROM goal_fallbacks
            WHERE issue_id = $1
            ORDER BY applied_at DESC
            "#
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await?;
        
        let fallbacks = rows.into_iter().map(|row| {
            GoalFallback {
                id: row.get("id"),
                issue_id: row.get("issue_id"),
                original_goal: row.get("original_goal"),
                fallback_goal: row.get("fallback_goal"),
                strategy: parse_strategy(row.get("strategy")),
                reason: row.get("reason"),
                applied_at: row.get("applied_at"),
            }
        }).collect();
        
        Ok(fallbacks)
    }
    
    pub async fn suggest_fallback_strategy(
        &self,
        _issue_id: Uuid,
        failure_reason: &str,
    ) -> IssueGoalFallbackResult<FallbackStrategy> {
        // 基于失败原因建议降级策略
        let strategy = if failure_reason.contains("timeout") || failure_reason.contains("resource") {
            FallbackStrategy::SimplifiedGoal
        } else if failure_reason.contains("constraint") || failure_reason.contains("validation") {
            FallbackStrategy::RetryWithRelaxedConstraints
        } else if failure_reason.contains("partial") {
            FallbackStrategy::SubsetGoal
        } else {
            FallbackStrategy::UserIntervention
        };
        
        Ok(strategy)
    }
}

fn parse_strategy(s: &str) -> FallbackStrategy {
    match s {
        "RetryWithRelaxedConstraints" => FallbackStrategy::RetryWithRelaxedConstraints,
        "SimplifiedGoal" => FallbackStrategy::SimplifiedGoal,
        "SubsetGoal" => FallbackStrategy::SubsetGoal,
        "UserIntervention" => FallbackStrategy::UserIntervention,
        _ => FallbackStrategy::UserIntervention,
    }
}
