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
}
