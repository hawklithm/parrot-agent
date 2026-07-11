use async_trait::async_trait;
use models::{AvailableSkillsResponse, SkillDetails, SkillIndexResponse};
use std::sync::Arc;

use crate::errors::ServiceResult;

/// Service for skill registry management
#[async_trait]
pub trait SkillRegistryService: Send + Sync {
    /// List all available skills (minimal metadata)
    async fn list_available_skills(&self) -> ServiceResult<AvailableSkillsResponse>;

    /// Get skill index (all skills with metadata)
    async fn get_skill_index(&self) -> ServiceResult<SkillIndexResponse>;

    /// Get skill details by name (full documentation with examples)
    async fn get_skill_details(&self, skill_name: &str) -> ServiceResult<SkillDetails>;
}

/// Mock implementation for testing
pub struct MockSkillRegistryService;

#[async_trait]
impl SkillRegistryService for MockSkillRegistryService {
    async fn list_available_skills(&self) -> ServiceResult<AvailableSkillsResponse> {
        use models::AvailableSkill;

        Ok(AvailableSkillsResponse {
            skills: vec![
                AvailableSkill {
                    name: "code-review".to_string(),
                    description: "Automated code review with best practices analysis".to_string(),
                    is_paperclip_managed: true,
                },
                AvailableSkill {
                    name: "test-generator".to_string(),
                    description: "Generate unit tests based on code analysis".to_string(),
                    is_paperclip_managed: true,
                },
                AvailableSkill {
                    name: "refactor-assistant".to_string(),
                    description: "Suggest refactoring improvements for code quality".to_string(),
                    is_paperclip_managed: false,
                },
            ],
        })
    }

    async fn get_skill_index(&self) -> ServiceResult<SkillIndexResponse> {
        use models::SkillIndexEntry;

        Ok(SkillIndexResponse {
            skills: vec![
                SkillIndexEntry {
                 name: "code-review".to_string(),
                    slug: "code-review".to_string(),
                    description: "Automated code review with best practices analysis".to_string(),
                    category: Some("Development".to_string()),
                    is_paperclip_managed: true,
                    version: Some("1.2.0".to_string()),
                    tags: Some(vec!["review".to_string(), "quality".to_string()]),
                },
                SkillIndexEntry {
                    name: "test-generator".to_string(),
                    slug: "test-generator".to_string(),
                    description: "Generate unit tests based on code analysis".to_string(),
                    category: Some("Testing".to_string()),
                    is_paperclip_managed: true,
                    version: Some("1.0.1".to_string()),
                    tags: Some(vec!["testing".to_string(), "automation".to_string()]),
                },
            ],
        })
    }

    async fn get_skill_details(&self, skill_name: &str) -> ServiceResult<SkillDetails> {
        use models::{SkillExample, SkillParameter};

        if skill_name == "code-review" {
            Ok(SkillDetails {
                name: "code-review".to_string(),
                slug: "code-review".to_string(),
                description: "Automated code review with best practices analysis".to_string(),
                is_paperclip_managed: true,
                category: Some("Development".to_string()),
                version: Some("1.2.0".to_string()),
                tags: Some(vec!["review".to_string(), "quality".to_string()]),
                parameters: Some(vec![
                    SkillParameter {
                        name: "file_path".to_string(),
                        param_type: "string".to_string(),
                        description: "Path to the file to review".to_string(),
                        required: true,
                        default_value: None,
                    },
                    SkillParameter {
                        name: "severity".to_string(),
                        param_type: "string".to_string(),
                        description: "Minimum severity level (info|warning|error)".to_string(),
                        required: false,
                        default_value: Some("warning".to_string()),
                    },
                ]),
                examples: Some(vec![
                    SkillExample {
                        title: "Review a TypeScript file".to_string(),
                        description: Some("Basic code review example".to_string()),
                        code: "/code-review file_path=src/utils.ts".to_string(),
                        expected_output: Some("Found 3 warnings: unused imports, missing error handling, complex function".to_string()),
                    },
                    SkillExample {
                        title: "Review with strict severity".to_string(),
                        description: Some("Only show errors".to_string()),
                        code: "/code-review file_path=src/api.ts severity=error".to_string(),
                        expected_output: Some("Found 1 error: potential SQL injection vulnerability".to_string()),
                    },
                ]),
                usage_notes: Some("This skill analyzes code for common issues including unused variables, missing error handling, security vulnerabilities, and style violations.".to_string()),
                documentation_url: Some("https://docs.paperclip.ai/skills/code-review".to_string()),
            })
        } else {
            Err(crate::errors::ServiceError::NotFound(format!(
                "Skill '{}' not found",
                skill_name
            )))
        }
    }
}
