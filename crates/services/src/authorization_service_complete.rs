use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use crate::ServiceError;

/// Authorization action types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuthorizationAction {
    // Company-level permissions
    UsersInvite,
    JoinsApprove,
    CompanyRead,
    CompanyUpdate,

    // Project-level permissions
    IssuesRead,
    IssuesWrite,
    IssuesDelete,
    ProjectsRead,
    ProjectsWrite,

    // Agent-specific permissions
    TasksAssign,
    AgentsCreate,
    AgentsRead,
    AgentsUpdate,
    AgentsDelete,

    // Resource-specific
    ResourceRead(String),
    ResourceWrite(String),
}

/// Actor types for authorization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthorizationActor {
    Board {
        user_id: Uuid,
        company_ids: Vec<Uuid>,
        is_instance_admin: bool,
        memberships: Vec<CompanyMembership>,
    },
    Agent {
        agent_id: Uuid,
        company_id: Uuid,
        responsible_user_id: Option<Uuid>,
        permissions: AgentPermissions,
    },
    None,
}

/// Company membership
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyMembership {
    pub company_id: Uuid,
    pub role: MembershipRole,
    pub status: MembershipStatus,
}

/// Membership roles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MembershipRole {
    Owner,
    Admin,
    Operator,
    Viewer,
}

/// Membership status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembershipStatus {
    Active,
    Archived,
}

/// Agent permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPermissions {
    pub can_create_agents: bool,
    pub can_create_skills: bool,
    pub can_assign_tasks: bool,
    pub trust_preset: TrustPreset,
}

/// Trust presets for agents
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustPreset {
    Untrusted,
    Low,
    Medium,
    High,
    Full,
}

/// Authorization decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationDecision {
    pub allowed: bool,
    pub action: AuthorizationAction,
    pub explanation: String,
    pub reason: DecisionReason,
}

/// Decision reasons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionReason {
    // Allow reasons
    AllowInstanceAdmin,
    AllowCompanyOwner,
    AllowCompanyAdmin,
    AllowRolePermission,
    AllowExplicitGrant,
    AllowAgentPermission,
    AllowPublicAccess,

    // Deny reasons
    DenyNoAuth,
    DenyNoCompanyAccess,
    DenyInsufficientRole,
    DenyNoPermission,
    DenyLowTrustBoundary,
    DenyViewerReadOnly,
    DenyResourceNotFound,
}

/// Authorization service trait
#[async_trait]
pub trait AuthorizationService: Send + Sync {
    /// Check if action is authorized for actor on company
    async fn authorize_company_action(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
        action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, ServiceError>;

    /// Check if action is authorized for actor on resource
    async fn authorize_resource_action(
        &self,
        actor: &AuthorizationActor,
        resource_type: &str,
        resource_id: Uuid,
        action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, ServiceError>;

    /// Assert company access (throws error if denied)
    async fn assert_company_access(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
    ) -> Result<(), ServiceError>;

    /// Assert write access (for mutating operations)
    async fn assert_write_access(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
    ) -> Result<(), ServiceError>;

    /// Check if actor has specific permission
    async fn has_permission(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
        permission: &str,
    ) -> Result<bool, ServiceError>;

    /// Get effective permissions for actor in company
    async fn get_effective_permissions(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
    ) -> Result<Vec<String>, ServiceError>;
}

/// Default authorization service implementation
pub struct DefaultAuthorizationService {
    // Permission mappings by role
    role_permissions: HashMap<MembershipRole, Vec<String>>,
}

impl DefaultAuthorizationService {
    pub fn new() -> Self {
        let mut role_permissions = HashMap::new();

        // Owner: full access
        role_permissions.insert(
            MembershipRole::Owner,
            vec![
                "users:invite".to_string(),
                "joins:approve".to_string(),
                "company:update".to_string(),
                "issues:read".to_string(),
                "issues:write".to_string(),
                "issues:delete".to_string(),
                "projects:read".to_string(),
                "projects:write".to_string(),
                "agents:create".to_string(),
                "agents:read".to_string(),
                "agents:update".to_string(),
                "agents:delete".to_string(),
                "tasks:assign".to_string(),
            ],
        );

        // Admin: most permissions except user management
        role_permissions.insert(
            MembershipRole::Admin,
            vec![
                "company:update".to_string(),
                "issues:read".to_string(),
                "issues:write".to_string(),
                "issues:delete".to_string(),
                "projects:read".to_string(),
                "projects:write".to_string(),
                "agents:create".to_string(),
                "agents:read".to_string(),
                "agents:update".to_string(),
                "tasks:assign".to_string(),
            ],
        );

        // Operator: read/write but no delete
        role_permissions.insert(
            MembershipRole::Operator,
            vec![
                "issues:read".to_string(),
                "issues:write".to_string(),
                "projects:read".to_string(),
                "projects:write".to_string(),
                "agents:read".to_string(),
                "tasks:assign".to_string(),
            ],
        );

        // Viewer: read-only
        role_permissions.insert(
            MembershipRole::Viewer,
            vec![
                "issues:read".to_string(),
                "projects:read".to_string(),
                "agents:read".to_string(),
            ],
        );

        Self { role_permissions }
    }

    /// Check if actor has membership in company
    fn get_membership<'a>(
        &self,
        actor: &'a AuthorizationActor,
        company_id: Uuid,
    ) -> Option<&'a CompanyMembership> {
        match actor {
            AuthorizationActor::Board { memberships, .. } => {
                memberships
                    .iter()
                    .find(|m| m.company_id == company_id && m.status == MembershipStatus::Active)
            }
            _ => None,
        }
    }

    /// Check if action is a write operation
    fn is_write_action(&self, action: &AuthorizationAction) -> bool {
        matches!(
            action,
            AuthorizationAction::IssuesWrite
                | AuthorizationAction::IssuesDelete
                | AuthorizationAction::ProjectsWrite
                | AuthorizationAction::AgentsCreate
                | AuthorizationAction::AgentsUpdate
                | AuthorizationAction::AgentsDelete
                | AuthorizationAction::CompanyUpdate
                | AuthorizationAction::ResourceWrite(_)
        )
    }

    /// Map action to permission string
    fn action_to_permission(&self, action: &AuthorizationAction) -> String {
        match action {
            AuthorizationAction::UsersInvite => "users:invite".to_string(),
            AuthorizationAction::JoinsApprove => "joins:approve".to_string(),
            AuthorizationAction::CompanyRead => "company:read".to_string(),
            AuthorizationAction::CompanyUpdate => "company:update".to_string(),
            AuthorizationAction::IssuesRead => "issues:read".to_string(),
            AuthorizationAction::IssuesWrite => "issues:write".to_string(),
            AuthorizationAction::IssuesDelete => "issues:delete".to_string(),
            AuthorizationAction::ProjectsRead => "projects:read".to_string(),
            AuthorizationAction::ProjectsWrite => "projects:write".to_string(),
            AuthorizationAction::TasksAssign => "tasks:assign".to_string(),
            AuthorizationAction::AgentsCreate => "agents:create".to_string(),
            AuthorizationAction::AgentsRead => "agents:read".to_string(),
            AuthorizationAction::AgentsUpdate => "agents:update".to_string(),
            AuthorizationAction::AgentsDelete => "agents:delete".to_string(),
            AuthorizationAction::ResourceRead(r) => format!("resource:{}:read", r),
            AuthorizationAction::ResourceWrite(r) => format!("resource:{}:write", r),
        }
    }
}

impl Default for DefaultAuthorizationService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuthorizationService for DefaultAuthorizationService {
    async fn authorize_company_action(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
        action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, ServiceError> {
        // Check actor type
        match actor {
            AuthorizationActor::None => {
                return Ok(AuthorizationDecision {
                    allowed: false,
                    action,
                    explanation: "No authentication provided".to_string(),
                    reason: DecisionReason::DenyNoAuth,
                });
            }
            AuthorizationActor::Board {
                is_instance_admin,
                company_ids,
                ..
            } => {
                // Instance admin has full access
                if *is_instance_admin {
             return Ok(AuthorizationDecision {
                        allowed: true,
                        action,
                        explanation: "Instance admin has full access".to_string(),
                        reason: DecisionReason::AllowInstanceAdmin,
                    });
                }

                // Check company membership
                if !company_ids.contains(&company_id) {
                    return Ok(AuthorizationDecision {
                        allowed: false,
                        action,
                        explanation: "No access to this company".to_string(),
                        reason: DecisionReason::DenyNoCompanyAccess,
                    });
                }

                // Check role-based permissions
                if let Some(membership) = self.get_membership(actor, company_id) {
                    // Owner/Admin can do most things
                    if membership.role == MembershipRole::Owner {
                        return Ok(AuthorizationDecision {
                            allowed: true,
                            action,
                            explanation: "Company owner has full access".to_string(),
                            reason: DecisionReason::AllowCompanyOwner,
                        });
                    }

                    // Viewer cannot write
                    if membership.role == MembershipRole::Viewer && self.is_write_action(&action) {
                        return Ok(AuthorizationDecision {
                            allowed: false,
                            action,
                            explanation: "Viewer role is read-only".to_string(),
                            reason: DecisionReason::DenyViewerReadOnly,
                        });
                    }

                    // Check permission by role
                    let permission = self.action_to_permission(&action);
                    if let Some(perms) = self.role_permissions.get(&membership.role) {
                        if perms.contains(&permission) {
                            return Ok(AuthorizationDecision {
                                allowed: true,
                                action,
                                explanation: format!("Role {} has permission {}", membership.role as i32, permission),
                                reason: DecisionReason::AllowRolePermission,
                            });
                        }
                    }

                    return Ok(AuthorizationDecision {
                        allowed: false,
                        action,
                        explanation: "Insufficient role permissions".to_string(),
                        reason: DecisionReason::DenyInsufficientRole,
                    });
                }

                Ok(AuthorizationDecision {
                    allowed: false,
                    action,
                    explanation: "No membership found".to_string(),
                    reason: DecisionReason::DenyNoCompanyAccess,
                })
            }
            AuthorizationActor::Agent {
                company_id: agent_company_id,
                permissions,
                ..
            } => {
                // Agent can only access its own company
                if *agent_company_id != company_id {
                    return Ok(AuthorizationDecision {
                        allowed: false,
                        action,
                        explanation: "Agent cannot access another company".to_string(),
                        reason: DecisionReason::DenyNoCompanyAccess,
                    });
                }

                // Check agent-specific permissions
                let allowed = match action {
                    AuthorizationAction::AgentsCreate => permissions.can_create_agents,
                    AuthorizationAction::TasksAssign => permissions.can_assign_tasks,
                    AuthorizationAction::IssuesRead | AuthorizationAction::ProjectsRead => true,
                    _ => false,
                };

                if allowed {
                    Ok(AuthorizationDecision {
                        allowed: true,
                        action,
                        explanation: "Agent has required permission".to_string(),
                        reason: DecisionReason::AllowAgentPermission,
                    })
                } else {
                    Ok(AuthorizationDecision {
                        allowed: false,
                        action,
                        explanation: "Agent lacks required permission".to_string(),
                        reason: DecisionReason::DenyNoPermission,
                    })
                }
            }
        }
    }

    async fn authorize_resource_action(
        &self,
        actor: &AuthorizationActor,
        _resource_type: &str,
        _resource_id: Uuid,
        action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, ServiceError> {
        // For now, delegate to company-level authorization
        // In production, would check resource ownership and ACLs
        let company_id = match actor {
            AuthorizationActor::Board { company_ids, .. } => company_ids.first().copied(),
            AuthorizationActor::Agent { company_id, .. } => Some(*company_id),
            AuthorizationActor::None => None,
        };

        if let Some(company_id) = company_id {
            self.authorize_company_action(actor, company_id, action).await
        } else {
            Ok(AuthorizationDecision {
                allowed: false,
                action,
                explanation: "No company context for resource".to_string(),
                reason: DecisionReason::DenyNoCompanyAccess,
            })
        }
    }

    async fn assert_company_access(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
    ) -> Result<(), ServiceError> {
        let decision = self
            .authorize_company_action(actor, company_id, AuthorizationAction::CompanyRead)
            .await?;

        if !decision.allowed {
            return Err(ServiceError::Forbidden(decision.explanation));
        }

        Ok(())
    }

    async fn assert_write_access(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
    ) -> Result<(), ServiceError> {
        let decision = self
            .authorize_company_action(actor, company_id, AuthorizationAction::CompanyUpdate)
            .await?;

        if !decision.allowed {
            return Err(ServiceError::Forbidden(decision.explanation));
        }

        Ok(())
    }

    async fn has_permission(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
        permission: &str,
    ) -> Result<bool, ServiceError> {
        match actor {
            AuthorizationActor::Board {
                is_instance_admin, ..
            } => {
                if *is_instance_admin {
                    return Ok(true);
                }

                if let Some(membership) = self.get_membership(actor, company_id) {
                    if let Some(perms) = self.role_permissions.get(&membership.role) {
                        return Ok(perms.iter().any(|p| p == permission));
                    }
                }

                Ok(false)
            }
            AuthorizationActor::Agent { permissions, .. } => {
                let has_perm = match permission {
                    "agents:create" => permissions.can_create_agents,
                    "tasks:assign" => permissions.can_assign_tasks,
                    "issues:read" | "projects:read" => true,
                    _ => false,
                };

                Ok(has_perm)
            }
            AuthorizationActor::None => Ok(false),
        }
    }

    async fn get_effective_permissions(
        &self,
        actor: &AuthorizationActor,
        company_id: Uuid,
    ) -> Result<Vec<String>, ServiceError> {
        match actor {
            AuthorizationActor::Board {
                is_instance_admin, ..
            } => {
                if *is_instance_admin {
                    // Instance admin has all permissions
                    return Ok(self
                        .role_permissions
                        .values()
                        .flatten()
                        .cloned()
                        .collect());
                }

                if let Some(membership) = self.get_membership(actor, company_id) {
                    if let Some(perms) = self.role_permissions.get(&membership.role) {
                        return Ok(perms.clone());
                    }
                }

                Ok(vec![])
            }
            AuthorizationActor::Agent { permissions, .. } => {
                let mut perms = vec!["issues:read".to_string(), "projects:read".to_string()];

                if permissions.can_create_agents {
                    perms.push("agents:create".to_string());
                }
                if permissions.can_assign_tasks {
                    perms.push("tasks:assign".to_string());
                }

                Ok(perms)
            }
            AuthorizationActor::None => Ok(vec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_instance_admin_full_access() {
        let service = DefaultAuthorizationService::new();

        let actor = AuthorizationActor::Board {
            user_id: Uuid::new_v4(),
            company_ids: vec![],
            is_instance_admin: true,
            memberships: vec![],
        };

        let decision = service
            .authorize_company_action(&actor, Uuid::new_v4(), AuthorizationAction::CompanyUpdate)
            .await
            .unwrap();

        assert!(decision.allowed);
        assert!(matches!(decision.reason, DecisionReason::AllowInstanceAdmin));
    }

    #[tokio::test]
    async fn test_viewer_cannot_write() {
        let service = DefaultAuthorizationService::new();

        let company_id = Uuid::new_v4();
        let actor = AuthorizationActor::Board {
            user_id: Uuid::new_v4(),
            company_ids: vec![company_id],
            is_instance_admin: false,
            memberships: vec![CompanyMembership {
                company_id,
                role: MembershipRole::Viewer,
                status: MembershipStatus::Active,
            }],
        };

        let decision = service
            .authorize_company_action(&actor, company_id, AuthorizationAction::IssuesWrite)
            .await
            .unwrap();

        assert!(!decision.allowed);
        assert!(matches!(decision.reason, DecisionReason::DenyViewerReadOnly));
    }

    #[tokio::test]
    async fn test_agent_company_isolation() {
        let service = DefaultAuthorizationService::new();

        let agent_company = Uuid::new_v4();
        let other_company = Uuid::new_v4();

        let actor = AuthorizationActor::Agent {
            agent_id: Uuid::new_v4(),
            company_id: agent_company,
            responsible_user_id: None,
            permissions: AgentPermissions {
                can_create_agents: false,
                can_create_skills: false,
                can_assign_tasks: true,
                trust_preset: TrustPreset::Medium,
            },
        };

        let decision = service
            .authorize_company_action(&actor, other_company, AuthorizationAction::IssuesRead)
            .await
            .unwrap();

        assert!(!decision.allowed);
        assert!(matches!(decision.reason, DecisionReason::DenyNoCompanyAccess));
    }
}
