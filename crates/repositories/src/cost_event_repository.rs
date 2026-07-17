use models::{CostEvent, CostSummary};
use sqlx::PgPool;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::RepositoryError as RepoError;

#[async_trait::async_trait]
pub trait CostEventRepository: Send + Sync {
    /// 创建成本事件记录
    async fn create(&self, cost_event: CostEvent) -> Result<CostEvent, RepoError>;

    /// 查询Agent的成本事件列表
    async fn list_by_agent(
        &self,
        agent_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostEvent>, RepoError>;

    /// 按月聚合Agent的花费
    async fn aggregate_monthly_spend(
        &self,
        agent_id: Uuid,
        year: i32,
        month: u32,
    ) -> Result<CostSummary, RepoError>;

    /// 批量聚合多个Agent的月度花费
    async fn aggregate_monthly_spend_batch(
        &self,
        agent_ids: Vec<Uuid>,
        year: i32,
        month: u32,
    ) -> Result<Vec<CostSummary>, RepoError>;
}

pub struct PgCostEventRepository {
    pool: PgPool,
}

impl PgCostEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl CostEventRepository for PgCostEventRepository {
    async fn create(&self, cost_event: CostEvent) -> Result<CostEvent, RepoError> {
        let result = sqlx::query_as::<_, CostEvent>(
            r#"
            INSERT INTO cost_events (
                id, company_id, agent_id, issue_id, project_id, goal_id,
                heartbeat_run_id, billing_code, provider, biller, billing_type,
                model, input_tokens, cached_input_tokens, output_tokens,
                cost_cents, occurred_at, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            RETURNING *
            "#
        )
        .bind(cost_event.id)
        .bind(cost_event.company_id)
        .bind(cost_event.agent_id)
        .bind(cost_event.issue_id)
        .bind(cost_event.project_id)
        .bind(cost_event.goal_id)
        .bind(cost_event.heartbeat_run_id)
        .bind(cost_event.billing_code)
        .bind(cost_event.provider)
        .bind(cost_event.biller)
        .bind(cost_event.billing_type)
        .bind(cost_event.model)
        .bind(cost_event.input_tokens)
        .bind(cost_event.cached_input_tokens)
        .bind(cost_event.output_tokens)
        .bind(cost_event.cost_cents)
        .bind(cost_event.occurred_at)
        .bind(cost_event.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(result)
    }

    async fn list_by_agent(
        &self,
        agent_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostEvent>, RepoError> {
        let results = sqlx::query_as::<_, CostEvent>(
            r#"
            SELECT * FROM cost_events
            WHERE agent_id = $1
              AND occurred_at >= $2
              AND occurred_at < $3
            ORDER BY occurred_at DESC
            "#
        )
        .bind(agent_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(results)
    }

    async fn aggregate_monthly_spend(
        &self,
        agent_id: Uuid,
        year: i32,
        month: u32,
    ) -> Result<CostSummary, RepoError> {
        let result = sqlx::query_as::<_, CostSummary>(
            r#"
            SELECT
                agent_id,
                COALESCE(SUM(cost_cents), 0)::int as total_cost_cents,
                COALESCE(SUM(input_tokens), 0)::bigint as total_input_tokens,
                COALESCE(SUM(cached_input_tokens), 0)::bigint as total_cached_input_tokens,
                COALESCE(SUM(output_tokens), 0)::bigint as total_output_tokens,
                COUNT(*)::bigint as event_count
            FROM cost_events
            WHERE agent_id = $1
              AND EXTRACT(YEAR FROM occurred_at) = $2
              AND EXTRACT(MONTH FROM occurred_at) = $3
            GROUP BY agent_id
            "#
        )
        .bind(agent_id)
        .bind(year)
        .bind(month as i32)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?
        .unwrap_or_else(|| CostSummary {
            agent_id,
            total_cost_cents: 0,
            total_input_tokens: 0,
            total_cached_input_tokens: 0,
            total_output_tokens: 0,
            event_count: 0,
        });

        Ok(result)
    }

    async fn aggregate_monthly_spend_batch(
        &self,
        agent_ids: Vec<Uuid>,
        year: i32,
        month: u32,
    ) -> Result<Vec<CostSummary>, RepoError> {
        let results = sqlx::query_as::<_, CostSummary>(
            r#"
            SELECT
                agent_id,
                COALESCE(SUM(cost_cents), 0)::int as total_cost_cents,
                COALESCE(SUM(input_tokens), 0)::bigint as total_input_tokens,
                COALESCE(SUM(cached_input_tokens), 0)::bigint as total_cached_input_tokens,
                COALESCE(SUM(output_tokens), 0)::bigint as total_output_tokens,
                COUNT(*)::bigint as event_count
            FROM cost_events
            WHERE agent_id = ANY($1)
              AND EXTRACT(YEAR FROM occurred_at) = $2
              AND EXTRACT(MONTH FROM occurred_at) = $3
            GROUP BY agent_id
            "#
        )
        .bind(&agent_ids)
        .bind(year)
        .bind(month as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(results)
    }
}
