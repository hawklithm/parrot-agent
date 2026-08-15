/// Change Consent Gate Service
/// 
/// 变更同意门控管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ChangeConsentError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("consent required: {0}")]
    ConsentRequired(String),
    
    #[error("consent denied: {0}")]
    ConsentDenied(String),
}

pub type ChangeConsentResult<T> = Result<T, ChangeConsentError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRequest {
    pub id: Uuid,
    pub change_type: String,
    pub description: String,
    pub requested_by: Uuid,
    pub requested_at: chrono::DateTime<chrono::Utc>,
    pub status: ConsentStatus,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConsentStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

pub struct ChangeConsentGateService {
    pool: PgPool,
    auto_approve_threshold: i32,
}

impl ChangeConsentGateService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            auto_approve_threshold: 1,
        }
    }
    
    pub fn with_auto_approve_threshold(mut self, threshold: i32) -> Self {
        self.auto_approve_threshold = threshold;
        self
    }
    
    pub async fn request_consent(
        &self,
        change_type: String,
        description: String,
        requested_by: Uuid,
    ) -> ChangeConsentResult<Uuid> {
        let id = Uuid::new_v4();
        
        // 检查是否需要同意
        let risk_level = self.assess_risk(&change_type, &description).await?;
        
        let status = if risk_level <= self.auto_approve_threshold {
            ConsentStatus::Approved
        } else {
            ConsentStatus::Pending
        };
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO consent_requests 
            (id, change_type, description, requested_by, requested_at, status)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(&change_type)
        .bind(&description)
        .bind(requested_by)
        .bind(chrono::Utc::now())
        .bind(format!("{:?}", status))
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn check_consent(&self, consent_id: Uuid) -> ChangeConsentResult<ConsentStatus> {
        let row = sqlx::query(
            r#"
            SELECT status
            FROM consent_requests
            WHERE id = $1
            "#
        )
        .bind(consent_id)
        .fetch_one(&self.pool)
        .await?;
        
        let status_str: String = row.get("status");
        Ok(parse_status(&status_str))
    }
    
    pub async fn require_consent(&self, consent_id: Uuid) -> ChangeConsentResult<()> {
        let status = self.check_consent(consent_id).await?;
        
        match status {
            ConsentStatus::Approved => Ok(()),
            ConsentStatus::Pending => Err(ChangeConsentError::ConsentRequired(
                "Change is pending approval".to_string()
            )),
            ConsentStatus::Denied => Err(ChangeConsentError::ConsentDenied(
                "Change was denied".to_string()
            )),
            ConsentStatus::Expired => Err(ChangeConsentError::ConsentRequired(
                "Consent request has expired".to_string()
            )),
        }
    }
    
    pub async fn approve_consent(
        &self,
        consent_id: Uuid,
        reviewed_by: Uuid,
    ) -> ChangeConsentResult<()> {
        sqlx::query(
            r#"
            UPDATE consent_requests 
            SET status = 'Approved', reviewed_by = $1, reviewed_at = $2
            WHERE id = $3
            "#
        )
        .bind(reviewed_by)
        .bind(chrono::Utc::now())
        .bind(consent_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn deny_consent(
        &self,
        consent_id: Uuid,
        reviewed_by: Uuid,
    ) -> ChangeConsentResult<()> {
        sqlx::query(
            r#"
            UPDATE consent_requests 
            SET status = 'Denied', reviewed_by = $1, reviewed_at = $2
            WHERE id = $3
            "#
        )
        .bind(reviewed_by)
        .bind(chrono::Utc::now())
        .bind(consent_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn list_pending_consents(&self) -> ChangeConsentResult<Vec<ConsentRequest>> {
        let rows = sqlx::query(
            r#"
            SELECT id, change_type, description, requested_by, requested_at, 
                   status, reviewed_by, reviewed_at
            FROM consent_requests
            WHERE status = 'Pending'
            ORDER BY requested_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        let requests = rows.into_iter().map(|row| {
            ConsentRequest {
                id: row.get("id"),
                change_type: row.get("change_type"),
                description: row.get("description"),
                requested_by: row.get("requested_by"),
                requested_at: row.get("requested_at"),
                status: parse_status(row.get("status")),
                reviewed_by: row.get("reviewed_by"),
                reviewed_at: row.get("reviewed_at"),
            }
        }).collect();
        
        Ok(requests)
    }
    
    async fn assess_risk(&self, change_type: &str, _description: &str) -> ChangeConsentResult<i32> {
        // 简化的风险评估
        let risk = match change_type {
            "delete" => 5,
            "modify" => 3,
            "create" => 1,
            _ => 2,
        };
        
        Ok(risk)
    }
}

fn parse_status(s: &str) -> ConsentStatus {
    match s {
        "Pending" => ConsentStatus::Pending,
        "Approved" => ConsentStatus::Approved,
        "Denied" => ConsentStatus::Denied,
        "Expired" => ConsentStatus::Expired,
        _ => ConsentStatus::Pending,
    }
}
