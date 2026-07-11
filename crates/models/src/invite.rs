use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Invite type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "invite_type", rename_all = "snake_case")]
pub enum InviteType {
    CompanyJoin,
    BootstrapCeo,
}

/// Allowed join types for a company
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "allowed_join_types", rename_all = "snake_case")]
pub enum AllowedJoinTypes {
    Human,
    Agent,
    Both,
}

/// Join request status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "join_request_status", rename_all = "snake_case")]
pub enum JoinRequestStatus {
    PendingApproval,
    Approved,
    Rejected,
}

/// Invite model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Invite {
    pub id: Uuid,
    pub company_id: Uuid,
    pub invite_type: InviteType,
    pub invited_email: Option<String>,
    pub invited_by_user_id: Option<Uuid>,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub accepted: bool,
    pub accepted_by_user_id: Option<Uuid>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Join request model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JoinRequest {
    pub id: Uuid,
    pub company_id: Uuid,
    pub requester_user_id: Uuid,
    pub status: JoinRequestStatus,
    pub message: Option<String>,
    pub reviewed_by_user_id: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Invite {
    pub fn new(company_id: Uuid, invite_type: InviteType, invited_by_user_id: Option<Uuid>) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let token: String = (0..32)
            .map(|_| format!("{:02x}", rng.gen::<u8>()))
            .collect();

        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            invite_type,
            invited_email: None,
            invited_by_user_id,
            token,
            expires_at: now + chrono::Duration::days(7),
            accepted: false,
            accepted_by_user_id: None,
            accepted_at: None,
            created_at: now,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn is_valid(&self) -> bool {
        !self.accepted && !self.is_expired()
    }

    pub fn accept(&mut self, user_id: Uuid) {
        self.accepted = true;
        self.accepted_by_user_id = Some(user_id);
        self.accepted_at = Some(Utc::now());
    }
}

impl JoinRequest {
    pub fn new(company_id: Uuid, requester_user_id: Uuid, message: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            requester_user_id,
            status: JoinRequestStatus::PendingApproval,
            message,
            reviewed_by_user_id: None,
            reviewed_at: None,
            rejection_reason: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn approve(&mut self, reviewer_id: Uuid) {
        self.status = JoinRequestStatus::Approved;
        self.reviewed_by_user_id = Some(reviewer_id);
        self.reviewed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn reject(&mut self, reviewer_id: Uuid, reason: String) {
        self.status = JoinRequestStatus::Rejected;
        self.reviewed_by_user_id = Some(reviewer_id);
        self.reviewed_at = Some(Utc::now());
        self.rejection_reason = Some(reason);
        self.updated_at = Utc::now();
    }

    pub fn is_pending(&self) -> bool {
        self.status == JoinRequestStatus::PendingApproval
    }
}
