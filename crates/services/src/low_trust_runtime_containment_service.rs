/// Low Trust Runtime Containment Service
/// 
/// 低信任运行时隔离管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum LowTrustContainmentError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("containment violation: {0}")]
    Violation(String),
}

pub type LowTrustContainmentResult<T> = Result<T, LowTrustContainmentError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainmentPolicy {
    pub id: Uuid,
    pub entity_id: Uuid,
    pub entity_type: String,
    pub restrictions: ContainmentRestrictions,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainmentRestrictions {
    pub network_access: bool,
    pub file_system_access: bool,
    pub allowed_directories: Vec<String>,
    pub max_memory_mb: i32,
    pub max_cpu_percent: i32,
    pub max_execution_time_seconds: i32,
    pub allowed_syscalls: Vec<String>,
}

impl Default for ContainmentRestrictions {
    fn default() -> Self {
        Self {
            network_access: false,
            file_system_access: false,
            allowed_directories: vec!["/tmp".to_string()],
            max_memory_mb: 512,
            max_cpu_percent: 50,
            max_execution_time_seconds: 300,
            allowed_syscalls: vec![],
        }
    }
}

pub struct LowTrustRuntimeContainmentService {
    pool: PgPool,
}

impl LowTrustRuntimeContainmentService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn apply_containment(
        &self,
        entity_id: Uuid,
        entity_type: String,
        restrictions: ContainmentRestrictions,
    ) -> LowTrustContainmentResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO containment_policies 
            (id, entity_id, entity_type, restrictions, created_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(entity_id)
        .bind(&entity_type)
        .bind(serde_json::to_value(&restrictions).unwrap())
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_containment(
        &self,
        entity_id: Uuid,
    ) -> LowTrustContainmentResult<Option<ContainmentPolicy>> {
        let row = sqlx::query(
            r#"
            SELECT id, entity_id, entity_type, restrictions, created_at
            FROM containment_policies
            WHERE entity_id = $1
            "#
        )
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| {
            ContainmentPolicy {
                id: r.get("id"),
                entity_id: r.get("entity_id"),
                entity_type: r.get("entity_type"),
                restrictions: serde_json::from_value(r.get("restrictions")).unwrap_or_default(),
                created_at: r.get("created_at"),
            }
        }))
    }
    
    pub async fn check_file_access(
        &self,
        entity_id: Uuid,
        file_path: &str,
    ) -> LowTrustContainmentResult<()> {
        if let Some(policy) = self.get_containment(entity_id).await? {
            if !policy.restrictions.file_system_access {
                return Err(LowTrustContainmentError::Violation(
                    "File system access is not allowed".to_string()
                ));
            }
            
            let allowed = policy.restrictions.allowed_directories.iter()
                .any(|dir| file_path.starts_with(dir));
            
            if !allowed {
                return Err(LowTrustContainmentError::Violation(
                    format!("Access to {} is not allowed", file_path)
                ));
            }
        }
        
        Ok(())
    }
    
    pub async fn check_network_access(
        &self,
        entity_id: Uuid,
    ) -> LowTrustContainmentResult<()> {
        if let Some(policy) = self.get_containment(entity_id).await? {
            if !policy.restrictions.network_access {
                return Err(LowTrustContainmentError::Violation(
                    "Network access is not allowed".to_string()
                ));
            }
        }
        
        Ok(())
    }
    
    pub async fn remove_containment(&self, entity_id: Uuid) -> LowTrustContainmentResult<()> {
        sqlx::query("DELETE FROM containment_policies WHERE entity_id = $1")
            .bind(entity_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}
