/// Execution Allowlist Service
/// 
/// 执行白名单管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ExecutionAllowlistError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("not allowed: {0}")]
    NotAllowed(String),
}

pub type ExecutionAllowlistResult<T> = Result<T, ExecutionAllowlistError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowlistEntry {
    pub id: Uuid,
    pub resource_type: ResourceType,
    pub resource_id: String,
    pub allowed_actions: Vec<String>,
    pub scope: AllowlistScope,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResourceType {
    Agent,
    Tool,
    Plugin,
    Workspace,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AllowlistScope {
    Global,
    Company(Uuid),
    User(Uuid),
}

pub struct ExecutionAllowlistService {
    pool: PgPool,
}

impl ExecutionAllowlistService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn add_entry(
        &self,
        resource_type: ResourceType,
        resource_id: String,
        allowed_actions: Vec<String>,
        scope: AllowlistScope,
        created_by: Uuid,
    ) -> ExecutionAllowlistResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO execution_allowlist 
            (id, resource_type, resource_id, allowed_actions, scope, created_at, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(format!("{:?}", resource_type))
        .bind(&resource_id)
        .bind(serde_json::to_value(&allowed_actions).unwrap())
        .bind(format!("{:?}", scope))
        .bind(chrono::Utc::now())
        .bind(created_by)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    
    pub async fn check_allowed(
        &self,
        resource_type: &ResourceType,
        resource_id: &str,
        action: &str,
        scope: &AllowlistScope,
    ) -> ExecutionAllowlistResult<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM execution_allowlist
            WHERE resource_type = $1
              AND resource_id = $2
              AND scope = $3
              AND allowed_actions @> $4
            "#
        )
        .bind(format!("{:?}", resource_type))
        .bind(resource_id)
        .bind(format!("{:?}", scope))
        .bind(serde_json::json!([action]))
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count > 0)
    }
    
    pub async fn check_or_deny(
        &self,
        resource_type: ResourceType,
        resource_id: &str,
        action: &str,
        scope: AllowlistScope,
    ) -> ExecutionAllowlistResult<()> {
        if !self.check_allowed(&resource_type, resource_id, action, &scope).await? {
            return Err(ExecutionAllowlistError::NotAllowed(
                format!("{:?} {} action {} is not allowed", resource_type, resource_id, action)
            ));
        }
        Ok(())
    }
    
    pub async fn list_entries(
        &self,
        resource_type: Option<ResourceType>,
        scope: Option<AllowlistScope>,
    ) -> ExecutionAllowlistResult<Vec<AllowlistEntry>> {
        let mut query = "SELECT id, resource_type, resource_id, allowed_actions, scope, created_at, created_by FROM execution_allowlist WHERE 1=1".to_string();
        
        if resource_type.is_some() {
            query.push_str(" AND resource_type = $1");
        }
        
        if scope.is_some() {
            query.push_str(" AND scope = $2");
        }
        
        query.push_str(" ORDER BY created_at DESC");
        
        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await?;
        
        let entries = rows.into_iter().map(|row| {
            AllowlistEntry {
                id: row.get("id"),
                resource_type: parse_resource_type(row.get("resource_type")),
                resource_id: row.get("resource_id"),
                allowed_actions: serde_json::from_value(row.get("allowed_actions")).unwrap_or_default(),
                scope: parse_scope(row.get("scope")),
                created_at: row.get("created_at"),
                created_by: row.get("created_by"),
            }
        }).collect();
        
        Ok(entries)
    }
    
    pub async fn remove_entry(&self, id: Uuid) -> ExecutionAllowlistResult<()> {
        sqlx::query("DELETE FROM execution_allowlist WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}

fn parse_resource_type(s: &str) -> ResourceType {
    match s {
        "Agent" => ResourceType::Agent,
        "Tool" => ResourceType::Tool,
        "Plugin" => ResourceType::Plugin,
        "Workspace" => ResourceType::Workspace,
        "File" => ResourceType::File,
        _ => ResourceType::Tool,
    }
}

fn parse_scope(s: &str) -> AllowlistScope {
    if s == "Global" {
        AllowlistScope::Global
    } else if s.starts_with("Company") {
        AllowlistScope::Company(Uuid::new_v4())
    } else {
        AllowlistScope::User(Uuid::new_v4())
    }
}
