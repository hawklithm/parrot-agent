use async_trait::async_trait;
use models::{
    AvailableSkill, AvailableSkillsResponse, SkillDetails, SkillIndexEntry, SkillIndexResponse,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::ServiceResult;
use crate::heartbeat_service::HeartbeatService;
use crate::issue_service::IssueService;
use crate::skill_registry_service::SkillRegistryService;
use repositories::{
    AgentRepository, CompanySkillRepository, SkillCatalogRepository, SkillCommentRepository,
    SkillFileRepository, SkillStarRepository, SkillTestInputRepository, SkillTestRunRepository,
    SkillTestRunTemplateRepository, SkillVersionRepository,
};

/// Default implementation of SkillRegistryService backed by PostgreSQL.
pub struct DefaultSkillRegistryServiceImpl {
    user_id: Option<Uuid>,
    catalog_repo: Arc<dyn SkillCatalogRepository>,
    company_skill_repo: Arc<dyn CompanySkillRepository>,
    version_repo: Arc<dyn SkillVersionRepository>,
    test_input_repo: Arc<dyn SkillTestInputRepository>,
    test_run_template_repo: Arc<dyn SkillTestRunTemplateRepository>,
    test_run_repo: Arc<dyn SkillTestRunRepository>,
    star_repo: Arc<dyn SkillStarRepository>,
    comment_repo: Arc<dyn SkillCommentRepository>,
    file_repo: Arc<dyn SkillFileRepository>,
    // Test runs open a real harness issue and wake its assignee, so the skill
    // service drives the issue/heartbeat machinery rather than duplicating it.
    agent_repo: Arc<dyn AgentRepository>,
    issue_service: Arc<dyn IssueService>,
    heartbeat_service: Arc<dyn HeartbeatService>,
}

impl DefaultSkillRegistryServiceImpl {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        user_id: Option<Uuid>,
        catalog_repo: Arc<dyn SkillCatalogRepository>,
        company_skill_repo: Arc<dyn CompanySkillRepository>,
        version_repo: Arc<dyn SkillVersionRepository>,
        test_input_repo: Arc<dyn SkillTestInputRepository>,
        test_run_template_repo: Arc<dyn SkillTestRunTemplateRepository>,
        test_run_repo: Arc<dyn SkillTestRunRepository>,
        star_repo: Arc<dyn SkillStarRepository>,
        comment_repo: Arc<dyn SkillCommentRepository>,
        file_repo: Arc<dyn SkillFileRepository>,
        agent_repo: Arc<dyn AgentRepository>,
        issue_service: Arc<dyn IssueService>,
        heartbeat_service: Arc<dyn HeartbeatService>,
    ) -> Self {
        Self {
            user_id,
            catalog_repo,
            company_skill_repo,
            version_repo,
            test_input_repo,
            test_run_template_repo,
            test_run_repo,
            star_repo,
            comment_repo,
            file_repo,
            agent_repo,
            issue_service,
            heartbeat_service,
        }
    }

    /// The skill's stored files as `(path, content)` pairs.
    async fn skill_file_contents(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<Vec<(String, String)>> {
        let rows = self
            .file_repo
            .list(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?;
        Ok(rows
            .iter()
            .filter_map(|row| {
                let path = row.get("path")?.as_str()?.to_string();
                let content = row.get("content")?.as_str().unwrap_or_default().to_string();
                Some((path, content))
            })
            .collect())
    }

    /// The revision a test run pins to.
    ///
    /// A run has to be graded against the files that were present when it
    /// started, so if the head revision no longer matches the current files a
    /// new revision is cut first.
    async fn ensure_run_skill_version(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        let files = self.skill_file_contents(company_id, skill_id).await?;
        let current =
            repositories::skill_inventory::skill_file_version_inventory(&files);
        if current.is_empty() {
            return Err(crate::errors::ServiceError::Unprocessable(
                "Cannot run a skill test for a skill with zero files.".to_string(),
            ));
        }

        let head = self
            .version_repo
            .list_versions(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
            .into_iter()
            .next();

        let head_matches = head
            .as_ref()
            .and_then(|version| version.get("fileInventory"))
            .and_then(|inventory| inventory.as_array())
            .map(|entries| {
                let mut sorted = entries.clone();
                sorted.sort_by_key(|entry| {
                    entry
                        .get("path")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string()
                });
                serde_json::Value::Array(sorted)
                    == serde_json::Value::Array(current.clone())
            })
            .unwrap_or(false);
        if head_matches {
            return Ok(head.expect("head is present when its inventory matches"));
        }

        self.version_repo
            .create_version(
                company_id,
                skill_id,
                Some("Auto version for test run".to_string()),
                None,
                self.user_id,
            )
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    /// Stop the harness issue a run owns: kill any in-flight execution and
    /// cancel the issue so the board stops showing it as work in progress.
    async fn cancel_harness_issue(&self, company_id: Uuid, issue_id: Uuid) {
        let Ok(Some(issue)) = self.issue_service.get(issue_id, company_id).await else {
            return;
        };
        if let Some(run_id) = issue.execution_run_id {
            if let Some(agent_id) = issue.assignee_agent_id {
                if let Err(error) = self
                    .heartbeat_service
                    .cancel_run(agent_id, issue_id, company_id, "Cancelled by skill test run request")
                    .await
                {
                    tracing::warn!(%error, %run_id, "failed to cancel harness run");
                }
            }
        }
        if issue.status != models::IssueStatus::Done && issue.status != models::IssueStatus::Cancelled
        {
            let _ = self
                .issue_service
                .update(
                    issue_id,
                    company_id,
                    models::UpdateIssueInput {
                        status: Some(models::IssueStatus::Cancelled),
                        ..Default::default()
                    },
                )
                .await;
        }
    }

    /// The harness instructions a run will use, or `None` when the run opts out
    /// of a template entirely.
    async fn resolve_test_run_template(
        &self,
        company_id: Uuid,
        template_snapshot: Option<&serde_json::Value>,
        template_id: Option<&serde_json::Value>,
    ) -> ServiceResult<Option<serde_json::Value>> {
        if let Some(snapshot) = template_snapshot.filter(|value| !value.is_null()) {
            let snapshot_template_id = snapshot.get("templateId").filter(|value| !value.is_null());
            if snapshot_template_id.is_none() {
                return Ok(None);
            }
            let body = snapshot
                .get("templateBody")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            validate_skill_test_template_placeholders(body)?;
            return Ok(Some(serde_json::json!({
                "templateId": snapshot_template_id,
                "templateName": snapshot.get("templateName").cloned().unwrap_or(serde_json::Value::Null),
                "templateBody": body,
            })));
        }

        match template_id {
            // An explicit `null` means "no template".
            Some(value) if value.is_null() => Ok(None),
            Some(value) => {
                let requested = value.as_str().unwrap_or_default();
                if requested == BUILT_IN_SKILL_TEST_RUN_TEMPLATE_ID {
                    return Ok(Some(serde_json::json!({
                        "templateId": BUILT_IN_SKILL_TEST_RUN_TEMPLATE_ID,
                        "templateName": "Default test template",
                        "templateBody": BUILT_IN_SKILL_TEST_RUN_TEMPLATE_BODY,
                    })));
                }
                let template = self
                    .test_run_template_repo
                    .list(company_id)
                    .await
                    .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
                    .into_iter()
                    .find(|row| {
                        row.get("id").and_then(|value| value.as_str()) == Some(requested)
                    })
                    .ok_or_else(|| {
                        crate::errors::ServiceError::NotFound(
                            "Test run template not found".to_string(),
                        )
                    })?;
                Ok(Some(serde_json::json!({
                    "templateId": template.get("id").cloned().unwrap_or(serde_json::Value::Null),
                    "templateName": template.get("name").cloned().unwrap_or(serde_json::Value::Null),
                    "templateBody": template.get("body").and_then(|value| value.as_str()).unwrap_or_default(),
                })))
            }
            // Omitted: fall back to the built-in template.
            None => Ok(Some(serde_json::json!({
                "templateId": BUILT_IN_SKILL_TEST_RUN_TEMPLATE_ID,
                "templateName": "Default test template",
                "templateBody": BUILT_IN_SKILL_TEST_RUN_TEMPLATE_BODY,
            }))),
        }
    }

    async fn load_catalog_skills(&self) -> ServiceResult<Vec<AvailableSkill>> {
        let catalogs = self.catalog_repo.list_catalogs().await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?;
        Ok(catalogs.into_iter().filter_map(|catalog| {
            let name = catalog.get("name")?.as_str()?.to_string();
            Some(AvailableSkill {
                name,
                description: catalog.get("description").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                is_paperclip_managed: catalog.get("isPaperclipManaged").and_then(|v| v.as_bool()).unwrap_or(false),
            })
        }).collect())
    }
}

#[async_trait]
impl SkillRegistryService for DefaultSkillRegistryServiceImpl {
    // ─── Original 3 methods (keep bundled implementation) ────

    async fn list_available_skills(&self) -> ServiceResult<AvailableSkillsResponse> {
        let skills = self.load_catalog_skills().await?;
        Ok(AvailableSkillsResponse { skills })
    }

    async fn get_skill_index(&self) -> ServiceResult<SkillIndexResponse> {
        let catalog = self.load_catalog_skills().await?;
        let skills: Vec<SkillIndexEntry> = catalog
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
        let catalog = self.load_catalog_skills().await?;
        let entry = catalog
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
                crate::errors::ServiceError::NotFound(format!("Skill {} not found", skill_id))
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
        let mut templates = vec![serde_json::json!({
            "id": BUILT_IN_SKILL_TEST_RUN_TEMPLATE_ID,
            "companyId": company_id,
            "name": "Default test template",
            "description": "Paperclip's read-only default harness instructions for Skills Studio runs.",
            "body": BUILT_IN_SKILL_TEST_RUN_TEMPLATE_BODY,
            "builtIn": true,
            "createdByAgentId": serde_json::Value::Null,
            "createdByUserId": serde_json::Value::Null,
            "updatedByAgentId": serde_json::Value::Null,
            "updatedByUserId": serde_json::Value::Null,
            "deletedAt": serde_json::Value::Null,
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z",
        })];
        templates.extend(
            self.test_run_template_repo
                .list(company_id)
                .await
                .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?,
        );
        Ok(templates)
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
        let run = self
            .test_run_repo
            .get(company_id, skill_id, run_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| {
                crate::errors::ServiceError::NotFound(format!("Test run {run_id} not found"))
            })?;
        // A terminal run is returned untouched; only an in-flight run has a
        // harness issue that still needs stopping.
        let terminal = run
            .get("status")
            .and_then(|value| value.as_str())
            .is_some_and(|status| matches!(status, "succeeded" | "failed" | "cancelled"));
        if !terminal {
            if let Some(issue_id) = run
                .get("issueId")
                .and_then(|value| value.as_str())
                .and_then(|value| Uuid::parse_str(value).ok())
            {
                self.cancel_harness_issue(company_id, issue_id).await;
            }
        }
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
        let run = self
            .test_run_repo
            .get(company_id, skill_id, run_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| {
                crate::errors::ServiceError::NotFound(format!("Test run {run_id} not found"))
            })?;
        self.test_run_repo
            .delete(company_id, skill_id, run_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?;
        // The run is gone from the Studio, so the harness issue it owns should
        // stop appearing on the board too.
        if let Some(issue_id) = run
            .get("issueId")
            .and_then(|value| value.as_str())
            .and_then(|value| Uuid::parse_str(value).ok())
        {
            let _ = self
                .issue_service
                .update(
                    issue_id,
                    company_id,
                    models::UpdateIssueInput {
                        hidden_at: Some(chrono::Utc::now()),
                        ..Default::default()
                    },
                )
                .await;
        }
        Ok(())
    }

    // ─── SK21-SK22: Star / Unstar ──────────────────────────

    async fn star_skill(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        let user_id = self.user_id.ok_or_else(|| {
            crate::errors::ServiceError::Forbidden(
                "Skill mutations require an authenticated user".to_string(),
            )
        })?;
        self.star_repo
            .star(company_id, skill_id, user_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn unstar_skill(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<()> {
        let user_id = self.user_id.ok_or_else(|| {
            crate::errors::ServiceError::Forbidden(
                "Skill mutations require an authenticated user".to_string(),
            )
        })?;
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

    async fn delete_skill_files(&self, company_id: Uuid, skill_id: Uuid) -> ServiceResult<()> {
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

    // ─── SK39: Create standalone (independent) skill ────────

    async fn create_company_skill(
        &self,
        company_id: Uuid,
        input: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        // Independent skills are company-owned and explicitly NOT paperclip-managed.
        let mut data = input.clone();
        data["isPaperclipManaged"] = serde_json::json!(false);
        let created = self
            .company_skill_repo
            .create(company_id, data)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?;

        // Persist any bundled files into skill_files (independent skill content).
        if let Some(skill_id_str) = created.get("id").and_then(|v| v.as_str()) {
            if let Ok(skill_id) = Uuid::parse_str(skill_id_str) {
                if let Some(files) = input.get("files").and_then(|v| v.as_array()) {
                    if !files.is_empty() {
                        let _ = self
                            .file_repo
                            .update(company_id, skill_id, serde_json::json!({ "files": files }))
                            .await;
                    }
                }
            }
        }
        Ok(created)
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
            let catalog_id_str = catalog.get("id").and_then(|v| v.as_str()).ok_or_else(|| {
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

    // ─── SK40: Update company skill ────────────────────────

    async fn update_company_skill(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        input: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        self.company_skill_repo
            .get_by_id(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| crate::errors::ServiceError::NotFound("Skill not found".to_string()))?;

        let mut patch = serde_json::Map::new();
        for (key, max_length) in [
            ("description", 2000_usize),
            ("iconUrl", 2000),
            ("homepageUrl", 2000),
            ("color", 64),
            ("tagline", 120),
            ("authorName", 200),
        ] {
            if let Some(value) = input.get(key) {
                patch.insert(key.to_string(), normalize_store_text(value, max_length));
            }
        }
        if let Some(value) = input.get("categories") {
            patch.insert("categories".to_string(), normalize_category_list(value));
        }
        // Sharing is deliberately narrower than the read model: `public_link` is a
        // real value on existing rows but not something this version can grant.
        if let Some(value) = input.get("sharingScope") {
            patch.insert(
                "sharingScope".to_string(),
                normalize_mutable_sharing_scope(value)?,
            );
        }

        self.company_skill_repo
            .update(company_id, skill_id, serde_json::Value::Object(patch))
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK41: Create skill version ────────────────────────

    async fn create_skill_version(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        label: Option<String>,
    ) -> ServiceResult<serde_json::Value> {
        self.company_skill_repo
            .get_by_id(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| crate::errors::ServiceError::NotFound("Skill not found".to_string()))?;

        self.version_repo
            .create_version(
                company_id,
                skill_id,
                label.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
                None,
                self.user_id,
            )
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    // ─── SK42: Create test run ─────────────────────────────

    async fn create_test_run(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        input: serde_json::Value,
        actor_agent_id: Option<Uuid>,
        actor_user_id: Option<Uuid>,
    ) -> ServiceResult<serde_json::Value> {
        let skill = self
            .company_skill_repo
            .get_by_id(company_id, skill_id)
            .await
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| crate::errors::ServiceError::NotFound("Skill not found".to_string()))?;

        // 1. The assigned agent has to exist in this company and be runnable.
        let agent_id = input
            .get("agentId")
            .and_then(|value| value.as_str())
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| {
                crate::errors::ServiceError::Validation("agentId is required.".to_string())
            })?;
        let agent = self
            .agent_repo
            .get_by_id(agent_id)
            .await
            .map_err(|error| match error {
                repositories::RepositoryError::NotFound(_) => {
                    crate::errors::ServiceError::NotFound("Agent not found".to_string())
                }
                other => crate::errors::ServiceError::Internal(other.to_string()),
            })?;
        if agent.company_id != company_id {
            return Err(crate::errors::ServiceError::NotFound(
                "Agent not found".to_string(),
            ));
        }
        if agent.status == models::AgentStatus::Paused {
            return Err(crate::errors::ServiceError::Unprocessable(
                "Paused agents cannot run skill tests.".to_string(),
            ));
        }

        // 2. Resolve the input snapshot: a stored input wins over inline content.
        let input_id: Option<Uuid> = input
            .get("inputId")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .and_then(|value| Uuid::parse_str(value).ok());
        let stored_input = match input_id {
            Some(input_id) => Some(
                self.test_input_repo
                    .list(company_id, skill_id)
                    .await
                    .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
                    .into_iter()
                    .find(|row| {
                        row.get("id").and_then(|value| value.as_str())
                            == Some(input_id.to_string().as_str())
                    })
                    .ok_or_else(|| {
                        crate::errors::ServiceError::NotFound("Test input not found".to_string())
                    })?,
            ),
            None => None,
        };
        let input_snapshot = stored_input
            .as_ref()
            .and_then(|row| row.get("content"))
            .and_then(json_scalar_text)
            .or_else(|| {
                input
                    .get("content")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        if input_snapshot.is_empty() {
            return Err(crate::errors::ServiceError::Unprocessable(
                "Test input content cannot be empty.".to_string(),
            ));
        }

        // 3. Pin the skill revision the run will be graded against.
        let pinned_version = match input
            .get("skillVersionId")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
        {
            Some(version_id) => {
                let version_id = Uuid::parse_str(version_id).map_err(|_| {
                    crate::errors::ServiceError::Validation(
                        "Invalid skillVersionId.".to_string(),
                    )
                })?;
                self.version_repo
                    .get_version(company_id, skill_id, version_id)
                    .await
                    .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?
                    .ok_or_else(|| {
                        crate::errors::ServiceError::NotFound(
                            "Skill version not found".to_string(),
                        )
                    })?
            }
            None => self.ensure_run_skill_version(company_id, skill_id).await?,
        };

        // 4. Resolve the harness instructions template.
        let template = self
            .resolve_test_run_template(company_id, input.get("templateSnapshot"), input.get("templateId"))
            .await?;

        let run_id = Uuid::new_v4();
        let issue_id = Uuid::new_v4();
        let output_document_key = "output".to_string();
        let skill_name = skill
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let skill_key = skill
            .get("key")
            .and_then(|value| value.as_str())
            .or_else(|| skill.get("slug").and_then(|value| value.as_str()))
            .unwrap_or_default()
            .to_string();
        let revision_number = pinned_version
            .get("revisionNumber")
            .and_then(|value| value.as_i64())
            .unwrap_or(1);

        let rendered_template_body = template
            .as_ref()
            .and_then(|template| template.get("templateBody"))
            .and_then(|value| value.as_str())
            .map(|body| {
                render_skill_test_template(
                    body,
                    &[
                        ("skillName", skill_name.clone()),
                        ("skillKey", skill_key.clone()),
                        ("skillInvocation", skill_key.clone()),
                        ("skillVersion", revision_number.to_string()),
                        ("runId", run_id.to_string()),
                        ("issueId", issue_id.to_string()),
                        ("outputDocumentKey", output_document_key.clone()),
                    ],
                )
            })
            .map(|body| body.trim().to_string());

        let harness_issue_description = match &rendered_template_body {
            Some(rendered) if !rendered.is_empty() => {
                format!("{input_snapshot}\n\n---\n\n{rendered}")
            }
            _ => input_snapshot.clone(),
        };

        // 5. The run is tracked through a real issue, so the agent executes it
        //    through the ordinary heartbeat path.
        self.issue_service
            .create(models::CreateIssueInput {
                // The run row references the harness issue, so its id is pinned
                // here rather than read back from the created issue.
                id: Some(issue_id),
                company_id,
                title: format!("Skill test: {skill_name}"),
                description: Some(harness_issue_description.clone()),
                status: Some(models::IssueStatus::Todo),
                priority: Some(models::IssuePriority::Medium),
                assignee_agent_id: Some(agent_id),
                work_mode: Some(models::IssueWorkMode::SkillTest),
                harness_kind: Some("skill_test".to_string()),
                origin_kind: Some("skill_test".to_string()),
                origin_id: Some(run_id.to_string()),
                origin_fingerprint: Some(format!("skill_test:{run_id}")),
                created_by_agent_id: actor_agent_id,
                created_by_user_id: actor_user_id,
                ..Default::default()
            })
            .await
            .map_err(|error| {
                crate::errors::ServiceError::Internal(format!("Failed to open harness issue: {error}"))
            })?;

        let created = self
            .test_run_repo
            .create(
                company_id,
                serde_json::json!({
                    "skillId": skill_id,
                    "inputId": input_id,
                    "inputSnapshot": input_snapshot,
                    "skillVersionId": pinned_version.get("id"),
                    "agentId": agent_id,
                    "agentConfigSnapshot": snapshot_agent_config(&agent),
                    "issueId": issue_id,
                    "templateId": template.as_ref().and_then(|t| t.get("templateId")).cloned(),
                    "templateName": template.as_ref().and_then(|t| t.get("templateName")).cloned(),
                    "templateBody": template.as_ref().and_then(|t| t.get("templateBody")).cloned(),
                    "renderedTemplateBody": rendered_template_body,
                    "harnessIssueDescription": harness_issue_description,
                    "outputDocumentKey": output_document_key,
                    "startedByAgentId": actor_agent_id,
                    "startedByUserId": actor_user_id,
                }),
            )
            .await;

        let created = match created {
            Ok(created) => created,
            Err(error) => {
                // The run row is the durable record; an issue without one would be
                // an orphan the board can never explain, so it is cancelled here.
                let _ = self
                    .issue_service
                    .update(
                        issue_id,
                        company_id,
                        models::UpdateIssueInput {
                            status: Some(models::IssueStatus::Cancelled),
                            ..Default::default()
                        },
                    )
                    .await;
                return Err(crate::errors::ServiceError::Internal(error.to_string()));
            }
        };

        if let Err(error) = self.heartbeat_service.wakeup(agent_id, issue_id, company_id).await {
            tracing::warn!(%error, %issue_id, "failed to wake agent for skill test run");
        }

        Ok(created)
    }
}

/// `normalizeStoreText`: absent fields stay absent, present fields truncate.
fn normalize_store_text(value: &serde_json::Value, max_length: usize) -> serde_json::Value {
    match value.as_str() {
        Some(text) => serde_json::Value::String(text.chars().take(max_length).collect()),
        None => serde_json::Value::Null,
    }
}

/// `normalizeCategoryList`: trim, collapse inner whitespace, drop empties, and
/// de-duplicate case-insensitively while keeping the first spelling.
fn normalize_category_list(value: &serde_json::Value) -> serde_json::Value {
    let Some(entries) = value.as_array() else {
        return serde_json::Value::Array(vec![]);
    };
    let mut seen: Vec<String> = Vec::new();
    let mut normalized: Vec<serde_json::Value> = Vec::new();
    for entry in entries {
        let Some(text) = entry.as_str() else { continue };
        let trimmed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if trimmed.is_empty() {
            continue;
        }
        let lookup = trimmed.to_lowercase();
        if seen.contains(&lookup) {
            continue;
        }
        seen.push(lookup);
        normalized.push(serde_json::Value::String(trimmed));
    }
    serde_json::Value::Array(normalized)
}

/// `normalizeMutableSharingScope`: only the scopes this version can actually
/// enforce are accepted.
fn normalize_mutable_sharing_scope(
    value: &serde_json::Value,
) -> ServiceResult<serde_json::Value> {
    let Some(scope) = value.as_str() else {
        return Err(crate::errors::ServiceError::Unprocessable(
            "Invalid skill sharing scope.".to_string(),
        ));
    };
    match scope {
        "private" | "company" => Ok(serde_json::Value::String(scope.to_string())),
        "public_link" => Err(crate::errors::ServiceError::Unprocessable(
            "Public skill sharing is not available in this version.".to_string(),
        )),
        _ => Err(crate::errors::ServiceError::Unprocessable(
            "Invalid skill sharing scope.".to_string(),
        )),
    }
}

/// Read a jsonb scalar as text; objects and arrays have no single text form.
fn json_scalar_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Null => None,
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => None,
        scalar => Some(scalar.to_string()),
    }
}

/// `snapshotAgentConfig`: the agent's identity as it was when the run started.
fn snapshot_agent_config(agent: &models::Agent) -> serde_json::Value {
    let adapter_config = &agent.adapter_config.0;
    let runtime_config = &agent.runtime_config.0;
    let model = adapter_config
        .get("model")
        .and_then(|value| value.as_str())
        .or_else(|| runtime_config.get("model").and_then(|value| value.as_str()));
    let instructions_ref = ["instructionsFilePath", "instructionsPath", "instructionsRef"]
        .iter()
        .find_map(|key| adapter_config.get(*key).and_then(|value| value.as_str()));
    serde_json::json!({
        "agentId": agent.id,
        "name": agent.name,
        "role": agent.role,
        "adapterType": agent.adapter_type,
        "model": model,
        "adapterConfig": adapter_config,
        "runtimeConfig": runtime_config,
        "assignedSkills": adapter_config.get("paperclipSkillSync").cloned().unwrap_or(serde_json::Value::Null),
        "instructionsRef": instructions_ref,
    })
}

/// Placeholders a test-run template may reference.
const ALLOWED_SKILL_TEST_TEMPLATE_PLACEHOLDERS: [&str; 7] = [
    "skillName",
    "skillKey",
    "skillInvocation",
    "skillVersion",
    "runId",
    "issueId",
    "outputDocumentKey",
];

/// The default harness instructions, used when a run selects no template.
const BUILT_IN_SKILL_TEST_RUN_TEMPLATE_ID: &str = "built-in:default-test-template";

const BUILT_IN_SKILL_TEST_RUN_TEMPLATE_BODY: &str = "\
You are running a Skills Studio test for `{{skillName}}` (`{{skillKey}}`), skill version v{{skillVersion}}.

Invoke and use the selected skill under test: `{{skillInvocation}}`. Use the pinned skill revision supplied by Paperclip as the source of truth, regardless of any other runtime skills.

This is a test run. Do not make durable changes outside this test task. Do not mutate unrelated issues, push, publish, send external messages, or affect real work.

If the skill would create documents, images, videos, files, or other assets, create test versions in an obviously test-scoped location when applicable, then post the results back to this task as issue documents, attachments, or work products.

Write the final result to issue document `{{outputDocumentKey}}`, then mark this test task done.";

/// A placeholder name in the shape the renderer's pattern accepts:
/// `[A-Za-z][A-Za-z0-9]*`.
fn is_placeholder_name(name: &str) -> bool {
    let mut characters = name.chars();
    matches!(characters.next(), Some(first) if first.is_ascii_alphabetic())
        && characters.all(|character| character.is_ascii_alphanumeric())
}

/// Scan for `{{name}}` placeholders, mirroring the renderer's pattern
/// `\{\{\s*([A-Za-z][A-Za-z0-9]*)\s*\}\}`.
///
/// Returns the placeholder names in order plus the body with every matched
/// placeholder removed, so the residue can be checked for stray braces.
fn scan_skill_test_template_placeholders(body: &str) -> (Vec<String>, String) {
    let mut names = Vec::new();
    let mut residue = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("{{") {
        let after_open = &rest[start + 2..];
        let trimmed_open = after_open.trim_start_matches(char::is_whitespace);
        let name_length = trimmed_open
            .char_indices()
            .take_while(|(_, character)| character.is_ascii_alphanumeric())
            .count();
        let (name, after_name) = trimmed_open.split_at(name_length);
        let after_space = after_name.trim_start_matches(char::is_whitespace);
        // Only a well-formed placeholder consumes its braces; anything else
        // stays in the residue so the malformed check can see it.
        if !is_placeholder_name(name) || !after_space.starts_with("}}") {
            residue.push_str(&rest[..start + 2]);
            rest = after_open;
            continue;
        }
        residue.push_str(&rest[..start]);
        names.push(name.to_string());
        rest = &after_space[2..];
    }
    residue.push_str(rest);
    (names, residue)
}

/// Reject templates that reference placeholders the renderer cannot fill.
///
/// Malformed braces are reported before unknown names so the author is told the
/// shape is wrong rather than that some fragment is unrecognized.
fn validate_skill_test_template_placeholders(body: &str) -> ServiceResult<()> {
    let (names, residue) = scan_skill_test_template_placeholders(body);
    if residue.contains("{{") || residue.contains("}}") {
        return Err(crate::errors::ServiceError::Unprocessable(
            "Malformed template placeholder. Use explicit placeholders like {{skillName}}."
                .to_string(),
        ));
    }
    let mut unknown: Vec<String> = names
        .into_iter()
        .filter(|name| !ALLOWED_SKILL_TEST_TEMPLATE_PLACEHOLDERS.contains(&name.as_str()))
        .collect();
    if !unknown.is_empty() {
        unknown.sort();
        unknown.dedup();
        let plural = if unknown.len() == 1 { "" } else { "s" };
        return Err(crate::errors::ServiceError::Unprocessable(format!(
            "Unknown template placeholder{plural}: {}",
            unknown.join(", ")
        )));
    }
    Ok(())
}

/// Substitute the allowed placeholders.
///
/// Validation runs first, so every placeholder here is well-formed and known.
fn render_skill_test_template(body: &str, values: &[(&str, String)]) -> String {
    let mut rendered = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("{{") {
        let after_open = &rest[start + 2..];
        let trimmed_open = after_open.trim_start_matches(char::is_whitespace);
        let name_length = trimmed_open
            .char_indices()
            .take_while(|(_, character)| character.is_ascii_alphanumeric())
            .count();
        let (name, after_name) = trimmed_open.split_at(name_length);
        let after_space = after_name.trim_start_matches(char::is_whitespace);
        if !is_placeholder_name(name) || !after_space.starts_with("}}") {
            rendered.push_str(&rest[..start + 2]);
            rest = after_open;
            continue;
        }
        rendered.push_str(&rest[..start]);
        if let Some((_, value)) = values.iter().find(|(key, _)| *key == name) {
            rendered.push_str(value);
        }
        rest = &after_space[2..];
    }
    rendered.push_str(rest);
    rendered
}
