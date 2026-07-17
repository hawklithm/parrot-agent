use async_trait::async_trait;
use models::{AvailableSkillsResponse, SkillDetails, SkillIndexResponse};
use uuid::Uuid;

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

    // --- P2: Skill 补齐 (SK1-SK38) ---

    /// SK1: Get skill catalog
    async fn get_catalog(&self) -> ServiceResult<Vec<serde_json::Value>>;

    /// SK2: Get catalog detail
    async fn get_catalog_detail(&self, catalog_id: Uuid) -> ServiceResult<serde_json::Value>;

    /// SK3: Get catalog files
    async fn get_catalog_files(&self) -> ServiceResult<Vec<serde_json::Value>>;

    /// SK4: Get skill categories
    async fn get_categories(&self, company_id: Uuid) -> ServiceResult<Vec<serde_json::Value>>;

    /// SK5: Get skill detail by id
    async fn get_skill_by_id(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value>;

    /// SK6: Fork precheck
    async fn fork_precheck(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value>;

    /// SK7: List skill versions
    async fn list_skill_versions(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<Vec<serde_json::Value>>;

    /// SK8: Get skill version detail
    async fn get_skill_version(&self, company_id: Uuid, skill_id: Uuid, version_id: Uuid) -> ServiceResult<serde_json::Value>;

    /// SK9-SK12: Test input management
    async fn list_test_inputs(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<Vec<serde_json::Value>>;
    async fn create_test_input(&self, company_id: Uuid, skill_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value>;
    async fn update_test_input(&self, company_id: Uuid, skill_id: Uuid, input_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value>;
    async fn delete_test_input(&self, company_id: Uuid, skill_id: Uuid, input_id: Uuid) -> ServiceResult<()>;

    /// SK13-SK16: Test run template management
    async fn list_test_run_templates(&self, company_id: Uuid) -> ServiceResult<Vec<serde_json::Value>>;
    async fn create_test_run_template(&self, company_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value>;
    async fn update_test_run_template(&self, company_id: Uuid, template_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value>;
    async fn delete_test_run_template(&self, company_id: Uuid, template_id: Uuid) -> ServiceResult<()>;

    /// SK17-SK20: Test run management
    async fn list_test_runs(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<Vec<serde_json::Value>>;
    async fn get_test_run(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> ServiceResult<serde_json::Value>;
    async fn cancel_test_run(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> ServiceResult<serde_json::Value>;
    async fn delete_test_run(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> ServiceResult<()>;

    /// SK21-SK22: Star/favorite
    async fn star_skill(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value>;
    async fn unstar_skill(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<()>;

    /// SK23: Fork skill
    async fn fork_skill(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value>;

    /// SK24: Audit skill
    async fn audit_skill(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value>;

    /// SK25: Install update
    async fn install_skill_update(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value>;

    /// SK26: Reset skill
    async fn reset_skill(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value>;

    /// SK27: Get update status
    async fn get_skill_update_status(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value>;

    /// SK28-SK31: Skill comments
    async fn list_skill_comments(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<Vec<serde_json::Value>>;
    async fn add_skill_comment(&self, company_id: Uuid, skill_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value>;
    async fn update_skill_comment(&self, company_id: Uuid, skill_id: Uuid, comment_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value>;
    async fn delete_skill_comment(&self, company_id: Uuid, skill_id: Uuid, comment_id: Uuid) -> ServiceResult<()>;

    /// SK32-SK34: Skill files
    async fn list_skill_files(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<Vec<serde_json::Value>>;
    async fn update_skill_files(&self, company_id: Uuid, skill_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value>;
    async fn delete_skill_files(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<()>;

    /// SK35: Import skill
    async fn import_skill(&self, company_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value>;

    /// SK36: Install catalog
    async fn install_catalog(&self, company_id: Uuid) -> ServiceResult<serde_json::Value>;

    /// SK37: Scan projects
    async fn scan_projects(&self, company_id: Uuid) -> ServiceResult<serde_json::Value>;

    /// SK38: Delete skill
    async fn delete_skill(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<()>;
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
                usage_example: None,
                author: Some("Paperclip".to_string()),
                created_at: Some(chrono::Utc::now()),
            })
        } else {
            Err(crate::errors::ServiceError::NotFound(format!(
                "Skill '{}' not found",
                skill_name
            )))
        }
    }

    // --- P2: SK Mock implementations ---

    async fn get_catalog(&self) -> ServiceResult<Vec<serde_json::Value>> {
        Ok(vec![
            serde_json::json!({"id": Uuid::new_v4(), "name": "Code Review", "category": "Development"}),
            serde_json::json!({"id": Uuid::new_v4(), "name": "Test Generator", "category": "Testing"}),
        ])
    }

    async fn get_catalog_detail(&self, catalog_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": catalog_id, "name": "Catalog", "skills": []}))
    }

    async fn get_catalog_files(&self) -> ServiceResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    async fn get_categories(&self, _company_id: Uuid) -> ServiceResult<Vec<serde_json::Value>> {
        Ok(vec![
            serde_json::json!({"id": "development", "name": "Development", "count": 5}),
            serde_json::json!({"id": "testing", "name": "Testing", "count": 3}),
            serde_json::json!({"id": "operations", "name": "Operations", "count": 2}),
        ])
    }

    async fn get_skill_by_id(&self, _company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": skill_id, "name": "Skill", "version": "1.0.0"}))
    }

    async fn fork_precheck(&self, _company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"skillId": skill_id, "canFork": true, "reason": null}))
    }

    async fn list_skill_versions(&self, _company_id: Uuid, _skill_id: Uuid) -> ServiceResult<Vec<serde_json::Value>> {
        Ok(vec![
            serde_json::json!({"id": Uuid::new_v4(), "version": "1.0.0", "createdAt": chrono::Utc::now()}),
            serde_json::json!({"id": Uuid::new_v4(), "version": "1.1.0", "createdAt": chrono::Utc::now()}),
        ])
    }

    async fn get_skill_version(&self, _company_id: Uuid, _skill_id: Uuid, version_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": version_id, "version": "1.0.0", "files": []}))
    }

    async fn list_test_inputs(&self, _company_id: Uuid, _skill_id: Uuid) -> ServiceResult<Vec<serde_json::Value>> {
        Ok(vec![
            serde_json::json!({"id": Uuid::new_v4(), "name": "Input 1", "content": {}}),
        ])
    }

    async fn create_test_input(&self, _company_id: Uuid, _skill_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": Uuid::new_v4(), "input": input, "created": true}))
    }

    async fn update_test_input(&self, _company_id: Uuid, _skill_id: Uuid, input_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": input_id, "input": input, "updated": true}))
    }

    async fn delete_test_input(&self, _company_id: Uuid, _skill_id: Uuid, _input_id: Uuid) -> ServiceResult<()> {
        Ok(())
    }

    async fn list_test_run_templates(&self, _company_id: Uuid) -> ServiceResult<Vec<serde_json::Value>> {
        Ok(vec![
            serde_json::json!({"id": Uuid::new_v4(), "name": "Default Template", "config": {}}),
        ])
    }

    async fn create_test_run_template(&self, _company_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": Uuid::new_v4(), "template": input, "created": true}))
    }

    async fn update_test_run_template(&self, _company_id: Uuid, template_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": template_id, "template": input, "updated": true}))
    }

    async fn delete_test_run_template(&self, _company_id: Uuid, _template_id: Uuid) -> ServiceResult<()> {
        Ok(())
    }

    async fn list_test_runs(&self, _company_id: Uuid, _skill_id: Uuid) -> ServiceResult<Vec<serde_json::Value>> {
        Ok(vec![
            serde_json::json!({"id": Uuid::new_v4(), "status": "completed", "startedAt": chrono::Utc::now()}),
        ])
    }

    async fn get_test_run(&self, _company_id: Uuid, _skill_id: Uuid, run_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": run_id, "status": "completed", "result": "passed"}))
    }

    async fn cancel_test_run(&self, _company_id: Uuid, _skill_id: Uuid, run_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": run_id, "status": "cancelled"}))
    }

    async fn delete_test_run(&self, _company_id: Uuid, _skill_id: Uuid, _run_id: Uuid) -> ServiceResult<()> {
        Ok(())
    }

    async fn star_skill(&self, _company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"skillId": skill_id, "starred": true}))
    }

    async fn unstar_skill(&self, _company_id: Uuid, _skill_id: Uuid) -> ServiceResult<()> {
        Ok(())
    }

    async fn fork_skill(&self, _company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"originalSkillId": skill_id, "forkedSkillId": Uuid::new_v4(), "forked": true}))
    }

    async fn audit_skill(&self, _company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"skillId": skill_id, "status": "compliant", "issues": []}))
    }

    async fn install_skill_update(&self, _company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"skillId": skill_id, "updated": true, "version": "1.1.0"}))
    }

    async fn reset_skill(&self, _company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"skillId": skill_id, "reset": true}))
    }

    async fn get_skill_update_status(&self, _company_id: Uuid, skill_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"skillId": skill_id, "updateAvailable": false, "currentVersion": "1.0.0", "latestVersion": "1.0.0"}))
    }

    async fn list_skill_comments(&self, _company_id: Uuid, _skill_id: Uuid) -> ServiceResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    async fn add_skill_comment(&self, _company_id: Uuid, _skill_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": Uuid::new_v4(), "comment": input, "created": true}))
    }

    async fn update_skill_comment(&self, _company_id: Uuid, _skill_id: Uuid, comment_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": comment_id, "comment": input, "updated": true}))
    }

    async fn delete_skill_comment(&self, _company_id: Uuid, _skill_id: Uuid, _comment_id: Uuid) -> ServiceResult<()> {
        Ok(())
    }

    async fn list_skill_files(&self, _company_id: Uuid, _skill_id: Uuid) -> ServiceResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    async fn update_skill_files(&self, _company_id: Uuid, _skill_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"skillId": _skill_id, "files": input, "updated": true}))
    }

    async fn delete_skill_files(&self, _company_id: Uuid, _skill_id: Uuid) -> ServiceResult<()> {
        Ok(())
    }

    async fn import_skill(&self, _company_id: Uuid, input: serde_json::Value) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"id": Uuid::new_v4(), "import": input, "imported": true}))
    }

    async fn install_catalog(&self, _company_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"companyId": _company_id, "catalogInstalled": true, "skillsInstalled": 3}))
    }

    async fn scan_projects(&self, _company_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({"companyId": _company_id, "scanComplete": true, "projectsScanned": 0}))
    }

    async fn delete_skill(&self, _company_id: Uuid, _skill_id: Uuid) -> ServiceResult<()> {
        Ok(())
    }
}
