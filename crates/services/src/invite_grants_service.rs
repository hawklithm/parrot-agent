/// Invite Grants Service
/// 
/// 邀请授权管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum InviteGrantsError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("invalid grant: {0}")]
    InvalidGrant(String),
}

pub type InviteGrantsResult<T> = Result<T, InviteGrantsError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteGrant {
    pub id: Uuid,
    pub company_id: Uuid,
    pub inviter_id: Uuid,
    pub invitee_email: String,
    pub granted_permissions: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct InviteGrantsService {
    pool: PgPool,
}

impl InviteGrantsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_grant(
        &self,
        company_id: Uuid,
        inviter_id: Uuid,
        invitee_email: String,
        granted_permissions: Vec<String>,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> InviteGrantsResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO invite_grants 
            (id, company_id, inviter_id, invitee_email, granted_permissions, expires_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(company_id)
        .bind(inviter_id)
        .bind(&invitee_email)
        .bind(&granted_permissions)
        .bind(expires_at)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_grant(&self, id: Uuid) -> InviteGrantsResult<Option<InviteGrant>> {
        let row = sqlx::query(
            r#"
            SELECT id, company_id, inviter_id, invitee_email, granted_permissions, expires_at, created_at
            FROM invite_grants
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| InviteGrant {
            id: r.get("id"),
            company_id: r.get("company_id"),
            inviter_id: r.get("inviter_id"),
            invitee_email: r.get("invitee_email"),
            granted_permissions: r.get("granted_permissions"),
            expires_at: r.get("expires_at"),
            created_at: r.get("created_at"),
        }))
    }
    
    pub async fn revoke_grant(&self, id: Uuid) -> InviteGrantsResult<()> {
        sqlx::query("DELETE FROM invite_grants WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}
