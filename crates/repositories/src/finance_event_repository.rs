use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use models::finance_event::FinanceEvent;
use crate::agent_repository::{RepositoryError, RepositoryResult};

/// Finance event summary row
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FinanceSummaryRow {
    pub dimension: String,
    pub debit_cents: i64,
    pub credit_cents: i64,
    pub estimated_debit_cents: i64,
    pub event_count: i64,
}

#[async_trait]
pub trait FinanceEventRepository: Send + Sync {
    /// Create a finance event
    async fn create(&self, event: &FinanceEvent) -> RepositoryResult<FinanceEvent>;

    /// Get finance summary for a company within a time range
    async fn summarize(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> RepositoryResult<FinanceSummaryRow>;

    /// Aggregate finance events by biller
    async fn by_biller(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> RepositoryResult<Vec<FinanceSummaryRow>>;

    /// Aggregate finance events by kind
    async fn by_kind(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> RepositoryResult<Vec<FinanceSummaryRow>>;

    /// List finance events for a company within a time range
    async fn list_by_company(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: i64,
    ) -> RepositoryResult<Vec<FinanceEvent>>;
}

pub struct PgFinanceEventRepository {
    pool: PgPool,
}

impl PgFinanceEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FinanceEventRepository for PgFinanceEventRepository {
    async fn create(&self, event: &FinanceEvent) -> RepositoryResult<FinanceEvent> {
        let result = sqlx::query_as::<_, FinanceEvent>(
            r#"
            INSERT INTO finance_events (
                id, company_id, agent_id, issue_id, project_id, goal_id,
                heartbeat_run_id, cost_event_id, biller, event_kind, direction,
                amount_cents, currency, estimated, description, occurred_at, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            RETURNING *
            "#,
        )
        .bind(event.id)
        .bind(event.company_id)
        .bind(event.agent_id)
        .bind(event.issue_id)
        .bind(event.project_id)
        .bind(event.goal_id)
        .bind(event.heartbeat_run_id)
        .bind(event.cost_event_id)
        .bind(&event.biller)
        .bind(&event.event_kind)
        .bind(event.direction)
        .bind(event.amount_cents)
        .bind(&event.currency)
        .bind(event.estimated)
        .bind(&event.description)
        .bind(event.occurred_at)
        .bind(event.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(result)
    }

    async fn summarize(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> RepositoryResult<FinanceSummaryRow> {
        let result = sqlx::query_as::<_, FinanceSummaryRow>(
            r#"
            SELECT
                $1::text as dimension,
                COALESCE(SUM(CASE WHEN direction = 'debit' THEN amount_cents ELSE 0 END), 0)::bigint as debit_cents,
                COALESCE(SUM(CASE WHEN direction = 'credit' THEN amount_cents ELSE 0 END), 0)::bigint as credit_cents,
                COALESCE(SUM(CASE WHEN direction = 'debit' AND estimated = true THEN amount_cents ELSE 0 END), 0)::bigint as estimated_debit_cents,
                COUNT(*)::bigint as event_count
            FROM finance_events
            WHERE company_id = $2
              AND occurred_at >= $3
              AND occurred_at < $4
            "#,
        )
        .bind(company_id.to_string())
        .bind(company_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(result)
    }

    async fn by_biller(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> RepositoryResult<Vec<FinanceSummaryRow>> {
        let results = sqlx::query_as::<_, FinanceSummaryRow>(
            r#"
            SELECT
                biller as dimension,
                COALESCE(SUM(CASE WHEN direction = 'debit' THEN amount_cents ELSE 0 END), 0)::bigint as debit_cents,
                COALESCE(SUM(CASE WHEN direction = 'credit' THEN amount_cents ELSE 0 END), 0)::bigint as credit_cents,
                COALESCE(SUM(CASE WHEN direction = 'debit' AND estimated = true THEN amount_cents ELSE 0 END), 0)::bigint as estimated_debit_cents,
                COUNT(*)::bigint as event_count
            FROM finance_events
            WHERE company_id = $1
              AND occurred_at >= $2
              AND occurred_at < $3
            GROUP BY biller
            ORDER BY debit_cents DESC
            "#,
        )
        .bind(company_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(results)
    }

    async fn by_kind(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> RepositoryResult<Vec<FinanceSummaryRow>> {
        let results = sqlx::query_as::<_, FinanceSummaryRow>(
            r#"
            SELECT
                event_kind as dimension,
                COALESCE(SUM(CASE WHEN direction = 'debit' THEN amount_cents ELSE 0 END), 0)::bigint as debit_cents,
                COALESCE(SUM(CASE WHEN direction = 'credit' THEN amount_cents ELSE 0 END), 0)::bigint as credit_cents,
                COALESCE(SUM(CASE WHEN direction = 'debit' AND estimated = true THEN amount_cents ELSE 0 END), 0)::bigint as estimated_debit_cents,
                COUNT(*)::bigint as event_count
            FROM finance_events
            WHERE company_id = $1
              AND occurred_at >= $2
              AND occurred_at < $3
            GROUP BY event_kind
            ORDER BY debit_cents DESC
            "#,
        )
        .bind(company_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(results)
    }

    async fn list_by_company(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: i64,
    ) -> RepositoryResult<Vec<FinanceEvent>> {
        let results = sqlx::query_as::<_, FinanceEvent>(
            r#"
            SELECT * FROM finance_events
            WHERE company_id = $1
              AND occurred_at >= $2
              AND occurred_at < $3
            ORDER BY occurred_at DESC, created_at DESC
            LIMIT $4
            "#,
        )
        .bind(company_id)
        .bind(start_time)
        .bind(end_time)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(results)
    }
}
