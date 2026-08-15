/// Git Credentials Service
/// 
/// Git 凭证管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum GitCredentialsError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("credential not found: {0}")]
    NotFound(Uuid),
}

pub type GitCredentialsResult<T> = Result<T, GitCredentialsError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCredential {
    pub id: Uuid,
    pub user_id: Uuid,
    pub credential_type: CredentialType,
    pub encrypted_token: String,
    pub repository_pattern: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CredentialType {
    PersonalAccessToken,
    SSHKey,
    OAuth,
}

pub struct GitCredentialsService {
    pool: PgPool,
}

impl GitCredentialsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn store_credential(
        &self,
        user_id: Uuid,
        credential_type: CredentialType,
        encrypted_token: String,
        repository_pattern: Option<String>,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> GitCredentialsResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO git_credentials 
            (id, user_id, credential_type, encrypted_token, repository_pattern, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(user_id)
        .bind(format!("{:?}", credential_type))
        .bind(&encrypted_token)
        .bind(&repository_pattern)
        .bind(chrono::Utc::now())
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_credential(
        &self,
        user_id: Uuid,
        repository: &str,
    ) -> GitCredentialsResult<Option<GitCredential>> {
        let row = sqlx::query(
            r#"
            SELECT id, user_id, credential_type, encrypted_token, repository_pattern, created_at, expires_at
            FROM git_credentials
            WHERE user_id = $1
              AND (repository_pattern IS NULL OR $2 LIKE repository_pattern)
              AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY created_at DESC
            LIMIT 1
            "#
        )
        .bind(user_id)
        .bind(repository)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| GitCredential {
            id: r.get("id"),
            user_id: r.get("user_id"),
            credential_type: parse_type(r.get("credential_type")),
            encrypted_token: r.get("encrypted_token"),
            repository_pattern: r.get("repository_pattern"),
            created_at: r.get("created_at"),
            expires_at: r.get("expires_at"),
        }))
    }
    
    pub async fn revoke_credential(&self, id: Uuid) -> GitCredentialsResult<()> {
        sqlx::query("DELETE FROM git_credentials WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}

fn parse_type(s: &str) -> CredentialType {
    match s {
        "PersonalAccessToken" => CredentialType::PersonalAccessToken,
        "SSHKey" => CredentialType::SSHKey,
        "OAuth" => CredentialType::OAuth,
        _ => CredentialType::PersonalAccessToken,
    }
}
