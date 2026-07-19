use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use models::budget::{
    BudgetIncident, BudgetIncidentStatus, BudgetPolicy,
    BudgetScopeType, BudgetThresholdType,
};
use crate::agent_repository::{RepositoryError, RepositoryResult};

// ============================================================================
// BudgetPolicyRepository
// ============================================================================

#[async_trait]
pub trait BudgetPolicyRepository: Send + Sync {
    /// Upsert a budget policy (unique on company_id + scope_type + scope_id + metric + window_kind)
    async fn upsert(&self, policy: &BudgetPolicy) -> RepositoryResult<BudgetPolicy>;

    /// Get a policy by its ID
    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<BudgetPolicy>;

    /// Get a policy by scope
    async fn get_by_scope(
        &self,
        company_id: Uuid,
        scope_type: BudgetScopeType,
        scope_id: Uuid,
    ) -> RepositoryResult<Option<BudgetPolicy>>;

    /// List all policies for a company
    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<BudgetPolicy>>;

    /// List active policies for a company
    async fn list_active_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<BudgetPolicy>>;

    /// Update policy amount
    async fn update_amount(&self, id: Uuid, amount: i64, updated_by_user_id: Option<Uuid>) -> RepositoryResult<BudgetPolicy>;

    /// Set policy active/inactive
    async fn set_active(&self, id: Uuid, is_active: bool) -> RepositoryResult<()>;
}

// ============================================================================
// BudgetIncidentRepository
// ============================================================================

#[async_trait]
pub trait BudgetIncidentRepository: Send + Sync {
    /// Create a new budget incident
    async fn create(&self, incident: &BudgetIncident) -> RepositoryResult<BudgetIncident>;

    /// Get incident by ID
    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<BudgetIncident>;

    /// List incidents for a company
    async fn list_by_company(&self, company_id: Uuid, status: Option<BudgetIncidentStatus>) -> RepositoryResult<Vec<BudgetIncident>>;

    /// Find existing open incident for a policy + window + threshold type
    async fn find_open(
        &self,
        policy_id: Uuid,
        window_start: DateTime<Utc>,
        threshold_type: BudgetThresholdType,
    ) -> RepositoryResult<Option<BudgetIncident>>;

    /// Resolve all open incidents for a policy
    async fn resolve_open_for_policy(&self, policy_id: Uuid) -> RepositoryResult<()>;

    /// Resolve open soft incidents for a policy
    async fn resolve_open_soft_for_policy(&self, policy_id: Uuid) -> RepositoryResult<()>;

    /// Dismiss an incident
    async fn dismiss(&self, id: Uuid) -> RepositoryResult<()>;
}

// ============================================================================
// PostgreSQL implementations
// ============================================================================

pub struct PgBudgetPolicyRepository {
    pool: PgPool,
}

impl PgBudgetPolicyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BudgetPolicyRepository for PgBudgetPolicyRepository {
    async fn upsert(&self, policy: &BudgetPolicy) -> RepositoryResult<BudgetPolicy> {
        let result = sqlx::query_as::<_, BudgetPolicy>(
            r#"
            INSERT INTO budget_policies (
                id, company_id, scope_type, scope_id, metric, window_kind,
                amount, warn_percent, hard_stop_enabled, notify_enabled, is_active,
                created_by_user_id, updated_by_user_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT (company_id, scope_type, scope_id, metric, window_kind)
            DO UPDATE SET
                amount = EXCLUDED.amount,
                warn_percent = EXCLUDED.warn_percent,
                hard_stop_enabled = EXCLUDED.hard_stop_enabled,
                notify_enabled = EXCLUDED.notify_enabled,
                is_active = EXCLUDED.is_active,
                updated_by_user_id = EXCLUDED.updated_by_user_id,
                updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(policy.id)
        .bind(policy.company_id)
        .bind(policy.scope_type)
        .bind(policy.scope_id)
        .bind(policy.metric)
        .bind(policy.window_kind)
        .bind(policy.amount)
        .bind(policy.warn_percent)
        .bind(policy.hard_stop_enabled)
        .bind(policy.notify_enabled)
        .bind(policy.is_active)
        .bind(policy.created_by_user_id)
        .bind(policy.updated_by_user_id)
        .bind(policy.created_at)
        .bind(policy.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(result)
    }

    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<BudgetPolicy> {
        sqlx::query_as::<_, BudgetPolicy>(
            "SELECT * FROM budget_policies WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => RepositoryError::NotFound(id),
            other => RepositoryError::DatabaseError(other),
        })
    }

    async fn get_by_scope(
        &self,
        company_id: Uuid,
        scope_type: BudgetScopeType,
        scope_id: Uuid,
    ) -> RepositoryResult<Option<BudgetPolicy>> {
        sqlx::query_as::<_, BudgetPolicy>(
            r#"
            SELECT * FROM budget_policies
            WHERE company_id = $1 AND scope_type = $2 AND scope_id = $3
            AND metric = 'billed_cents' AND window_kind = 'calendar_month_utc'
            "#,
        )
        .bind(company_id)
        .bind(scope_type)
        .bind(scope_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<BudgetPolicy>> {
        sqlx::query_as::<_, BudgetPolicy>(
            "SELECT * FROM budget_policies WHERE company_id = $1 ORDER BY updated_at DESC",
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn list_active_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<BudgetPolicy>> {
        sqlx::query_as::<_, BudgetPolicy>(
            "SELECT * FROM budget_policies WHERE company_id = $1 AND is_active = true ORDER BY updated_at DESC",
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn update_amount(&self, id: Uuid, amount: i64, updated_by_user_id: Option<Uuid>) -> RepositoryResult<BudgetPolicy> {
        sqlx::query_as::<_, BudgetPolicy>(
            r#"
            UPDATE budget_policies
            SET amount = $2, updated_by_user_id = $3, updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(amount)
        .bind(updated_by_user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => RepositoryError::NotFound(id),
            other => RepositoryError::DatabaseError(other),
        })
    }

    async fn set_active(&self, id: Uuid, is_active: bool) -> RepositoryResult<()> {
        sqlx::query(
            "UPDATE budget_policies SET is_active = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(is_active)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        Ok(())
    }
}

pub struct PgBudgetIncidentRepository {
    pool: PgPool,
}

impl PgBudgetIncidentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BudgetIncidentRepository for PgBudgetIncidentRepository {
    async fn create(&self, incident: &BudgetIncident) -> RepositoryResult<BudgetIncident> {
        let result = sqlx::query_as::<_, BudgetIncident>(
            r#"
            INSERT INTO budget_incidents (
                id, company_id, policy_id, scope_type, scope_id, metric, window_kind,
                window_start, window_end, threshold_type, amount_limit, amount_observed,
                status, approval_id, resolved_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            RETURNING *
            "#,
        )
        .bind(incident.id)
        .bind(incident.company_id)
        .bind(incident.policy_id)
        .bind(incident.scope_type)
        .bind(incident.scope_id)
        .bind(incident.metric)
        .bind(incident.window_kind)
        .bind(incident.window_start)
        .bind(incident.window_end)
        .bind(incident.threshold_type)
        .bind(incident.amount_limit)
        .bind(incident.amount_observed)
        .bind(incident.status)
        .bind(incident.approval_id)
        .bind(incident.resolved_at)
        .bind(incident.created_at)
        .bind(incident.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(result)
    }

    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<BudgetIncident> {
        sqlx::query_as::<_, BudgetIncident>(
            "SELECT * FROM budget_incidents WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => RepositoryError::NotFound(id),
            other => RepositoryError::DatabaseError(other),
        })
    }

    async fn list_by_company(&self, company_id: Uuid, status: Option<BudgetIncidentStatus>) -> RepositoryResult<Vec<BudgetIncident>> {
        if let Some(s) = status {
            sqlx::query_as::<_, BudgetIncident>(
                "SELECT * FROM budget_incidents WHERE company_id = $1 AND status = $2 ORDER BY created_at DESC",
            )
            .bind(company_id)
            .bind(s)
            .fetch_all(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)
        } else {
            sqlx::query_as::<_, BudgetIncident>(
                "SELECT * FROM budget_incidents WHERE company_id = $1 ORDER BY created_at DESC",
            )
            .bind(company_id)
            .fetch_all(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)
        }
    }

    async fn find_open(
        &self,
        policy_id: Uuid,
        window_start: DateTime<Utc>,
        threshold_type: BudgetThresholdType,
    ) -> RepositoryResult<Option<BudgetIncident>> {
        sqlx::query_as::<_, BudgetIncident>(
            r#"
            SELECT * FROM budget_incidents
            WHERE policy_id = $1
              AND window_start = $2
              AND threshold_type = $3
              AND status = 'open'
            LIMIT 1
            "#,
        )
        .bind(policy_id)
        .bind(window_start)
        .bind(threshold_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn resolve_open_for_policy(&self, policy_id: Uuid) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            UPDATE budget_incidents
            SET status = 'resolved', resolved_at = NOW(), updated_at = NOW()
            WHERE policy_id = $1 AND status = 'open'
            "#,
        )
        .bind(policy_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        Ok(())
    }

    async fn resolve_open_soft_for_policy(&self, policy_id: Uuid) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            UPDATE budget_incidents
            SET status = 'resolved', resolved_at = NOW(), updated_at = NOW()
            WHERE policy_id = $1 AND threshold_type = 'soft' AND status = 'open'
            "#,
        )
        .bind(policy_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        Ok(())
    }

    async fn dismiss(&self, id: Uuid) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            UPDATE budget_incidents
            SET status = 'dismissed', resolved_at = NOW(), updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        Ok(())
    }
}
