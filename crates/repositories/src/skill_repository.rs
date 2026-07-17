use async_trait::async_trait;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::RepositoryError;

// ─── Skill Catalog ────────────────────────────────────────────

#[async_trait]
pub trait SkillCatalogRepository: Send + Sync {
    async fn list_catalogs(&self) -> Result<Vec<JsonValue>, RepositoryError>;
    async fn get_catalog(&self, catalog_id: Uuid) -> Result<Option<JsonValue>, RepositoryError>;
    async fn get_catalog_files(&self) -> Result<Vec<JsonValue>, RepositoryError>;
}

// ─── Company Skill ────────────────────────────────────────────

#[async_trait]
pub trait CompanySkillRepository: Send + Sync {
    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError>;
    async fn get_by_id(&self, company_id: Uuid, skill_id: Uuid) -> Result<Option<JsonValue>, RepositoryError>;
    async fn create(&self, company_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError>;
    async fn update(&self, company_id: Uuid, skill_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError>;
    async fn delete(&self, company_id: Uuid, skill_id: Uuid) -> Result<(), RepositoryError>;
    async fn get_categories(&self, company_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError>;

    // Fork operations
    async fn fork_precheck(&self, company_id: Uuid, skill_id: Uuid) -> Result<JsonValue, RepositoryError>;
    async fn fork_skill(&self, company_id: Uuid, skill_id: Uuid, new_owner_company_id: Uuid) -> Result<JsonValue, RepositoryError>;

    // Update/status
    async fn check_update_status(&self, company_id: Uuid, skill_id: Uuid) -> Result<JsonValue, RepositoryError>;
    async fn install_update(&self, company_id: Uuid, skill_id: Uuid) -> Result<JsonValue, RepositoryError>;
    async fn reset_skill(&self, company_id: Uuid, skill_id: Uuid) -> Result<JsonValue, RepositoryError>;

    // Import / install catalog / scan projects
    async fn import_skill(&self, company_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError>;
    async fn install_catalog(&self, company_id: Uuid, catalog_id: Uuid) -> Result<JsonValue, RepositoryError>;
}

// ─── Skill Version ────────────────────────────────────────────

#[async_trait]
pub trait SkillVersionRepository: Send + Sync {
    async fn list_versions(&self, company_id: Uuid, skill_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError>;
    async fn get_version(&self, company_id: Uuid, skill_id: Uuid, version_id: Uuid) -> Result<Option<JsonValue>, RepositoryError>;
}

// ─── Skill Test Input ─────────────────────────────────────────

#[async_trait]
pub trait SkillTestInputRepository: Send + Sync {
    async fn list(&self, company_id: Uuid, skill_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError>;
    async fn create(&self, company_id: Uuid, skill_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError>;
    async fn update(&self, company_id: Uuid, skill_id: Uuid, input_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError>;
    async fn delete(&self, company_id: Uuid, skill_id: Uuid, input_id: Uuid) -> Result<(), RepositoryError>;
}

// ─── Skill Test Run Template ──────────────────────────────────

#[async_trait]
pub trait SkillTestRunTemplateRepository: Send + Sync {
    async fn list(&self, company_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError>;
    async fn create(&self, company_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError>;
    async fn update(&self, company_id: Uuid, template_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError>;
    async fn delete(&self, company_id: Uuid, template_id: Uuid) -> Result<(), RepositoryError>;
}

// ─── Skill Test Run ───────────────────────────────────────────

#[async_trait]
pub trait SkillTestRunRepository: Send + Sync {
    async fn list(&self, company_id: Uuid, skill_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError>;
    async fn get(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> Result<Option<JsonValue>, RepositoryError>;
    async fn cancel(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> Result<JsonValue, RepositoryError>;
    async fn delete(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> Result<(), RepositoryError>;
}

// ─── Skill Star ───────────────────────────────────────────────

#[async_trait]
pub trait SkillStarRepository: Send + Sync {
    async fn star(&self, company_id: Uuid, skill_id: Uuid, user_id: Uuid) -> Result<JsonValue, RepositoryError>;
    async fn unstar(&self, company_id: Uuid, skill_id: Uuid, user_id: Uuid) -> Result<(), RepositoryError>;
}

// ─── Skill Comment ────────────────────────────────────────────

#[async_trait]
pub trait SkillCommentRepository: Send + Sync {
    async fn list(&self, company_id: Uuid, skill_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError>;
    async fn create(&self, company_id: Uuid, skill_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError>;
    async fn update(&self, company_id: Uuid, skill_id: Uuid, comment_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError>;
    async fn delete(&self, company_id: Uuid, skill_id: Uuid, comment_id: Uuid) -> Result<(), RepositoryError>;
}

// ─── Skill File ───────────────────────────────────────────────

#[async_trait]
pub trait SkillFileRepository: Send + Sync {
    async fn list(&self, company_id: Uuid, skill_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError>;
    async fn update(&self, company_id: Uuid, skill_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError>;
    async fn delete(&self, company_id: Uuid, skill_id: Uuid) -> Result<(), RepositoryError>;
}
