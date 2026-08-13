use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SkillPolicyRepositoryError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[async_trait]
pub trait CompanySkillPolicyRepository: Send + Sync {
    /// 读取某公司的 skill policy（不存在返回 None）。
    async fn get_by_company(
        &self,
        company_id: Uuid,
    ) -> Result<Option<CompanySkillPolicyRow>, SkillPolicyRepositoryError>;

    /// 写入/更新某公司的 skill policy（upsert），返回最新行。
    async fn upsert(
        &self,
        company_id: Uuid,
        policy: Value,
        version: i32,
    ) -> Result<CompanySkillPolicyRow, SkillPolicyRepositoryError>;

    /// 删除某公司的 skill policy（恢复默认开放）。
    async fn delete_by_company(&self, company_id: Uuid) -> Result<(), SkillPolicyRepositoryError>;
}

#[derive(Debug, Clone)]
pub struct CompanySkillPolicyRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub policy: Value,
    pub version: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct PgCompanySkillPolicyRepository {
    pool: PgPool,
}

impl PgCompanySkillPolicyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CompanySkillPolicyRepository for PgCompanySkillPolicyRepository {
    async fn get_by_company(
        &self,
        company_id: Uuid,
    ) -> Result<Option<CompanySkillPolicyRow>, SkillPolicyRepositoryError> {
        let row = sqlx::query(
            "SELECT id, company_id, policy, version, created_at, updated_at \
             FROM company_skill_policies WHERE company_id = $1",
        )
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| CompanySkillPolicyRow {
            id: r.get("id"),
            company_id: r.get("company_id"),
            policy: r.get("policy"),
            version: r.get("version"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    async fn upsert(
        &self,
        company_id: Uuid,
        policy: Value,
        version: i32,
    ) -> Result<CompanySkillPolicyRow, SkillPolicyRepositoryError> {
        let row = sqlx::query(
            r#"
            INSERT INTO company_skill_policies (company_id, policy, version, updated_at)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (company_id)
            DO UPDATE SET policy = EXCLUDED.policy,
                            version = EXCLUDED.version,
                            updated_at = NOW()
            RETURNING id, company_id, policy, version, created_at, updated_at
            "#,
        )
        .bind(company_id)
        .bind(&policy)
        .bind(version)
        .fetch_one(&self.pool)
        .await?;

        Ok(CompanySkillPolicyRow {
            id: row.get("id"),
            company_id: row.get("company_id"),
            policy: row.get("policy"),
            version: row.get("version"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    async fn delete_by_company(&self, company_id: Uuid) -> Result<(), SkillPolicyRepositoryError> {
        sqlx::query("DELETE FROM company_skill_policies WHERE company_id = $1")
            .bind(company_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
