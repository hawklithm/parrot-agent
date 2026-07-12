//! Board 认证辅助：根据 Board API Key / 会话解析用户身份与公司成员关系。
//!
//! 对应任务拆解 §5 阶段二 `resolve_board_access` 与 §6 `load_responsible_user_memberships`。

use sqlx::PgPool;
use uuid::Uuid;

use repositories::auth_repositories::{
    AuthUserRepository, CompanyMembershipRepository, PgAuthUserRepository,
    PgCompanyMembershipRepository,
};
use repositories::models::auth::AuthUser;

use crate::auth::membership::CompanyMembership;
use crate::auth::{AuthError, AuthResult};

/// 解析 Board 用户的完整访问上下文。
///
/// 返回：(用户信息, 公司成员关系列表, 是否实例管理员)
pub async fn resolve_board_access(
    pool: &PgPool,
    user_id: Uuid,
) -> AuthResult<(AuthUser, Vec<CompanyMembership>, bool)> {
    let user_repo = PgAuthUserRepository::new(pool.clone());
    let user = user_repo.find_by_id(user_id).await.map_err(|e| {
        AuthError::Internal {
            message: format!("Failed to load auth user {}: {}", user_id, e),
        }
    })?
    .ok_or_else(|| AuthError::InvalidApiKey {
        reason: format!("User {} not found", user_id),
    })?;

    let is_instance_admin = resolve_instance_admin(pool, user_id).await?;

    let membership_repo = PgCompanyMembershipRepository::new(pool.clone());
    let memberships = membership_repo
        .list_by_principal("user", user_id)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to load memberships: {}", e),
        })?
        .into_iter()
        .map(CompanyMembership::from_row)
        .collect();

    Ok((user, memberships, is_instance_admin))
}

/// 查询用户是否为实例管理员（查询 instance_user_roles 表）。
pub async fn resolve_instance_admin(pool: &PgPool, user_id: Uuid) -> AuthResult<bool> {
    let is_admin: Option<bool> = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM instance_user_roles WHERE user_id = $1 AND role = 'instance_admin')",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::Internal {
        message: format!("Failed to check instance admin: {}", e),
    })?;

    Ok(is_admin.unwrap_or(false))
}

/// 加载 Agent 的 responsible user 的成员关系（对应 §6 阶段二）。
///
/// 返回该 responsible user 在指定公司内的活跃成员关系。
pub async fn load_responsible_user_memberships(
    pool: &PgPool,
    responsible_user_id: Uuid,
    company_id: Uuid,
) -> AuthResult<Vec<CompanyMembership>> {
    let membership_repo = PgCompanyMembershipRepository::new(pool.clone());
    let memberships = membership_repo
        .list_by_principal("user", responsible_user_id)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to load memberships: {}", e),
        })?
        .into_iter()
        .map(CompanyMembership::from_row)
        .filter(|m| m.company_id == company_id && m.status.is_active())
        .collect();

    Ok(memberships)
}
