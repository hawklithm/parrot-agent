use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{RepositoryError, RepositoryResult};

/// Importar tipos desde services
pub use parrot_services::saga::{SagaInstance, SagaStatus};

/// Saga步骤执行记录
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SagaStepExecution {
    pub id: Uuid,
    pub saga_instance_id: Uuid,
    pub step_index: i32,
    pub step_name: String,
    pub status: String, // success / failed
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

/// Saga补偿历史记录
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SagaCompensation {
    pub id: Uuid,
    pub saga_instance_id: Uuid,
    pub step_index: i32,
    pub step_name: String,
    pub compensated_at: DateTime<Utc>,
    pub error_message: Option<String>,
}

#[async_trait]
pub trait SagaRepository: Send + Sync {
    /// 创建Saga实例
    async fn create(&self, instance: &SagaInstance) -> RepositoryResult<SagaInstance>;

    /// 更新Saga状态
    async fn update_status(
        &self,
        id: Uuid,
        status: SagaStatus,
        error_message: Option<String>,
    ) -> RepositoryResult<()>;

    /// 更新当前步骤
    async fn update_current_step(&self, id: Uuid, current_step: usize) -> RepositoryResult<()>;

    /// 更新上下文
    async fn update_context(&self, id: Uuid, context: serde_json::Value) -> RepositoryResult<()>;

    /// 根据ID获取Saga实例
    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<Option<SagaInstance>>;

    /// 列出指定状态的Saga实例
    async fn list_by_status(&self, company_id: Uuid, status: SagaStatus) -> RepositoryResult<Vec<SagaInstance>>;

    /// 记录步骤执行
    async fn record_step_execution(&self, execution: &SagaStepExecution) -> RepositoryResult<()>;

    /// 记录补偿历史
    async fn record_compensation(&self, compensation: &SagaCompensation) -> RepositoryResult<()>;

    /// 获取Saga的所有步骤执行记录
    async fn get_step_executions(&self, saga_instance_id: Uuid) -> RepositoryResult<Vec<SagaStepExecution>>;

    /// 获取Saga的所有补偿记录
    async fn get_compensations(&self, saga_instance_id: Uuid) -> RepositoryResult<Vec<SagaCompensation>>;
}

/// PostgreSQL implementation
pub struct PgSagaRepository {
    pool: PgPool,
}

impl PgSagaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SagaRepository for PgSagaRepository {
    async fn create(&self, instance: &SagaInstance) -> RepositoryResult<SagaInstance> {
        let row = sqlx::query_as::<_, SagaInstance>(
            r#"
            INSERT INTO saga_instances
            (id, saga_name, company_id, status, current_step, context, started_at, completed_at, error_message)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(&instance.id)
        .bind(&instance.saga_name)
        .bind(&instance.company_id)
        .bind(&instance.status)
        .bind(instance.current_step as i32)
        .bind(&instance.context)
        .bind(&instance.started_at)
        .bind(&instance.completed_at)
        .bind(&instance.error_message)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::DatabaseError(e.to_string()))?;

        Ok(row)
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: SagaStatus,
        error_message: Option<String>,
    ) -> RepositoryResult<()> {
        let completed_at = if matches!(status, SagaStatus::Completed | SagaStatus::Compensated | SagaStatus::Failed) {
            Some(Utc::now())
        } else {
            None
        };

        sqlx::query(
            r#"
            UPDATE saga_instances
            SET status = $1, error_message = $2, completed_at = $3
            WHERE id = $4
            "#,
        )
        .bind(&status)
        .bind(&error_message)
        .bind(&completed_at)
        .bind(&id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn update_current_step(&self, id: Uuid, current_step: usize) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            UPDATE saga_instances
            SET current_step = $1
            WHERE id = $2
            "#,
        )
        .bind(current_step as i32)
        .bind(&id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn update_context(&self, id: Uuid, context: serde_json::Value) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            UPDATE saga_instances
            SET context = $1
            WHERE id = $2
            "#,
        )
        .bind(&context)
        .bind(&id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<Option<SagaInstance>> {
        let row = sqlx::query_as::<_, SagaInstance>(
            r#"
            SELECT id, saga_name, company_id, status, current_step, context, started_at, completed_at, error_message
            FROM saga_instances
            WHERE id = $1
            "#,
        )
        .bind(&id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::DatabaseError(e.to_string()))?;

        Ok(row)
    }

    async fn list_by_status(&self, company_id: Uuid, status: SagaStatus) -> RepositoryResult<Vec<SagaInstance>> {
        let rows = sqlx::query_as::<_, SagaInstance>(
            r#"
            SELECT id, saga_name, company_id, status, current_step, context, started_at, completed_at, error_message
            FROM saga_instances
            WHERE company_id = $1 AND status = $2
            ORDER BY started_at DESC
            "#,
        )
        .bind(&company_id)
        .bind(&status)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::DatabaseError(e.to_string()))?;

        Ok(rows)
    }

    async fn record_step_execution(&self, execution: &SagaStepExecution) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            INSERT INTO saga_step_executions
            (id, saga_instance_id, step_index, step_name, status, started_at, completed_at, error_message)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(&execution.id)
        .bind(&execution.saga_instance_id)
        .bind(&execution.step_index)
        .bind(&execution.step_name)
        .bind(&execution.status)
        .bind(&execution.started_at)
        .bind(&execution.completed_at)
        .bind(&execution.error_message)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn record_compensation(&self, compensation: &SagaCompensation) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            INSERT INTO saga_compensations
            (id, saga_instance_id, step_index, step_name, compensated_at, error_message)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&compensation.id)
        .bind(&compensation.saga_instance_id)
        .bind(&compensation.step_index)
        .bind(&compensation.step_name)
        .bind(&compensation.compensated_at)
        .bind(&compensation.error_message)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn get_step_executions(&self, saga_instance_id: Uuid) -> RepositoryResult<Vec<SagaStepExecution>> {
        let rows = sqlx::query_as::<_, SagaStepExecution>(
            r#"
            SELECT id, saga_instance_id, step_index, step_name, status, started_at, completed_at, error_message
            FROM saga_step_executions
            WHERE saga_instance_id = $1
            ORDER BY step_index ASC
            "#,
        )
        .bind(&saga_instance_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::DatabaseError(e.to_string()))?;

        Ok(rows)
    }

    async fn get_compensations(&self, saga_instance_id: Uuid) -> RepositoryResult<Vec<SagaCompensation>> {
        let rows = sqlx::query_as::<_, SagaCompensation>(
            r#"
            SELECT id, saga_instance_id, step_index, step_name, compensated_at, error_message
            FROM saga_compensations
            WHERE saga_instance_id = $1
            ORDER BY compensated_at DESC
            "#,
        )
        .bind(&saga_instance_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::DatabaseError(e.to_string()))?;

        Ok(rows)
    }
}
