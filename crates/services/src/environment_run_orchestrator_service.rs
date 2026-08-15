/// Environment Run Orchestrator Service
/// 
/// 环境 Run 编排管理

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RunOrchestratorError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("orchestration failed: {0}")]
    OrchestrationFailed(String),
}

pub type RunOrchestratorResult<T> = Result<T, RunOrchestratorError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationTask {
    pub id: Uuid,
    pub run_id: Uuid,
    pub environment_id: Uuid,
    pub status: OrchestratorStatus,
    pub steps: Vec<OrchestratorStep>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorStep {
    pub name: String,
    pub status: StepStatus,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrchestratorStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

pub struct EnvironmentRunOrchestratorService {
    pool: PgPool,
}

impl EnvironmentRunOrchestratorService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_orchestration(
        &self,
        run_id: Uuid,
        environment_id: Uuid,
        steps: Vec<String>,
    ) -> RunOrchestratorResult<Uuid> {
        let id = Uuid::new_v4();
        
        let orchestrator_steps: Vec<OrchestratorStep> = steps.into_iter()
            .map(|name| OrchestratorStep {
                name,
                status: StepStatus::Pending,
                started_at: None,
                completed_at: None,
            })
            .collect();
        
        let _result: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO orchestration_tasks 
            (id, run_id, environment_id, status, steps, started_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(run_id)
        .bind(environment_id)
        .bind(format!("{:?}", OrchestratorStatus::Pending))
        .bind(serde_json::to_value(&orchestrator_steps).unwrap())
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn execute_orchestration(&self, id: Uuid) -> RunOrchestratorResult<()> {
        // 标记为运行中
        sqlx::query(
            "UPDATE orchestration_tasks SET status = $1 WHERE id = $2"
        )
        .bind(format!("{:?}", OrchestratorStatus::Running))
        .bind(id)
        .execute(&self.pool)
        .await?;
        
        // 简化实现：实际应该逐步执行
        
        // 标记为完成
        sqlx::query(
            r#"
            UPDATE orchestration_tasks 
            SET status = $1, completed_at = $2
            WHERE id = $3
            "#
        )
        .bind(format!("{:?}", OrchestratorStatus::Completed))
        .bind(chrono::Utc::now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_orchestration_status(
        &self,
        id: Uuid,
    ) -> RunOrchestratorResult<Option<OrchestrationTask>> {
        let row = sqlx::query(
            r#"
            SELECT id, run_id, environment_id, status, steps, started_at, completed_at
            FROM orchestration_tasks
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| {
            let steps_json: serde_json::Value = r.get("steps");
            let steps: Vec<OrchestratorStep> = serde_json::from_value(steps_json).unwrap_or_default();
            
            OrchestrationTask {
                id: r.get("id"),
                run_id: r.get("run_id"),
                environment_id: r.get("environment_id"),
                status: parse_orch_status(r.get("status")),
                steps,
                started_at: r.get("started_at"),
                completed_at: r.get("completed_at"),
            }
        }))
    }
}

fn parse_orch_status(s: &str) -> OrchestratorStatus {
    match s {
        "Pending" => OrchestratorStatus::Pending,
        "Running" => OrchestratorStatus::Running,
        "Completed" => OrchestratorStatus::Completed,
        "Failed" => OrchestratorStatus::Failed,
        _ => OrchestratorStatus::Pending,
    }
}
