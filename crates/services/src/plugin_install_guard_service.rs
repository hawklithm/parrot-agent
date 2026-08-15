/// Plugin Install Guard Service
/// 
/// Plugin安装守卫

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PluginInstallGuardError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("安装被阻止: {0}")]
    InstallBlocked(String),
}

pub type PluginInstallGuardResult<T> = Result<T, PluginInstallGuardError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallCheck {
    pub plugin_name: String,
    pub version: String,
    pub passed: bool,
    pub reason: Option<String>,
}

pub struct PluginInstallGuardService {
    pool: PgPool,
}

impl PluginInstallGuardService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn check_installation(
        &self,
        plugin_name: &str,
        version: &str,
    ) -> PluginInstallGuardResult<InstallCheck> {
        // 检查黑名单
        let blacklisted = sqlx::query(
            "SELECT 1 FROM plugin_blacklist WHERE plugin_name = $1"
        )
        .bind(plugin_name)
        .fetch_optional(&self.pool)
        .await?;
        
        if blacklisted.is_some() {
            return Ok(InstallCheck {
                plugin_name: plugin_name.to_string(),
                version: version.to_string(),
                passed: false,
                reason: Some("Plugin is blacklisted".to_string()),
            });
        }
        
        // 检查版本兼容性
        let compatible = self.check_version_compatibility(version)?;
        
        if !compatible {
            return Ok(InstallCheck {
                plugin_name: plugin_name.to_string(),
                version: version.to_string(),
                passed: false,
                reason: Some("Version incompatible".to_string()),
            });
        }
        
        // 检查依赖冲突
        let has_conflict = self.check_dependency_conflicts(plugin_name).await?;
        
        if has_conflict {
            return Ok(InstallCheck {
                plugin_name: plugin_name.to_string(),
                version: version.to_string(),
                passed: false,
                reason: Some("Dependency conflict detected".to_string()),
            });
        }
        
        Ok(InstallCheck {
            plugin_name: plugin_name.to_string(),
            version: version.to_string(),
            passed: true,
            reason: None,
        })
    }
    
    fn check_version_compatibility(&self, _version: &str) -> PluginInstallGuardResult<bool> {
        // 简化实现
        Ok(true)
    }
    
    async fn check_dependency_conflicts(&self, _plugin_name: &str) -> PluginInstallGuardResult<bool> {
        // 简化实现
        Ok(false)
    }
    
    pub async fn add_to_blacklist(
        &self,
        plugin_name: String,
        reason: String,
    ) -> PluginInstallGuardResult<()> {
        sqlx::query(
            r#"
            INSERT INTO plugin_blacklist (id, plugin_name, reason, created_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (plugin_name) DO NOTHING
            "#
        )
        .bind(Uuid::new_v4())
        .bind(&plugin_name)
        .bind(&reason)
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
