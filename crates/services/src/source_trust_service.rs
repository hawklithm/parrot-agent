/// Source Trust Service
/// 
/// 来源信任管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SourceTrustError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("untrusted source: {0}")]
    Untrusted(String),
}

pub type SourceTrustResult<T> = Result<T, SourceTrustError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustLevel {
    Untrusted = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Verified = 4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceTrust {
    pub id: Uuid,
    pub source_type: String,
    pub source_id: String,
    pub trust_level: TrustLevel,
    pub verified_by: Option<Uuid>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct SourceTrustService {
    pool: PgPool,
}

impl SourceTrustService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn set_trust(
        &self,
        source_type: String,
        source_id: String,
        trust_level: TrustLevel,
        verified_by: Option<Uuid>,
    ) -> SourceTrustResult<Uuid> {
        let id = Uuid::new_v4();
        let verified_at = if verified_by.is_some() {
            Some(chrono::Utc::now())
        } else {
            None
        };
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO source_trust 
            (id, source_type, source_id, trust_level, verified_by, verified_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (source_type, source_id)
            DO UPDATE SET trust_level = $4, verified_by = $5, verified_at = $6
            RETURNING id
            "#
        )
        .bind(id)
        .bind(&source_type)
        .bind(&source_id)
        .bind(trust_level as i32)
        .bind(verified_by)
        .bind(verified_at)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_trust(
        &self,
        source_type: &str,
        source_id: &str,
    ) -> SourceTrustResult<Option<TrustLevel>> {
        let row = sqlx::query(
            r#"
            SELECT trust_level
            FROM source_trust
            WHERE source_type = $1 AND source_id = $2
            "#
        )
        .bind(source_type)
        .bind(source_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| {
            let level: i32 = r.get("trust_level");
            match level {
                0 => TrustLevel::Untrusted,
                1 => TrustLevel::Low,
                2 => TrustLevel::Medium,
                3 => TrustLevel::High,
                4 => TrustLevel::Verified,
                _ => TrustLevel::Untrusted,
            }
        }))
    }
    
    pub async fn check_trust(
        &self,
        source_type: &str,
        source_id: &str,
        minimum_level: TrustLevel,
    ) -> SourceTrustResult<bool> {
        let trust = self.get_trust(source_type, source_id).await?;
        
        match trust {
            Some(level) => Ok(level >= minimum_level),
            None => Ok(false),
        }
    }
    
    pub async fn require_trust(
        &self,
        source_type: &str,
        source_id: &str,
        minimum_level: TrustLevel,
    ) -> SourceTrustResult<()> {
        if !self.check_trust(source_type, source_id, minimum_level).await? {
            return Err(SourceTrustError::Untrusted(
                format!("Source {}:{} does not meet minimum trust level", source_type, source_id)
            ));
        }
        Ok(())
    }
    
    pub async fn list_trusted_sources(
        &self,
        source_type: Option<&str>,
        minimum_level: TrustLevel,
    ) -> SourceTrustResult<Vec<SourceTrust>> {
        let mut query = "SELECT id, source_type, source_id, trust_level, verified_by, verified_at, created_at FROM source_trust WHERE trust_level >= $1".to_string();
        
        if source_type.is_some() {
            query.push_str(" AND source_type = $2");
        }
        
        query.push_str(" ORDER BY trust_level DESC");
        
        let rows = sqlx::query(&query)
            .bind(minimum_level as i32)
            .fetch_all(&self.pool)
            .await?;
        
        let sources = rows.into_iter().map(|row| {
            let level: i32 = row.get("trust_level");
            SourceTrust {
                id: row.get("id"),
                source_type: row.get("source_type"),
                source_id: row.get("source_id"),
                trust_level: match level {
                    0 => TrustLevel::Untrusted,
                    1 => TrustLevel::Low,
                    2 => TrustLevel::Medium,
                    3 => TrustLevel::High,
                    4 => TrustLevel::Verified,
                    _ => TrustLevel::Untrusted,
                },
                verified_by: row.get("verified_by"),
                verified_at: row.get("verified_at"),
                created_at: row.get("created_at"),
            }
        }).collect();
        
        Ok(sources)
    }
}
