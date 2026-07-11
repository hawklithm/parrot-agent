use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{ServiceError, ServiceResult};

/// Invite service trait with complete workflow support
#[async_trait]
pub trait InviteServiceComplete: Send + Sync {
    /// Verify invite token and return company info
    async fn verify_invite_token(&self, token: &str) -> ServiceResult<InviteInfo>;

    /// Accept invitation and join company
    async fn accept_invite(
        &self,
        token: &str,
        agent_id: Uuid,
        acceptance_data: AcceptanceData,
    ) -> ServiceResult<InviteAcceptanceResult>;

    /// Create join request for agent to join company
    async fn create_join_request(
        &self,
        agent_id: Uuid,
        company_id: Uuid,
        message: String,
    ) -> ServiceResult<JoinRequest>;

    /// Approve join request (admin action)
    async fn approve_join_request(
        &self,
        join_request_id: Uuid,
        approver_id: Uuid,
    ) -> ServiceResult<JoinRequestApprovalResult>;

    /// Reject join request (admin action)
    async fn reject_join_request(
        &self,
        join_request_id: Uuid,
        approver_id: Uuid,
        reason: String,
    ) -> ServiceResult<JoinRequestRejectionResult>;

    /// List pending join requests for company
    async fn list_pending_join_requests(
        &self,
        company_id: Uuid,
    ) -> ServiceResult<Vec<JoinRequest>>;

    /// Get company logo for invite
    async fn get_invite_logo(&self, token: &str) -> ServiceResult<Vec<u8>>;

    /// Get onboarding documentation
    async fn get_invite_onboarding(&self, token: &str) -> ServiceResult<String>;
}

/// Invite information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteInfo {
    pub company_id: Uuid,
    pub company_name: String,
    pub invite_type: InviteType,
    pub invited_by: Uuid,
   invited_by_name: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_uses: Option<i32>,
    pub current_uses: i32,
    pub metadata: serde_json::Value,
}

/// Invite type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InviteType {
    Agent,
    BoardUser,
    Collaborator,
}

/// Acceptance data from invitee
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceData {
    pub agent_name: String,
    pub agent_email: Option<String>,
    pub adapter_type: String,
    pub initial_config: serde_json::Value,
}

/// Invite acceptance result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteAcceptanceResult {
    pub success: bool,
    pub company_id: Uuid,
    pub agent_id: Uuid,
    pub membership_id: Uuid,
    pub onboarding_url: String,
    pub access_token: String,
}

/// Join request entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub message: String,
    pub status: JoinRequestStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
}

/// Join request status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinRequestStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

/// Join request approval result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequestApprovalResult {
    pub join_request_id: Uuid,
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub membership_id: Uuid,
    pub approved_at: DateTime<Utc>,
    pub notification_sent: bool,
}

/// Join request rejection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequestRejectionResult {
    pub join_request_id: Uuid,
    pub rejected_at: DateTime<Utc>,
    pub reason: String,
    pub notification_sent: bool,
}

/// Repository trait for invite operations
#[async_trait]
pub trait InviteRepository: Send + Sync {
    async fn find_by_token(&self, token: &str) -> Result<Option<InviteRecord>, String>;
    async fn increment_uses(&self, invite_id: Uuid) -> Result<(), String>;
    async fn create_membership(
        &self,
        agent_id: Uuid,
        company_id: Uuid,
    ) -> Result<Uuid, String>;
}

/// Repository trait for join requests
#[async_trait]
pub trait JoinRequestRepository: Send + Sync {
    async fn create(&self, join_request: JoinRequest) -> Result<JoinRequest, String>;
    async fn update(&self, join_request: JoinRequest) -> Result<JoinRequest, String>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<JoinRequest>, String>;
    async fn list_by_company(&self, comp) -> Result<Vec<JoinRequest>, String>;
}

/// Invite database record
#[derive(Debug, Clone)]
pub struct InviteRecord {
    pub id: Uuid,
    pub token: String,
    pub company_id: Uuid,
    pub company_name: String,
    pub invite_type: InviteType,
    pub invited_by: Uuid,
    pub invited_by_name: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_uses: Option<i32>,
    pub current_uses: i32,
    pub metadata: serde_json::Value,
}

/// Email notification service trait
#[async_trait]
pub trait EmailNotificationService: Send + Sync {
    async fn send_invite_acceptance_notification(
        &self,
        company_id: Uuid,
        agent_name: &str,
    ) -> Result<(), String>;

    async fn send_join_request_notification(
        &self,
        company_id: Uuid,
        agent_name: &str,
        message: &str,
    ) -> Result<(), String>;

    async fn send_join_approval_notification(
        &self,
        agent_id: Uuid,
        company_name: &str,
    ) -> Result<(), String>;

    async fn send_join_rejection_notification(
        &self,
        agent_id: Uuid,
        company_name: &str,
        reason: &str,
    ) -> Result<(), String>;
}

/// Default impion of InviteServiceComplete
pub struct DefaultInviteServiceComplete {
    invite_repo: Arc<dyn InviteRepository>,
    join_request_repo: Arc<dyn JoinRequestRepository>,
    email_service: Arc<dyn EmailNotificationService>,
}

impl DefaultInviteServiceComplete {
    pub fn new(
        invite_repo: Arc<dyn InviteRepository>,
        join_request_repo: Arc<dyn JoinRequestRepository>,
        email_service: Arc<dyn EmailNotificationService>,
    ) -> Self {
        Self {
            invite_repo,
            join_request_repo,
            email_service,
        }
    }

    /// Validate invite token and check expiration
    async fn validate_token(&self, token: &str) -> ServiceResult<InviteRecord> {
        if token.is_empty() {
            return Err(ServiceError::InvalidInput(
                "Invite token cannot be empty".to_string(),
            ));
        }

        let invite = self
            .invite_repo
            .find_by_token(token)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find invite: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Invite not found".to_string()))?;

        // Check expiration
        if let Some(expires_at) = invite.expires_at {
            if expires_at < Utc::now() {
                return Err(ServiceError::InvalidInput(
                    "Invite has expired".to_string(),
                ));
            }
        }

        // Check max uses
        if let Some(max_uses) = invite.max_uses {
            if invite.current_uses >= max_uses {
                return Err(ServiceError::InvalidInput(
                    "Invite has reached maximum uses".to_string(),
                ));
            }
        }

        Ok(invite)
    }

    /// Generate access token for new member
    fn generate_access_token(&self, agent_id: Uuid, company_id: Uuid) -> String {
        // In production: use JWT service
        format!("token_{}_{}", agent_id, company_id)
    }

    /// Generate onboarding URL
    fn generate_onboarding_url(&self, company_id: Uuid, agent_id: Uuid) -> String {
        format!(
            "https://app.parrot-agent.com/onboarding?company={}&agent={}",
            company_id, agent_id
        )
    }
}

#[async_trait]
impl InviteServiceComplete for DefaultInviteServiceComplete {
    async fn verify_invite_token(&self, token: &str) -> ServiceResult<InviteInfo> {
        let invite = self.validate_token(token).await?;

        Ok(InviteInfo {
            company_id: invite.company_id,
            company_name: invite.company_name,
            invite_type: invite.invite_type,
            invited_by: invite.invited_by,
            invited_by_name: invite.invited_by_name,
            expires_at: invite.expires_at,
            max_uses: invite.max_uses,
            current_uses: invite.current_uses,
            metadata: invite.metadata,
        })
    }

    async fn accept_invite(
        &self,
        token: &str,
        agent_id: Uuid,
        acceptance_data: AcceptanceData,
    ) -> ServiceResult<InviteAcceptanceResult> {
        // Validate token
        let invite = self.validate_token(token).await?;

        // Create membership
        let membership_id = self
            .invite_repo
            .create_membership(agent_id, invite.company_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create membership: {}", e)))?;

        // Increment invite usage
        self.invite_repo
            .increment_uses(invite.id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to increment uses: {}", e)))?;

        // Generate access token
        let access_token = self.generate_access_token(agent_id, invite.company_id);

        // Generate onboarding URL
        let onboarding_url = self.generate_onboarding_url(invite.company_id, agent_id);

        // Send notification to company admins
        let notification_sent = self
     l_service
            .send_invite_acceptance_notification(invite.company_id, &acceptance_data.agent_name)
            .await
            .is_ok();

        Ok(InviteAcceptanceResult {
            success: true,
            company_id: invite.company_id,
            agent_id,
            membership_id,
            onboarding_url,
            access_token,
        })
    }

    async fn create_join_request(
        &self,
        agent_id: Uuid,
        company_id: Uuid,
        message: String,
    ) -> ServiceResult<JoinRequest> {
        if message.trim().is_empty() {          return Err(ServiceError::InvalidInput(
                "Join request message cannot be empty".to_string(),
            ));
        }

        let join_request = JoinRequest {
            id: Uuid::new_v4(),
            agent_id,
            company_id,
            message,
            status: JoinRequestStatus::Pending,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            reviewed_by: None,
            reviewed_at: None,
            rejection_reason: None,
        };

        let created = self
            .join_request_repo
            .create(join_request.clone())
            .await
            .map_err(|e| {
                ServiceError::Internal(format!("Failed to create join request: {}", e))
            })?;

        // Send notification to company admins
        let _ = self
            .email_service
            .send_join_request_notification(company_id, "Agent", &message)
            .await;

        Ok(created)
    }

    async fn approve_join_request(
        &self,
        join_request_id: Uuid,
        approver_id: Uuid,
    ) -> ServiceResult<JoinRequestApprovalResult> {
        // Fetch join request
        let mut join_request = self
            .join_request_repo
            .find_by_id(join_request_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find join request: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Join request not found".to_string()))?;

        // Check status
        if join_request.status != JoinRequestStatus::Pending {
            return Err(ServiceError::InvalidInput(format!(
                "Join request is not pending (status: {:?})",
                join_request.status
            )));
        }

        // Update status
        join_request.status = JoinRequestStatus::Approved;
        join_request.reviewed_by = Some(approver_id);
        join_request.reviewed_at = Some(Utc::now());
        join_request.updated_at = Utc::now();

        self.join_request_repo
            .update(join_request.clone())
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update join request: {}", e)))?;

        // Create membership
        let membership_id = self
            .invite_repo
            .create_membership(join_request.agent_id, join_request.company_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create membership: {}", e)))?;

        // Send notification to agent
        let notification_sent = self
            .email_service
            .send_join_approval_notification(join_request.agent_id, "Company Name")
            .await
            .is_ok();

        Ok(JoinRequestApprovalResult {
            join_request_id,
            agent_id: join_request.agent_id,
            company_id: join_request.company_id,
            membership_id,
            approved_at: Utc::now(),
            notification_sent,
        })
    }

    async fn reject_join_request(
        &self,
        join_request_id: Uuid,
        approver_id: Uuid,
        reason: String,
    ) -> ServiceResult<JoinRequestRejectionResult> {
        // Fetch join request
        let mut join_request = self
            .join_request_repo
            .find_by_id(join_request_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find join request: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Join request not found".to_string()))?;

        // Check status
        if join_request.status != JoinRequestStatus::Pending {
            return Err(ServiceError::InvalidInput(format!(
                "Join request is not pending (status: {:?})",
                join_request.status
            )));
        }

        // Update status
        join_request.status = JoinRequestStatus::Rejected;
        join_request.reviewed_by = Some(approver_id);
        join_request.reviewed_at = Some(Utc::now());
        join_request.rejection_reason = Some(reason.clone());
        join_request.updated_at = Utc::now();

        self.join_request_repo
            .update(join_request.clone())
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update join request: {}", e)))?;

        // Send notification to agent
        let notification_sent = self
            .email_service
            .send_join_rejection_notification(join_request.agent_id, "Company Name", &reason)
            .await
            .is_ok();

        Ok(JoinRequestRejectionResult {
            join_request_id,
            rejected_at: Utc::now(),
            reason,
            notification_sent,
        })
    }

    async fn list_pending_join_requests(
        &self,
        company_id: Uuid,
    ) -> ServiceResult<Vec<JoinRequest>> {
        let all_requests = self
            .join_request_repo
            .list_by_company(company_id)
            .await
            .map_err(|e| {
                ServiceError::Internal(format!("Failed to list join requests: {}", e))
            })?;

        Ok(all_requests
            .into_iter()
            .filter(|r| r.status == JoinRequestStatus::Pending)
            .collect())
    }

    async fn get_invite_logo(&self, token: &str) -> ServiceResult<Vec<u8>> {
        self.validate_token(token).await?;

        // Placeholder: 1x1 transparent PNG
        Ok(vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ])
    }

    async fn get_invite_onboarding(&self, token: &str) -> ServiceResult<String> {
        self.validate_token(token).await?;

        Ok(r#"# Welcome to Parrot Agent

## Getting Started

Follow these steps to join the team:

1. **Accept the Invite**
   - Your invitation has been verified
   - Complete your profile setup below

2. **Configure Your Agent**
   - Choose your adapter type
   - Set up environment variables
   - Test your connection

3. **Start Working**
   - Browse available skills
   - Create your first routine
   - Collaborate with your team

For more information, visit our [documentation](https://docs.parrot-agent.com).
"#
        .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockInviteRepository;
    struct MockJoinRequestRepository;
    struct MockEmailService;

    #[async_trait]
    impl InviteRepository for MockInviteRepository {
        async fn find_by_token(&self, _token: &str) -> Result<Option<InviteRecord>, String> {
            Ok(Some(InviteRecord {
                id: Uuid::new_v4(),
                token: "test_token".to_string(),
                company_id: Uuid::new_v4(),
                company_name: "Test Company".to_string(),
                invite_type: InviteType::Agent,
                invited_by: Uuid::new_v4(),
                invited_by_name: "Admin".to_string(),
                expires_at: Some(Utc::now() + chrono::Duration::days(7)),
                max_uses: Some(10),
                current_uses: 0,
                metadata: serde_json::json!({}),
            }))
        }

        async fn increment_uses(&self, _invite_id: Uuid) -> Result<(), String> {
            Ok(())
        }

        async fn create_membership(
            &self,
            _agent_id: Uuid,
            _company_id: Uuid,
        ) -> Result<Uuid, String> {
            Ok(Uuid::new_v4())
        }
    }

    #[async_trait]
    impl JoinRequestRepository for MockJoinRequestRepository {
        async fn create(&self, join_request: JoinRequest) -> Result<JoinRequest, String> {
            Ok(join_request)
        }

        async fn update(&self, join_request: JoinRequest) -> Result<JoinRequest, String> {
            Ok(join_request)
        }

        async fn find_by_id(&self, id: Uuid) -> Result<Option<JoinRequest>, String> {
            Ok(Some(JoinRequest {
                id,
                agent_id: Uuid::new_v4(),
                company_id: Uuid::new_v4(),
                message: "Test message".to_string(),
                status: JoinRequestStatus::Pending,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                reviewed_by: None,
                reviewed_at: None,
                rejection_reason: None,
            }))
        }

        async fn list_by_company(&self, _company_id: Uuid) -> Result<Vec<JoinRequest>, String> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl EmailNotificationService for MockEmailService {
        async fn send_invite_acceptance_notification(
            &self,
            _company_id: Uuid,
            _agent_name: &str,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn send_join_request_notification(
            &self,
            _company_id: Uuid,
            _agent_name: &str,
            _message: &str,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn send_join_approval_notification(
            &self,
            _agent_id: Uuid,
            _company_name: &str,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn send_join_rejection_notification(
            &self,
            _agent_id: Uuid,
            _company_name: &str,
            _reason: &str,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_verify_invite_token() {
        let service = DefaultInviteServiceComplete::new(
            Arc::new(MockInviteRepository),
            Arc::new(MockJoinRequestRepository),
            Arc::new(MockEmailService),
        );

        let result = service.verify_invite_token("test_token").await;
        assert!(result.is_ok());

        let invite_info = result.unwrap();
        assert_eq!(invite_info.company_name, "Test Company");
    }

    #[tokio::test]
    async fn test_create_join_request() {
        let service = DefaultInviteServiceComplete::new(
            Arc::new(MockInviteRepository),
            Arc::new(MockJoinRequestRepository),
            Arc::new(MockEmailService),
        );

        let result = service
            .create_join_request(
                Uuid::new_v4(),
                Uuid::new_v4(),
                "I want to join".to_string(),
            )
            .await;

        assert!(result.is_ok());
        let join_request = result.unwrap();
        assert_eq!(join_request.status, JoinRequestStatus::Pending);
    }

    #[tokio::test]
    async fn test_approve_join_request() {
        let service = DefaultInviteServiceComplete::new(
            Arc::new(MockInviteRepository),
            Arc::new(MockJoinRequestRepository),
            Arc::new(MockEmailService),
        );

        let result = service
            .approve_join_request(Uuid::new_v4(), Uuid::new_v4())
            .await;

        assert!(result.is_ok());
        let approval_result = result.unwrap();
        assert!(approval_result.notification_sent);
    }
}
