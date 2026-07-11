use async_trait::async_trait;
use models::{Asset, CreateAssetInput};
use uuid::Uuid;
use sqlx::PgPool;
use crate::RepositoryError;

#[async_trait]
pub trait AssetRepository: Send + Sync {
    /// Create a new asset
    async fn create(&self, input: CreateAssetInput) -> Result<Asset, RepositoryError>;

    /// Get an asset by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Asset>, RepositoryError>;

    /// Get an asset by SHA256 hash (for deduplication)
    async fn get_by_sha256(&self, company_id: Uuid, sha256: &str) -> Result<Option<Asset>, RepositoryError>;

    /// List assets by company
    async fn list_by_company(&self, company_id: Uuid, limit: i64, offset: i64) -> Result<Vec<Asset>, RepositoryError>;

    /// Delete an asset
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;

    /// List assets by creator (agent or user)
    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<Asset>, RepositoryError>;
    async fn list_by_user(&self, user_id: Uuid) -> Result<Vec<Asset>, RepositoryError>;

    /// Get asset count by company
    async fn count_by_company(&self, company_id: Uuid) -> Result<i64, RepositoryError>;
}

/// PostgreSQL implementation of AssetRepository
pub struct PgAssetRepository {
    pool: PgPool,
}

impl PgAssetRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AssetRepository for PgAssetRepository {
    async fn create(&self, input: CreateAssetInput) -> Result<Asset, RepositoryError> {
        let asset = sqlx::query_as::<_, Asset>(
            r#"
            INSERT INTO assets (
                company_id, provider, object_key, content_type, byte_size, sha256,
                original_filename, created_by_agent_id, created_by_user_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, company_id, provider, object_key, content_type, byte_size, sha256,
                      original_filename, created_by_agent_id, created_by_user_id, created_at, updated_at
            "#
        )
        .bind(&input.company_id)
        .bind(&input.provider)
        .bind(&input.object_key)
        .bind(&input.content_type)
        .bind(&input.byte_size)
        .bind(&input.sha256)
        .bind(&input.original_filename)
        .bind(&input.created_by_agent_id)
        .bind(&input.created_by_user_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(asset)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<Asset>, RepositoryError> {
        let asset = sqlx::query_as::<_, Asset>(
            r#"
            SELECT id, company_id, provider, object_key, content_type, byte_size, sha256,
                   original_filename, created_by_agent_id, created_by_user_id, created_at, updated_at
            FROM assets
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(asset)
    }

    async fn get_by_sha256(&self, company_id: Uuid, sha256: &str) -> Result<Option<Asset>, RepositoryError> {
        let asset = sqlx::query_as::<_, Asset>(
            r#"
            SELECT id, company_id, provider, object_key, content_type, byte_size, sha256,
                   original_filename, created_by_agent_id, created_by_user_id, created_at, updated_at
            FROM assets
            WHERE company_id = $1 AND sha256 = $2
            ORDER BY created_at DESC
            LIMIT 1
            "#
        )
        .bind(company_id)
        .bind(sha256)
        .fetch_optional(&self.pool)
        .await?;

        Ok(asset)
    }

    async fn list_by_company(&self, company_id: Uuid, limit: i64, offset: i64) -> Result<Vec<Asset>, RepositoryError> {
        let assets = sqlx::query_as::<_, Asset>(
            r#"
            SELECT id, company_id, provider, object_key, content_type, byte_size, sha256,
                   original_filename, created_by_agent_id, created_by_user_id, created_at, updated_at
            FROM assets
            WHERE company_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(company_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(assets)
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM assets
            WHERE id = $1
            "#
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<Asset>, RepositoryError> {
        let assets = sqlx::query_as::<_, Asset>(
            r#"
            SELECT id, company_id, provider, object_key, content_type, byte_size, sha256,
                   original_filename, created_by_agent_id, created_by_user_id, created_at, updated_at
            FROM assets
            WHERE created_by_agent_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(assets)
    }

    async fn list_by_user(&self, user_id: Uuid) -> Result<Vec<Asset>, RepositoryError> {
        let assets = sqlx::query_as::<_, Asset>(
            r#"
            SELECT id, company_id, provider, object_key, content_type, byte_size, sha256,
                   original_filename, created_by_agent_id, created_by_user_id, created_at, updated_at
            FROM assets
            WHERE created_by_user_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(assets)
    }

    async fn count_by_company(&self, company_id: Uuid) -> Result<i64, RepositoryError> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM assets WHERE company_id = $1
            "#
        )
        .bind(company_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count.0)
    }
}
