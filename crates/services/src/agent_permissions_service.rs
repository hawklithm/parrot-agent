/// Agent Permissions Service
/// 
/// Agent权限管理服务，实现细粒度权限控制

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("invalid permission: {0}")]
    InvalidPermission(String),
    
    #[error("agent not found: {0}")]
    AgentNotFound(Uuid),
}

pub type PermissionResult<T> = Result<T, PermissionError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Permission {
    // 工具权限
    ToolExecute(String),
    ToolDiscover,
    
    // 资源权限
    ResourceRead(String),
    ResourceWrite(String),
    ResourceDelete(String),
    
    // Agent权限
    AgentCreate,
    AgentUpdate,
    AgentDelete,
    AgentInvoke(Uuid),
    
    // Workspace权限
    WorkspaceAccess(Uuid),
    WorkspaceModify(Uuid),
    
    // 管理权限
    AdminFullAccess,
}

impl Permission {
    pub fn resource_type(&self) -> &str {
        match self {
            Permission::ToolExecute(_) | Permission::ToolDiscover => "tool",
            Permission::ResourceRead(_) | Permission::ResourceWrite(_) | Permission::ResourceDelete(_) => "resource",
            Permission::AgentCreate | Permission::AgentUpdate | Permission::AgentDelete | Permission::AgentInvoke(_) => "agent",
            Permission::WorkspaceAccess(_) | Permission::WorkspaceModify(_) => "workspace",
            Permission::AdminFullAccess => "admin",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGroup {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub permissions: HashSet<Permission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPermissions {
    pub agent_id: Uuid,
    pub direct_permissions: HashSet<Permission>,
    pub inherited_groups: Vec<Uuid>,
}

impl AgentPermissions {
    pub fn new(agent_id: Uuid) -> Self {
        Self {
            agent_id,
            direct_permissions: HashSet::new(),
            inherited_groups: Vec::new(),
        }
    }
    
    pub fn grant_permission(&mut self, permission: Permission) {
        self.direct_permissions.insert(permission);
    }
    
    pub fn revoke_permission(&mut self, permission: &Permission) {
        self.direct_permissions.remove(permission);
    }
    
    pub fn add_to_group(&mut self, group_id: Uuid) {
        if !self.inherited_groups.contains(&group_id) {
            self.inherited_groups.push(group_id);
        }
    }
    
    pub fn remove_from_group(&mut self, group_id: &Uuid) {
        self.inherited_groups.retain(|g| g != group_id);
    }
}

pub struct AgentPermissionsService {
    permissions: HashMap<Uuid, AgentPermissions>,
    groups: HashMap<Uuid, PermissionGroup>,
}

impl AgentPermissionsService {
    pub fn new() -> Self {
        Self {
            permissions: HashMap::new(),
            groups: HashMap::new(),
        }
    }
    
    /// 创建权限组
    pub fn create_group(&mut self, name: String, description: String, permissions: HashSet<Permission>) -> Uuid {
        let group_id = Uuid::new_v4();
        let group = PermissionGroup {
            id: group_id,
            name,
            description,
            permissions,
        };
        self.groups.insert(group_id, group);
        group_id
    }
    
    /// 删除权限组
    pub fn delete_group(&mut self, group_id: &Uuid) -> PermissionResult<()> {
        self.groups.remove(group_id);
        
        // 从所有agent中移除此组
        for perms in self.permissions.values_mut() {
            perms.remove_from_group(group_id);
        }
        
        Ok(())
    }
    
    /// 获取agent权限
    pub fn get_agent_permissions(&mut self, agent_id: Uuid) -> &mut AgentPermissions {
        self.permissions.entry(agent_id).or_insert_with(|| AgentPermissions::new(agent_id))
    }
    
    /// 授予直接权限
    pub fn grant_permission(&mut self, agent_id: Uuid, permission: Permission) {
        let perms = self.get_agent_permissions(agent_id);
        perms.grant_permission(permission);
    }
    
    /// 撤销直接权限
    pub fn revoke_permission(&mut self, agent_id: Uuid, permission: &Permission) {
        if let Some(perms) = self.permissions.get_mut(&agent_id) {
            perms.revoke_permission(permission);
        }
    }
    
    /// 将agent加入权限组
    pub fn add_to_group(&mut self, agent_id: Uuid, group_id: Uuid) -> PermissionResult<()> {
        if !self.groups.contains_key(&group_id) {
            return Err(PermissionError::InvalidPermission(format!("group {} not found", group_id)));
        }
        
        let perms = self.get_agent_permissions(agent_id);
        perms.add_to_group(group_id);
        Ok(())
    }
    
    /// 从权限组移除agent
    pub fn remove_from_group(&mut self, agent_id: Uuid, group_id: &Uuid) {
        if let Some(perms) = self.permissions.get_mut(&agent_id) {
            perms.remove_from_group(group_id);
        }
    }
    
    /// 检查权限（包含继承）
    pub fn has_permission(&self, agent_id: Uuid, permission: &Permission) -> bool {
        let perms = match self.permissions.get(&agent_id) {
            Some(p) => p,
            None => return false,
        };
        
        // 检查管理员权限
        if perms.direct_permissions.contains(&Permission::AdminFullAccess) {
            return true;
        }
        
        // 检查直接权限
        if perms.direct_permissions.contains(permission) {
            return true;
        }
        
        // 检查继承的组权限
        for group_id in &perms.inherited_groups {
            if let Some(group) = self.groups.get(group_id) {
                if group.permissions.contains(&Permission::AdminFullAccess) {
                    return true;
                }
                if group.permissions.contains(permission) {
                    return true;
                }
            }
        }
        
        false
    }
    
    /// 获取agent的所有有效权限
    pub fn get_effective_permissions(&self, agent_id: Uuid) -> HashSet<Permission> {
        let mut effective = HashSet::new();
        
        if let Some(perms) = self.permissions.get(&agent_id) {
            // 添加直接权限
            effective.extend(perms.direct_permissions.iter().cloned());
            
            // 添加组权限
            for group_id in &perms.inherited_groups {
                if let Some(group) = self.groups.get(group_id) {
                    effective.extend(group.permissions.iter().cloned());
                }
            }
        }
        
        effective
    }
    
    /// 列出agent的权限组
    pub fn get_agent_groups(&self, agent_id: Uuid) -> Vec<&PermissionGroup> {
        if let Some(perms) = self.permissions.get(&agent_id) {
            perms.inherited_groups.iter()
                .filter_map(|g| self.groups.get(g))
                .collect()
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_direct_permissions() {
        let mut service = AgentPermissionsService::new();
        let agent_id = Uuid::new_v4();
        
        let perm = Permission::ToolExecute("github:search".to_string());
        service.grant_permission(agent_id, perm.clone());
        
        assert!(service.has_permission(agent_id, &perm));
        
        service.revoke_permission(agent_id, &perm);
        assert!(!service.has_permission(agent_id, &perm));
    }
    
    #[test]
    fn test_permission_inheritance() {
        let mut service = AgentPermissionsService::new();
        let agent_id = Uuid::new_v4();
        
        // 创建权限组
        let mut group_perms = HashSet::new();
        group_perms.insert(Permission::ToolDiscover);
        let group_id = service.create_group(
            "Developers".to_string(),
            "Developer permissions".to_string(),
            group_perms
        );
        
        // 将agent加入组
        service.add_to_group(agent_id, group_id).unwrap();
        
        // 应该继承组权限
        assert!(service.has_permission(agent_id, &Permission::ToolDiscover));
    }
    
    #[test]
    fn test_admin_permission() {
        let mut service = AgentPermissionsService::new();
        let agent_id = Uuid::new_v4();
        
        service.grant_permission(agent_id, Permission::AdminFullAccess);
        
        // 管理员应该有所有权限
        assert!(service.has_permission(agent_id, &Permission::ToolDiscover));
        assert!(service.has_permission(agent_id, &Permission::AgentCreate));
        assert!(service.has_permission(agent_id, &Permission::WorkspaceAccess(Uuid::new_v4())));
    }
    
    #[test]
    fn test_effective_permissions() {
        let mut service = AgentPermissionsService::new();
        let agent_id = Uuid::new_v4();
        
        // 直接权限
        service.grant_permission(agent_id, Permission::ToolDiscover);
        
        // 组权限
        let mut group_perms = HashSet::new();
        group_perms.insert(Permission::AgentCreate);
        let group_id = service.create_group(
            "Test".to_string(),
            "".to_string(),
            group_perms
        );
        service.add_to_group(agent_id, group_id).unwrap();
        
        let effective = service.get_effective_permissions(agent_id);
        assert_eq!(effective.len(), 2);
        assert!(effective.contains(&Permission::ToolDiscover));
        assert!(effective.contains(&Permission::AgentCreate));
    }
}
