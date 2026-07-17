use async_trait::async_trait;
use models::skill::{SkillDetail, SkillIndexEntry};

use crate::ServiceError;

pub type ServiceResult<T> = Result<T, ServiceError>;

/// Invite service trait for token-based resource access
#[async_trait]
pub trait InviteService: Send + Sync {
    /// Verify invite token and return company info
    async fn verify_invite_token(&self, token: &str) -> ServiceResult<InviteInfo>;

    /// Get company logo for invite
    async fn get_invite_logo(&self, token: &str) -> ServiceResult<Vec<u8>>;

    /// Get onboarding documentation (Markdown)
    async fn get_invite_onboarding(&self, token: &str) -> ServiceResult<String>;

    /// Get onboarding documentation (plain text)
    async fn get_invite_onboarding_text(&self, token: &str) -> ServiceResult<String>;

    /// Get skills index for invite scope
    async fn get_invite_skills_index(&self, token: &str) -> ServiceResult<Vec<SkillIndexEntry>>;

    /// Get specific skill details for invite scope
    async fn get_invite_skill_detail(&self, token: &str, skill_name: &str) -> ServiceResult<SkillDetail>;
}

/// Invite information
#[derive(Debug, Clone)]
pub struct InviteInfo {
    pub company_id: uuid::Uuid,
    pub company_name: String,
    pub invite_type: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Default implementation of InviteService
pub struct InviteServiceImpl {
    // In production: would contain InviteRepository, CompanyRepository, SkillsService
}

impl InviteServiceImpl {
    pub fn new() -> Self {
        Self {}
    }

    fn mock_onboarding_markdown() -> String {
        r#"# Welcome to Parrot Agent

## Getting Started

Follow these steps to join the team:

1. **Accept the Invite**
   - Click the accept button to join the company
   - Complete your profile setup

2. **Configure Your Agent**
   - Set up your adapter type
   - Configure environment variables
   - Test your connection

3. **Start Working**
   - Browse available skills
   - Create your first routine
   - Collaborate with your team

For more information, visit our [documentation](https://docs.example.com).
"#.to_string()
    }

    fn mock_onboarding_text() -> String {
        r#"Welcome to Parrot Agent

Getting Started
===============

Follow these steps to join the team:

1. Accept the Invite
   - Click the accept button to join the company
   - Complete your profile setup

2. Configure Your Agent
   - Set up your adapter type
   - Configure environment variables
   - Test your connection

3. Start Working
   - Browse available skills
   - Create your first routine
   - Collaborate with your team

For more information, visit our documentation at https://docs.example.com
"#.to_string()
    }
}

impl Default for InviteServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InviteService for InviteServiceImpl {
    async fn verify_invite_token(&self, token: &str) -> ServiceResult<InviteInfo> {
        // Placeholder: In production, would query database
        if token.is_empty() {
            return Err(ServiceError::Unauthorized("Invalid token".to_string()));
        }

        Ok(InviteInfo {
            company_id: uuid::Uuid::new_v4(),
            company_name: "Parrot Agent Company".to_string(),
            invite_type: "agent".to_string(),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::days(7)),
        })
    }

    async fn get_invite_logo(&self, token: &str) -> ServiceResult<Vec<u8>> {
        self.verify_invite_token(token).await?;

        // Placeholder: Return 1x1 transparent PNG
        Ok(vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
            0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
            0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00,
            0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
            0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
            0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ])
    }

    async fn get_invite_onboarding(&self, token: &str) -> ServiceResult<String> {
        self.verify_invite_token(token).await?;
        Ok(Self::mock_onboarding_markdown())
    }

    async fn get_invite_onboarding_text(&self, token: &str) -> ServiceResult<String> {
        self.verify_invite_token(token).await?;
        Ok(Self::mock_onboarding_text())
    }

    async fn get_invite_skills_index(&self, token: &str) -> ServiceResult<Vec<SkillIndexEntry>> {
        self.verify_invite_token(token).await?;

        // Placeholder: Return mock skills
        Ok(vec![
            SkillIndexEntry {
                name: "code-review".to_string(),
                slug: "code-review".to_string(),
                description: "Automated code review".to_string(),
                category: None,
                is_paperclip_managed: true,
                version: Some("1.0.0".to_string()),
                tags: Some(vec!["automation".to_string()]),
            },
            SkillIndexEntry {
                name: "test-generation".to_string(),
                slug: "test-generation".to_string(),
                description: "Generate unit tests".to_string(),
                category: None,
                is_paperclip_managed: true,
                version: Some("1.0.0".to_string()),
                tags: Some(vec!["testing".to_string()]),
            },
        ])
    }

    async fn get_invite_skill_detail(&self, token: &str, skill_name: &str) -> ServiceResult<SkillDetail> {
        self.verify_invite_token(token).await?;

        let skills = self.get_invite_skills_index(token).await?;
        let skill_entry = skills
            .into_iter()
            .find(|s| s.name == skill_name)
            .ok_or_else(|| ServiceError::NotFound(format!("Skill '{}' not found", skill_name)))?;

        Ok(SkillDetail {
            name: skill_entry.name.clone(),
            slug: skill_entry.slug.clone(),
            description: skill_entry.description.clone(),
            is_paperclip_managed: skill_entry.is_paperclip_managed,
            category: None,
            version: skill_entry.version.clone(),
            tags: skill_entry.tags.clone(),
            parameters: None,
            examples: None,
            usage_notes: None,
            documentation_url: None,
            usage_example: Some(format!("agent.use_skill('{}')", skill_entry.name)),
            author: Some("Parrot Agent Team".to_string()),
            created_at: Some(chrono::Utc::now()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_verify_invite_token() {
        let service = InviteServiceImpl::new();
        let result = service.verify_invite_token("valid_token").await;
        assert!(result.is_ok());

        let invalid_result = service.verify_invite_token("").await;
        assert!(invalid_result.is_err());
    }

    #[tokio::test]
    async fn test_get_invite_logo() {
        let service = InviteServiceImpl::new();
        let result = service.get_invite_logo("valid_token").await;
        assert!(result.is_ok());
        let logo = result.unwrap();
        assert!(!logo.is_empty());
        // Check PNG header
        assert_eq!(&logo[0..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[tokio::test]
    async fn test_get_invite_onboarding() {
        let service = InviteServiceImpl::new();
        let result = service.get_invite_onboarding("valid_token").await;
        assert!(result.is_ok());
        let markdown = result.unwrap();
        assert!(markdown.contains("# Welcome"));
    }

    #[tokio::test]
    async fn test_get_invite_skills_index() {
        let service = InviteServiceImpl::new();
        let result = service.get_invite_skills_index("valid_token").await;
        assert!(result.is_ok());
        let skills = result.unwrap();
        assert!(!skills.is_empty());
    }
}
