use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use sqlx::FromRow;
use uuid::Uuid;

use crate::RepositoryResult;

/// A single managed-resource binding for a built-in agent.
///
/// One row ties `(company_id, built_in_key, resource_type, canonical_resource_key)`
/// to the managed resource. `stock_version` is the version shipped with the
/// built-in definition; `current_version` is what this company is bound to. A
/// mismatch (after a stock upgrade) sets `drift_detected` and is repaired by
/// reconcile.
#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct BuiltInManagedResource {
    pub id: Uuid,
    pub company_id: Uuid,
    pub built_in_key: String,
    pub resource_type: String,
    pub canonical_resource_key: String,
    pub target_resource_id: Option<Uuid>,
    pub stock_version: String,
    pub current_version: String,
    pub status: String,
    pub drift_detected: bool,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[async_trait]
pub trait BuiltInManagedResourceRepository: Send + Sync {
    /// Idempotently create or update the binding for a company + built-in key +
    /// resource type + canonical key. Returns the persisted row.
    async fn upsert(
        &self,
        company_id: Uuid,
        built_in_key: &str,
        resource_type: &str,
        canonical_resource_key: &str,
        target_resource_id: Option<Uuid>,
        stock_version: &str,
        current_version: &str,
    ) -> RepositoryResult<BuiltInManagedResource>;

    /// Fetch a single binding row.
    async fn get(
        &self,
        company_id: Uuid,
        built_in_key: &str,
        resource_type: &str,
        canonical_resource_key: &str,
    ) -> RepositoryResult<Option<BuiltInManagedResource>>;

    /// List all binding rows for a company + built-in key.
    async fn list_by_company_and_key(
        &self,
        company_id: Uuid,
        built_in_key: &str,
    ) -> RepositoryResult<Vec<BuiltInManagedResource>>;

    /// Repair a drifted binding: clear `drift_detected` and align
    /// `current_version`/`stock_version` to the supplied stock version.
    async fn repair_drift(
        &self,
        id: Uuid,
        stock_version: &str,
    ) -> RepositoryResult<()>;

    /// Delete every binding row for a company + built-in key. Returns the number
    /// of rows removed.
    async fn delete_by_company_and_key(
        &self,
        company_id: Uuid,
        built_in_key: &str,
    ) -> RepositoryResult<u64>;

    /// Delete every binding row for a company + built-in key + resource type.
    async fn delete_by_company_key_and_type(
        &self,
        company_id: Uuid,
        built_in_key: &str,
        resource_type: &str,
    ) -> RepositoryResult<u64>;

    /// Count all binding rows for a company (used by tests/observability).
    async fn count_by_company(&self, company_id: Uuid) -> RepositoryResult<i64>;
}

pub struct PgBuiltInManagedResourceRepository {
    pool: PgPool,
}

impl PgBuiltInManagedResourceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BuiltInManagedResourceRepository for PgBuiltInManagedResourceRepository {
    async fn upsert(
        &self,
        company_id: Uuid,
        built_in_key: &str,
        resource_type: &str,
        canonical_resource_key: &str,
        target_resource_id: Option<Uuid>,
        stock_version: &str,
        current_version: &str,
    ) -> RepositoryResult<BuiltInManagedResource> {
        let row = sqlx::query_as::<_, BuiltInManagedResource>(
            r#"INSERT INTO builtin_managed_resources
               (company_id, built_in_key, resource_type, canonical_resource_key,
                target_resource_id, stock_version, current_version, status,
                drift_detected, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, 'active', FALSE, now(), now())
               ON CONFLICT (company_id, built_in_key, resource_type, canonical_resource_key)
               DO UPDATE SET
                   target_resource_id = COALESCE(EXCLUDED.target_resource_id, builtin_managed_resources.target_resource_id),
                   stock_version = EXCLUDED.stock_version,
                   current_version = EXCLUDED.current_version,
                   status = 'active',
                   drift_detected = FALSE,
                   updated_at = now()
               RETURNING id, company_id, built_in_key, resource_type, canonical_resource_key,
                         target_resource_id, stock_version, current_version, status,
                         drift_detected, created_at, updated_at"#,
        )
        .bind(company_id)
        .bind(built_in_key)
        .bind(resource_type)
        .bind(canonical_resource_key)
        .bind(target_resource_id)
        .bind(stock_version)
        .bind(current_version)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    async fn get(
        &self,
        company_id: Uuid,
        built_in_key: &str,
        resource_type: &str,
        canonical_resource_key: &str,
    ) -> RepositoryResult<Option<BuiltInManagedResource>> {
        let row = sqlx::query_as::<_, BuiltInManagedResource>(
            r#"SELECT id, company_id, built_in_key, resource_type, canonical_resource_key,
                      target_resource_id, stock_version, current_version, status,
                      drift_detected, created_at, updated_at
               FROM builtin_managed_resources
               WHERE company_id = $1 AND built_in_key = $2 AND resource_type = $3
                 AND canonical_resource_key = $4"#,
        )
        .bind(company_id)
        .bind(built_in_key)
        .bind(resource_type)
        .bind(canonical_resource_key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn list_by_company_and_key(
        &self,
        company_id: Uuid,
        built_in_key: &str,
    ) -> RepositoryResult<Vec<BuiltInManagedResource>> {
        let rows = sqlx::query_as::<_, BuiltInManagedResource>(
            r#"SELECT id, company_id, built_in_key, resource_type, canonical_resource_key,
                      target_resource_id, stock_version, current_version, status,
                      drift_detected, created_at, updated_at
               FROM builtin_managed_resources
               WHERE company_id = $1 AND built_in_key = $2
               ORDER BY resource_type, canonical_resource_key"#,
        )
        .bind(company_id)
        .bind(built_in_key)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn repair_drift(&self, id: Uuid, stock_version: &str) -> RepositoryResult<()> {
        sqlx::query(
            r#"UPDATE builtin_managed_resources
               SET current_version = $2, stock_version = $2, drift_detected = FALSE,
                   status = 'active', updated_at = now()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(stock_version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_by_company_and_key(
        &self,
        company_id: Uuid,
        built_in_key: &str,
    ) -> RepositoryResult<u64> {
        let result = sqlx::query(
            r#"DELETE FROM builtin_managed_resources
               WHERE company_id = $1 AND built_in_key = $2"#,
        )
        .bind(company_id)
        .bind(built_in_key)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn delete_by_company_key_and_type(
        &self,
        company_id: Uuid,
        built_in_key: &str,
        resource_type: &str,
    ) -> RepositoryResult<u64> {
        let result = sqlx::query(
            r#"DELETE FROM builtin_managed_resources
               WHERE company_id = $1 AND built_in_key = $2 AND resource_type = $3"#,
        )
        .bind(company_id)
        .bind(built_in_key)
        .bind(resource_type)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn count_by_company(&self, company_id: Uuid) -> RepositoryResult<i64> {
        let (count,): (i64,) =
            sqlx::query_as(r#"SELECT COUNT(*) FROM builtin_managed_resources WHERE company_id = $1"#)
                .bind(company_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(count)
    }
}
