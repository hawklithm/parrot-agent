//! Instance Settings Service — 实例级设置管理
//!
//! 管理实例级别的配置项：通用设置、实验性功能、数据库备份等。
//! 当前使用内存存储，后续可迁移到数据库。

use crate::task_watchdog::WatchdogService;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
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
        let row = sqlx::query(
            "SELECT instance_name,version,general,experimental FROM instance_settings WHERE id=1",
        )
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(InstanceSettings {
            instance_name: row.get("instance_name"),
            version: row.get("version"),
            general: serde_json::from_value(row.get("general")).map_err(|e| e.to_string())?,
            experimental: serde_json::from_value(row.get("experimental"))
                .map_err(|e| e.to_string())?,
        })
    }

    async fn persist(&self, settings: &InstanceSettings) -> Result<(), String> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| "instance settings persistence is not configured".to_string())?;
        sqlx::query("UPDATE instance_settings SET instance_name=$1,version=$2,general=$3,experimental=$4,updated_at=now() WHERE id=1").bind(&settings.instance_name).bind(&settings.version).bind(serde_json::to_value(&settings.general).map_err(|e|e.to_string())?).bind(serde_json::to_value(&settings.experimental).map_err(|e|e.to_string())?).execute(pool).await.map_err(|e|e.to_string())?;
        Ok(())
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
        Err("database backup is not configured; configure the deployment backup worker before requesting a backup".to_string())
    }
}
