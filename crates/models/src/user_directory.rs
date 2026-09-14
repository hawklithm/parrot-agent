use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User profile information for directory entries
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// Company user directory entry (active members only)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyUserDirectoryEntry {
    pub principal_id: Uuid,
    pub status: String, // "active"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserProfile>,
}

/// Response for company user directory listing.
///
/// Paperclip（`access.ts:4465-4470`）返回 `{ users }`，前端
/// `parrot-web-ui/src/api/access.ts:156` 亦只声明 `users`，无分页字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyUserDirectoryResponse {
    pub users: Vec<CompanyUserDirectoryEntry>,
}

/// Admin user directory entry (instance-wide).
///
/// `user` 为对外用户资料投影，展开后即 `{id,email,name,image}`，与 Paperclip
/// `{...toUserProfile(user), isInstanceAdmin, activeCompanyMembershipCount}` 同形。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserDirectoryEntry {
    #[serde(flatten)]
    pub user: UserProfile,
    pub is_instance_admin: bool,
    pub active_company_membership_count: i32,
}

/// Response for admin user directory listing.
///
/// Paperclip（`access.ts:4783-4841`）返回**裸数组**，前端
/// `parrot-web-ui/src/api/access.ts:403` 亦声明为数组，故此处同样只承载数组。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserDirectoryResponse {
    pub users: Vec<AdminUserDirectoryEntry>,
}

/// Query parameters for instance admin user search.
///
/// 对齐 Paperclip `searchAdminUsersQuerySchema`：只有 `query`，无分页参数
/// （结果集固定截断到前 50 条）。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserDirectoryQuery {
    #[serde(default)]
    pub query: String,
}

/// 实例管理员用户搜索返回的条数上限（对齐 Paperclip `filteredUsers.slice(0, 50)`）。
pub const ADMIN_USER_DIRECTORY_LIMIT: usize = 50;

