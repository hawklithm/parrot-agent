/// Agent Secret Bindings Service
/// 
/// Agent密钥绑定管理服务

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SecretBindingError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("binding not found: {0}")]
    NotFound(Uuid),
    
    #[error("access denied: {0}")]
    AccessDenied(String),
}

pub type SecretBindingResult<T> = Result<T, SecretBindingError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecretScope {
    Agent(Uuid),
    Workspace(Uuid),
    Global,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretBinding {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub secret_name: String,
    pub secret_value: String, // 实际应该加密存储
    pub scope: SecretScope,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SecretBinding {
    pub fn new(agent_id: Uuid, secret_name: String, secret_value: String, scope: SecretScope) -> Self {
        Self {
            id: Uuid::new_v4(),
            agent_id,
            secret_name,
            secret_value,
            scope,
            created_at: chrono::Utc::now(),
            expires_at: None,
        }
    }
    
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            chrono::Utc::now() > expires_at
        } else {
            false
        }
    }
}

pub struct AgentSecretBindingsService {
    pool: PgPool,
}

impl AgentSecretBindingsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// 创建密钥绑定
    pub async fn create_binding(&self, binding: SecretBinding) -> SecretBindingResult<Uuid> {
        let id = sqlx::query_scalar(
            r#"
            INSERT INTO agent_secret_bindings 
            (id, agent_id, secret_name, secret_value, scope, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#
        )
        .bind(&binding.id)
        .bind(&binding.agent_id)
        .bind(&binding.secret_name)
        .bind(&binding.secret_value)
        .bind(serde_json::to_value(&binding.scope).unwrap())
        .bind(&binding.created_at)
        .bind(&binding.expires_at)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    /// 删除密钥绑定
    pub async fn delete_binding(&self, binding_id: Uuid) -> SecretBindingResult<()> {
        sqlx::query("DELETE FROM agent_secret_bindings WHERE id = $1")
            .bind(binding_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    /// 获取agent的密钥绑定
    pub async fn get_agent_bindings(&self, agent_id: Uuid) -> SecretBindingResult<Vec<SecretBinding>> {
        let rows = sqlx::query(
            r#"
            SELECT id, agent_id, secret_name, secret_value, scope, created_at, expires_at
            FROM agent_secret_bindings
            WHERE agent_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        
        let bindings = rows.into_iter().map(|row| {
            SecretBinding {
                id: row.get("id"),
                agent_id: row.get("agent_id"),
                secret_name: row.get("secret_name"),
                secret_value: row.get("secret_value"),
                scope: serde_json::from_value(row.get("scope")).unwrap(),
                created_at: row.get("created_at"),
                expires_at: row.get("expires_at"),
            }
        }).collect();
        
        Ok(bindings)
    }
    
    /// 检查访问权限
    pub fn check_access(&self, binding: &SecretBinding, requester_id: Uuid, workspace_id: Option<Uuid>) -> SecretBindingResult<()> {
        // 检查过期
        if binding.is_expired() {
            return Err(SecretBindingError::AccessDenied("secret expired".to_string()));
        }
        
        // 检查作用域
        match &binding.scope {
            SecretScope::Agent(agent_id) => {
                if requester_id != *agent_id {
                    return Err(SecretBindingError::AccessDenied("secret is agent-scoped".to_string()));
                }
            }
            SecretScope::Workspace(ws_id) => {
                if workspace_id != Some(*ws_id) {
                    return Err(SecretBindingError::AccessDenied("secret is workspace-scoped".to_string()));
                }
            }
            SecretScope::Global => {
                // 全局作用域，任何人都可以访问
            }
        }
        
        Ok(())
    }
    
    /// 获取密钥值（带权限检查）
    pub async fn get_secret_value(
        &self,
        agent_id: Uuid,
        secret_name: &str,
        workspace_id: Option<Uuid>,
    ) -> SecretBindingResult<String> {
        let bindings = self.get_agent_bindings(agent_id).await?;
        
        for binding in bindings {
            if binding.secret_name == secret_name {
                self.check_access(&binding, agent_id, workspace_id)?;
                return Ok(binding.secret_value);
            }
        }
        
        Err(SecretBindingError::NotFound(agent_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_secret_expiration() {
        let binding = SecretBinding {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            secret_name: "test".to_string(),
            secret_value: "value".to_string(),
            scope: SecretScope::Global,
            created_at: chrono::Utc::now(),
            expires_at: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
        };
        
        assert!(binding.is_expired());
    }
    
    #[test]
    fn test_scope_check() {
        let service = AgentSecretBindingsService::new(PgPool::connect("").await.unwrap());
        let agent_id = Uuid::new_v4();
        let other_agent = Uuid::new_v4();
        
        let binding = SecretBinding {
            id: Uuid::new_v4(),
            agent_id,
            secret_name: "test".to_string(),
            secret_value: "value".to_string(),
            scope: SecretScope::Agent(agent_id),
            created_at: chrono::Utc::now(),
            expires_at: None,
        };
        
        // 自己可以访问
        assert!(service.check_access(&binding, agent_id, None).is_ok());
        
        // 其他agent不能访问
        assert!(service.check_access(&binding, other_agent, None).is_err());
    }
}
