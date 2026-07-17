//! 授权决策引擎（对应任务拆解 §7 阶段二/三）。
//!
//! 提供：
//! - `TrustPresetResolver`：核心信任预设解析（高/低信任边界）
//! - `AuthorizationService::decide()`：按优先级链决策的授权主函数
//! - 权限继承链查询：`check_explicit_grants` / `check_manager_chain` / `check_issue_mention_grant`
//! - `RolePermissions` 角色默认权限映射：`default_permissions_for_role`
//! - onBehalfOf 委托：Agent 以 responsible user 的成员关系进行权限检查

use sqlx::PgPool;
use uuid::Uuid;

use repositories::auth_repositories::{
    CompanyMembershipRepository, PrincipalPermissionGrantRepository,
};

use super::actor::{ActorSource, AuthorizationActor};
use super::decision::{AuthorizationAction, AuthorizationDecision, DecisionReason};
use super::membership::{CompanyMembership, MembershipRole};
use super::permission::PermissionKey;

/// 核心信任预设（对应 §7 阶段二 TrustPresetResolver）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustPreset {
    /// 高信任：同公司内常规操作，按角色/授权即可。
    High,
    /// 低信任：跨边界敏感操作（如 Agent 在 Issue 中 @ 另一个 Agent），
    /// 必须存在显式授予（explicit grant）才允许。
    Low,
}

/// 信任预设解析结果。
#[derive(Debug, Clone)]
pub struct TrustPresetResolution {
    /// 解析出的信任预设
    pub preset: TrustPreset,
    /// 是否需要显式授予（低信任边界场景）
    pub requires_explicit_grant: bool,
    /// 人类可读说明
    pub reason: String,
}

/// 信任预设解析器：根据 Actor 与资源推导核心信任边界。
///
/// 规则：
/// - Board 用户（会话/API Key/本地隐式）默认处于高信任边界。
/// - Agent 主体访问敏感资源（issue mention / 读取其他 Agent 配置）进入低信任边界，
///   需要显式授予。
pub struct TrustPresetResolver;

impl TrustPresetResolver {
    /// 解析某个操作的核心信任预设。
    ///
    /// `resource_company_id` 为资源所属公司（用于跨公司检测）；`action` 描述具体操作。
    pub fn resolve_core_trust_preset(
        actor: &AuthorizationActor,
        action: &AuthorizationAction,
        resource_company_id: Option<Uuid>,
    ) -> TrustPresetResolution {
        // Agent 主体天然处于更严格的信任边界。
        if let AuthorizationActor::Agent { .. } = actor {
            // Issue mention：低信任边界，需要显式授予。
            if matches!(action, AuthorizationAction::IssueMention { .. }) {
                return TrustPresetResolution {
                    preset: TrustPreset::Low,
                    requires_explicit_grant: true,
                    reason: "Agent issue mention requires explicit grant (low trust boundary)"
                        .to_string(),
                };
            }

            // 读取其他 Agent 配置 / 敏感资源：低信任边界。
            if matches!(
                action,
                AuthorizationAction::AgentRead { .. }
                    | AuthorizationAction::AgentUpdate { .. }
                    | AuthorizationAction::Custom { .. }
            ) {
                return TrustPresetResolution {
                    preset: TrustPreset::Low,
                    requires_explicit_grant: true,
                    reason: "Agent accessing sensitive agent resource requires explicit grant"
                        .to_string(),
                };
            }

            // 跨公司访问：低信任边界。
            if let (Some(rc), Some(ac)) = (resource_company_id, actor.company_id()) {
                if rc != ac {
                    return TrustPresetResolution {
                        preset: TrustPreset::Low,
                        requires_explicit_grant: true,
                        reason: "Agent cross-company access requires explicit grant".to_string(),
                    };
                }
            }

            return TrustPresetResolution {
                preset: TrustPreset::High,
                requires_explicit_grant: false,
                reason: "Agent intra-company operation within high trust boundary".to_string(),
            };
        }

        // Board 用户默认高信任边界。
        TrustPresetResolution {
            preset: TrustPreset::High,
            requires_explicit_grant: false,
            reason: "Board user operates within high trust boundary".to_string(),
        }
    }
}

/// 角色默认权限映射（对应 §7 阶段三 RolePermissions）。
///
/// Owner -> 全部权限；Admin -> 管理能力；Operator -> 读写；Viewer -> 只读。
pub struct RolePermissions;

impl RolePermissions {
    /// 返回某角色默认拥有的权限键集合。
    pub fn default_permissions_for_role(role: MembershipRole) -> Vec<PermissionKey> {
        match role {
            MembershipRole::Owner => vec![
                PermissionKey::from_const(PermissionKey::USERS_INVITE),
                PermissionKey::from_const(PermissionKey::JOINS_APPROVE),
                PermissionKey::from_const(PermissionKey::COMPANY_SETTINGS_UPDATE),
                PermissionKey::from_const(PermissionKey::COMPANY_DELETE),
                PermissionKey::from_const(PermissionKey::MEMBERS_MANAGE),
                PermissionKey::from_const(PermissionKey::ROLES_ASSIGN),
                PermissionKey::from_const(PermissionKey::ISSUES_READ),
                PermissionKey::from_const(PermissionKey::ISSUES_WRITE),
                PermissionKey::from_const(PermissionKey::ISSUES_DELETE),
                PermissionKey::from_const(PermissionKey::ISSUES_ASSIGN),
                PermissionKey::from_const(PermissionKey::AGENTS_CREATE),
                PermissionKey::from_const(PermissionKey::AGENTS_UPDATE),
                PermissionKey::from_const(PermissionKey::AGENTS_DELETE),
                PermissionKey::from_const(PermissionKey::AGENTS_HIRE),
                PermissionKey::from_const(PermissionKey::TASKS_ASSIGN),
                PermissionKey::from_const(PermissionKey::ENVIRONMENTS_LEASE),
                PermissionKey::from_const(PermissionKey::ENVIRONMENTS_RELEASE),
                PermissionKey::from_const(PermissionKey::ENVIRONMENTS_MANAGE),
                PermissionKey::from_const(PermissionKey::ROUTINES_CREATE),
                PermissionKey::from_const(PermissionKey::ROUTINES_UPDATE),
                PermissionKey::from_const(PermissionKey::ROUTINES_DELETE),
                PermissionKey::from_const(PermissionKey::ROUTINES_TRIGGER),
                PermissionKey::from_const(PermissionKey::GOALS_CREATE),
                PermissionKey::from_const(PermissionKey::GOALS_UPDATE),
                PermissionKey::from_const(PermissionKey::GOALS_DELETE),
            ],
            MembershipRole::Admin => vec![
                PermissionKey::from_const(PermissionKey::USERS_INVITE),
                PermissionKey::from_const(PermissionKey::JOINS_APPROVE),
                PermissionKey::from_const(PermissionKey::COMPANY_SETTINGS_UPDATE),
                PermissionKey::from_const(PermissionKey::MEMBERS_MANAGE),
                PermissionKey::from_const(PermissionKey::ROLES_ASSIGN),
                PermissionKey::from_const(PermissionKey::ISSUES_READ),
                PermissionKey::from_const(PermissionKey::ISSUES_WRITE),
                PermissionKey::from_const(PermissionKey::ISSUES_DELETE),
                PermissionKey::from_const(PermissionKey::ISSUES_ASSIGN),
                PermissionKey::from_const(PermissionKey::AGENTS_CREATE),
                PermissionKey::from_const(PermissionKey::AGENTS_UPDATE),
                PermissionKey::from_const(PermissionKey::AGENTS_DELETE),
                PermissionKey::from_const(PermissionKey::AGENTS_HIRE),
                PermissionKey::from_const(PermissionKey::TASKS_ASSIGN),
                PermissionKey::from_const(PermissionKey::ENVIRONMENTS_LEASE),
                PermissionKey::from_const(PermissionKey::ENVIRONMENTS_RELEASE),
                PermissionKey::from_const(PermissionKey::ENVIRONMENTS_MANAGE),
                PermissionKey::from_const(PermissionKey::ROUTINES_CREATE),
                PermissionKey::from_const(PermissionKey::ROUTINES_UPDATE),
                PermissionKey::from_const(PermissionKey::ROUTINES_DELETE),
                PermissionKey::from_const(PermissionKey::ROUTINES_TRIGGER),
                PermissionKey::from_const(PermissionKey::GOALS_CREATE),
                PermissionKey::from_const(PermissionKey::GOALS_UPDATE),
                PermissionKey::from_const(PermissionKey::GOALS_DELETE),
            ],
            MembershipRole::Operator => vec![
                PermissionKey::from_const(PermissionKey::ISSUES_READ),
                PermissionKey::from_const(PermissionKey::ISSUES_WRITE),
                PermissionKey::from_const(PermissionKey::ISSUES_ASSIGN),
                PermissionKey::from_const(PermissionKey::AGENTS_READ),
                PermissionKey::from_const(PermissionKey::TASKS_ASSIGN),
                PermissionKey::from_const(PermissionKey::ENVIRONMENTS_LEASE),
                PermissionKey::from_const(PermissionKey::ROUTINES_TRIGGER),
                PermissionKey::from_const(PermissionKey::GOALS_CREATE),
                PermissionKey::from_const(PermissionKey::GOALS_UPDATE),
            ],
            MembershipRole::Viewer => vec![
                PermissionKey::from_const(PermissionKey::ISSUES_READ),
                PermissionKey::from_const(PermissionKey::AGENTS_READ),
            ],
        }
    }
}

/// 判断某角色是否默认包含给定权限键。
pub fn role_has_permission(role: MembershipRole, key: &PermissionKey) -> bool {
    RolePermissions::default_permissions_for_role(role)
        .iter()
        .any(|k| k == key)
}

/// 权限继承链查询（对应 §7 阶段二）。

/// 检查某主体在公司内是否拥有指定权限键的有效显式授予。
pub async fn check_explicit_grants(
    pool: &PgPool,
    company_id: Uuid,
    principal_type: &str,
    principal_id: Uuid,
    permission_key: &str,
) -> bool {
    let repo = repositories::auth_repositories::PgPrincipalPermissionGrantRepository::new(pool.clone());
    repo.find_valid_grant(company_id, principal_type, principal_id, permission_key)
        .await
        .unwrap_or(None)
        .is_some()
}

/// 检查管理者继承链：若 `principal_id` 是资源所有者（`resource_owner_id`）的管理者，
/// 则继承其权限授予。
///
/// 简化实现：管理者（Owner/Admin）在公司内对所有成员的资源具有继承权限。
/// 返回 true 表示管理者关系成立（调用方仍需结合具体权限键判断）。
pub async fn check_manager_chain(
    pool: &PgPool,
    company_id: Uuid,
    manager_id: Uuid,
    resource_owner_id: Uuid,
) -> bool {
    if manager_id == resource_owner_id {
        return true;
    }

    let membership_repo =
        repositories::auth_repositories::PgCompanyMembershipRepository::new(pool.clone());
    let manager_membership = match membership_repo
        .find_by_principal(company_id, "user", manager_id)
        .await
        .unwrap_or(None)
    {
        Some(m) => m,
        None => return false,
    };

    if !manager_membership.status.eq_ignore_ascii_case("active") {
        return false;
    }

    // Owner / Admin 视为管理者，可继承下属资源的权限。
    matches!(
        manager_membership.role.to_ascii_lowercase().as_str(),
        "owner" | "admin"
    )
}

/// 检查 Issue mention 授予：Agent 在指定 Issue 中 @ 另一个 Agent 是否被显式授予。
pub async fn check_issue_mention_grant(
    pool: &PgPool,
    company_id: Uuid,
    agent_id: Uuid,
    issue_id: Uuid,
) -> bool {
    // Issue mention 授予以 permission_key = "issues:mention:<issue_id>" 形式存储，
    // 作用域限制在该 issue。
    let key = format!("issues:mention:{}", issue_id);
    check_explicit_grants(pool, company_id, "agent", agent_id, &key).await
}

/// 授权决策引擎（对应 §7 阶段二 decide() 主函数）。
///
/// 优先级链：
/// 1. 未认证 -> deny (DenyUnauthenticated)
/// 2. 实例管理员 -> allow (AllowInstanceAdmin)
/// 3. 本地隐式 Board 用户（单用户模式）-> allow (AllowLocalImplicit)
/// 4. Agent 跨公司边界 -> deny (DenyCrossCompanyAccess)
/// 5. 公司成员授予（角色默认权限 + 显式授予；低信任边界需显式授予）
pub struct AuthorizationService;

impl AuthorizationService {
    /// 执行授权决策。
    ///
    /// `scope` 为可选的资源公司 ID（用于跨公司检测与信任预设解析）。
    pub async fn decide(
        pool: &PgPool,
        actor: &AuthorizationActor,
        action: &AuthorizationAction,
        scope: Option<Uuid>,
    ) -> AuthorizationDecision {
        // 1. 未认证
        if actor.is_anonymous() {
            return AuthorizationDecision::deny(
                action.clone(),
                DecisionReason::DenyUnauthenticated,
                "Authentication required".to_string(),
            );
        }

        // 2. 实例管理员（全局允许）
        if actor.is_instance_admin() {
            return AuthorizationDecision::allow(
                action.clone(),
                DecisionReason::AllowInstanceAdmin,
                "Actor is instance administrator".to_string(),
            );
        }

        // 3. 本地隐式 Board 用户（单用户/开发模式）
        if let AuthorizationActor::Board { source, .. } = actor {
            if matches!(source, ActorSource::LocalImplicit) {
                return AuthorizationDecision::allow(
                    action.clone(),
                    DecisionReason::AllowLocalImplicit,
                    "Local implicit board user (single-user mode)".to_string(),
                );
            }
        }

        // 推导资源公司 ID 用于信任预设与跨公司检测。
        let resource_company = scope.or_else(|| action.company_id());

        // 解析信任预设。
        let trust = TrustPresetResolver::resolve_core_trust_preset(actor, action, resource_company);

        // 4. Agent 跨公司边界
        if let AuthorizationActor::Agent { .. } = actor {
            if let (Some(rc), Some(ac)) = (resource_company, actor.company_id()) {
                if rc != ac {
                    return AuthorizationDecision::deny(
                        action.clone(),
                        DecisionReason::DenyCrossCompanyAccess {
                            resource_company: rc,
                            actor_company: ac,
                        },
                        "Agent cross-company access denied".to_string(),
                    );
                }
            }
        }

        // 5. 公司成员授予
        Self::decide_company_grant(pool, actor, action, resource_company, &trust).await
    }

    /// 公司成员授予决策（角色默认权限 + 显式授予 + onBehalfOf 委托）。
    async fn decide_company_grant(
        pool: &PgPool,
        actor: &AuthorizationActor,
        action: &AuthorizationAction,
        resource_company: Option<Uuid>,
        trust: &TrustPresetResolution,
    ) -> AuthorizationDecision {
        // 需要检查的权限键：从 action 推导（Permission 变体直接取 key，其余映射到常量）。
        let permission_key = match action {
            AuthorizationAction::Permission { key } => Some(key.clone()),
            _ => permission_key_for_action(action),
        };

        // Issue mention 低信任边界：必须存在显式授予。
        if let AuthorizationAction::IssueMention {
            issue_id,
            mentioned_agent_id,
        } = action
        {
            let company_id = match resource_company {
                Some(c) => c,
                None => {
                    return AuthorizationDecision::deny(
                        action.clone(),
                        DecisionReason::DenyCustom {
                            reason: "Issue mention requires a company scope".to_string(),
                        },
                        "Missing company scope for issue mention".to_string(),
                    );
                }
            };
            let granted =
                check_issue_mention_grant(pool, company_id, *mentioned_agent_id, *issue_id).await;
            return if granted {
                AuthorizationDecision::allow(
                    action.clone(),
                    DecisionReason::AllowIssueMentionGrant { issue_id: *issue_id },
                    "Issue mention explicit grant present".to_string(),
                )
            } else {
                AuthorizationDecision::deny(
                    action.clone(),
                    DecisionReason::DenyLowTrustBoundary {
                        reason: "Issue mention requires explicit grant".to_string(),
                    },
                    "Low trust boundary: issue mention requires explicit grant".to_string(),
                )
            };
        }

        let permission_key = match permission_key {
            Some(k) => k,
            None => {
                // 无明确权限键的操作：同公司内默认允许（如读自己的资源）。
                return AuthorizationDecision::allow(
                    action.clone(),
                    DecisionReason::AllowResourceOwner,
                    "No explicit permission key required; intra-company allowed".to_string(),
                );
            }
        };

        // 低信任边界且要求显式授予：跳过角色默认权限，仅查显式授予。
        let allow_role_default = !trust.requires_explicit_grant;

        // 解析用于权限检查的主体（支持 onBehalfOf 委托）。
        let resolved = resolve_check_principal(actor, resource_company);
        let principal_type = resolved.principal_type;
        let principal_id = resolved.principal_id;
        let memberships = resolved.memberships;

        // 角色默认权限检查。
        if allow_role_default {
            for m in &memberships {
                if let Some(role) = membership_role(&m) {
                    if role_has_permission(role, &permission_key) {
                        let reason = match role {
                            MembershipRole::Owner => DecisionReason::AllowCompanyOwner,
                            MembershipRole::Admin => DecisionReason::AllowCompanyAdmin,
                            MembershipRole::Operator => DecisionReason::AllowCompanyOperator,
                            MembershipRole::Viewer => DecisionReason::AllowCompanyOperator,
                        };
                        return AuthorizationDecision::allow(
                            action.clone(),
                            reason,
                            format!("Role {:?} grants permission {}", role, permission_key),
                        );
                    }
                }
            }
        }

        // 显式授予检查（含 onBehalfOf 委托主体）。
        if let Some(company_id) = resource_company {
            let granted = check_explicit_grants(
                pool,
                company_id,
                principal_type,
                principal_id,
                permission_key.as_str(),
            )
            .await;
            if granted {
                // 查询 grant id 用于决策记录（best effort）。
                let repo = repositories::auth_repositories::PgPrincipalPermissionGrantRepository::new(
                    pool.clone(),
                );
                let grant_id = repo
                    .find_valid_grant(company_id, principal_type, principal_id, permission_key.as_str())
                    .await
                    .ok()
                    .flatten()
                    .map(|g| g.id);
                return AuthorizationDecision::allow(
                    action.clone(),
                    DecisionReason::AllowExplicitGrant {
                        grant_id: grant_id.unwrap_or_else(Uuid::nil),
                    },
                    format!("Explicit grant present for {}", permission_key),
                );
            }
        }

        // 拒绝：缺少权限。若使用了 onBehalfOf 但 responsible user 无活跃成员关系，
        // 设置专用决策代码 RESPONSIBLE_USER_UNAVAILABLE（对应 §7 阶段三）。
        let mut decision = AuthorizationDecision::deny(
            action.clone(),
            DecisionReason::DenyMissingPermission {
                required: permission_key,
            },
            "No role default permission or explicit grant found".to_string(),
        );
        if resolved.on_behalf_of && !resolved.has_active_membership {
            decision = decision.with_code("RESPONSIBLE_USER_UNAVAILABLE");
        }
        decision
    }
}

/// 从 action 推导出对应的 PermissionKey（非 Permission 变体）。
fn permission_key_for_action(action: &AuthorizationAction) -> Option<PermissionKey> {
    let key = match action {
        AuthorizationAction::IssueRead { .. } => PermissionKey::ISSUES_READ,
        AuthorizationAction::IssueWrite { .. } => PermissionKey::ISSUES_WRITE,
        AuthorizationAction::IssueDelete { .. } => PermissionKey::ISSUES_DELETE,
        AuthorizationAction::IssueAssign { .. } => PermissionKey::ISSUES_ASSIGN,
        AuthorizationAction::AgentCreate { .. } | AuthorizationAction::AgentHire { .. } => {
            PermissionKey::AGENTS_CREATE
        }
        AuthorizationAction::AgentRead { .. } => PermissionKey::AGENTS_READ,
        AuthorizationAction::AgentUpdate { .. } => PermissionKey::AGENTS_UPDATE,
        AuthorizationAction::AgentDelete { .. } => PermissionKey::AGENTS_DELETE,
        AuthorizationAction::CompanyRead { .. } => PermissionKey::COMPANY_SETTINGS_UPDATE,
        AuthorizationAction::CompanyUpdate { .. } => PermissionKey::COMPANY_SETTINGS_UPDATE,
        AuthorizationAction::CompanyDelete { .. } => PermissionKey::COMPANY_DELETE,
        AuthorizationAction::MembershipInvite { .. } => PermissionKey::USERS_INVITE,
        AuthorizationAction::MembershipApprove { .. } => PermissionKey::JOINS_APPROVE,
        AuthorizationAction::MembershipRevoke { .. } => PermissionKey::MEMBERS_MANAGE,
        AuthorizationAction::EnvironmentLease { .. } => PermissionKey::ENVIRONMENTS_LEASE,
        AuthorizationAction::EnvironmentRelease { .. } => PermissionKey::ENVIRONMENTS_RELEASE,
        AuthorizationAction::RoutineCreate { .. } => PermissionKey::ROUTINES_CREATE,
        AuthorizationAction::RoutineUpdate { .. } => PermissionKey::ROUTINES_UPDATE,
        AuthorizationAction::RoutineDelete { .. } => PermissionKey::ROUTINES_DELETE,
        AuthorizationAction::RoutineTrigger { .. } => PermissionKey::ROUTINES_TRIGGER,
        AuthorizationAction::GoalCreate { .. } => PermissionKey::GOALS_CREATE,
        AuthorizationAction::GoalUpdate { .. } => PermissionKey::GOALS_UPDATE,
        AuthorizationAction::GoalDelete { .. } => PermissionKey::GOALS_DELETE,
        _ => return None,
    };
    Some(PermissionKey::from_const(key))
}

/// 解析用于权限检查的主体与成员关系（对应 §7 阶段三 onBehalfOf 委托）。
///
/// 若 Actor 为 Agent 且携带 `on_behalf_of_user_id` / `on_behalf_of_memberships`，
/// 则以被委托用户为主体进行权限检查；否则以 Agent 自身为主体。
struct ResolvedPrincipal {
    principal_type: &'static str,
    principal_id: Uuid,
    memberships: Vec<CompanyMembership>,
    /// 是否使用了 onBehalfOf 委托（Agent 以 responsible user 身份检查）。
    on_behalf_of: bool,
    /// onBehalfOf 委托的 responsible user 是否拥有活跃成员关系。
    has_active_membership: bool,
}

fn resolve_check_principal(
    actor: &AuthorizationActor,
    _resource_company: Option<Uuid>,
) -> ResolvedPrincipal {
    match actor {
        AuthorizationActor::Board { user_id, memberships, .. } => ResolvedPrincipal {
            principal_type: "user",
            principal_id: *user_id,
            memberships: memberships.clone(),
            on_behalf_of: false,
            has_active_membership: memberships.iter().any(|m| m.status.is_active()),
        },
        AuthorizationActor::Agent {
            agent_id,
            on_behalf_of_user_id,
            on_behalf_of_memberships,
            ..
        } => {
            if let (Some(uid), memberships) =
                (on_behalf_of_user_id, on_behalf_of_memberships)
            {
                let has_active = memberships.iter().any(|m| m.status.is_active());
                return ResolvedPrincipal {
                    principal_type: "user",
                    principal_id: *uid,
                    memberships: memberships.clone(),
                    on_behalf_of: true,
                    has_active_membership: has_active,
                };
            }
            ResolvedPrincipal {
                principal_type: "agent",
                principal_id: *agent_id,
                memberships: Vec::new(),
                on_behalf_of: false,
                has_active_membership: false,
            }
        }
        AuthorizationActor::None => ResolvedPrincipal {
            principal_type: "anonymous",
            principal_id: Uuid::nil(),
            memberships: Vec::new(),
            on_behalf_of: false,
            has_active_membership: false,
        },
    }
}

/// 从 CompanyMembership 提取角色。
fn membership_role(m: &CompanyMembership) -> Option<MembershipRole> {
    Some(m.role)
}

/// 便捷函数：执行决策并返回是否允许。
pub async fn decide_access(
    pool: &PgPool,
    actor: &AuthorizationActor,
    action: &AuthorizationAction,
    scope: Option<Uuid>,
) -> bool {
    AuthorizationService::decide(pool, actor, action, scope)
        .await
        .allowed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::actor::AuthorizationActor;

    #[test]
    fn test_trust_preset_agent_issue_mention_is_low() {
        let agent = AuthorizationActor::agent(Uuid::new_v4(), Uuid::new_v4(), None);
        let action = AuthorizationAction::IssueMention {
            issue_id: Uuid::new_v4(),
            mentioned_agent_id: Uuid::new_v4(),
        };
        let resolution = TrustPresetResolver::resolve_core_trust_preset(&agent, &action, None);
        assert_eq!(resolution.preset, TrustPreset::Low);
        assert!(resolution.requires_explicit_grant);
    }

    #[test]
    fn test_trust_preset_board_is_high() {
        let board = AuthorizationActor::board(Uuid::new_v4(), Uuid::new_v4());
        let action = AuthorizationAction::IssueRead {
            issue_id: Uuid::new_v4(),
        };
        let resolution =
            TrustPresetResolver::resolve_core_trust_preset(&board, &action, Some(Uuid::new_v4()));
        assert_eq!(resolution.preset, TrustPreset::High);
        assert!(!resolution.requires_explicit_grant);
    }

    #[test]
    fn test_role_default_permissions() {
        let owner = RolePermissions::default_permissions_for_role(MembershipRole::Owner);
        assert!(owner.iter().any(|k| k.as_str() == PermissionKey::COMPANY_DELETE));

        let viewer = RolePermissions::default_permissions_for_role(MembershipRole::Viewer);
        assert!(viewer.iter().any(|k| k.as_str() == PermissionKey::ISSUES_READ));
        assert!(!viewer.iter().any(|k| k.as_str() == PermissionKey::ISSUES_WRITE));
    }

    #[test]
    fn test_role_has_permission() {
        assert!(role_has_permission(
            MembershipRole::Owner,
            &PermissionKey::from_const(PermissionKey::AGENTS_DELETE)
        ));
        assert!(!role_has_permission(
            MembershipRole::Viewer,
            &PermissionKey::from_const(PermissionKey::AGENTS_DELETE)
        ));
    }

    #[test]
    fn test_permission_key_for_action() {
        let action = AuthorizationAction::IssueWrite {
            issue_id: Uuid::new_v4(),
        };
        assert_eq!(
            permission_key_for_action(&action),
            Some(PermissionKey::from_const(PermissionKey::ISSUES_WRITE))
        );
    }
}
