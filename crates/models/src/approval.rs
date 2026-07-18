use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "approval_type", rename_all = "snake_case")]
pub enum ApprovalType {
    HireAgent,
    SpendCredits,
    CreateResource,
    DeployAgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "approval_status", rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    RevisionRequested,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: Uuid,
    pub company_id: Uuid,
    pub approval_type: ApprovalType,
    pub requested_by_agent_id: Option<Uuid>,
    pub requested_by_user_id: Option<Uuid>,
    pub status: ApprovalStatus,
    pub payload: JsonValue,
    pub decision_note: Option<String>,
    pub decided_by_user_id: Option<Uuid>,
    pub decided_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct IssueApproval {
    pub id: Uuid,
    pub approval_id: Uuid,
    pub issue_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateApprovalInput {
    pub company_id: Uuid,
    pub approval_type: ApprovalType,
    pub requested_by_agent_id: Option<Uuid>,
    pub requested_by_user_id: Option<Uuid>,
    pub payload: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionInput {
    pub status: ApprovalStatus,
    pub decision_note: Option<String>,
    pub decided_by_user_id: Uuid,
}

impl Approval {
    pub fn new(input: CreateApprovalInput) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id: input.company_id,
            approval_type: input.approval_type,
            requested_by_agent_id: input.requested_by_agent_id,
            requested_by_user_id: input.requested_by_user_id,
            status: ApprovalStatus::Pending,
            payload: input.payload,
            decision_note: None,
            decided_by_user_id: None,
            decided_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn decide(&mut self, input: DecisionInput) {
        self.status = input.status;
        self.decision_note = input.decision_note;
        self.decided_by_user_id = Some(input.decided_by_user_id);
        self.decided_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn is_pending(&self) -> bool {
        self.status == ApprovalStatus::Pending
    }

    pub fn is_approved(&self) -> bool {
        self.status == ApprovalStatus::Approved
    }

    pub fn is_rejected(&self) -> bool {
        self.status == ApprovalStatus::Rejected
    }
}

impl IssueApproval {
    pub fn new(approval_id: Uuid, issue_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            approval_id,
            issue_id,
        }
    }
}
