use uuid::Uuid;

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
        // Agent默认允许写操作（由其他权限检查控制）
        if let AuthorizationActor::Board { .. } = actor {
            // Board用户需要检查成员资格和角色
            // TODO: 从memberships中获取角色并验证
            // 当前简化实现：假设已在中间件中加载了memberships
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

    // TODO: 检查is_instance_admin标志
    // 当前简化实现：需要从actor中获取is_instance_admin字段
    // 这需要扩展AuthorizationActor结构体

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

    // TODO: 从actor的memberships中查找对应公司的角色
    // 当前简化实现：需要在ActorResolver中加载memberships

    // TODO: 使用MembershipRole::has_privilege()检查权限级别

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

    // TODO: 检查角
    // 使用 role.can_update_resources() 或 role.can_create_resources()

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

    // TODO: 检查角色是否允许管理成员
    // 使用 role.can_manage_members()

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

    // TODO: 检查角色是否允许删除资源
    // 使用 role.can_delete_resources()

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_assert_can_write() {
        let company_id = Uuid::new_v4();
        let actor = AuthorizationActor::board(Uuid::new_v4(), company_id);

        let result = assert_can_write(&actor, company_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_assert_can_manage_members() {
        let company_id = Uuid::new_v4();
        let actor = AuthorizationActor::board(Uuid::new_v4(), company_id);

        let result = assert_can_manage_members(&actor, company_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_assert_can_delete() {
        let company_id = Uuid::new_v4();
        let actor = AuthorizationActor::board(Uuid::new_v4(), company_id);

        let result = assert_can_delete(&actor, company_id);
        assert!(result.is_ok());
    }
}
