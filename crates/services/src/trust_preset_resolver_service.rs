/// Trust Preset Resolver Service
/// 
/// 信任预设解析

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum TrustPresetError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("preset not found: {0}")]
    PresetNotFound(String),
}

pub type TrustPresetResult<T> = Result<T, TrustPresetError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrustPreset {
    Public,
    Internal,
    Restricted,
    Private,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustPresetConfig {
    pub id: Uuid,
    pub preset_name: String,
    pub min_trust_level: i32,
    pub allowed_actions: Vec<String>,
    pub restrictions: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct TrustPresetResolverService {
    pool: PgPool,
}

impl TrustPresetResolverService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn resolve_preset(&self, preset: &TrustPreset) -> TrustPresetResult<TrustPresetConfig> {
        let preset_name = match preset {
            TrustPreset::Public => "public",
            TrustPreset::Internal => "internal",
            TrustPreset::Restricted => "restricted",
            TrustPreset::Private => "private",
            TrustPreset::Custom(name) => name.as_str(),
        };
        
        let row = sqlx::query(
            r#"
            SELECT id, preset_name, min_trust_level, allowed_actions, restrictions, created_at
            FROM trust_presets
            WHERE preset_name = $1
            "#
        )
        .bind(preset_name)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(TrustPresetConfig {
            id: row.get("id"),
            preset_name: row.get("preset_name"),
            min_trust_level: row.get("min_trust_level"),
            allowed_actions: serde_json::from_value(row.get("allowed_actions")).unwrap_or_default(),
            restrictions: row.get("restrictions"),
            created_at: row.get("created_at"),
        })
    }
    
    pub async fn create_preset(
        &self,
        preset_name: String,
        min_trust_level: i32,
        allowed_actions: Vec<String>,
        restrictions: serde_json::Value,
    ) -> TrustPresetResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO trust_presets 
            (id, preset_name, min_trust_level, allowed_actions, restrictions, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(&preset_name)
        .bind(min_trust_level)
        .bind(serde_json::to_value(&allowed_actions).unwrap())
        .bind(&restrictions)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_min_trust_level(&self, preset: &TrustPreset) -> TrustPresetResult<i32> {
        let config = self.resolve_preset(preset).await?;
        Ok(config.min_trust_level)
    }
    
    pub async fn is_action_allowed(
        &self,
        preset: &TrustPreset,
        action: &str,
    ) -> TrustPresetResult<bool> {
        let config = self.resolve_preset(preset).await?;
        Ok(config.allowed_actions.contains(&action.to_string()))
    }
    
    pub async fn list_presets(&self) -> TrustPresetResult<Vec<TrustPresetConfig>> {
        let rows = sqlx::query(
            r#"
            SELECT id, preset_name, min_trust_level, allowed_actions, restrictions, created_at
            FROM trust_presets
            ORDER BY preset_name
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        let presets = rows.into_iter().map(|row| {
            TrustPresetConfig {
                id: row.get("id"),
                preset_name: row.get("preset_name"),
                min_trust_level: row.get("min_trust_level"),
                allowed_actions: serde_json::from_value(row.get("allowed_actions")).unwrap_or_default(),
                restrictions: row.get("restrictions"),
                created_at: row.get("created_at"),
            }
        }).collect();
        
        Ok(presets)
    }
}
