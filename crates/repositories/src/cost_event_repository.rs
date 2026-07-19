use models::{CostEvent, CostSummary, CostSummaryRow, IssueTreeCostSummary, RunSummaryRow};
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

    /// 查询公司的成本事件列表
    async fn list_by_company(
        &self,
        company_id: Uuid,
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

    /// 成本汇总（公司级别）
    async fn summarize(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<CostSummaryRow, RepoError>;

    /// 按 Agent 聚合成本
    async fn by_agent(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostSummaryRow>, RepoError>;

    /// 按 Agent + Model 聚合成本
    async fn by_agent_model(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostSummaryRow>, RepoError>;

    /// 按 Provider 聚合成本
    async fn by_provider(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostSummaryRow>, RepoError>;

    /// 按 Biller 聚合成本
    async fn by_biller(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostSummaryRow>, RepoError>;

    /// 按 Project 聚合成本
    async fn by_project(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostSummaryRow>, RepoError>;

    /// 窗口期花费
    async fn window_spend(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<i64, RepoError>;

    /// Issue 成本汇总
    async fn issue_cost_summary(
        &self,
        issue_id: Uuid,
    ) -> Result<CostSummaryRow, RepoError>;

    /// Issue 树成本汇总（含子 issue 的递归聚合 + 运行次数和运行时间）
    async fn issue_tree_cost_summary(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        exclude_root: bool,
    ) -> Result<IssueTreeCostSummary, RepoError>;

    /// 当月已花费金额（公司级别）
    async fn current_month_spend(
        &self,
        company_id: Uuid,
    ) -> Result<i64, RepoError>;

    /// 当月已花费金额（Agent级别）
    async fn current_month_spend_by_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<i64, RepoError>;
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

    async fn list_by_company(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostEvent>, RepoError> {
        let results = sqlx::query_as::<_, CostEvent>(
            r#"
            SELECT * FROM cost_events
            WHERE company_id = $1
              AND occurred_at >= $2
              AND occurred_at < $3
            ORDER BY occurred_at DESC
            "#
        )
        .bind(company_id)
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

    async fn summarize(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<CostSummaryRow, RepoError> {
        let result = sqlx::query_as::<_, CostSummaryRow>(
            r#"
            SELECT
                $1::text as dimension,
                COALESCE(SUM(cost_cents), 0)::bigint as total_cost_cents,
                COALESCE(SUM(input_tokens), 0)::bigint as total_input_tokens,
                COALESCE(SUM(cached_input_tokens), 0)::bigint as total_cached_input_tokens,
                COALESCE(SUM(output_tokens), 0)::bigint as total_output_tokens,
                COUNT(*)::bigint as event_count,
                COUNT(DISTINCT CASE WHEN billing_type = 'metered_api' THEN heartbeat_run_id END)::bigint as api_run_count,
                COUNT(DISTINCT CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN heartbeat_run_id END)::bigint as subscription_run_count,
                COALESCE(SUM(CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN cached_input_tokens ELSE 0 END), 0)::bigint as subscription_cached_input_tokens,
                COALESCE(SUM(CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN input_tokens ELSE 0 END), 0)::bigint as subscription_input_tokens,
                COALESCE(SUM(CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN output_tokens ELSE 0 END), 0)::bigint as subscription_output_tokens,
                COUNT(DISTINCT provider)::bigint as provider_count,
                COUNT(DISTINCT model)::bigint as model_count
            FROM cost_events
            WHERE company_id = $2
              AND occurred_at >= $3
              AND occurred_at < $4
            "#
        )
        .bind(company_id.to_string())
        .bind(company_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_one(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(result)
    }

    async fn by_agent(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostSummaryRow>, RepoError> {
        let results = sqlx::query_as::<_, CostSummaryRow>(
            r#"
            SELECT
                agent_id::text as dimension,
                COALESCE(SUM(cost_cents), 0)::bigint as total_cost_cents,
                COALESCE(SUM(input_tokens), 0)::bigint as total_input_tokens,
                COALESCE(SUM(cached_input_tokens), 0)::bigint as total_cached_input_tokens,
                COALESCE(SUM(output_tokens), 0)::bigint as total_output_tokens,
                COUNT(*)::bigint as event_count,
                COUNT(DISTINCT CASE WHEN billing_type = 'metered_api' THEN heartbeat_run_id END)::bigint as api_run_count,
                COUNT(DISTINCT CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN heartbeat_run_id END)::bigint as subscription_run_count,
                COALESCE(SUM(CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN cached_input_tokens ELSE 0 END), 0)::bigint as subscription_cached_input_tokens,
                COALESCE(SUM(CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN input_tokens ELSE 0 END), 0)::bigint as subscription_input_tokens,
                COALESCE(SUM(CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN output_tokens ELSE 0 END), 0)::bigint as subscription_output_tokens,
                COUNT(DISTINCT provider)::bigint as provider_count,
                COUNT(DISTINCT model)::bigint as model_count
            FROM cost_events
            WHERE company_id = $1
              AND occurred_at >= $2
              AND occurred_at < $3
            GROUP BY agent_id
            ORDER BY total_cost_cents DESC
            "#
        )
        .bind(company_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(results)
    }

    async fn by_agent_model(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostSummaryRow>, RepoError> {
        let results = sqlx::query_as::<_, CostSummaryRow>(
            r#"
            SELECT
                (agent_id::text || ':' || provider || ':' || biller || ':' || billing_type || ':' || model) as dimension,
                COALESCE(SUM(cost_cents), 0)::bigint as total_cost_cents,
                COALESCE(SUM(input_tokens), 0)::bigint as total_input_tokens,
                COALESCE(SUM(cached_input_tokens), 0)::bigint as total_cached_input_tokens,
                COALESCE(SUM(output_tokens), 0)::bigint as total_output_tokens,
                COUNT(*)::bigint as event_count,
                0::bigint as api_run_count, 0::bigint as subscription_run_count,
                0::bigint as subscription_cached_input_tokens, 0::bigint as subscription_input_tokens,
                0::bigint as subscription_output_tokens, 1::bigint as provider_count, 1::bigint as model_count
            FROM cost_events
            WHERE company_id = $1
              AND occurred_at >= $2
              AND occurred_at < $3
            GROUP BY agent_id, provider, biller, billing_type, model
            ORDER BY total_cost_cents DESC
            "#
        )
        .bind(company_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(results)
    }

    async fn by_provider(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostSummaryRow>, RepoError> {
        let results = sqlx::query_as::<_, CostSummaryRow>(
            r#"
            SELECT
                (provider || ':' || biller || ':' || billing_type || ':' || model) as dimension,
                COALESCE(SUM(cost_cents), 0)::bigint as total_cost_cents,
                COALESCE(SUM(input_tokens), 0)::bigint as total_input_tokens,
                COALESCE(SUM(cached_input_tokens), 0)::bigint as total_cached_input_tokens,
                COALESCE(SUM(output_tokens), 0)::bigint as total_output_tokens,
                COUNT(*)::bigint as event_count,
                0::bigint as api_run_count, 0::bigint as subscription_run_count,
                0::bigint as subscription_cached_input_tokens, 0::bigint as subscription_input_tokens,
                0::bigint as subscription_output_tokens, 1::bigint as provider_count, 1::bigint as model_count
            FROM cost_events
            WHERE company_id = $1
              AND occurred_at >= $2
              AND occurred_at < $3
            GROUP BY provider, biller, billing_type, model
            ORDER BY total_cost_cents DESC
            "#
        )
        .bind(company_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(results)
    }

    async fn by_biller(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostSummaryRow>, RepoError> {
        let results = sqlx::query_as::<_, CostSummaryRow>(
            r#"
            SELECT
                biller as dimension,
                COALESCE(SUM(cost_cents), 0)::bigint as total_cost_cents,
                COALESCE(SUM(input_tokens), 0)::bigint as total_input_tokens,
                COALESCE(SUM(cached_input_tokens), 0)::bigint as total_cached_input_tokens,
                COALESCE(SUM(output_tokens), 0)::bigint as total_output_tokens,
                COUNT(*)::bigint as event_count,
                COUNT(DISTINCT CASE WHEN billing_type = 'metered_api' THEN heartbeat_run_id END)::bigint as api_run_count,
                COUNT(DISTINCT CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN heartbeat_run_id END)::bigint as subscription_run_count,
                COALESCE(SUM(CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN cached_input_tokens ELSE 0 END), 0)::bigint as subscription_cached_input_tokens,
                COALESCE(SUM(CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN input_tokens ELSE 0 END), 0)::bigint as subscription_input_tokens,
                COALESCE(SUM(CASE WHEN billing_type IN ('subscription_included', 'subscription_overage') THEN output_tokens ELSE 0 END), 0)::bigint as subscription_output_tokens,
                COUNT(DISTINCT provider)::bigint as provider_count, COUNT(DISTINCT model)::bigint as model_count
            FROM cost_events
            WHERE company_id = $1
              AND occurred_at >= $2
              AND occurred_at < $3
            GROUP BY biller
            ORDER BY total_cost_cents DESC
            "#
        )
        .bind(company_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(results)
    }

    async fn by_project(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<CostSummaryRow>, RepoError> {
        let results = sqlx::query_as::<_, CostSummaryRow>(
            r#"
            SELECT
                COALESCE(ce.project_id::text, run_project_links.project_id::text) as dimension,
                COALESCE(SUM(cost_cents), 0)::bigint as total_cost_cents,
                COALESCE(SUM(input_tokens), 0)::bigint as total_input_tokens,
                COALESCE(SUM(cached_input_tokens), 0)::bigint as total_cached_input_tokens,
                COALESCE(SUM(output_tokens), 0)::bigint as total_output_tokens,
                COUNT(*)::bigint as event_count,
                0::bigint as api_run_count, 0::bigint as subscription_run_count,
                0::bigint as subscription_cached_input_tokens, 0::bigint as subscription_input_tokens,
                0::bigint as subscription_output_tokens, 0::bigint as provider_count, 0::bigint as model_count
            FROM cost_events ce
            LEFT JOIN (
                SELECT DISTINCT ON (al.run_id, i.project_id) al.run_id, i.project_id
                FROM activity_logs al
                JOIN issues i ON al.resource_type = 'issue' AND al.resource_id = i.id
                WHERE al.company_id = $1 AND i.company_id = $1 AND al.run_id IS NOT NULL AND i.project_id IS NOT NULL
                ORDER BY al.run_id, i.project_id, al.created_at DESC
            ) run_project_links ON ce.heartbeat_run_id = run_project_links.run_id
            JOIN projects p ON p.id = COALESCE(ce.project_id, run_project_links.project_id)
            WHERE ce.company_id = $1
              AND ce.occurred_at >= $2
              AND ce.occurred_at < $3
            GROUP BY COALESCE(ce.project_id, run_project_links.project_id)
            ORDER BY total_cost_cents DESC
            "#
        )
        .bind(company_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(results)
    }

    async fn window_spend(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<i64, RepoError> {
        let result: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM(cost_cents), 0)::bigint
            FROM cost_events
            WHERE company_id = $1
              AND occurred_at >= $2
              AND occurred_at < $3
            "#
        )
        .bind(company_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(result.map(|r| r.0).unwrap_or(0))
    }

    async fn issue_cost_summary(
        &self,
        issue_id: Uuid,
    ) -> Result<CostSummaryRow, RepoError> {
        let result = sqlx::query_as::<_, CostSummaryRow>(
            r#"
            SELECT
                $1::text as dimension,
                COALESCE(SUM(cost_cents), 0)::bigint as total_cost_cents,
                COALESCE(SUM(input_tokens), 0)::bigint as total_input_tokens,
                COALESCE(SUM(cached_input_tokens), 0)::bigint as total_cached_input_tokens,
                COALESCE(SUM(output_tokens), 0)::bigint as total_output_tokens,
                COUNT(*)::bigint as event_count,
                0::bigint as api_run_count, 0::bigint as subscription_run_count,
                0::bigint as subscription_cached_input_tokens, 0::bigint as subscription_input_tokens,
                0::bigint as subscription_output_tokens, 0::bigint as provider_count, 0::bigint as model_count
            FROM cost_events
            WHERE issue_id = $2
            "#
        )
        .bind(issue_id.to_string())
        .bind(issue_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(result)
    }

    async fn issue_tree_cost_summary(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        exclude_root: bool,
    ) -> Result<IssueTreeCostSummary, RepoError> {
        // CTE seed: if exclude_root, start from direct children; otherwise include the root issue
        let cte_seed = if exclude_root {
            format!(
                "SELECT id FROM issues WHERE company_id = '{}'::uuid AND parent_id = '{}'::uuid AND hidden_at IS NULL AND harness_kind IS NULL",
                company_id, issue_id
            )
        } else {
            format!(
                "SELECT id FROM issues WHERE company_id = '{}'::uuid AND id = '{}'::uuid AND hidden_at IS NULL AND harness_kind IS NULL",
                company_id, issue_id
            )
        };

        // Cost aggregation query using recursive CTE
        let cost_sql = format!(
            r#"
            WITH RECURSIVE issue_tree(id) AS (
                {}
                UNION ALL
                SELECT child.id FROM issues child
                JOIN issue_tree ON child.parent_id = issue_tree.id
                WHERE child.company_id = '{}'::uuid
                  AND child.hidden_at IS NULL
                  AND child.harness_kind IS NULL
            )
            SELECT
                '{}'::uuid as issue_id,
                COUNT(DISTINCT issues.id)::bigint as issue_count,
                true as include_descendants,
                COALESCE(SUM(ce.cost_cents), 0)::bigint as cost_cents,
                COALESCE(SUM(ce.input_tokens), 0)::bigint as input_tokens,
                COALESCE(SUM(ce.cached_input_tokens), 0)::bigint as cached_input_tokens,
                COALESCE(SUM(ce.output_tokens), 0)::bigint as output_tokens,
                0::bigint as run_count,
                0.0::double precision as runtime_ms
            FROM issues
            LEFT JOIN cost_events ce ON ce.issue_id = issues.id
            WHERE issues.id IN (SELECT id FROM issue_tree)
            "#,
            cte_seed, company_id, issue_id
        );

        let cost_row = sqlx::query_as::<_, IssueTreeCostSummary>(&cost_sql)
            .fetch_one(&self.pool)
            .await
            .map_err(RepoError::DatabaseError)?;

        // Run aggregation query (separate to avoid double-counting)
        let run_cte_seed = if exclude_root {
            format!(
                "SELECT id::text FROM issues WHERE company_id = '{}'::uuid AND parent_id = '{}'::uuid AND hidden_at IS NULL AND harness_kind IS NULL",
                company_id, issue_id
            )
        } else {
            format!(
                "SELECT id::text FROM issues WHERE company_id = '{}'::uuid AND id = '{}'::uuid AND hidden_at IS NULL AND harness_kind IS NULL",
                company_id, issue_id
            )
        };

        let run_sql = format!(
            r#"
            WITH RECURSIVE issue_tree(id) AS (
                {}
                UNION ALL
                SELECT (child.id)::text FROM issues child
                JOIN issue_tree ON (child.parent_id)::text = issue_tree.id
                WHERE child.company_id = '{}'::uuid
                  AND child.hidden_at IS NULL
                  AND child.harness_kind IS NULL
            )
            SELECT
                COUNT(DISTINCT hr.id)::bigint as run_count,
                COALESCE(SUM(EXTRACT(EPOCH FROM (COALESCE(hr.finished_at, NOW()) - hr.started_at)) * 1000), 0)::double precision as runtime_ms
            FROM heartbeat_runs hr
            WHERE hr.company_id = '{}'::uuid
              AND hr.started_at IS NOT NULL
              AND (
                (hr.context_snapshot ->> 'issueId')::text IN (SELECT id FROM issue_tree)
                OR EXISTS (
                    SELECT 1 FROM activity_log al
                    JOIN issue_tree ON al.entity_id = issue_tree.id
                    WHERE al.company_id = '{}'::uuid
                      AND al.entity_type = 'issue'
                      AND al.run_id = hr.id
                )
              )
            "#,
            run_cte_seed, company_id, company_id, company_id
        );

        let run_row = sqlx::query_as::<_, RunSummaryRow>(&run_sql)
            .fetch_one(&self.pool)
            .await
            .map_err(RepoError::DatabaseError)?;

        Ok(IssueTreeCostSummary {
            issue_id,
            issue_count: cost_row.issue_count,
            include_descendants: true,
            cost_cents: cost_row.cost_cents,
            input_tokens: cost_row.input_tokens,
            cached_input_tokens: cost_row.cached_input_tokens,
            output_tokens: cost_row.output_tokens,
            run_count: run_row.run_count,
            runtime_ms: run_row.runtime_ms,
        })
    }

    async fn current_month_spend(
        &self,
        company_id: Uuid,
    ) -> Result<i64, RepoError> {
        let result: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM(cost_cents), 0)::bigint
            FROM cost_events
            WHERE company_id = $1
              AND EXTRACT(YEAR FROM occurred_at) = EXTRACT(YEAR FROM NOW())
              AND EXTRACT(MONTH FROM occurred_at) = EXTRACT(MONTH FROM NOW())
            "#
        )
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(result.map(|r| r.0).unwrap_or(0))
    }

    async fn current_month_spend_by_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<i64, RepoError> {
        let result: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM(cost_cents), 0)::bigint
            FROM cost_events
            WHERE agent_id = $1
              AND EXTRACT(YEAR FROM occurred_at) = EXTRACT(YEAR FROM NOW())
              AND EXTRACT(MONTH FROM occurred_at) = EXTRACT(MONTH FROM NOW())
            "#
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(result.map(|r| r.0).unwrap_or(0))
    }
}
