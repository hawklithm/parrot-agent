/// 数据模型模块 - 数据库表结构映射
///
/// 包含所有数据库表的Rust结构体定义，支持sqlx::FromRow自动映射

pub mod auth;
pub mod auth_keys;
pub mod authorization;
pub mod invite;

// 重新导出核心类型
pub use auth::{AuthSession, AuthUser, Company, InstanceUserRole};
pub use auth_keys::{AgentApiKey, BoardApiKey, CliAuthChallenge};
pub use authorization::{CompanyMembershipRow, PrincipalPermissionGrantRow};
pub use invite::{InviteRow, JoinRequestRow};
