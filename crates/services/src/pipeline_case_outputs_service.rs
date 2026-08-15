/// Pipeline Case Outputs Service
/// 
/// Pipeline Case 输出管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PipelineCaseOutputsError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type PipelineCaseOutputsResult<T> = Result<T, PipelineCaseOutputsError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseOutput {
    pub id: Uuid,
    pub case_id: Uuid,
    pub output_type: String,
    pub content: serde_json::Value,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct PipelineCaseOutputsService {
    pool: PgPool,
}

impl PipelineCaseOutputsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_output(
        &self,
        case_id: Uuid,
        output_type: String,
        content: serde_json::Value,
        metadata: serde_json::Value,
    ) -> PipelineCaseOutputsResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO pipeline_case_outputs 
            (id, case_id, output_type, content, metadata, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(case_id)
        .bind(&output_type)
        .bind(&content)
        .bind(&metadata)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_outputs(&self, case_id: Uuid) -> PipelineCaseOutputsResult<Vec<CaseOutput>> {
        let rows = sqlx::query(
            r#"
            SELECT id, case_id, output_type, content, metadata, created_at
            FROM pipeline_case_outputs
            WHERE case_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await?;
        
        let outputs = rows.into_iter().map(|row| {
            CaseOutput {
                id: row.get("id"),
                case_id: row.get("case_id"),
                output_type: row.get("output_type"),
                content: row.get("content"),
                metadata: row.get("metadata"),
                created_at: row.get("created_at"),
            }
        }).collect();
        
        Ok(outputs)
    }
}
