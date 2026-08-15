/// Issue Continuation Service
/// 
/// Issue继续执行摘要和状态管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum IssueContinuationError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("continuation not found: {0}")]
    NotFound(Uuid),
}

pub type IssueContinuationResult<T> = Result<T, IssueContinuationError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueContinuation {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub run_id: Uuid,
    pub continuation_summary: String,
    pub context: serde_json::Value,
    pub reason: ContinuationReason,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContinuationReason {
    UserRequest,
    ErrorRecovery,
    AdditionalInformation,
    ContextExpansion,
    StrategyChange,
}

pub struct IssueContinuationService {
    pool: PgPool,
}

impl IssueContinuationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_continuation(
        &self,
        issue_id: Uuid,
        run_id: Uuid,
        summary: String,
        reason: ContinuationReason,
        context: serde_json::Value,
    ) -> IssueContinuationResult<Uuid> {
        let id = Uuid::new_v4();
        
        sqlx::query_scalar(
            r#"
            INSERT INTO issue_continuations 
            (id, issue_id, run_id, continuation_summary, context, reason, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(issue_id)
        .bind(run_id)
        .bind(&summary)
        .bind(&context)
        .bind(format!("{:?}", reason))
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_continuation(&self, id: Uuid) -> IssueContinuationResult<IssueContinuation> {
        let row = sqlx::query(
            r#"
            SELECT id, issue_id, run_id, continuation_summary, context, reason, created_at
            FROM issue_continuations
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(IssueContinuation {
            id: row.get("id"),
            issue_id: row.get("issue_id"),
            run_id: row.get("run_id"),
            continuation_summary: row.get("continuation_summary"),
            context: row.get("context"),
            reason: parse_reason(row.get("reason")),
            created_at: row.get("created_at"),
        })
    }
    
    pub async fn list_by_issue(&self, issue_id: Uuid) -> IssueContinuationResult<Vec<IssueContinuation>> {
        let rows = sqlx::query(
            r#"
            SELECT id, issue_id, run_id, continuation_summary, context, reason, created_at
            FROM issue_continuations
            WHERE issue_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await?;
        
        let continuations = rows.into_iter().map(|row| {
            IssueContinuation {
                id: row.get("id"),
                issue_id: row.get("issue_id"),
                run_id: row.get("run_id"),
                continuation_summary: row.get("continuation_summary"),
                context: row.get("context"),
                reason: parse_reason(row.get("reason")),
                created_at: row.get("created_at"),
            }
        }).collect();
        
        Ok(continuations)
    }
}

fn parse_reason(s: &str) -> ContinuationReason {
    match s {
        "UserRequest" => ContinuationReason::UserRequest,
        "ErrorRecovery" => ContinuationReason::ErrorRecovery,
        "AdditionalInformation" => ContinuationReason::AdditionalInformation,
        "ContextExpansion" => ContinuationReason::ContextExpansion,
        "StrategyChange" => ContinuationReason::StrategyChange,
        _ => ContinuationReason::UserRequest,
    }
}
