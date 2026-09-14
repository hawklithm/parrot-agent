use async_trait::async_trait;
use models::Plugin;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::path::{Path, PathBuf};
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
    async fn get_company_config(
        &self,
        plugin_id: Uuid,
        company_id: Uuid,
    ) -> PluginResult<Option<Value>>;
    async fn update_company_config(
        &self,
        plugin_id: Uuid,
        company_id: Uuid,
        config: Value,
    ) -> PluginResult<Value>;
    async fn list_state(
        &self,
        plugin_id: Uuid,
        scope_kind: Option<&str>,
        scope_id: Option<&str>,
        namespace: Option<&str>,
    ) -> PluginResult<Vec<Value>>;
    async fn upsert_state(
        &self,
        plugin_id: Uuid,
        scope_kind: &str,
        scope_id: Option<&str>,
        namespace: &str,
        state_key: &str,
        value: Value,
    ) -> PluginResult<Value>;
    async fn delete_state(
        &self,
        plugin_id: Uuid,
        scope_kind: &str,
        scope_id: Option<&str>,
        namespace: &str,
        state_key: &str,
    ) -> PluginResult<()>;
    async fn list_entities(
        &self,
        plugin_id: Uuid,
        company_id: Option<Uuid>,
        entity_type: Option<&str>,
    ) -> PluginResult<Vec<Value>>;
    async fn upsert_entity(
        &self,
        plugin_id: Uuid,
        company_id: Option<Uuid>,
        entity_type: &str,
        scope_kind: &str,
        scope_id: Option<&str>,
        external_id: Option<&str>,
        title: Option<&str>,
        status: Option<&str>,
        data: Value,
    ) -> PluginResult<Value>;
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
        company_id: Option<Uuid>,
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
    async fn get_company_config(
        &self,
        plugin_id: Uuid,
        company_id: Uuid,
    ) -> PluginResult<Option<Value>> {
        self.get(plugin_id).await?;
        let row = sqlx::query(
            "SELECT id, plugin_id, company_id, config_json, last_error, created_at, updated_at
             FROM plugin_config WHERE plugin_id = $1 AND company_id = $2",
        )
        .bind(plugin_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"),
                "pluginId": row.get::<Uuid, _>("plugin_id"),
                "companyId": row.get::<Uuid, _>("company_id"),
                "configJson": row.get::<Value, _>("config_json"),
                "lastError": row.get::<Option<String>, _>("last_error"),
                "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            })
        }))
    }
    async fn update_company_config(
        &self,
        plugin_id: Uuid,
        company_id: Uuid,
        config: Value,
    ) -> PluginResult<Value> {
        let plugin = self.get(plugin_id).await?;
        crate::plugin_config_validator::validate_config(&plugin.manifest, &config)
            .map_err(PluginServiceError::InvalidState)?;
        let row = sqlx::query(
            "INSERT INTO plugin_config
                (plugin_id, company_id, config_json, last_error)
             VALUES ($1,$2,$3,NULL)
             ON CONFLICT (plugin_id, company_id)
             DO UPDATE SET config_json = EXCLUDED.config_json,
                           last_error = NULL,
                           updated_at = NOW()
             RETURNING id, plugin_id, company_id, config_json, last_error, created_at, updated_at",
        )
        .bind(plugin_id)
        .bind(company_id)
        .bind(config)
        .fetch_one(&self.pool)
        .await?;
        Ok(json!({
            "id": row.get::<Uuid, _>("id"),
            "pluginId": row.get::<Uuid, _>("plugin_id"),
            "companyId": row.get::<Uuid, _>("company_id"),
            "configJson": row.get::<Value, _>("config_json"),
            "lastError": row.get::<Option<String>, _>("last_error"),
            "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        }))
    }
    async fn list_state(
        &self,
        plugin_id: Uuid,
        scope_kind: Option<&str>,
        scope_id: Option<&str>,
        namespace: Option<&str>,
    ) -> PluginResult<Vec<Value>> {
        self.get(plugin_id).await?;
        let rows = sqlx::query(
            "SELECT id, plugin_id, scope_kind, scope_id, namespace, state_key,
                    value_json, updated_at
             FROM plugin_state
             WHERE plugin_id = $1
               AND ($2::text IS NULL OR scope_kind = $2)
               AND ($3::text IS NULL OR scope_id = $3)
               AND ($4::text IS NULL OR namespace = $4)
             ORDER BY updated_at DESC, state_key ASC",
        )
        .bind(plugin_id)
        .bind(scope_kind)
        .bind(scope_id)
        .bind(namespace)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "pluginId": row.get::<Uuid, _>("plugin_id"),
                    "scopeKind": row.get::<String, _>("scope_kind"),
                    "scopeId": row.get::<Option<String>, _>("scope_id"),
                    "namespace": row.get::<String, _>("namespace"),
                    "key": row.get::<String, _>("state_key"),
                    "value": row.get::<Value, _>("value_json"),
                    "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            })
            .collect())
    }
    async fn upsert_state(
        &self,
        plugin_id: Uuid,
        scope_kind: &str,
        scope_id: Option<&str>,
        namespace: &str,
        state_key: &str,
        value: Value,
    ) -> PluginResult<Value> {
        self.get(plugin_id).await?;
        let row = sqlx::query(
            "INSERT INTO plugin_state
                (plugin_id, scope_kind, scope_id, namespace, state_key, value_json)
             VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT (plugin_id, scope_kind, scope_id, namespace, state_key)
             DO UPDATE SET value_json = EXCLUDED.value_json, updated_at = NOW()
             RETURNING id, updated_at",
        )
        .bind(plugin_id)
        .bind(scope_kind)
        .bind(scope_id)
        .bind(namespace)
        .bind(state_key)
        .bind(&value)
        .fetch_one(&self.pool)
        .await?;
        Ok(json!({
            "id": row.get::<Uuid, _>("id"),
            "pluginId": plugin_id,
            "scopeKind": scope_kind,
            "scopeId": scope_id,
            "namespace": namespace,
            "key": state_key,
            "value": value,
            "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        }))
    }
    async fn delete_state(
        &self,
        plugin_id: Uuid,
        scope_kind: &str,
        scope_id: Option<&str>,
        namespace: &str,
        state_key: &str,
    ) -> PluginResult<()> {
        self.get(plugin_id).await?;
        sqlx::query(
            "DELETE FROM plugin_state
             WHERE plugin_id = $1 AND scope_kind = $2
               AND scope_id IS NOT DISTINCT FROM $3
               AND namespace = $4 AND state_key = $5",
        )
        .bind(plugin_id)
        .bind(scope_kind)
        .bind(scope_id)
        .bind(namespace)
        .bind(state_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
    async fn list_entities(
        &self,
        plugin_id: Uuid,
        company_id: Option<Uuid>,
        entity_type: Option<&str>,
    ) -> PluginResult<Vec<Value>> {
        self.get(plugin_id).await?;
        let rows = sqlx::query(
            "SELECT id, plugin_id, company_id, entity_type, scope_kind, scope_id,
                    external_id, title, status, data, created_at, updated_at
             FROM plugin_entities
             WHERE plugin_id = $1
               AND company_id IS NOT DISTINCT FROM $2
               AND ($3::text IS NULL OR entity_type = $3)
             ORDER BY updated_at DESC, created_at DESC",
        )
        .bind(plugin_id)
        .bind(company_id)
        .bind(entity_type)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "pluginId": row.get::<Uuid, _>("plugin_id"),
                    "companyId": row.get::<Option<Uuid>, _>("company_id"),
                    "entityType": row.get::<String, _>("entity_type"),
                    "scopeKind": row.get::<String, _>("scope_kind"),
                    "scopeId": row.get::<Option<String>, _>("scope_id"),
                    "externalId": row.get::<Option<String>, _>("external_id"),
                    "title": row.get::<Option<String>, _>("title"),
                    "status": row.get::<Option<String>, _>("status"),
                    "data": row.get::<Value, _>("data"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            })
            .collect())
    }
    async fn upsert_entity(
        &self,
        plugin_id: Uuid,
        company_id: Option<Uuid>,
        entity_type: &str,
        scope_kind: &str,
        scope_id: Option<&str>,
        external_id: Option<&str>,
        title: Option<&str>,
        status: Option<&str>,
        data: Value,
    ) -> PluginResult<Value> {
        self.get(plugin_id).await?;
        let row = sqlx::query(
            "INSERT INTO plugin_entities
                (plugin_id, company_id, entity_type, scope_kind, scope_id,
                 external_id, title, status, data)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (company_id, plugin_id, entity_type, external_id)
             DO UPDATE SET scope_kind = EXCLUDED.scope_kind,
                           scope_id = EXCLUDED.scope_id,
                           title = EXCLUDED.title,
                           status = EXCLUDED.status,
                           data = EXCLUDED.data,
                           updated_at = NOW()
             RETURNING id, created_at, updated_at",
        )
        .bind(plugin_id)
        .bind(company_id)
        .bind(entity_type)
        .bind(scope_kind)
        .bind(scope_id)
        .bind(external_id)
        .bind(title)
        .bind(status)
        .bind(&data)
        .fetch_one(&self.pool)
        .await?;
        Ok(json!({
            "id": row.get::<Uuid, _>("id"),
            "pluginId": plugin_id,
            "companyId": company_id,
            "entityType": entity_type,
            "scopeKind": scope_kind,
            "scopeId": scope_id,
            "externalId": external_id,
            "title": title,
            "status": status,
            "data": data,
            "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        }))
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
        company_id: Option<Uuid>,
        payload: Value,
    ) -> PluginResult<Value> {
        let plugin = self.get(plugin_id).await?;
        if endpoint_key.is_empty() {
            return Err(PluginServiceError::InvalidState(
                "endpoint key is required".into(),
            ));
        }
        if plugin.status != "ready" {
            return Err(PluginServiceError::InvalidState(format!(
                "plugin is not ready (current status: {})",
                plugin.status
            )));
        }
        let receives_webhooks = plugin
            .manifest
            .get("capabilities")
            .and_then(Value::as_array)
            .is_some_and(|capabilities| {
                capabilities
                    .iter()
                    .any(|capability| capability.as_str() == Some("webhooks.receive"))
            });
        if !receives_webhooks {
            return Err(PluginServiceError::InvalidState(
                "plugin does not have the webhooks.receive capability".into(),
            ));
        }
        let declared = plugin
            .manifest
            .get("webhooks")
            .and_then(Value::as_array)
            .is_some_and(|webhooks| {
                webhooks.iter().any(|webhook| {
                    webhook
                        .get("endpointKey")
                        .or_else(|| webhook.get("endpoint_key"))
                        .and_then(Value::as_str)
                        == Some(endpoint_key)
                })
            });
        if !declared {
            return Err(PluginServiceError::InvalidState(format!(
                "webhook endpoint '{}' is not declared by this plugin",
                endpoint_key
            )));
        }

        // Keep an immutable ingress record even when this installation does
        // not have a plugin worker manager. This matches Paperclip's delivery
        // ledger and makes the missing runtime explicit rather than silently
        // dropping an external request.
        let started_at = chrono::Utc::now();
        let delivery_id: Uuid = sqlx::query_scalar(
            "INSERT INTO plugin_webhook_deliveries
                (plugin_id, company_id, webhook_key, status, payload, headers, started_at)
             VALUES ($1, $2, $3, 'pending', $4, '{}'::jsonb, $5)
             RETURNING id",
        )
        .bind(plugin_id)
        .bind(company_id)
        .bind(endpoint_key)
        .bind(&payload)
        .bind(started_at)
        .fetch_one(&self.pool)
        .await?;
        let finished_at = chrono::Utc::now();
        let duration_ms = (finished_at - started_at).num_milliseconds().max(0) as i32;
        let runtime_error =
            "plugin webhook runtime is not configured in parrot; delivery was recorded";
        sqlx::query(
            "UPDATE plugin_webhook_deliveries
             SET status = 'failed', duration_ms = $2, error = $3, finished_at = $4
             WHERE id = $1",
        )
        .bind(delivery_id)
        .bind(duration_ms)
        .bind(runtime_error)
        .bind(finished_at)
        .execute(&self.pool)
        .await?;

        Err(PluginServiceError::FeatureDisabled(
            format!("{runtime_error} (deliveryId: {delivery_id})"),
        ))
    }

    async fn list_local_folders(
        &self,
        plugin_id: Uuid,
        company_id: Uuid,
    ) -> PluginResult<Vec<Value>> {
        let plugin = self.get(plugin_id).await?;
        let company_config = self
            .get_company_config(plugin_id, company_id)
            .await?
            .and_then(|record| record.get("configJson").cloned());
        let config = company_config.unwrap_or_else(|| plugin.config.clone());
        let folders = config
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
        let mut config = self
            .get_company_config(plugin_id, company_id)
            .await?
            .and_then(|record| record.get("configJson").cloned())
            .unwrap_or_else(|| plugin.config.clone());
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

        self.update_company_config(plugin_id, company_id, config).await?;
        Ok(folder_result)
    }

    async fn serve_ui_asset(&self, plugin_id: Uuid, rel_path: &str) -> PluginResult<Vec<u8>> {
        let plugin = self.get(plugin_id).await?;
        if plugin.status != "ready" {
            return Err(PluginServiceError::InvalidState(
                "plugin UI is only available for ready plugins".into(),
            ));
        }
        let install_path = plugin
            .install_path
            .ok_or_else(|| {
                PluginServiceError::FeatureDisabled("plugin has no install path".into())
            })?;
        let entrypoint = plugin_ui_entrypoint(&plugin.manifest).ok_or_else(|| {
            PluginServiceError::FeatureDisabled("plugin does not declare a UI bundle".into())
        })?;
        let full = resolve_plugin_ui_asset_path(Path::new(&install_path), entrypoint, rel_path)
            .map_err(|error| match error {
                PluginUiAssetPathError::InvalidEntrypoint => PluginServiceError::InvalidState(
                    "plugin UI entrypoint is not a safe relative path".into(),
                ),
                PluginUiAssetPathError::InvalidRelativePath => PluginServiceError::InvalidState(
                    "ui asset path is not a safe relative path".into(),
                ),
                PluginUiAssetPathError::EscapesRoot => PluginServiceError::InvalidState(
                    "ui asset path escapes plugin UI directory".into(),
                ),
                PluginUiAssetPathError::Missing | PluginUiAssetPathError::NotFile => {
                    PluginServiceError::NotFound(plugin_id)
                }
            })?;
        std::fs::read(full).map_err(|_| PluginServiceError::NotFound(plugin_id))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PluginUiAssetPathError {
    InvalidEntrypoint,
    InvalidRelativePath,
    Missing,
    EscapesRoot,
    NotFile,
}

/// Resolve the declared UI bundle and requested asset after resolving symlinks.
/// The containment check must operate on canonical paths, not only on the
/// lexical path, because a file inside the bundle can point outside it.
fn resolve_plugin_ui_asset_path(
    install_path: &Path,
    entrypoint: &str,
    rel_path: &str,
) -> Result<PathBuf, PluginUiAssetPathError> {
    if !is_safe_relative_path(entrypoint) {
        return Err(PluginUiAssetPathError::InvalidEntrypoint);
    }
    if !is_safe_relative_path(rel_path) {
        return Err(PluginUiAssetPathError::InvalidRelativePath);
    }

    let ui_root = std::fs::canonicalize(install_path.join(entrypoint))
        .map_err(|_| PluginUiAssetPathError::Missing)?;
    if !ui_root.is_dir() {
        return Err(PluginUiAssetPathError::Missing);
    }

    let resolved_file = std::fs::canonicalize(ui_root.join(rel_path))
        .map_err(|_| PluginUiAssetPathError::Missing)?;
    let relative = resolved_file
        .strip_prefix(&ui_root)
        .map_err(|_| PluginUiAssetPathError::EscapesRoot)?;
    if relative.as_os_str().is_empty() {
        return Err(PluginUiAssetPathError::NotFile);
    }
    if !resolved_file.is_file() {
        return Err(PluginUiAssetPathError::NotFile);
    }
    Ok(resolved_file)
}

/// Return the manifest-declared UI bundle path. `entrypoints.ui` is the
/// Paperclip contract; a string-valued `ui` is retained for old Parrot
/// manifests, while contribution arrays intentionally do not qualify.
fn plugin_ui_entrypoint(manifest: &Value) -> Option<&str> {
    manifest
        .get("entrypoints")
        .and_then(Value::as_object)
        .and_then(|entrypoints| entrypoints.get("ui"))
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .or_else(|| {
            manifest
                .get("ui")
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
        })
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
    use super::{
        is_safe_relative_path, plugin_ui_entrypoint, resolve_plugin_ui_asset_path,
        PluginUiAssetPathError,
    };
    use serde_json::json;
    use tempfile::tempdir;

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

    #[test]
    fn paperclip_ui_entrypoint_wins_over_legacy_string() {
        let manifest = json!({
            "ui": "legacy-ui",
            "entrypoints": { "ui": "./dist/ui" }
        });
        assert_eq!(plugin_ui_entrypoint(&manifest), Some("./dist/ui"));
    }

    #[test]
    fn contribution_array_does_not_declare_a_bundle() {
        let manifest = json!({ "ui": [{ "slot": "dashboard" }] });
        assert_eq!(plugin_ui_entrypoint(&manifest), None);
    }

    #[test]
    fn canonical_asset_resolution_rejects_lexical_and_symlink_escape() {
        let root = tempdir().unwrap();
        let ui_root = root.path().join("dist").join("ui");
        std::fs::create_dir_all(&ui_root).unwrap();
        std::fs::write(ui_root.join("index.js"), b"bundle").unwrap();

        let resolved = resolve_plugin_ui_asset_path(root.path(), "./dist/ui", "index.js")
            .expect("declared asset should resolve");
        assert_eq!(std::fs::read(resolved).unwrap(), b"bundle");
        assert_eq!(
            resolve_plugin_ui_asset_path(root.path(), "./dist/ui", "../secret"),
            Err(PluginUiAssetPathError::InvalidRelativePath)
        );

        let outside = root.path().join("outside.txt");
        std::fs::write(&outside, b"private").unwrap();
        let link = ui_root.join("outside.txt");
        let link_result = {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&outside, &link)
            }
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_file(&outside, &link)
            }
        };
        if link_result.is_ok() {
            assert_eq!(
                resolve_plugin_ui_asset_path(root.path(), "./dist/ui", "outside.txt"),
                Err(PluginUiAssetPathError::EscapesRoot)
            );
        }
    }
}
