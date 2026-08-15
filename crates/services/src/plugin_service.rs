use async_trait::async_trait;
use models::Plugin;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PluginServiceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("plugin not found: {0}")]
    NotFound(Uuid),
    #[error("invalid plugin state: {0}")]
    InvalidState(String),
    #[error("feature disabled: {0}")]
    FeatureDisabled(String),
}
pub type PluginResult<T> = Result<T, PluginServiceError>;

#[async_trait]
pub trait PluginService: Send + Sync {
    async fn list(&self, status: Option<String>) -> PluginResult<Vec<Plugin>>;
    async fn get(&self, id: Uuid) -> PluginResult<Plugin>;
    async fn install(&self, body: Value) -> PluginResult<Plugin>;
    async fn transition(&self, id: Uuid, status: &str) -> PluginResult<Plugin>;
    async fn remove(&self, id: Uuid) -> PluginResult<()>;
    async fn update_config(&self, id: Uuid, config: Value) -> PluginResult<Plugin>;
    async fn get_data(&self, id: Uuid, key: &str) -> PluginResult<Value>;
    async fn set_data(&self, id: Uuid, key: &str, value: Value) -> PluginResult<Value>;
    async fn jobs(&self, id: Uuid) -> PluginResult<Vec<Value>>;
    async fn job_runs(&self, plugin_id: Uuid, job_id: Uuid) -> PluginResult<Vec<Value>>;
    async fn trigger_job(&self, plugin_id: Uuid, job_id: Uuid) -> PluginResult<Value>;
    async fn logs(&self, id: Uuid) -> PluginResult<Vec<Value>>;
    async fn dispatch_tool(&self, id: Uuid, tool: &str, parameters: Value) -> PluginResult<Value>;
    async fn dispatch_action(&self, id: Uuid, action: &str, payload: Value) -> PluginResult<Value>;

    // ---- P1.2: Plugin 扩展面 ----

    /// 该 plugin 是否支持 bridge SSE 流（基于 manifest 声明）。
    async fn bridge_stream_supported(&self, plugin_id: Uuid) -> PluginResult<bool>;

    /// 接收 plugin webhook ingress（company-scoped）。
    /// 当前 parrot 未实现 webhook runtime，返回 feature-disabled（不伪造成功）。
    async fn ingest_webhook(
        &self,
        plugin_id: Uuid,
        endpoint_key: &str,
        company_id: Uuid,
        payload: Value,
    ) -> PluginResult<Value>;

    /// 列出 plugin 声明的本地文件夹（来自 config.localFolders）。
    async fn list_local_folders(
        &self,
        plugin_id: Uuid,
        company_id: Uuid,
    ) -> PluginResult<Vec<Value>>;

    /// 查询单个本地文件夹状态（含磁盘存在性）。
    async fn get_local_folder_status(
        &self,
        plugin_id: Uuid,
        company_id: Uuid,
        folder_key: &str,
    ) -> PluginResult<Value>;

    /// 校验本地文件夹路径安全性（拒绝绝对路径 / `..` 穿越 / 空字节）。
    async fn validate_local_folder_path(&self, path: &str) -> PluginResult<()>;

    /// 更新本地文件夹状态/元数据（写入 plugin config.localFolders[key]）。
    async fn update_local_folder(
        &self,
        plugin_id: Uuid,
        company_id: Uuid,
        folder_key: &str,
        body: Value,
    ) -> PluginResult<Value>;

    /// 安全读取 plugin UI 静态资源（防路径穿越）。
    /// 当前实现返回 feature-disabled（未挂载 UI 资源目录）。
    async fn serve_ui_asset(&self, plugin_id: Uuid, rel_path: &str) -> PluginResult<Vec<u8>>;

    /// 取消一个 plugin job run（仅当未终态时可取消）。
    async fn cancel_job_run(
        &self,
        plugin_id: Uuid,
        job_id: Uuid,
        run_id: Uuid,
    ) -> PluginResult<Value>;

    /// 重试一个 plugin job run（重置为 queued）。
    async fn retry_job_run(
        &self,
        plugin_id: Uuid,
        job_id: Uuid,
        run_id: Uuid,
    ) -> PluginResult<Value>;
}

pub struct DefaultPluginService {
    pool: PgPool,
}
impl DefaultPluginService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_plugin(row: &sqlx::postgres::PgRow) -> Plugin {
    Plugin {
        id: row.get("id"),
        plugin_key: row.get("plugin_key"),
        name: row.get("name"),
        version: row.get("version"),
        api_version: row.get("api_version"),
        categories: row.get("categories"),
        install_order: row.get("install_order"),
        status: row.get("status"),
        package_name: row.get("package_name"),
        install_path: row.get("install_path"),
        manifest: row.get("manifest"),
        config: row.get("config"),
        last_error: row.get("last_error"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[async_trait]
impl PluginService for DefaultPluginService {
    async fn list(&self, status: Option<String>) -> PluginResult<Vec<Plugin>> {
        let rows = sqlx::query(
            "SELECT * FROM plugins WHERE ($1::text IS NULL OR status = $1) ORDER BY name",
        )
        .bind(status)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(row_plugin).collect())
    }
    async fn get(&self, id: Uuid) -> PluginResult<Plugin> {
        sqlx::query("SELECT * FROM plugins WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .map(|r| row_plugin(&r))
            .ok_or(PluginServiceError::NotFound(id))
    }
    async fn install(&self, body: Value) -> PluginResult<Plugin> {
        crate::plugin_loader::parse_manifest(&body).map_err(PluginServiceError::InvalidState)?;
        let id = Uuid::new_v4();
        let key = body
            .get("pluginKey")
            .or_else(|| body.get("packageName"))
            .and_then(Value::as_str)
            .unwrap_or("local.plugin")
            .to_string();
        let name = body
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&key)
            .to_string();
        let version = body
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("0.0.0")
            .to_string();
        let package_name = body
            .get("packageName")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let install_path = body
            .get("localPath")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let api_version = body.get("apiVersion").and_then(Value::as_i64).unwrap_or(1) as i32;
        let categories = body.get("categories").cloned().unwrap_or_else(|| json!([]));
        let row = sqlx::query("INSERT INTO plugins(id,plugin_key,name,version,api_version,categories,status,package_name,install_path,manifest) VALUES($1,$2,$3,$4,$5,$6,'ready',$7,$8,$9) ON CONFLICT(plugin_key) DO UPDATE SET version=EXCLUDED.version, status='ready', manifest=EXCLUDED.manifest, updated_at=NOW() RETURNING *")
            .bind(id).bind(key).bind(name).bind(version).bind(api_version).bind(categories)
            .bind(package_name).bind(install_path).bind(body).fetch_one(&self.pool).await?;
        Ok(row_plugin(&row))
    }
    async fn transition(&self, id: Uuid, status: &str) -> PluginResult<Plugin> {
        let current = self.get(id).await?;
        let valid = match (current.status.as_str(), status) {
            ("installed", "ready" | "error" | "uninstalled")
            | ("ready", "disabled" | "error" | "upgrade_pending" | "ready")
            | ("disabled" | "error" | "upgrade_pending", "ready")
            | (_, "uninstalled") => true,
            _ => false,
        };
        if !valid {
            return Err(PluginServiceError::InvalidState(format!(
                "{} -> {}",
                current.status, status
            )));
        }
        let row = sqlx::query("UPDATE plugins SET status=$2, updated_at=NOW(), last_error=CASE WHEN $2='error' THEN last_error ELSE NULL END WHERE id=$1 RETURNING *").bind(id).bind(status).fetch_one(&self.pool).await?;
        Ok(row_plugin(&row))
    }
    async fn remove(&self, id: Uuid) -> PluginResult<()> {
        self.get(id).await?;
        sqlx::query("DELETE FROM plugins WHERE id=$1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    async fn update_config(&self, id: Uuid, config: Value) -> PluginResult<Plugin> {
        let plugin = self.get(id).await?;
        crate::plugin_config_validator::validate_config(&plugin.manifest, &config).map_err(PluginServiceError::InvalidState)?;
        let r =
            sqlx::query("UPDATE plugins SET config=$2, updated_at=NOW() WHERE id=$1 RETURNING *")
                .bind(id)
                .bind(config)
                .fetch_one(&self.pool)
                .await?;
        Ok(row_plugin(&r))
    }
    async fn get_data(&self, id: Uuid, key: &str) -> PluginResult<Value> {
        self.get(id).await?;
        Ok(
            sqlx::query("SELECT value FROM plugin_data WHERE plugin_id=$1 AND data_key=$2")
                .bind(id)
                .bind(key)
                .fetch_optional(&self.pool)
                .await?
                .map(|r| r.get("value"))
                .unwrap_or(Value::Null),
        )
    }
    async fn set_data(&self, id: Uuid, key: &str, value: Value) -> PluginResult<Value> {
        self.get(id).await?;
        sqlx::query("INSERT INTO plugin_data(plugin_id,data_key,value) VALUES($1,$2,$3) ON CONFLICT(plugin_id,data_key) DO UPDATE SET value=EXCLUDED.value,updated_at=NOW()").bind(id).bind(key).bind(&value).execute(&self.pool).await?;
        Ok(json!({"pluginId":id,"key":key,"value":value}))
    }
    async fn jobs(&self, id: Uuid) -> PluginResult<Vec<Value>> {
        self.get(id).await?;
        let rs=sqlx::query("SELECT id,job_key,name,schedule,enabled,definition FROM plugin_jobs WHERE plugin_id=$1 ORDER BY name").bind(id).fetch_all(&self.pool).await?;
        Ok(rs.into_iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"pluginId":id,"key":r.get::<String,_>("job_key"),"name":r.get::<String,_>("name"),"schedule":r.get::<Option<String>,_>("schedule"),"enabled":r.get::<bool,_>("enabled"),"definition":r.get::<Value,_>("definition")})).collect())
    }
    async fn job_runs(&self, plugin_id: Uuid, job_id: Uuid) -> PluginResult<Vec<Value>> {
        self.get(plugin_id).await?;
        let rs=sqlx::query("SELECT id,status,result,created_at,completed_at FROM plugin_job_runs WHERE plugin_id=$1 AND job_id=$2 ORDER BY created_at DESC").bind(plugin_id).bind(job_id).fetch_all(&self.pool).await?;
        Ok(rs.into_iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"jobId":job_id,"status":r.get::<String,_>("status"),"result":r.get::<Value,_>("result"),"createdAt":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"completedAt":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("completed_at")})).collect())
    }
    async fn trigger_job(&self, plugin_id: Uuid, job_id: Uuid) -> PluginResult<Value> {
        self.get(plugin_id).await?;
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO plugin_job_runs(id,plugin_id,job_id,status) VALUES($1,$2,$3,'queued')",
        )
        .bind(id)
        .bind(plugin_id)
        .bind(job_id)
        .execute(&self.pool)
        .await?;
        Ok(json!({"id":id,"pluginId":plugin_id,"jobId":job_id,"status":"queued"}))
    }
    async fn logs(&self, id: Uuid) -> PluginResult<Vec<Value>> {
        self.get(id).await?;
        let rs=sqlx::query("SELECT id,level,message,metadata,created_at FROM plugin_logs WHERE plugin_id=$1 ORDER BY created_at DESC LIMIT 500").bind(id).fetch_all(&self.pool).await?;
        Ok(rs.into_iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"level":r.get::<String,_>("level"),"message":r.get::<String,_>("message"),"metadata":r.get::<Value,_>("metadata"),"createdAt":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at")})).collect())
    }
    async fn dispatch_tool(&self, id: Uuid, tool: &str, parameters: Value) -> PluginResult<Value> {
        let plugin = self.get(id).await?;
        if plugin.status != "ready" { return Err(PluginServiceError::InvalidState("plugin is not ready".into())); }
        let declared = crate::plugin_tool_dispatcher::declared_tool(&plugin.manifest, tool);
        if !declared { return Err(PluginServiceError::InvalidState(format!("tool '{}' is not declared by plugin", tool))); }
        let result = json!({"pluginId": id, "tool": tool, "parameters": parameters, "dispatched": true});
        sqlx::query("INSERT INTO plugin_logs(id,plugin_id,level,message,metadata) VALUES($1,$2,'info',$3,$4)")
            .bind(Uuid::new_v4()).bind(id).bind(format!("tool dispatched: {tool}")).bind(&result).execute(&self.pool).await?;
        Ok(result)
    }
    async fn dispatch_action(&self, id: Uuid, action: &str, payload: Value) -> PluginResult<Value> {
        let plugin = self.get(id).await?;
        if plugin.status != "ready" { return Err(PluginServiceError::InvalidState("plugin is not ready".into())); }
        let declared = crate::plugin_tool_dispatcher::declared_action(&plugin.manifest, action);
        if !declared { return Err(PluginServiceError::InvalidState(format!("action '{}' is not declared by plugin", action))); }
        let result = json!({"pluginId": id, "action": action, "payload": payload, "dispatched": true});
        sqlx::query("INSERT INTO plugin_logs(id,plugin_id,level,message,metadata) VALUES($1,$2,'info',$3,$4)")
            .bind(Uuid::new_v4()).bind(id).bind(format!("action dispatched: {action}")).bind(&result).execute(&self.pool).await?;
        Ok(result)
    }

    // ---- P1.2: Plugin 扩展面实现 ----

    async fn bridge_stream_supported(&self, plugin_id: Uuid) -> PluginResult<bool> {
        let plugin = self.get(plugin_id).await?;
        // 仅当 manifest 显式声明 bridge.stream 能力时才启用 SSE 流。
        let supported = plugin
            .manifest
            .get("bridge")
            .and_then(|b| b.as_object())
            .map(|m| m.contains_key("stream"))
            .unwrap_or(false);
        Ok(supported)
    }

    async fn ingest_webhook(
        &self,
        plugin_id: Uuid,
        endpoint_key: &str,
        _company_id: Uuid,
        _payload: Value,
    ) -> PluginResult<Value> {
        // 确认 plugin 存在（company scope 由路由层校验）
        self.get(plugin_id).await?;
        if endpoint_key.is_empty() {
            return Err(PluginServiceError::InvalidState(
                "endpoint key is required".into(),
            ));
        }
        // parrot 当前未实现 webhook runtime：显式返回 feature-disabled，不伪造成功。
        Err(PluginServiceError::FeatureDisabled(
            "plugin webhook ingress is not implemented in parrot".into(),
        ))
    }

    async fn list_local_folders(
        &self,
        plugin_id: Uuid,
        _company_id: Uuid,
    ) -> PluginResult<Vec<Value>> {
        let plugin = self.get(plugin_id).await?;
        let folders = plugin
            .config
            .get("localFolders")
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(folders)
    }

    async fn get_local_folder_status(
        &self,
        plugin_id: Uuid,
        company_id: Uuid,
        folder_key: &str,
    ) -> PluginResult<Value> {
        let folders = self.list_local_folders(plugin_id, company_id).await?;
        let folder = folders
            .iter()
            .find(|f| f.get("key").and_then(|k| k.as_str()) == Some(folder_key))
            .ok_or_else(|| {
                PluginServiceError::InvalidState(format!("local folder '{}' not found", folder_key))
            })?;
        let path = folder.get("path").and_then(|p| p.as_str()).unwrap_or("");
        let exists = !path.is_empty() && std::path::Path::new(path).exists();
        Ok(json!({
            "key": folder_key,
            "path": path,
            "exists": exists,
            "status": if exists { "available" } else { "missing" },
        }))
    }

    async fn validate_local_folder_path(&self, path: &str) -> PluginResult<()> {
        if !is_safe_relative_path(path) {
            return Err(PluginServiceError::InvalidState(format!(
                "local folder path '{}' is not a safe relative path (absolute paths, '..' traversal and null bytes are forbidden)",
                path
            )));
        }
        Ok(())
    }

    async fn update_local_folder(
        &self,
        plugin_id: Uuid,
        company_id: Uuid,
        folder_key: &str,
        body: Value,
    ) -> PluginResult<Value> {
        let plugin = self.get(plugin_id).await?;
        let mut config = plugin.config.clone();
        let folder_result = {
            let folders = config
                .get_mut("localFolders")
                .and_then(|f| f.as_array_mut())
                .ok_or_else(|| {
                    PluginServiceError::InvalidState("plugin has no localFolders".into())
                })?;
            let folder = folders
                .iter_mut()
                .find(|f| f.get("key").and_then(|k| k.as_str()) == Some(folder_key))
                .ok_or_else(|| {
                    PluginServiceError::InvalidState(format!(
                        "local folder '{}' not found",
                        folder_key
                    ))
                })?;

            // 若提供了 path，先做安全校验
            if let Some(p) = body.get("path").and_then(|v| v.as_str()) {
                self.validate_local_folder_path(p).await?;
                folder["path"] = json!(p);
            }
            if let Some(status) = body.get("status").and_then(|v| v.as_str()) {
                folder["status"] = json!(status);
            }
            folder["lastValidatedAt"] = json!(chrono::Utc::now());
            folder.clone()
        };

        sqlx::query("UPDATE plugins SET config=$2, updated_at=NOW() WHERE id=$1")
            .bind(plugin_id)
            .bind(&config)
            .execute(&self.pool)
            .await?;

        // company_id 仅用于 scope 语义（已用），避免未使用变量告警
        let _ = company_id;
        Ok(folder_result)
    }

    async fn serve_ui_asset(&self, plugin_id: Uuid, rel_path: &str) -> PluginResult<Vec<u8>> {
        let plugin = self.get(plugin_id).await?;
        let install_path = plugin
            .install_path
            .ok_or_else(|| {
                PluginServiceError::FeatureDisabled("plugin has no install path".into())
            })?;
        if !is_safe_relative_path(rel_path) {
            return Err(PluginServiceError::InvalidState(
                "ui asset path is not a safe relative path".into(),
            ));
        }
        let base = std::path::Path::new(&install_path).join("ui");
        let full = base.join(rel_path);
        // 二次校验：解析后仍需落在 base 内
        if !full.starts_with(&base) {
            return Err(PluginServiceError::InvalidState(
                "ui asset path escapes plugin ui directory".into(),
            ));
        }
        std::fs::read(&full).map_err(|_| PluginServiceError::NotFound(plugin_id))
    }

    async fn cancel_job_run(
        &self,
        plugin_id: Uuid,
        job_id: Uuid,
        run_id: Uuid,
    ) -> PluginResult<Value> {
        self.get(plugin_id).await?;
        let res = sqlx::query(
            "UPDATE plugin_job_runs SET status='cancelled', completed_at=NOW() \
             WHERE id=$1 AND plugin_id=$2 AND job_id=$3 \
               AND status NOT IN ('succeeded','failed','cancelled')",
        )
        .bind(run_id)
        .bind(plugin_id)
        .bind(job_id)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(PluginServiceError::InvalidState(
                "job run is not cancellable".into(),
            ));
        }
        Ok(json!({"id": run_id, "status": "cancelled"}))
    }

    async fn retry_job_run(
        &self,
        plugin_id: Uuid,
        job_id: Uuid,
        run_id: Uuid,
    ) -> PluginResult<Value> {
        self.get(plugin_id).await?;
        let res = sqlx::query(
            "UPDATE plugin_job_runs SET status='queued', completed_at=NULL, result=NULL \
             WHERE id=$1 AND plugin_id=$2 AND job_id=$3",
        )
        .bind(run_id)
        .bind(plugin_id)
        .bind(job_id)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(PluginServiceError::NotFound(run_id));
        }
        Ok(json!({"id": run_id, "status": "queued"}))
    }
}

/// 校验相对路径安全性：禁止空串、空字节、绝对路径与 `..` 穿越。
pub fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() || path.contains('\0') {
        return false;
    }
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return false;
    }
    for comp in p.components() {
        match comp {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return false,
            _ => {}
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::is_safe_relative_path;

    #[test]
    fn safe_relative_paths_accepted() {
        assert!(is_safe_relative_path("ui/index.js"));
        assert!(is_safe_relative_path("assets/style.css"));
        assert!(is_safe_relative_path("./local/data"));
        assert!(is_safe_relative_path("a/b/c"));
    }

    #[test]
    fn unsafe_paths_rejected() {
        // 绝对路径
        assert!(!is_safe_relative_path("/etc/passwd"));
        // 父目录穿越
        assert!(!is_safe_relative_path("../secrets"));
        assert!(!is_safe_relative_path("a/../../b"));
        assert!(!is_safe_relative_path("a/../b"));
        // 空串与空字节
        assert!(!is_safe_relative_path(""));
        assert!(!is_safe_relative_path("a\0b"));
    }
}
