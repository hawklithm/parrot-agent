use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "company_status", rename_all = "snake_case")]
pub enum CompanyStatus {
    Active,
    Paused,
    Archived,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Company {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub status: CompanyStatus,
    pub pause_reason: Option<String>,
    pub paused_at: Option<DateTime<Utc>>,
    pub issue_prefix: String,
    pub issue_counter: i32,
    pub budget_monthly_cents: Option<i64>,
    pub spent_monthly_cents: i64,
    pub attachment_max_bytes: i64,
    pub default_responsible_user_id: Option<Uuid>,
    pub require_board_approval_for_new_agents: bool,
    pub feedback_data_sharing_enabled: bool,
    pub feedback_data_sharing_consent_at: Option<DateTime<Utc>>,
    pub feedback_data_sharing_consent_by_user_id: Option<Uuid>,
    pub feedback_data_sharing_terms_version: Option<String>,
    pub brand_color: Option<String>,
    pub logo_asset_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCompanyInput {
    pub name: String,
    pub description: Option<String>,
    pub issue_prefix: String,
    pub budget_monthly_cents: Option<i64>,
    pub attachment_max_bytes: Option<i64>,
    pub default_responsible_user_id: Option<Uuid>,
    pub require_board_approval_for_new_agents: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCompanyInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<CompanyStatus>,
    pub pause_reason: Option<String>,
    pub budget_monthly_cents: Option<i64>,
    pub attachment_max_bytes: Option<i64>,
    pub default_responsible_user_id: Option<Uuid>,
    pub require_board_approval_for_new_agents: Option<bool>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct CompanyStats {
    pub company_id: Uuid,
    pub total_projects: i64,
    pub total_agents: i64,
    pub total_issues: i64,
    pub spent_monthly_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "principal_type", rename_all = "snake_case")]
pub enum PrincipalType {
    User,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "membership_role", rename_all = "snake_case")]
pub enum MembershipRole {
    Owner,
    Member,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "company_membership_status", rename_all = "snake_case")]
pub enum CompanyMembershipStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct CompanyMembership {
    pub id: Uuid,
    pub company_id: Uuid,
    pub principal_type: PrincipalType,
    pub principal_id: Uuid,
    pub status: CompanyMembershipStatus,
    pub membership_role: MembershipRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
