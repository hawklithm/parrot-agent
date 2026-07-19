//! Instance Settings Service — 实例级设置管理
//!
//! 管理实例级别的配置项：通用设置、实验性功能、数据库备份等。
//! 当前使用内存存储，后续可迁移到数据库。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
}

impl Default for ExperimentalSettings {
    fn default() -> Self {
        Self {
            issue_graph_liveness_auto_recovery: false,
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
    async fn update_settings(&self, settings: serde_json::Value) -> Result<InstanceSettings, String>;

    /// 获取通用设置
    async fn get_general_settings(&self) -> Result<GeneralSettings, String>;

    /// 更新通用设置
    async fn update_general_settings(&self, settings: serde_json::Value) -> Result<GeneralSettings, String>;

    /// 获取实验性功能设置
    async fn get_experimental_settings(&self) -> Result<ExperimentalSettings, String>;

    /// 更新实验性功能设置
    async fn update_experimental_settings(&self, settings: serde_json::Value) -> Result<ExperimentalSettings, String>;

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
}

impl DefaultInstanceSettingsService {
    pub fn new() -> Self {
        Self {
            settings: Arc::new(RwLock::new(InstanceSettings::default())),
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
        let settings = self.settings.read().await;
        Ok(settings.clone())
    }

    async fn update_settings(&self, updates: serde_json::Value) -> Result<InstanceSettings, String> {
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
        let settings = self.settings.read().await;
        Ok(settings.general.clone())
    }

    async fn update_general_settings(&self, updates: serde_json::Value) -> Result<GeneralSettings, String> {
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
        let settings = self.settings.read().await;
        Ok(settings.experimental.clone())
    }

    async fn update_experimental_settings(&self, updates: serde_json::Value) -> Result<ExperimentalSettings, String> {
        let mut settings = self.settings.write().await;

        if let Some(val) = updates.get("issueGraphLivenessAutoRecovery").and_then(|v| v.as_bool()) {
            settings.experimental.issue_graph_liveness_auto_recovery = val;
        }

        Ok(settings.experimental.clone())
    }

    async fn preview_auto_recovery(&self) -> Result<AutoRecoveryPreview, String> {
        // TODO: 查询 watchdog 子系统获取受影响的 issue 列表
        Ok(AutoRecoveryPreview {
            affected_issues: 0,
            preview_complete: true,
        })
    }

    async fn run_auto_recovery(&self) -> Result<AutoRecoveryResult, String> {
        // TODO: 触发 watchdog 恢复流程
        Ok(AutoRecoveryResult {
            recovered_issues: 0,
            recovery_complete: true,
        })
    }

    async fn create_database_backup(&self) -> Result<DatabaseBackupResult, String> {
        // TODO: 调用 pg_dump 或其他备份机制
        Ok(DatabaseBackupResult {
            backup_id: Uuid::new_v4(),
            status: "started".to_string(),
        })
    }
}
