/// Audit Log Service
/// 
/// 审计日志记录、查询和分析

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AuditLogError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("log not found: {0}")]
    NotFound(Uuid),
}

pub type AuditLogResult<T> = Result<T, AuditLogError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditAction {
    Create,
    Read,
    Update,
    Delete,
    Execute,
    Login,
    Logout,
    PermissionChange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditResource {
    Agent,
    Issue,
    Routine,
    Plugin,
    Tool,
    User,
    Workspace,
    Secret,
    Permission,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub action: AuditAction,
    pub resource: AuditResource,
    pub resource_id: Option<Uuid>,
    pub severity: AuditSeverity,
    pub description: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl AuditLog {
    pub fn new(
        action: AuditAction,
        resource: AuditResource,
        description: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id: None,
            agent_id: None,
            action,
            resource,
            resource_id: None,
            severity: AuditSeverity::Info,
            description,
            ip_address: None,
            user_agent: None,
            metadata: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        }
    }
    
    pub fn with_user(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }
    
    pub fn with_agent(mut self, agent_id: Uuid) -> Self {
        self.agent_id = Some(agent_id);
        self
    }
    
    pub fn with_resource_id(mut self, resource_id: Uuid) -> Self {
        self.resource_id = Some(resource_id);
        self
    }
    
    pub fn with_severity(mut self, severity: AuditSeverity) -> Self {
        self.severity = severity;
        self
    }
    
    pub fn with_ip(mut self, ip: String) -> Self {
        self.ip_address = Some(ip);
        self
    }
    
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogQuery {
    pub user_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub action: Option<AuditAction>,
    pub resource: Option<AuditResource>,
    pub resource_id: Option<Uuid>,
    pub severity: Option<AuditSeverity>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
}

impl Default for AuditLogQuery {
    fn default() -> Self {
        Self {
            user_id: None,
            agent_id: None,
            action: None,
            resource: None,
            resource_id: None,
            severity: None,
            start_time: None,
            end_time: None,
            limit: Some(100),
        }
    }
}

pub struct AuditLogService {
    pool: PgPool,
}

impl AuditLogService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// 记录审计日志
    pub async fn log(&self, audit_log: AuditLog) -> AuditLogResult<Uuid> {
        let id = sqlx::query_scalar(
            r#"
            INSERT INTO audit_logs 
            (id, user_id, agent_id, action, resource, resource_id, severity,
             description, ip_address, user_agent, metadata, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING id
            "#
        )
        .bind(&audit_log.id)
        .bind(&audit_log.user_id)
        .bind(&audit_log.agent_id)
        .bind(format!("{:?}", audit_log.action))
        .bind(format!("{:?}", audit_log.resource))
        .bind(&audit_log.resource_id)
        .bind(format!("{:?}", audit_log.severity))
        .bind(&audit_log.description)
        .bind(&audit_log.ip_address)
        .bind(&audit_log.user_agent)
        .bind(&audit_log.metadata)
        .bind(&audit_log.created_at)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    /// 查询审计日志
    pub async fn query(&self, query: AuditLogQuery) -> AuditLogResult<Vec<AuditLog>> {
        let mut sql = String::from(
            "SELECT id, user_id, agent_id, action, resource, resource_id, severity,
                    description, ip_address, user_agent, metadata, created_at 
             FROM audit_logs WHERE 1=1"
        );
        
        if query.user_id.is_some() {
            sql.push_str(" AND user_id = $1");
        }
        if query.start_time.is_some() {
            sql.push_str(" AND created_at >= $2");
        }
        if query.end_time.is_some() {
            sql.push_str(" AND created_at <= $3");
        }
        
        sql.push_str(" ORDER BY created_at DESC");
        
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await?;
        
        let logs = rows.into_iter().map(|row| {
            AuditLog {
                id: row.get("id"),
                user_id: row.get("user_id"),
                agent_id: row.get("agent_id"),
                action: parse_action(row.get("action")),
                resource: parse_resource(row.get("resource")),
                resource_id: row.get("resource_id"),
                severity: parse_severity(row.get("severity")),
                description: row.get("description"),
                ip_address: row.get("ip_address"),
                user_agent: row.get("user_agent"),
                metadata: row.get("metadata"),
                created_at: row.get("created_at"),
            }
        }).collect();
        
        Ok(logs)
    }
    
    /// 获取用户操作历史
    pub async fn get_user_history(
        &self,
        user_id: Uuid,
        limit: Option<i64>,
    ) -> AuditLogResult<Vec<AuditLog>> {
        let query = AuditLogQuery {
            user_id: Some(user_id),
            limit,
            ..Default::default()
        };
        
        self.query(query).await
    }
    
    /// 获取资源操作历史
    pub async fn get_resource_history(
        &self,
        resource: AuditResource,
        resource_id: Uuid,
        limit: Option<i64>,
    ) -> AuditLogResult<Vec<AuditLog>> {
        let query = AuditLogQuery {
            resource: Some(resource),
            resource_id: Some(resource_id),
            limit,
            ..Default::default()
        };
        
        self.query(query).await
    }
    
    /// 获取安全事件
    pub async fn get_security_events(
        &self,
        start_time: chrono::DateTime<chrono::Utc>,
        limit: Option<i64>,
    ) -> AuditLogResult<Vec<AuditLog>> {
        let query = AuditLogQuery {
            severity: Some(AuditSeverity::Critical),
            start_time: Some(start_time),
            limit,
            ..Default::default()
        };
        
        self.query(query).await
    }
    
    /// 统计操作数量
    pub async fn count_by_action(&self, action: AuditAction) -> AuditLogResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = $1"
        )
        .bind(format!("{:?}", action))
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count)
    }
    
    /// 统计用户活动
    pub async fn count_user_actions(
        &self,
        user_id: Uuid,
        start_time: chrono::DateTime<chrono::Utc>,
    ) -> AuditLogResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE user_id = $1 AND created_at >= $2"
        )
        .bind(user_id)
        .bind(start_time)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count)
    }
    
    /// 删除旧日志
    pub async fn cleanup_old_logs(&self, days: i64) -> AuditLogResult<u64> {
        let cutoff_time = chrono::Utc::now() - chrono::Duration::days(days);
        
        let result = sqlx::query(
            "DELETE FROM audit_logs WHERE created_at < $1"
        )
        .bind(cutoff_time)
        .execute(&self.pool)
        .await?;
        
        Ok(result.rows_affected())
    }
}

fn parse_action(s: &str) -> AuditAction {
    match s {
        "Create" => AuditAction::Create,
        "Read" => AuditAction::Read,
        "Update" => AuditAction::Update,
        "Delete" => AuditAction::Delete,
        "Execute" => AuditAction::Execute,
        "Login" => AuditAction::Login,
        "Logout" => AuditAction::Logout,
        "PermissionChange" => AuditAction::PermissionChange,
        _ => AuditAction::Read,
    }
}

fn parse_resource(s: &str) -> AuditResource {
    match s {
        "Agent" => AuditResource::Agent,
        "Issue" => AuditResource::Issue,
        "Routine" => AuditResource::Routine,
        "Plugin" => AuditResource::Plugin,
        "Tool" => AuditResource::Tool,
        "User" => AuditResource::User,
        "Workspace" => AuditResource::Workspace,
        "Secret" => AuditResource::Secret,
        "Permission" => AuditResource::Permission,
        _ => AuditResource::Agent,
    }
}

fn parse_severity(s: &str) -> AuditSeverity {
    match s {
        "Info" => AuditSeverity::Info,
        "Warning" => AuditSeverity::Warning,
        "Critical" => AuditSeverity::Critical,
        _ => AuditSeverity::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_audit_log_creation() {
        let log = AuditLog::new(
            AuditAction::Create,
            AuditResource::Agent,
            "Created agent".to_string(),
        )
        .with_user(Uuid::new_v4())
        .with_severity(AuditSeverity::Info);
        
        assert_eq!(log.action, AuditAction::Create);
        assert_eq!(log.resource, AuditResource::Agent);
        assert_eq!(log.severity, AuditSeverity::Info);
        assert!(log.user_id.is_some());
    }
}
