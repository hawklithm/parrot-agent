use crate::errors::ServiceResult;
use async_trait::async_trait;
use models::{OpenClawInvitePromptRequest, OpenClawInvitePromptResponse};
use std::sync::Arc;
use uuid::Uuid;

/// Service for OpenClaw integration operations
#[async_trait]
pub trait OpenClawService: Send + Sync {
    /// Generate personalized invite prompt for OpenClaw agents
    async fn generate_invite_prompt(
        &self,
        company_id: Uuid,
        request: OpenClawInvitePromptRequest,
    ) -> ServiceResult<OpenClawInvitePromptResponse>;
}

/// Placeholder implementation of OpenClawService
pub struct OpenClawServiceImpl {}

impl OpenClawServiceImpl {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl OpenClawService for OpenClawServiceImpl {
    async fn generate_invite_prompt(
        &self,
        company_id: Uuid,
        request: OpenClawInvitePromptRequest,
    ) -> ServiceResult<OpenClawInvitePromptResponse> {
        // Mock implementation: generate personalized prompt based on company context
        let company_name = format!("Company {}", &company_id.to_string()[..8]);

        let base_prompt = format!(
            "Welcome to {}! We're excited to have you join our team as an OpenClaw agent.",
            company_name
        );

        let prompt = if let Some(custom_message) = request.agent_message {
            format!("{}\n\n{}", base_prompt, custom_message)
        } else {
            format!(
                "{}\n\nYou'll be working alongside our team to help solve complex problems. \
                Please ure your webhook endpoints and API credentials to get started.",
                base_prompt
            )
        };

        Ok(OpenClawInvitePromptResponse {
            prompt,
            company_name,
            company_id,
        })
    }
}

/// Factory function to create OpenClawService
pub fn create_openclaw_service() -> Arc<dyn OpenClawService> {
    Arc::new(OpenClawServiceImpl::new())
}
