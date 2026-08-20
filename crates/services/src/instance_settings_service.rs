//! Instance Settings Service — 实例级设置管理
//!
//! 管理实例级别的配置项：通用设置、实验性功能、数据库备份等。
//! 当前使用内存存储，后续可迁移到数据库。

use crate::task_watchdog::WatchdogService;
use crate::database_backup_health_service::{
    inspect_database_backup_health, DatabaseBackupHealthStatus, InspectDatabaseBackupHealthOptions,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::{path::{Path, PathBuf}, sync::Arc, time::SystemTime};
use tokio::process::Command;
use tokio::sync::RwLock;
use uuid::Uuid;

/// 实例设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSettings {
    pub instance_name: String,
    pub version: String,
    pub general: GeneralSettings,
    pub experimental: ExperimentalSettings,
}

impl Default for InstanceSettings {
    fn default() -> Self {
        Self {
            instance_name: "Parrot Agent".to_string(),
            version: "0.1.0".to_string(),
            general: GeneralSettings::default(),
            experimental: ExperimentalSettings::default(),
        }
    }
}

/// 通用设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettings {
    pub timezone: String,
    pub language: String,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            timezone: "UTC".to_string(),
            language: "en".to_string(),
        }
    }
}

/// 实验性功能设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalSettings {
    pub issue_graph_liveness_auto_recovery: bool,
    pub enable_cloud_sync: bool,
    #[serde(default = "default_true")]
    pub enable_built_in_agents: bool,
    #[serde(default = "default_true")]
    pub enable_cases: bool,
    #[serde(default)]
    pub enable_conference_room_chat: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ExperimentalSettings {
    fn default() -> Self {
        Self {
            issue_graph_liveness_auto_recovery: false,
            enable_cloud_sync: false,
            enable_built_in_agents: true,
            enable_cases: true,
            enable_conference_room_chat: false,
        }
    }
}

/// 自动恢复预览结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRecoveryPreview {
    pub affected_issues: i64,
    pub preview_complete: bool,
}

/// 自动恢复执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRecoveryResult {
    pub recovered_issues: i64,
    pub recovery_complete: bool,
}

/// 数据库备份结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseBackupResult {
    pub backup_id: Uuid,
    pub status: String,
    pub backup_file: String,
    pub size_bytes: u64,
    pub pruned_count: u32,
}

/// 实例设置服务接口
#[async_trait]
pub trait InstanceSettingsService: Send + Sync {
    /// 获取全部实例设置
    async fn get_settings(&self) -> Result<InstanceSettings, String>;

    /// 更新实例设置
    async fn update_settings(
        &self,
        settings: serde_json::Value,
    ) -> Result<InstanceSettings, String>;

    /// 获取通用设置
    async fn get_general_settings(&self) -> Result<GeneralSettings, String>;

    /// 更新通用设置
    async fn update_general_settings(
        &self,
        settings: serde_json::Value,
    ) -> Result<GeneralSettings, String>;

    /// 获取实验性功能设置
    async fn get_experimental_settings(&self) -> Result<ExperimentalSettings, String>;

    /// 更新实验性功能设置
    async fn update_experimental_settings(
        &self,
        settings: serde_json::Value,
    ) -> Result<ExperimentalSettings, String>;

    /// 预览自动恢复
    async fn preview_auto_recovery(&self) -> Result<AutoRecoveryPreview, String>;

    /// 执行自动恢复
    async fn run_auto_recovery(&self) -> Result<AutoRecoveryResult, String>;

    /// 创建数据库备份
    async fn create_database_backup(&self) -> Result<DatabaseBackupResult, String>;

    /// 查询数据库备份健康状态
    async fn get_database_backup_health(&self) -> Result<DatabaseBackupHealthStatus, String>;
}

/// 内存实现的实例设置服务
pub struct DefaultInstanceSettingsService {
    settings: Arc<RwLock<InstanceSettings>>,
    pool: Option<PgPool>,
    watchdog_service: Option<Arc<dyn WatchdogService>>,
}

impl DefaultInstanceSettingsService {
    pub fn new() -> Self {
        Self {
            settings: Arc::new(RwLock::new(InstanceSettings::default())),
            pool: None,
            watchdog_service: None,
        }
    }

    pub fn with_pool(pool: PgPool) -> Self {
        Self {
            settings: Arc::new(RwLock::new(InstanceSettings::default())),
            pool: Some(pool),
            watchdog_service: None,
        }
    }

    pub fn with_pool_and_watchdog(
        pool: PgPool,
        watchdog_service: Arc<dyn WatchdogService>,
    ) -> Self {
        Self {
            settings: Arc::new(RwLock::new(InstanceSettings::default())),
            pool: Some(pool),
            watchdog_service: Some(watchdog_service),
        }
    }

    async fn load_from_db(&self) -> Result<InstanceSettings, String> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| "instance settings persistence is not configured".to_string())?;
        
        // 尝试查询现有数据
        let row = sqlx::query(
            "SELECT instance_name,version,general,experimental FROM instance_settings WHERE id=1",
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
        
        if let Some(row) = row {
            // 数据存在，直接返回
            Ok(InstanceSettings {
                instance_name: row.get("instance_name"),
                version: row.get("version"),
                general: serde_json::from_value(row.get("general")).map_err(|e| e.to_string())?,
                experimental: serde_json::from_value(row.get("experimental"))
                    .map_err(|e| e.to_string())?,
            })
        } else {
            // 数据不存在，创建默认数据
            tracing::info!("instance_settings row not found, creating default entry");
            let default_settings = InstanceSettings::default();
            self.persist(&default_settings).await?;
            Ok(default_settings)
        }
    }

    async fn persist(&self, settings: &InstanceSettings) -> Result<(), String> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| "instance settings persistence is not configured".to_string())?;
        
        // 使用INSERT ... ON CONFLICT来处理初次插入和后续更新
        sqlx::query(
            "INSERT INTO instance_settings (id, instance_name, version, general, experimental) 
             VALUES (1, $1, $2, $3, $4) 
             ON CONFLICT (id) DO UPDATE SET 
               instance_name = EXCLUDED.instance_name,
               version = EXCLUDED.version,
               general = EXCLUDED.general,
               experimental = EXCLUDED.experimental,
               updated_at = now()"
        )
        .bind(&settings.instance_name)
        .bind(&settings.version)
        .bind(serde_json::to_value(&settings.general).map_err(|e| e.to_string())?)
        .bind(serde_json::to_value(&settings.experimental).map_err(|e| e.to_string())?)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
        
        Ok(())
    }

    fn database_backup_health_options() -> InspectDatabaseBackupHealthOptions {
        let backup_dir = std::env::var("PARROT_DATABASE_BACKUP_DIR")
            .unwrap_or_else(|_| "data/backups".to_string());
        let max_age_hours = std::env::var("PARROT_DATABASE_BACKUP_MAX_AGE_HOURS")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(24.0);
        let alert_file = std::env::var("PARROT_DATABASE_BACKUP_FAILURE_FILE").ok();
        InspectDatabaseBackupHealthOptions {
            enabled: std::env::var("PARROT_DATABASE_BACKUP_ENABLED")
                .ok()
                .map(|value| !matches!(value.as_str(), "0" | "false" | "FALSE" | "no" | "NO"))
                .unwrap_or(true),
            backup_dir,
            max_age_hours,
            alert_file,
            alert_files: None,
            now: None,
        }
    }
}

impl Default for DefaultInstanceSettingsService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InstanceSettingsService for DefaultInstanceSettingsService {
    async fn get_settings(&self) -> Result<InstanceSettings, String> {
        if self.pool.is_some() {
            return self.load_from_db().await;
        }
        let settings = self.settings.read().await;
        Ok(settings.clone())
    }

    async fn update_settings(
        &self,
        updates: serde_json::Value,
    ) -> Result<InstanceSettings, String> {
        if self.pool.is_some() {
            let mut settings = self.load_from_db().await?;
            if let Some(name) = updates.get("instanceName").and_then(|v| v.as_str()) {
                settings.instance_name = name.into();
            }
            if let Some(version) = updates.get("version").and_then(|v| v.as_str()) {
                settings.version = version.into();
            }
            self.persist(&settings).await?;
            return Ok(settings);
        }
        let mut settings = self.settings.write().await;

        if let Some(name) = updates.get("instanceName").and_then(|v| v.as_str()) {
            settings.instance_name = name.to_string();
        }
        if let Some(version) = updates.get("version").and_then(|v| v.as_str()) {
            settings.version = version.to_string();
        }

        Ok(settings.clone())
    }

    async fn get_general_settings(&self) -> Result<GeneralSettings, String> {
        if self.pool.is_some() {
            return Ok(self.load_from_db().await?.general);
        }
        let settings = self.settings.read().await;
        Ok(settings.general.clone())
    }

    async fn update_general_settings(
        &self,
        updates: serde_json::Value,
    ) -> Result<GeneralSettings, String> {
        if self.pool.is_some() {
            let mut settings = self.load_from_db().await?;
            if let Some(v) = updates.get("timezone").and_then(|v| v.as_str()) {
                settings.general.timezone = v.into();
            }
            if let Some(v) = updates.get("language").and_then(|v| v.as_str()) {
                settings.general.language = v.into();
            }
            self.persist(&settings).await?;
            return Ok(settings.general);
        }
        let mut settings = self.settings.write().await;

        if let Some(tz) = updates.get("timezone").and_then(|v| v.as_str()) {
            settings.general.timezone = tz.to_string();
        }
        if let Some(lang) = updates.get("language").and_then(|v| v.as_str()) {
            settings.general.language = lang.to_string();
        }

        Ok(settings.general.clone())
    }

    async fn get_experimental_settings(&self) -> Result<ExperimentalSettings, String> {
        if self.pool.is_some() {
            return Ok(self.load_from_db().await?.experimental);
        }
        let settings = self.settings.read().await;
        Ok(settings.experimental.clone())
    }

    async fn update_experimental_settings(
        &self,
        updates: serde_json::Value,
    ) -> Result<ExperimentalSettings, String> {
        if self.pool.is_some() {
            let mut settings = self.load_from_db().await?;
            if let Some(v) = updates
                .get("issueGraphLivenessAutoRecovery")
                .and_then(|v| v.as_bool())
            {
                settings.experimental.issue_graph_liveness_auto_recovery = v;
            }
            if let Some(v) = updates.get("enableCloudSync").and_then(|v| v.as_bool()) {
                settings.experimental.enable_cloud_sync = v;
            }
            if let Some(v) = updates.get("enableBuiltInAgents").and_then(|v| v.as_bool()) {
                settings.experimental.enable_built_in_agents = v;
            }
            if let Some(v) = updates.get("enableCases").and_then(|v| v.as_bool()) {
                settings.experimental.enable_cases = v;
            }
            if let Some(v) = updates.get("enableConferenceRoomChat").and_then(|v| v.as_bool()) {
                settings.experimental.enable_conference_room_chat = v;
            }
            self.persist(&settings).await?;
            return Ok(settings.experimental);
        }
        let mut settings = self.settings.write().await;

        if let Some(val) = updates
            .get("issueGraphLivenessAutoRecovery")
            .and_then(|v| v.as_bool())
        {
            settings.experimental.issue_graph_liveness_auto_recovery = val;
        }
        if let Some(val) = updates.get("enableBuiltInAgents").and_then(|v| v.as_bool()) {
            settings.experimental.enable_built_in_agents = val;
        }
        if let Some(val) = updates.get("enableCases").and_then(|v| v.as_bool()) {
            settings.experimental.enable_cases = val;
        }
        if let Some(val) = updates.get("enableConferenceRoomChat").and_then(|v| v.as_bool()) {
            settings.experimental.enable_conference_room_chat = val;
        }

        Ok(settings.experimental.clone())
    }

    async fn preview_auto_recovery(&self) -> Result<AutoRecoveryPreview, String> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| "instance settings persistence is not configured".to_string())?;
        let affected_issues = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM issue_watchdogs WHERE status = 'active'",
        )
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(AutoRecoveryPreview {
            affected_issues,
            preview_complete: true,
        })
    }

    async fn run_auto_recovery(&self) -> Result<AutoRecoveryResult, String> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| "instance settings persistence is not configured".to_string())?;
        let watchdog_service = self
            .watchdog_service
            .as_ref()
            .ok_or_else(|| "watchdog recovery is not configured".to_string())?;
        let companies = sqlx::query_scalar::<_, Uuid>(
            "SELECT DISTINCT company_id FROM issue_watchdogs WHERE status = 'active'",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        let mut recovered_issues = 0_i64;
        for company_id in companies {
            recovered_issues += watchdog_service
                .evaluate_all(company_id)
                .await
                .map_err(|e| e.to_string())? as i64;
        }
        Ok(AutoRecoveryResult {
            recovered_issues,
            recovery_complete: true,
        })
    }

    async fn create_database_backup(&self) -> Result<DatabaseBackupResult, String> {
        let connection_string = std::env::var("DATABASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "database backup is not configured; DATABASE_URL is required".to_string())?;
        let backup_dir = std::env::var("PARROT_DATABASE_BACKUP_DIR")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("data/backups"));
        tokio::fs::create_dir_all(&backup_dir)
            .await
            .map_err(|error| format!("failed to create database backup directory: {}", error))?;

        let backup_id = Uuid::new_v4();
        let file_name = format!(
            "parrot-{}-{}.sql",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            backup_id.simple()
        );
        let backup_path = backup_dir.join(file_name);
        let pg_dump = std::env::var("PARROT_PG_DUMP")
            .unwrap_or_else(|_| "pg_dump".to_string());
        let output = Command::new(pg_dump)
            .args(["--no-owner", "--no-privileges", "--format=plain", "--file"])
            .arg(&backup_path)
            .arg(&connection_string)
            .output()
            .await
            .map_err(|error| format!("failed to start pg_dump: {}", error))?;
        if !output.status.success() {
            let _ = tokio::fs::remove_file(&backup_path).await;
            let diagnostics = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!(
                "pg_dump exited with code {}{}",
                output.status.code().unwrap_or(-1),
                if diagnostics.is_empty() { String::new() } else { format!(": {}", diagnostics) }
            ));
        }

        let size_bytes = tokio::fs::metadata(&backup_path)
            .await
            .map_err(|error| format!("failed to stat database backup: {}", error))?
            .len();
        let retention_days = std::env::var("PARROT_DATABASE_BACKUP_RETENTION_DAYS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(7);
        let pruned_count = prune_database_backups(&backup_dir, retention_days)
            .await
            .map_err(|error| format!("failed to prune database backups: {}", error))?;
        Ok(DatabaseBackupResult {
            backup_id,
            status: "completed".to_string(),
            backup_file: backup_path.to_string_lossy().to_string(),
            size_bytes,
            pruned_count,
        })
    }

    async fn get_database_backup_health(&self) -> Result<DatabaseBackupHealthStatus, String> {
        Ok(inspect_database_backup_health(
            Self::database_backup_health_options(),
        ))
    }
}

async fn prune_database_backups(directory: &Path, retention_days: u64) -> std::io::Result<u32> {
    let mut entries = tokio::fs::read_dir(directory).await?;
    let cutoff = SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(retention_days.saturating_mul(86_400)))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut removed = 0;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("parrot-") || !name.ends_with(".sql") {
            continue;
        }
        let metadata = entry.metadata().await?;
        if metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH) < cutoff {
            tokio::fs::remove_file(entry.path()).await?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::prune_database_backups;
    use tempfile::tempdir;

    #[tokio::test]
    async fn prune_removes_only_parrot_sql_backups() {
        let directory = tempdir().unwrap();
        tokio::fs::write(directory.path().join("parrot-old.sql"), "old")
            .await
            .unwrap();
        tokio::fs::write(directory.path().join("keep.txt"), "keep")
            .await
            .unwrap();

        let removed = prune_database_backups(directory.path(), 0).await.unwrap();
        assert_eq!(removed, 1);
        assert!(!directory.path().join("parrot-old.sql").exists());
        assert!(directory.path().join("keep.txt").exists());
    }
}
