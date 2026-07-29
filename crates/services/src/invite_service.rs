use async_trait::async_trait;
use models::skill::{SkillDetail, SkillIndexEntry};

use crate::ServiceError;
use sqlx::{PgPool, Row};

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
    pool: Option<PgPool>,
}

impl InviteServiceImpl {
    pub fn new() -> Self {
        Self { pool: None }
    }

    pub fn with_pool(pool: PgPool) -> Self {
        Self { pool: Some(pool) }
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
        if token.is_empty() {
            return Err(ServiceError::Unauthorized("Invalid token".to_string()));
        }
        let pool = self.pool.as_ref().ok_or_else(|| ServiceError::Internal("invite persistence is not configured".into()))?;
        let row = sqlx::query("SELECT i.company_id, c.name, i.invite_type::text, i.expires_at FROM invites i JOIN companies c ON c.id=i.company_id WHERE i.token=$1 AND i.accepted=false AND i.expires_at > now()")
            .bind(token).fetch_optional(pool).await.map_err(|e| ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| ServiceError::Unauthorized("Invalid or expired invite token".into()))?;
        Ok(InviteInfo {
            company_id: row.get("company_id"), company_name: row.get("name"),
            invite_type: row.get("invite_type"), expires_at: Some(row.get("expires_at")),
        })
    }

    async fn get_invite_logo(&self, token: &str) -> ServiceResult<Vec<u8>> {
        self.verify_invite_token(token).await?;

        Err(ServiceError::NotFound("invite logo is not configured".into()))
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

        let _info = self.verify_invite_token(token).await?;
        let pool = self.pool.as_ref().ok_or_else(|| ServiceError::Internal("invite persistence is not configured".into()))?;
        let rows = sqlx::query("SELECT name, description, category, is_paperclip_managed FROM skill_catalogs ORDER BY name").fetch_all(pool).await.map_err(|e| ServiceError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(|row| SkillIndexEntry { name:row.get("name"), slug:row.get("name"), description:row.get("description"), category:row.get("category"), is_paperclip_managed:row.get("is_paperclip_managed"), version:None, tags:None }).collect())
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
