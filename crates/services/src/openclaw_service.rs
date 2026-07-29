use crate::errors::ServiceResult;
use async_trait::async_trait;
use models::{OpenClawInvitePromptRequest, OpenClawInvitePromptResponse};
use std::sync::Arc;
use uuid::Uuid;
use sqlx::{PgPool, Row};

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
pub struct OpenClawServiceImpl {
    pool: Option<PgPool>,
}

impl OpenClawServiceImpl {
    pub fn new() -> Self {
        Self { pool: None }
    }

    pub fn with_pool(pool: PgPool) -> Self {
        Self { pool: Some(pool) }
    }
}

#[async_trait]
impl OpenClawService for OpenClawServiceImpl {
    async fn generate_invite_prompt(
        &self,
        company_id: Uuid,
        request: OpenClawInvitePromptRequest,
    ) -> ServiceResult<OpenClawInvitePromptResponse> {
        let pool = self.pool.as_ref().ok_or_else(|| crate::errors::ServiceError::Internal("OpenClaw persistence is not configured".into()))?;
        let company_name: String = sqlx::query("SELECT name FROM companies WHERE id=$1").bind(company_id).fetch_optional(pool).await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?.ok_or_else(|| crate::errors::ServiceError::NotFound(format!("company {} not found", company_id)))?.get("name");

        let base_prompt = format!(
            "Welcome to {}! We're excited to have you join our team as an OpenClaw agent.",
            company_name
        );

        let prompt = if let Some(custom_message) = request.agent_message {
            format!("{}\n\n{}", base_prompt, custom_message)
        } else {
            format!(
                "{}\n\nYou'll be working alongside our team to help solve complex problems. \
                Please configure your webhook endpoints and API credentials to get started.",
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
