//! Instance Settings Service — 实例级设置管理
//!
//! 管理实例级别的配置项：通用设置、实验性功能、数据库备份等。
//! 当前使用内存存储，后续可迁移到数据库。

use crate::database_backup_health_service::{
    inspect_database_backup_health, DatabaseBackupHealthStatus, InspectDatabaseBackupHealthOptions,
};
use crate::task_watchdog::WatchdogService;
use async_trait::async_trait;
use flate2::{write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};
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
    #[serde(default)]
    pub censor_username_in_logs: bool,
    #[serde(default)]
    pub keyboard_shortcuts: bool,
    #[serde(default = "default_feedback_preference")]
    pub feedback_data_sharing_preference: String,
    #[serde(default)]
    pub backup_retention: BackupRetentionPolicy,
    #[serde(default)]
    pub execution_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRetentionPolicy {
    #[serde(default = "default_daily_retention")]
    pub daily_days: u32,
    #[serde(default = "default_weekly_retention")]
    pub weekly_weeks: u32,
    #[serde(default = "default_monthly_retention")]
    pub monthly_months: u32,
}

fn default_feedback_preference() -> String {
    "prompt".to_string()
}

fn default_daily_retention() -> u32 {
    7
}

fn default_weekly_retention() -> u32 {
    4
}

fn default_monthly_retention() -> u32 {
    1
}

impl Default for BackupRetentionPolicy {
    fn default() -> Self {
        Self {
            daily_days: default_daily_retention(),
            weekly_weeks: default_weekly_retention(),
            monthly_months: default_monthly_retention(),
        }
    }
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            timezone: "UTC".to_string(),
            language: "en".to_string(),
            censor_username_in_logs: false,
            keyboard_shortcuts: false,
            feedback_data_sharing_preference: default_feedback_preference(),
            backup_retention: BackupRetentionPolicy::default(),
            execution_mode: None,
        }
    }
}

fn apply_general_updates(
    settings: &mut GeneralSettings,
    updates: &serde_json::Value,
) -> Result<(), String> {
    if let Some(v) = updates.get("timezone").and_then(|v| v.as_str()) {
        settings.timezone = v.to_string();
    }
    if let Some(v) = updates.get("language").and_then(|v| v.as_str()) {
        settings.language = v.to_string();
    }
    if let Some(v) = updates
        .get("censorUsernameInLogs")
        .and_then(|v| v.as_bool())
    {
        settings.censor_username_in_logs = v;
    }
    if let Some(v) = updates.get("keyboardShortcuts").and_then(|v| v.as_bool()) {
        settings.keyboard_shortcuts = v;
    }
    if let Some(v) = updates
        .get("feedbackDataSharingPreference")
        .and_then(|v| v.as_str())
    {
        if !matches!(v, "allowed" | "not_allowed" | "prompt") {
            return Err(
                "feedbackDataSharingPreference must be allowed, not_allowed, or prompt".to_string(),
            );
        }
        settings.feedback_data_sharing_preference = v.to_string();
    }
    if let Some(retention) = updates.get("backupRetention") {
        let current = &mut settings.backup_retention;
        if let Some(v) = retention.get("dailyDays").and_then(|v| v.as_u64()) {
            if !matches!(v, 3 | 7 | 14) {
                return Err("backupRetention.dailyDays must be 3, 7, or 14".to_string());
            }
            current.daily_days = v as u32;
        }
        if let Some(v) = retention.get("weeklyWeeks").and_then(|v| v.as_u64()) {
            if !matches!(v, 1 | 2 | 4) {
                return Err("backupRetention.weeklyWeeks must be 1, 2, or 4".to_string());
            }
            current.weekly_weeks = v as u32;
        }
        if let Some(v) = retention.get("monthlyMonths").and_then(|v| v.as_u64()) {
            if !matches!(v, 1 | 3 | 6) {
                return Err("backupRetention.monthlyMonths must be 1, 3, or 6".to_string());
            }
            current.monthly_months = v as u32;
        }
    }
    if let Some(mode) = updates.get("executionMode") {
        settings.execution_mode = match mode {
            serde_json::Value::Null => None,
            serde_json::Value::String(value) if matches!(value.as_str(), "any" | "kubernetes") => {
                Some(value.clone())
            }
            _ => return Err("executionMode must be any, kubernetes, or null".to_string()),
        };
    }
    Ok(())
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
               updated_at = now()",
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
            apply_general_updates(&mut settings.general, &updates)?;
            self.persist(&settings).await?;
            return Ok(settings.general);
        }
        let mut settings = self.settings.write().await;
        apply_general_updates(&mut settings.general, &updates)?;

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
            if let Some(v) = updates
                .get("enableConferenceRoomChat")
                .and_then(|v| v.as_bool())
            {
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
        if let Some(val) = updates
            .get("enableConferenceRoomChat")
            .and_then(|v| v.as_bool())
        {
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
            .ok_or_else(|| {
                "database backup is not configured; DATABASE_URL is required".to_string()
            })?;
        let backup_dir = std::env::var("PARROT_DATABASE_BACKUP_DIR")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("data/backups"));
        tokio::fs::create_dir_all(&backup_dir)
            .await
            .map_err(|error| format!("failed to create database backup directory: {}", error))?;

        let backup_id = Uuid::new_v4();
        let file_stem = format!(
            "parrot-{}-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            backup_id.simple()
        );
        let plain_path = backup_dir.join(format!("{}.sql", file_stem));
        let backup_path = backup_dir.join(format!("{}.sql.gz", file_stem));
        let compressed_tmp_path = backup_dir.join(format!("{}.sql.gz.tmp", file_stem));
        let pg_dump = std::env::var("PARROT_PG_DUMP").unwrap_or_else(|_| "pg_dump".to_string());
        let output = Command::new(pg_dump)
            .args(["--no-owner", "--no-privileges", "--format=plain", "--file"])
            .arg(&plain_path)
            .arg(&connection_string)
            .output()
            .await
            .map_err(|error| format!("failed to start pg_dump: {}", error))?;
        if !output.status.success() {
            let _ = tokio::fs::remove_file(&plain_path).await;
            let diagnostics = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!(
                "pg_dump exited with code {}{}",
                output.status.code().unwrap_or(-1),
                if diagnostics.is_empty() {
                    String::new()
                } else {
                    format!(": {}", diagnostics)
                }
            ));
        }

        let plain_sql = tokio::fs::read(&plain_path)
            .await
            .map_err(|error| format!("failed to read database backup: {}", error))?;
        let compressed = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder
                .write_all(&plain_sql)
                .map_err(|error| format!("failed to compress database backup: {}", error))?;
            encoder
                .finish()
                .map_err(|error| format!("failed to finish database backup compression: {}", error))
        })
        .await
        .map_err(|error| format!("database backup compression task failed: {}", error))??;
        tokio::fs::write(&compressed_tmp_path, compressed)
            .await
            .map_err(|error| format!("failed to write compressed database backup: {}", error))?;
        tokio::fs::rename(&compressed_tmp_path, &backup_path)
            .await
            .map_err(|error| format!("failed to commit compressed database backup: {}", error))?;
        let _ = tokio::fs::remove_file(&plain_path).await;

        let size_bytes = tokio::fs::metadata(&backup_path)
            .await
            .map_err(|error| format!("failed to stat database backup: {}", error))?
            .len();
        let settings = self.get_general_settings().await.ok();
        let configured_daily_retention = settings
            .as_ref()
            .map(|settings| settings.backup_retention.daily_days)
            .unwrap_or(7);
        let retention_days_override = std::env::var("PARROT_DATABASE_BACKUP_RETENTION_DAYS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0);
        let retention = BackupRetentionPolicy {
            daily_days: retention_days_override.unwrap_or(configured_daily_retention as u64) as u32,
            weekly_weeks: settings
                .as_ref()
                .map(|settings| settings.backup_retention.weekly_weeks)
                .unwrap_or(4),
            monthly_months: settings
                .as_ref()
                .map(|settings| settings.backup_retention.monthly_months)
                .unwrap_or(1),
        };
        let pruned_count = prune_database_backups_with_policy(&backup_dir, &retention)
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
    prune_database_backups_with_policy(
        directory,
        &BackupRetentionPolicy {
            daily_days: retention_days as u32,
            weekly_weeks: 0,
            monthly_months: 0,
        },
    )
    .await
}

async fn prune_database_backups_with_policy(
    directory: &Path,
    policy: &BackupRetentionPolicy,
) -> std::io::Result<u32> {
    let mut entries = tokio::fs::read_dir(directory).await?;
    let now = SystemTime::now();
    let mut latest_by_bucket: HashMap<(char, u64), SystemTime> = HashMap::new();
    let mut candidates = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("parrot-") || (!name.ends_with(".sql") && !name.ends_with(".sql.gz")) {
            continue;
        }
        let modified = entry
            .metadata()
            .await?
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let age_seconds = now.duration_since(modified).unwrap_or_default().as_secs();
        let age_days = age_seconds / 86_400;
        let bucket = if age_days < policy.daily_days as u64 {
            Some(('d', age_days))
        } else if age_days < policy.weekly_weeks as u64 * 7 {
            Some(('w', age_seconds / 604_800))
        } else if age_days < policy.monthly_months as u64 * 31 {
            Some(('m', age_seconds / 2_592_000))
        } else {
            None
        };
        if let Some(bucket) = bucket {
            let replace = latest_by_bucket
                .get(&bucket)
                .map(|latest| modified > *latest)
                .unwrap_or(true);
            if replace {
                latest_by_bucket.insert(bucket, modified);
            }
        }
        candidates.push((entry.path(), modified, age_days));
    }
    let mut removed = 0;
    for (path, modified, age_days) in candidates {
        let keep = if age_days < policy.daily_days as u64 {
            true
        } else if age_days < policy.weekly_weeks as u64 * 7 {
            latest_by_bucket.values().any(|value| *value == modified)
        } else if age_days < policy.monthly_months as u64 * 31 {
            latest_by_bucket.values().any(|value| *value == modified)
        } else {
            false
        };
        if !keep {
            tokio::fs::remove_file(path).await?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::{apply_general_updates, prune_database_backups, GeneralSettings};
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn prune_removes_only_parrot_backup_files() {
        let directory = tempdir().unwrap();
        tokio::fs::write(directory.path().join("parrot-old.sql"), "old")
            .await
            .unwrap();
        tokio::fs::write(directory.path().join("parrot-old.sql.gz"), "old")
            .await
            .unwrap();
        tokio::fs::write(directory.path().join("keep.txt"), "keep")
            .await
            .unwrap();

        let removed = prune_database_backups(directory.path(), 0).await.unwrap();
        assert_eq!(removed, 2);
        assert!(!directory.path().join("parrot-old.sql").exists());
        assert!(!directory.path().join("parrot-old.sql.gz").exists());
        assert!(directory.path().join("keep.txt").exists());
    }

    #[test]
    fn general_settings_defaults_match_paperclip_defaults() {
        let settings = GeneralSettings::default();
        assert!(!settings.censor_username_in_logs);
        assert!(!settings.keyboard_shortcuts);
        assert_eq!(settings.feedback_data_sharing_preference, "prompt");
        assert_eq!(settings.backup_retention.daily_days, 7);
        assert_eq!(settings.backup_retention.weekly_weeks, 4);
        assert_eq!(settings.backup_retention.monthly_months, 1);
        assert_eq!(settings.execution_mode, None);
    }

    #[test]
    fn general_settings_update_validates_paperclip_compatible_values() {
        let mut settings = GeneralSettings::default();
        apply_general_updates(
            &mut settings,
            &json!({
                "censorUsernameInLogs": true,
                "keyboardShortcuts": true,
                "feedbackDataSharingPreference": "allowed",
                "backupRetention": { "dailyDays": 14, "weeklyWeeks": 2, "monthlyMonths": 6 },
                "executionMode": "kubernetes"
            }),
        )
        .unwrap();
        assert!(settings.censor_username_in_logs);
        assert!(settings.keyboard_shortcuts);
        assert_eq!(settings.feedback_data_sharing_preference, "allowed");
        assert_eq!(settings.backup_retention.daily_days, 14);
        assert_eq!(settings.backup_retention.weekly_weeks, 2);
        assert_eq!(settings.backup_retention.monthly_months, 6);
        assert_eq!(settings.execution_mode.as_deref(), Some("kubernetes"));

        for update in [
            json!({ "feedbackDataSharingPreference": "unknown" }),
            json!({ "backupRetention": { "dailyDays": 30 } }),
            json!({ "executionMode": "docker" }),
        ] {
            assert!(apply_general_updates(&mut settings, &update).is_err());
        }
    }
}
