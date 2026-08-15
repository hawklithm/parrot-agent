/// Execution Policy Bootstrap Service
/// 
/// 执行策略引导

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ExecutionPolicyBootstrapError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type ExecutionPolicyBootstrapResult<T> = Result<T, ExecutionPolicyBootstrapError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPolicy {
    pub id: Uuid,
    pub name: String,
    pub rules: serde_json::Value,
    pub priority: i32,
    pub active: bool,
}

pub struct ExecutionPolicyBootstrapService {
    pool: PgPool,
}

impl ExecutionPolicyBootstrapService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn bootstrap_default_policies(&self) -> ExecutionPolicyBootstrapResult<Vec<Uuid>> {
        let policies = vec![
            ("security_baseline", serde_json::json!({
                "deny_dangerous_commands": true,
                "require_approval_for_delete": true
            }), 100),
            ("resource_limits", serde_json::json!({
                "max_cpu_percent": 80,
                "max_memory_mb": 4096
            }), 90),
            ("audit_logging", serde_json::json!({
                "log_all_executions": true,
                "retention_days": 90
            }), 80),
        ];
        
        let mut policy_ids = Vec::new();
        
        for (name, rules, priority) in policies {
            let id = Uuid::new_v4();
            
            sqlx::query(
                r#"
                INSERT INTO execution_policies 
                (id, name, rules, priority, active, created_at)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (name) DO NOTHING
                "#
            )
            .bind(id)
            .bind(name)
            .bind(&rules)
            .bind(priority)
            .bind(true)
            .bind(chrono::Utc::now())
            .execute(&self.pool)
            .await?;
            
            policy_ids.push(id);
        }
        
        Ok(policy_ids)
    }
    
    pub async fn get_active_policies(&self) -> ExecutionPolicyBootstrapResult<Vec<ExecutionPolicy>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, rules, priority, active
            FROM execution_policies
            WHERE active = true
            ORDER BY priority DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        let policies = rows.into_iter().map(|row| {
            ExecutionPolicy {
                id: row.get("id"),
                name: row.get("name"),
                rules: row.get("rules"),
                priority: row.get("priority"),
                active: row.get("active"),
            }
        }).collect();
        
        Ok(policies)
    }
}
