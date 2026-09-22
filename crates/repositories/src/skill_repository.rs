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

    /// Rewrite the identity columns of a skill under a row lock.
    ///
    /// `update` cannot express this: it COALESCEs a fixed set of columns and
    /// never touches `slug`/`key`. A rename must move all three together (plus
    /// the rewritten markdown) or the `(company_id, slug)` / `(company_id, key)`
    /// unique indexes would reject a half-applied rename.
    async fn rename(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        new_name: &str,
        new_slug: &str,
        new_key: &str,
        markdown: &str,
    ) -> Result<JsonValue, RepositoryError>;

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
    /// Cut a new immutable revision of the skill's current file inventory.
    async fn create_version(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        label: Option<String>,
        author_agent_id: Option<Uuid>,
        author_user_id: Option<Uuid>,
    ) -> Result<JsonValue, RepositoryError>;
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

    /// The most recent non-superseded run for a skill, or `None`.
    ///
    /// This is the head the test-run harness pins against: a new run only cuts a
    /// fresh skill version when the head's inventory differs from the current
    /// files.
    async fn latest_active_run(&self, company_id: Uuid, skill_id: Uuid) -> Result<Option<JsonValue>, RepositoryError>;

    /// Persist a run, superseding the runs it replaces in the same transaction.
    ///
    /// Superseded runs are cancelled with a retention deadline rather than
    /// deleted so their harness tasks can be cleaned up later.
    async fn create(&self, company_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError>;

    /// Flip the run owned by a harness issue to `running`, if it is still queued.
    async fn mark_running(&self, company_id: Uuid, issue_id: Uuid) -> Result<Option<JsonValue>, RepositoryError>;

    /// Settle the run owned by a harness issue, capturing the issue's output
    /// document as the run's output snapshot.
    async fn complete_for_issue(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        outcome: &str,
        error: Option<String>,
    ) -> Result<Option<JsonValue>, RepositoryError>;
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
