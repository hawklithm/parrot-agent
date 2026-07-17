use async_trait::async_trait;
use models::{AvailableSkill, AvailableSkillsResponse, SkillDetail, SkillIndexEntry};

use crate::ServiceError;

pub type ServiceResult<T> = Result<T, ServiceError>;

/// Skills registry service trait
#[async_trait]
pub trait SkillsService: Send + Sync {
    /// List all available skills
    async fn list_available_skills(&self) -> ServiceResult<AvailableSkillsResponse>;

    /// Get skill index with metadata
    async fn get_skill_index(&self) -> ServiceResult<Vec<SkillIndexEntry>>;

    /// Get detailed information about a specific skill
    async fn get_skill_details(&self, skill_name: &str) -> ServiceResult<SkillDetail>;
}

/// In-memory skills service implementation
pub struct SkillsServiceImpl {
    // In a production system, this would load from:
    // - Database for custom company skills
    // - Filesystem for bundled skills
    // - External registry for marketplace skills
}

impl SkillsServiceImpl {
    pub fn new() -> Self {
        Self {}
    }

    /// Load hardcoded available skills (placeholder implementation)
    fn load_available_skills(&self) -> Vec<AvailableSkill> {
        vec![
            AvailableSkill {
                name: "code-review".to_string(),
                description: "Perform automated code review with security checks".to_string(),
                is_paperclip_managed: true,
            },
            AvailableSkill {
                name: "test-generation".to_string(),
                description: "Generate unit tests based on code analysis".to_string(),
                is_paperclip_managed: true,
            },
            AvailableSkill {
                name: "documentation".to_string(),
                description: "Auto-generate API documentation from code".to_string(),
                is_paperclip_managed: true,
            },
            AvailableSkill {
                name: "refactoring".to_string(),
                description: "Suggest and apply code refactoring patterns".to_string(),
                is_paperclip_managed: true,
            },
        ]
    }

    fn load_skill_index(&self) -> Vec<SkillIndexEntry> {
        self.load_available_skills()
            .into_iter()
            .map(|skill| SkillIndexEntry {
                name: skill.name.clone(),
                slug: skill.name.clone(),
                description: skill.description.clone(),
                category: None,
                is_paperclip_managed: skill.is_paperclip_managed,
                version: Some("1.0.0".to_string()),
                tags: Some(vec!["automation".to_string(), "development".to_string()]),
            })
            .collect()
    }
}

impl Default for SkillsServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SkillsService for SkillsServiceImpl {
    async fn list_available_skills(&self) -> ServiceResult<AvailableSkillsResponse> {
        let skills = self.load_available_skills();
        Ok(AvailableSkillsResponse { skills })
    }

    async fn get_skill_index(&self) -> ServiceResult<Vec<SkillIndexEntry>> {
        Ok(self.load_skill_index())
    }

    async fn get_skill_details(&self, skill_name: &str) -> ServiceResult<SkillDetail> {
        let index = self.load_skill_index();
        let entry = index
            .into_iter()
            .find(|s| s.name == skill_name)
            .ok_or_else(|| ServiceError::NotFound(format!("Skill '{}' not found", skill_name)))?;

        Ok(SkillDetail {
            name: entry.name.clone(),
            slug: entry.slug.clone(),
            description: entry.description.clone(),
            is_paperclip_managed: entry.is_paperclip_managed,
            category: None,
            version: entry.version.clone(),
            tags: entry.tags.clone(),
            parameters: None,
            examples: None,
            usage_notes: None,
            documentation_url: None,
            usage_example: Some(format!("agent.use_skill('{}')", entry.name)),
            author: Some("Paperclip Team".to_string()),
            created_at: Some(chrono::Utc::now()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_available_skills() {
        let service = SkillsServiceImpl::new();
        let result = service.list_available_skills().await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(!response.skills.is_empty());
        assert_eq!(response.skills[0].name, "code-review");
    }

    #[tokio::test]
    async fn test_get_skill_index() {
        let service = SkillsServiceImpl::new();
        let result = service.get_skill_index().await;
        assert!(result.is_ok());
        let index = result.unwrap();
        assert!(!index.is_empty());
        assert!(index[0].version.is_some());
    }

    #[tokio::test]
    async fn test_get_skill_details() {
        let service = SkillsServiceImpl::new();
        let result = service.get_skill_details("code-review").await;
        assert!(result.is_ok());
        let detail = result.unwrap();
        assert_eq!(detail.name, "code-review");
        assert!(detail.usage_example.is_some());
    }

    #[tokio::test]
    async fn test_get_nonexistent_skill() {
        let service = SkillsServiceImpl::new();
        let result = service.get_skill_details("nonexistent-skill").await;
        assert!(result.is_err());
    }
}
