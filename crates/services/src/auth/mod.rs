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
pub mod audit;
pub mod authorization_service;
pub mod board_access;
pub mod board_claim;
pub mod cli_auth;
pub mod decision;
pub mod decision_engine;
pub mod error;
pub mod invite;
pub mod jwt;
pub mod key_rotation;
pub mod membership;
pub mod middleware;
pub mod permission;
pub mod rate_limiter;

// 重新导出核心类型
pub use actor::{ActorSource, AgentApiKeyScope, AuthorizationActor};
pub use board_access::{
    load_responsible_user_memberships, resolve_board_access, resolve_instance_admin,
};
pub use board_claim::{BoardClaimService, ClaimChallenge};
pub use cli_auth::{
    approve_cli_auth_challenge, cancel_cli_auth_challenge, create_cli_auth_challenge,
    get_cli_auth_challenge,
};
pub use decision::{AuthorizationAction, AuthorizationDecision, DecisionReason};
pub use decision_engine::{
    AuthorizationService, RolePermissions, TrustPreset, TrustPresetResolver,
    check_explicit_grants, check_issue_mention_grant, check_manager_chain, decide_access,
    role_has_permission,
};
pub use error::{AuthError, AuthResult};
pub use invite::{AllowedJoinTypes, Invite, InviteType, JoinRequest, JoinRequestStatus};
pub use jwt::{JwtConfig, LocalAgentJwtClaims, verify_local_agent_jwt};
pub use membership::{CompanyMembership, MembershipRole, MembershipStatus, PrincipalType};
pub use middleware::{
    ActorResolver, AuthMiddleware, AuthMode, BearerTokenResolver, CloudTenantHeaderResolver,
    SessionCookieResolver, auth_middleware_fn, authenticated_middleware, extract_actor,
    local_trusted_middleware, middleware_from_env, require_agent, require_board,
    auth_cookie_prefix, auth_trusted_origins,
};
pub use permission::{GrantScope, PermissionGrant, PermissionKey};
