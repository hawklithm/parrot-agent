/// 工具访问控制
/// 
/// 实现工具级别的权限控制和访问策略

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AccessControlError {
    #[error("access denied: {0}")]
    AccessDenied(String),
    
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    
    #[error("invalid permission: {0}")]
    InvalidPermission(String),
}

pub type AccessControlResult<T> = Result<T, AccessControlError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Permission {
    Read,
    Write,
    Execute,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermission {
    pub tool_name: String,
    pub agent_id: Uuid,
    pub permissions: Vec<Permission>,
    pub granted_at: chrono::DateTime<chrono::Utc>,
}

/// 工具访问控制服务
pub struct ToolAccessControlService {
    permissions: Arc<RwLock<HashMap<(String, Uuid), Vec<Permission>>>>,
}

impl ToolAccessControlService {
    pub fn new() -> Self {
        Self {
            permissions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 授予权限
    pub async fn grant_permission(
        &self,
        tool_name: String,
        agent_id: Uuid,
        permission: Permission,
    ) -> AccessControlResult<()> {
        let mut perms = self.permissions.write().await;
        perms.entry((tool_name, agent_id))
            .or_insert_with(Vec::new)
            .push(permission);
        Ok(())
    }
    
    /// 撤销权限
    pub async fn revoke_permission(
        &self,
        tool_name: &str,
        agent_id: Uuid,
        permission: &Permission,
    ) -> AccessControlResult<()> {
        let mut perms = self.permissions.write().await;
        if let Some(agent_perms) = perms.get_mut(&(tool_name.to_string(), agent_id)) {
            agent_perms.retain(|p| p != permission);
        }
        Ok(())
    }
    
    /// 检查权限
    pub async fn check_permission(
        &self,
        tool_name: &str,
        agent_id: Uuid,
        required_permission: &Permission,
    ) -> bool {
        let perms = self.permissions.read().await;
        
        if let Some(agent_perms) = perms.get(&(tool_name.to_string(), agent_id)) {
            agent_perms.contains(required_permission) || agent_perms.contains(&Permission::Admin)
        } else {
            false
        }
    }
    
    /// 列出 Agent 的所有工具权限
    pub async fn list_agent_permissions(&self, agent_id: Uuid) -> Vec<ToolPermission> {
        let perms = self.permissions.read().await;
        
        perms.iter()
            .filter(|((_, aid), _)| *aid == agent_id)
            .map(|((tool_name, agent_id), permissions)| ToolPermission {
                tool_name: tool_name.clone(),
                agent_id: *agent_id,
                permissions: permissions.clone(),
                granted_at: chrono::Utc::now(),
            })
            .collect()
    }
    
    /// 清除 Agent 的所有权限
    pub async fn revoke_all_permissions(&self, agent_id: Uuid) -> AccessControlResult<()> {
        let mut perms = self.permissions.write().await;
        perms.retain(|(_, aid), _| *aid != agent_id);
        Ok(())
    }
}

impl Default for ToolAccessControlService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_grant_and_check_permission() {
        let service = ToolAccessControlService::new();
        let agent_id = Uuid::new_v4();
        
        service.grant_permission("test_tool".to_string(), agent_id, Permission::Execute).await.unwrap();
        
        assert!(service.check_permission("test_tool", agent_id, &Permission::Execute).await);
        assert!(!service.check_permission("test_tool", agent_id, &Permission::Write).await);
    }
    
    #[tokio::test]
    async fn test_revoke_permission() {
        let service = ToolAccessControlService::new();
        let agent_id = Uuid::new_v4();
        
        service.grant_permission("test_tool".to_string(), agent_id, Permission::Execute).await.unwrap();
        service.revoke_permission("test_tool", agent_id, &Permission::Execute).await.unwrap();
        
        assert!(!service.check_permission("test_tool", agent_id, &Permission::Execute).await);
    }
}
