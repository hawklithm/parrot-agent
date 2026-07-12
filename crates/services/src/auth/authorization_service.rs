use uuid::Uuid;

use repositories::auth_repositories::PrincipalPermissionGrantRepository;
use sqlx::PgPool;

use super::{AuthError, AuthResult, AuthorizationActor, MembershipRole};

/// 授权决策引擎 - 核心守卫函数
///
/// 提供统一的授权检查接口，处理跨公司访问、角色权限、实例管理员等场景

/// 断言Actor有权访问指定公司
///
/// 检查规则：
/// 1. actor.type = none -> 401 Unauthenticated
/// 2. Agent跨公司访问 -> 403 Forbidden
/// 3. Board用户跨公司访问 -> 403 Forbidden
/// 4. Viewer角色执行写操作 -> 403 Forbidden
pub fn assert_company_access(
    actor: &AuthorizationActor,
    company_id: Uuid,
    is_write_op: bool,
) -> AuthResult<()> {
    // 检查是否已认证
    if actor.is_anonymous() {
        return Err(AuthError::unauthenticated("Authentication required"));
    }

    // 获取Actor所属公司
    let actor_company = actor.company_id().ok_or_else(|| {
        AuthError::forbidden("Actor has no company association")
    })?;

    // 检查跨公司访问
    if actor_company != company_id {
        return Err(AuthError::forbidden_with_code(
            format!(
                "Cross-company access denied: actor company {} != resource company {}",
                actor_company, company_id
            ),
            "auth.cross_company_access",
        ));
    }

    // 如果是写操作，检查角色权限
    if is_write_op {
        // Agent 默认允许写操作（由其他权限检查控制）
        if let AuthorizationActor::Board { .. } = actor {
            match actor.role_in(company_id) {
                Some(role) if role.can_update_resources() => {}
                Some(_) => {
                    return Err(AuthError::forbidden_with_code(
                        "Role does not permit write operations in this company",
                        "auth.role_no_write",
                    ));
                }
                None => {
                    return Err(AuthError::forbidden_with_code(
                        "No active membership in this company",
                        "auth.no_membership",
                    ));
                }
            }
        }
    }

    Ok(())
}

/// 断言Actor是实例管理员
///
/// 检查规则：
/// 1. actor.type = none -> 401 Unauthenticated
/// 2. actor.is_instance_admin = false -> 403 Forbidden
pub fn assert_instance_admin(actor: &AuthorizationActor) -> AuthResult<()> {
    // 检查是否已n    if actor.is_anonymous() {
        return Err(AuthError::unauthenticated("Authentication required"));
    }

    // 检查是否为Board用户（Agent不能是实例管理员）
    if !actor.is_board() {
        return Err(AuthError::forbidden_with_code(
            "Only Board users can be instance administrators",
            "auth.not_board_user",
        ));
    }

    // 检查实例管理员标志
    if !actor.is_instance_admin() {
        return Err(AuthError::forbidden_with_code(
            "Actor is not an instance administrator",
            "auth.not_instance_admin",
        ));
    }

    Ok(())
}

/// 断言Actor有特定角色权限
///
/// 检查规则：
/// 1. 获取Actor在指定公司的成员关系
/// 2. 验证角色是否满足required_role的权限级别
pub fn assert_role(
    actor: &AuthorizationActor,
    company_id: Uuid,
    required_role: MembershipRole,
) -> AuthResult<()> {
    // 先检查公司访问权限
    assert_company_access(actor, company_id, false)?;

    let role = actor.role_in(company_id).ok_or_else(|| {
        AuthError::forbidden_with_code("No active membership in this company", "auth.no_membership")
    })?;

    if !role.has_privilege(&required_role) {
        return Err(AuthError::forbidden_with_code(
            format!(
                "Role {:?} does not meet required role {:?}",
                role, required_role
            ),
            "auth.role_insufficient",
        ));
    }

    Ok(())
}

/// 断言Actor可以执行写操作
///
/// 检查规则：
/// 1. 检查公司访问权限
/// 2. 检查角色是否允许写操作（Owner/Admin/Operator）
pub fn assert_can_write(
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> AuthResult<()> {
    assert_company_access(actor, company_id, true)?;

    // assert_company_access 内部已校验 Board 用户的写角色权限

    Ok(())
}

/// 断言Actor可以管理成员
///
/// 检查规则：
/// 1. 检查公司访问权限
/// 2. 检查角色是否允许管理成员（Owner/Admin）
pub fn assert_can_manage_members(
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> AuthResult<()> {
    assert_company_access(actor, company_id, false)?;

    let role = actor.role_in(company_id).ok_or_else(|| {
        AuthError::forbidden_with_code("No active membership in this company", "auth.no_membership")
    })?;

    if !role.can_manage_members() {
        return Err(AuthError::forbidden_with_code(
            format!("Role {:?} cannot manage members", role),
            "auth.role_no_manage",
        ));
    }

    Ok(())
}

/// 断言Actor可以删除资源
///
/// 检查规则：
/// 1. 检查公司访问权限
/// 2. 检查角色是否允许删除资源（Owner/Admin）
pub fn assert_can_delete(
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> AuthResult<()> {
    assert_company_access(actor, company_id, true)?;

    let role = actor.role_in(company_id).ok_or_else(|| {
        AuthError::forbidden_with_code("No active membership in this company", "auth.no_membership")
    })?;

    if !role.can_delete_resources() {
        return Err(AuthError::forbidden_with_code(
            format!("Role {:?} cannot delete resources", role),
            "auth.role_no_delete",
        ));
    }

    Ok(())
}

/// 断言 Actor 在公司内拥有指定权限键的显式授予。
///
/// 检查规则：
/// 1. 先调用 `assert_company_access` 校验公司边界
/// 2. 根据 actor 类型推导 principal_type / principal_id
///    （Board -> user / Agent -> agent）
/// 3. 查询 `principal_permission_grants` 是否存在**有效**（未过期）授予
/// 4. 无有效授予时返回 403 Forbidden
pub async fn assert_company_permission(
    pool: &PgPool,
    actor: &AuthorizationActor,
    company_id: Uuid,
    permission_key: &str,
) -> AuthResult<()> {
    // 公司边界校验
    assert_company_access(actor, company_id, false)?;

    // 推导主体类型与 ID
    let (principal_type, principal_id) = match actor {
        AuthorizationActor::Board { user_id, .. } => ("user", *user_id),
        AuthorizationActor::Agent { agent_id, .. } => ("agent", *agent_id),
        AuthorizationActor::None => {
            return Err(AuthError::unauthenticated("Authentication required"));
        }
    };

    let repo = repositories::auth_repositories::PgPrincipalPermissionGrantRepository::new(pool.clone());
    let grant = repo
        .find_valid_grant(company_id, principal_type, principal_id, permission_key)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to query permission grant: {}", e),
        })?;

    match grant {
        Some(_) => Ok(()),
        None => Err(AuthError::forbidden_with_code(
            format!(
                "Missing permission grant '{}' for principal {} in company {}",
                permission_key, principal_id, company_id
            ),
            "auth.no_permission_grant",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::membership::{CompanyMembership, PrincipalType};

    #[test]
    fn test_assert_company_access_anonymous() {
        let actor = AuthorizationActor::none();
        let company_id = Uuid::new_v4();

        let result = assert_company_access(&actor, company_id, false);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AuthError::Unauthenticated { .. }));
    }

    #[test]
    fn test_assert_company_access_cross_company() {
        let actor_company = Uuid::new_v4();
        let resource_company = Uuid::new_v4();
        let actor = AuthorizationActor::board(Uuid::new_v4(), actor_company);

        let result = assert_company_access(&actor, resource_company, false);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AuthError::Forbidden { .. }));
    }

    #[test]
    fn test_assert_company_access_same_company() {
        let company_id = Uuid::new_v4();
        let actor = AuthorizationActor::board(Uuid::new_v4(), company_id);

        let result = assert_company_access(&actor, company_id, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_assert_instance_admin_anonymous() {
        let actor = AuthorizationActor::none();

        let result = assert_instance_admin(&actor);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AuthError::Unauthenticated { .. }));
    }

    #[test]
    fn test_assert_instance_admin_agent() {
        let actor = AuthorizationActor::agent(Uuid::new_v4(), Uuid::new_v4(), None);

        let result = assert_instance_admin(&actor);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AuthError::Forbidden { .. }));
    }

    #[test]
    fn test_assert_role_anonymous() {
        let actor = AuthorizationActor::none();
        let company_id = Uuid::new_v4();

        let result = assert_role(&actor, company_id, MembershipRole::Viewer);
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_can_write_viewer_denied() {
        let company_id = Uuid::new_v4();
        let membership = CompanyMembership::new(
            company_id,
            PrincipalType::User,
            Uuid::new_v4(),
            MembershipRole::Viewer,
        );
        let actor = AuthorizationActor::board_with_memberships(
            Uuid::new_v4(),
            company_id,
            vec![membership],
            false,
        );

        let result = assert_can_write(&actor, company_id);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AuthError::Forbidden { .. }));
    }

    #[test]
    fn test_assert_can_write_operator_ok() {
        let company_id = Uuid::new_v4();
        let membership = CompanyMembership::new(
            company_id,
            PrincipalType::User,
            Uuid::new_v4(),
            MembershipRole::Operator,
        );
        let actor = AuthorizationActor::board_with_memberships(
            Uuid::new_v4(),
            company_id,
            vec![membership],
            false,
        );

        assert!(assert_can_write(&actor, company_id).is_ok());
    }

    #[test]
    fn test_assert_can_manage_members_owner() {
        let company_id = Uuid::new_v4();
        let membership = CompanyMembership::new(
            company_id,
            PrincipalType::User,
            Uuid::new_v4(),
            MembershipRole::Owner,
        );
        let actor = AuthorizationActor::board_with_memberships(
            Uuid::new_v4(),
            company_id,
            vec![membership],
            false,
        );

        assert!(assert_can_manage_members(&actor, company_id).is_ok());
        assert!(assert_can_delete(&actor, company_id).is_ok());
    }

    #[test]
    fn test_assert_can_manage_members_operator_denied() {
        let company_id = Uuid::new_v4();
        let membership = CompanyMembership::new(
            company_id,
            PrincipalType::User,
            Uuid::new_v4(),
            MembershipRole::Operator,
        );
        let actor = AuthorizationActor::board_with_memberships(
            Uuid::new_v4(),
            company_id,
            vec![membership],
            false,
        );

        assert!(assert_can_manage_members(&actor, company_id).is_err());
        assert!(assert_can_delete(&actor, company_id).is_err());
    }

    #[test]
    fn test_assert_instance_admin_flag() {
        let company_id = Uuid::new_v4();
        let actor = AuthorizationActor::board_with_memberships(
            Uuid::new_v4(),
            company_id,
            vec![],
            true,
        );
        assert!(assert_instance_admin(&actor).is_ok());

        let non_admin = AuthorizationActor::board_with_memberships(
            Uuid::new_v4(),
            company_id,
            vec![],
            false,
        );
        assert!(assert_instance_admin(&non_admin).is_err());
    }

    #[test]
    fn test_assert_role_insufficient() {
        let company_id = Uuid::new_v4();
        let membership = CompanyMembership::new(
            company_id,
            PrincipalType::User,
            Uuid::new_v4(),
            MembershipRole::Viewer,
        );
        let actor = AuthorizationActor::board_with_memberships(
            Uuid::new_v4(),
            company_id,
            vec![membership],
            false,
        );

        assert!(assert_role(&actor, company_id, MembershipRole::Admin).is_err());
        assert!(assert_role(&actor, company_id, MembershipRole::Viewer).is_ok());
    }
}
