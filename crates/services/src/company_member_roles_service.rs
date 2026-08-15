/// Company Member Roles Service
/// 
/// Company 成员角色管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CompanyMemberRolesError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type CompanyMemberRolesResult<T> = Result<T, CompanyMemberRolesError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
    Guest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberRoleAssignment {
    pub id: Uuid,
    pub company_id: Uuid,
    pub user_id: Uuid,
    pub role: MemberRole,
    pub assigned_by: Uuid,
    pub assigned_at: chrono::DateTime<chrono::Utc>,
}

pub struct CompanyMemberRolesService {
    pool: PgPool,
}

impl CompanyMemberRolesService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn assign_role(
        &self,
        company_id: Uuid,
        user_id: Uuid,
        role: MemberRole,
        assigned_by: Uuid,
    ) -> CompanyMemberRolesResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO company_member_roles 
            (id, company_id, user_id, role, assigned_by, assigned_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (company_id, user_id)
            DO UPDATE SET role = $4, assigned_by = $5, assigned_at = $6
            RETURNING id
            "#
        )
        .bind(id)
        .bind(company_id)
        .bind(user_id)
        .bind(format!("{:?}", role))
        .bind(assigned_by)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_user_role(
        &self,
        company_id: Uuid,
        user_id: Uuid,
    ) -> CompanyMemberRolesResult<Option<MemberRole>> {
        let row = sqlx::query(
            r#"
            SELECT role
            FROM company_member_roles
            WHERE company_id = $1 AND user_id = $2
            "#
        )
        .bind(company_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| parse_role(r.get("role"))))
    }
    
    pub async fn has_role(
        &self,
        company_id: Uuid,
        user_id: Uuid,
        required_role: MemberRole,
    ) -> CompanyMemberRolesResult<bool> {
        if let Some(user_role) = self.get_user_role(company_id, user_id).await? {
            Ok(role_hierarchy(&user_role) >= role_hierarchy(&required_role))
        } else {
            Ok(false)
        }
    }
    
    pub async fn list_company_members(
        &self,
        company_id: Uuid,
    ) -> CompanyMemberRolesResult<Vec<MemberRoleAssignment>> {
        let rows = sqlx::query(
            r#"
            SELECT id, company_id, user_id, role, assigned_by, assigned_at
            FROM company_member_roles
            WHERE company_id = $1
            ORDER BY assigned_at DESC
            "#
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        
        let assignments = rows.into_iter().map(|row| {
            MemberRoleAssignment {
                id: row.get("id"),
                company_id: row.get("company_id"),
                user_id: row.get("user_id"),
                role: parse_role(row.get("role")),
                assigned_by: row.get("assigned_by"),
                assigned_at: row.get("assigned_at"),
            }
        }).collect();
        
        Ok(assignments)
    }
    
    pub async fn remove_member(
        &self,
        company_id: Uuid,
        user_id: Uuid,
    ) -> CompanyMemberRolesResult<()> {
        sqlx::query(
            "DELETE FROM company_member_roles WHERE company_id = $1 AND user_id = $2"
        )
        .bind(company_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}

fn parse_role(s: &str) -> MemberRole {
    match s {
        "Owner" => MemberRole::Owner,
        "Admin" => MemberRole::Admin,
        "Member" => MemberRole::Member,
        "Guest" => MemberRole::Guest,
        _ => MemberRole::Guest,
    }
}

fn role_hierarchy(role: &MemberRole) -> i32 {
    match role {
        MemberRole::Owner => 4,
        MemberRole::Admin => 3,
        MemberRole::Member => 2,
        MemberRole::Guest => 1,
    }
}
