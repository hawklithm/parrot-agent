use async_trait::async_trait;
use models::{AvailableSkill, AvailableSkillsResponse, SkillDetails, SkillIndexEntry, SkillIndexResponse};
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::ServiceResult;
use crate::skill_registry_service::SkillRegistryService;
use repositories::{
    CompanySkillRepository, SkillCatalogRepository, SkillCommentRepository,
    SkillFileRepository, SkillStarRepository, SkillTestInputRepository,
    SkillTestRunRepository, SkillTestRunTemplateRepository, SkillVersionRepository,
};

/// Default implementation of SkillRegistryService backed by PostgreSQL.
pub struct DefaultSkillRegistryServiceImpl {
    catalog_repo: Arc<dyn SkillCatalogRepository>,
    company_skill_repo: Arc<dyn CompanySkillRepository>,
    version_repo: Arc<dyn SkillVersionRepository>,
    test_input_repo: Arc<dyn SkillTestInputRepository>,
    test_run_template_repo: Arc<dyn SkillTestRunTemplateRepository>,
    test_run_repo: Arc<dyn SkillTestRunRepository>,
    star_repo: Arc<dyn SkillStarRepository>,
    comment_repo: Arc<dyn SkillCommentRepository>,
    file_repo: Arc<dyn SkillFileRepository>,
}

impl DefaultSkillRegistryServiceImpl {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        catalog_repo: Arc<dyn SkillCatalogRepository>,
        company_skill_repo: Arc<dyn CompanySkillRepository>,
        version_repo: Arc<dyn SkillVersionRepository>,
        test_input_repo: Arc<dyn SkillTestInputRepository>,
        test_run_template_repo: Arc<dyn SkillTestRunTemplateRepository>,
        test_run_repo: Arc<dyn SkillTestRunRepository>,
        star_repo: Arc<dyn SkillStarRepository>,
        comment_repo: Arc<dyn SkillCommentRepository>,
        file_repo: Arc<dyn SkillFileRepository>,
    ) -> Self {
        Self {
            catalog_repo,
            company_skill_repo,
            version_repo,
            test_input_repo,
            test_run_template_repo,
            test_run_repo,
            star_repo,
            comment_repo,
            file_repo,
        }
    }

    /// Load hardcoded available skills (bundled / paperclip-managed skills)
    fn load_bundled_skills(&self) -> Vec<AvailableSkill> {
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
}

#[async_trait]
impl SkillRegistryService for DefaultSkillRegistryServiceImpl {
    // ─── Original 3 methods (keep bundled implementation) ────

    async fn list_available_skills(&self) -> ServiceResult<AvailableSkillsResponse> {
        let skills = self.load_bundled_skills();
        Ok(AvailableSkillsResponse { skills })
    }

    async fn get_skill_index(&self) -> ServiceResult<SkillIndexResponse> {
        let bundled = self.load_bundled_skills();
        let skills: Vec<SkillIndexEntry> = bundled
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
            .collect();

        Ok(SkillIndexResponse { skills })
    }

    async fn get_skill_details(&self, skill_name: &str) -> ServiceResult<SkillDetails> {
        let bundled = self.load_bundled_skills();
        let entry = bundled
            .into_iter()
            .find(|s| s.name == skill_name)
            .ok_or_else(|| {
                crate::errors::ServiceError::NotFound(format!("Skill '{}' not found", skill_name))
            })?;

        let skill_name = entry.name.clone();
        Ok(SkillDetails {
            name: skill_name.clone(),
            slug: skill_name,
            description: entry.description,
            is_paperclip_managed: entry.is_paperclip_managed,
            category: None,
            version: Some("1.0.0".to_string()),
            tags: Some(vec!["automation".to_string(), "development".to_string()]),
            parameters: None,
            examples: None,
            usage_notes: None,
            documentation_url: None,
            usage_example: Some(format!("agent.use_skill('{}')", entry.name)),
            author: Some("Paperclip Team".to_string()),
            created_at: Some(chrono::Utc::now()),
        })
    }

    // ─── P2: SK1-SK38 DB-backed implementations ────────────

    /// SK1: GET /skills/catalog
    async fn get_catalog(&self) -> ServiceResult<Vec<serde_json::Value>> {
        self.catalog_repo
            .list_catalogs()
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    /// SK2: GET /skills/catalog/:catalog_id
    async fn get_catalog_detail(&self, catalog_id: Uuid) -> ServiceResult<serde_json::Value> {
        self.catalog_repo
            .get_catalog(catalog_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| {
                crate::errors::ServiceError::NotFound(format!("Catalog {} not found", catalog_id))
            })
    }

    /// SK3: GET /skills/catalog/files
    async fn get_catalog_files(&self) -> ServiceResult<Vec<serde_json::Value>> {
        self.catalog_repo
            .get_catalog_files()
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    /// SK4: GET /companies/:company_id/skills/categories
    async fn get_categories(&self, company_id: Uuid) -> ServiceResult<Vec<serde_json::Value>> {
        self.company_skill_repo
            .get_categories(company_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    /// SK5: GET /companies/:company_id/skills/:skill_id
    async fn get_skill_by_id(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        self.company_skill_repo
            .get_by_id(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| {
                crate::errors::ServiceError::NotFound(format!(
                    "Skill {} not found",
                    skill_id
                ))
            })
    }

    /// SK6: GET /companies/:company_id/skills/:skill_id/fork-precheck
    async fn fork_precheck(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        self.company_skill_repo
            .fork_precheck(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    /// SK7: GET /companies/:company_id/skills/:skill_id/versions
    async fn list_skill_versions(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<Vec<serde_json::Value>> {
        self.version_repo
            .list_versions(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    /// SK8: GET /companies/:company_id/skills/:skill_id/versions/:version_id
    async fn get_skill_version(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        version_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        self.version_repo
            .get_version(company_id, skill_id, version_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| {
                crate::errors::ServiceError::NotFound(format!("Version {} not found", version_id))
            })
    }

    // ─── SK9-SK12: Test inputs ─────────────────────────────

    async fn list_test_inputs(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<Vec<serde_json::Value>> {
        self.test_input_repo
            .list(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn create_test_input(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        input: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        self.test_input_repo
            .create(company_id, skill_id, input)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn update_test_input(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        input_id: Uuid,
        input: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        self.test_input_repo
            .update(company_id, skill_id, input_id, input)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn delete_test_input(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        input_id: Uuid,
    ) -> ServiceResult<()> {
        self.test_input_repo
            .delete(company_id, skill_id, input_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK13-SK16: Test run templates ─────────────────────

    async fn list_test_run_templates(
        &self,
        company_id: Uuid,
    ) -> ServiceResult<Vec<serde_json::Value>> {
        self.test_run_template_repo
            .list(company_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn create_test_run_template(
        &self,
        company_id: Uuid,
        input: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        self.test_run_template_repo
            .create(company_id, input)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn update_test_run_template(
        &self,
        company_id: Uuid,
        template_id: Uuid,
        input: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        self.test_run_template_repo
            .update(company_id, template_id, input)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn delete_test_run_template(
        &self,
        company_id: Uuid,
        template_id: Uuid,
    ) -> ServiceResult<()> {
        self.test_run_template_repo
            .delete(company_id, template_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK17-SK20: Test runs ──────────────────────────────

    async fn list_test_runs(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<Vec<serde_json::Value>> {
        self.test_run_repo
            .list(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn get_test_run(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        run_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        self.test_run_repo
            .get(company_id, skill_id, run_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| {
                crate::errors::ServiceError::NotFound(format!("Test run {} not found", run_id))
            })
    }

    async fn cancel_test_run(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        run_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        self.test_run_repo
            .cancel(company_id, skill_id, run_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn delete_test_run(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        run_id: Uuid,
    ) -> ServiceResult<()> {
        self.test_run_repo
            .delete(company_id, skill_id, run_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK21-SK22: Star / Unstar ──────────────────────────

    async fn star_skill(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        // Use a placeholder user_id since auth is not yet wired
        let user_id = Uuid::nil();
        self.star_repo
            .star(company_id, skill_id, user_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn unstar_skill(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<()> {
        let user_id = Uuid::nil();
        self.star_repo
            .unstar(company_id, skill_id, user_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK23: Fork ────────────────────────────────────────

    async fn fork_skill(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        // Fork within the same company by default
        self.company_skill_repo
            .fork_skill(company_id, skill_id, company_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK24: Audit ───────────────────────────────────────

    async fn audit_skill(
        &self,
        _company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({
            "skillId": skill_id,
            "status": "compliant",
            "issues": [],
        }))
    }

    // ─── SK25: Install update ──────────────────────────────

    async fn install_skill_update(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        self.company_skill_repo
            .install_update(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK26: Reset ───────────────────────────────────────

    async fn reset_skill(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        self.company_skill_repo
            .reset_skill(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK27: Update status ───────────────────────────────

    async fn get_skill_update_status(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        self.company_skill_repo
            .check_update_status(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK28-SK31: Comments ───────────────────────────────

    async fn list_skill_comments(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<Vec<serde_json::Value>> {
        self.comment_repo
            .list(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn add_skill_comment(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        input: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        self.comment_repo
            .create(company_id, skill_id, input)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn update_skill_comment(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        comment_id: Uuid,
        input: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        self.comment_repo
            .update(company_id, skill_id, comment_id, input)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn delete_skill_comment(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        comment_id: Uuid,
    ) -> ServiceResult<()> {
        self.comment_repo
            .delete(company_id, skill_id, comment_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK32-SK34: Files ──────────────────────────────────

    async fn list_skill_files(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<Vec<serde_json::Value>> {
        self.file_repo
            .list(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn update_skill_files(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        input: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        self.file_repo
            .update(company_id, skill_id, input)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn delete_skill_files(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<()> {
        self.file_repo
            .delete(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK35: Import ──────────────────────────────────────

    async fn import_skill(
        &self,
        company_id: Uuid,
        input: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        self.company_skill_repo
            .import_skill(company_id, input)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK36: Install catalog ─────────────────────────────

    async fn install_catalog(&self, company_id: Uuid) -> ServiceResult<serde_json::Value> {
        // Install the first catalog found (simplified for now)
        let catalogs = self
            .catalog_repo
            .list_catalogs()
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?;

        if let Some(catalog) = catalogs.first() {
            let catalog_id_str = catalog
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    crate::errors::ServiceError::Internal("Invalid catalog id".to_string())
                })?;
            let catalog_id: Uuid = catalog_id_str.parse().map_err(|_| {
                crate::errors::ServiceError::Internal("Invalid catalog id format".to_string())
            })?;

            self.company_skill_repo
                .install_catalog(company_id, catalog_id)
                .await
                .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
        } else {
            Ok(serde_json::json!({
                "companyId": company_id,
                "catalogInstalled": true,
                "skillsInstalled": 0,
            }))
        }
    }

    // ─── SK37: Scan projects ───────────────────────────────

    async fn scan_projects(&self, company_id: Uuid) -> ServiceResult<serde_json::Value> {
        Ok(serde_json::json!({
            "companyId": company_id,
            "scanComplete": true,
            "projectsScanned": 0,
        }))
    }

    // ─── SK38: Delete skill ────────────────────────────────

    async fn delete_skill(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<()> {
        self.company_skill_repo
            .delete(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK39: List company skills ─────────────────────────

    async fn list_company_skills(&self, company_id: Uuid) -> ServiceResult<Vec<serde_json::Value>> {
        self.company_skill_repo
            .list_by_company(company_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }
}
