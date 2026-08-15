/// Managed Environment Service
/// 
/// 托管环境管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ManagedEnvironmentError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type ManagedEnvironmentResult<T> = Result<T, ManagedEnvironmentError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedEnvironment {
    pub id: Uuid,
    pub name: String,
    pub environment_type: EnvironmentType,
    pub config: serde_json::Value,
    pub status: EnvironmentStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnvironmentType {
    Development,
    Staging,
    Production,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnvironmentStatus {
    Provisioning,
    Ready,
    Degraded,
    Terminated,
}

pub struct ManagedEnvironmentService {
    pool: PgPool,
}

impl ManagedEnvironmentService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_environment(
        &self,
        name: String,
        environment_type: EnvironmentType,
        config: serde_json::Value,
    ) -> ManagedEnvironmentResult<Uuid> {
        let id = Uuid::new_v4();
        
        sqlx::query_scalar(
            r#"
            INSERT INTO managed_environments 
            (id, name, environment_type, config, status, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(&name)
        .bind(format!("{:?}", environment_type))
        .bind(&config)
        .bind(format!("{:?}", EnvironmentStatus::Provisioning))
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_environment(&self, id: Uuid) -> ManagedEnvironmentResult<Option<ManagedEnvironment>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, environment_type, config, status, created_at
            FROM managed_environments
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| ManagedEnvironment {
            id: r.get("id"),
            name: r.get("name"),
            environment_type: serde_json::from_str(&r.get::<String, _>("environment_type")).unwrap(),
            config: r.get("config"),
            status: serde_json::from_str(&r.get::<String, _>("status")).unwrap(),
            created_at: r.get("created_at"),
        }))
    }
    
    pub async fn update_status(
        &self,
        id: Uuid,
        status: EnvironmentStatus,
    ) -> ManagedEnvironmentResult<()> {
        sqlx::query(
            "UPDATE managed_environments SET status = $1 WHERE id = $2"
        )
        .bind(format!("{:?}", status))
        .bind(id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
