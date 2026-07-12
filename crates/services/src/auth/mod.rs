/// 认证授权模块 - 统一的认证授权类型系统
///
/// 核心组件：
/// - actor: Actor类型系统（AuthorizationActor, ActorSource, AgentApiKeyScope）
/// - decision: 授权决策类型（AuthorizationAction, AuthorizationDecision, DecisionReason）
/// - error: 统一错误类型（AuthError, AuthResult）
/// - membership: 成员与角色类型（MembershipRole, PrincipalType, MembershipStatus）
/// - permission: 权限授予类型（PermissionKey, PermissionGrant, GrantScope）
/// - invite: 邀请与加入类型（InviteType, AllowedJoinTypes, JoinRequestStatus）

pub mod actor;
pub mod board_access;
pub mod decision;
pub mod error;
pub mod invite;
pub mod jwt;
pub mod membership;
pub mod middleware;
pub mod permission;

// 重新导出核心类型
pub use actor::{ActorSource, AgentApiKeyScope, AuthorizationActor};
pub use board_access::{
    load_responsible_user_memberships, resolve_board_access, resolve_instance_admin,
};
pub use decision::{AuthorizationAction, AuthorizationDecision, DecisionReason};
pub use error::{AuthError, AuthResult};
pub use invite::{AllowedJoinTypes, Invite, InviteType, JoinRequest, JoinRequestStatus};
pub use jwt::{JwtConfig, LocalAgentJwtClaims, verify_local_agent_jwt};
pub use membership::{CompanyMembership, MembershipRole, MembershipStatus, PrincipalType};
pub use middleware::{
    ActorResolver, AuthMiddleware, AuthMode, BearerTokenResolver, CloudTenantHeaderResolver,
    SessionCookieResolver, auth_middleware_fn, authenticated_middleware, extract_actor,
    local_trusted_middleware,
};
pub use permission::{GrantScope, PermissionGrant, PermissionKey};
