use async_trait::async_trait;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryError;
use crate::skill_repository::{
    CompanySkillRepository, SkillCatalogRepository, SkillCommentRepository,
    SkillFileRepository, SkillStarRepository, SkillTestInputRepository,
    SkillTestRunRepository, SkillTestRunTemplateRepository, SkillVersionRepository,
};

// ─── PgSkillCatalogRepository ─────────────────────────────────

pub struct PgSkillCatalogRepository {
    pool: PgPool,
}

impl PgSkillCatalogRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SkillCatalogRepository for PgSkillCatalogRepository {
    async fn list_catalogs(&self) -> Result<Vec<JsonValue>, RepositoryError> {
        let rows: Vec<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', id,
                'name', name,
                'description', description,
                'category', category,
                'metadata', metadata,
                'isPaperclipManaged', is_paperclip_managed,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            FROM skill_catalogs
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn get_catalog(&self, catalog_id: Uuid) -> Result<Option<JsonValue>, RepositoryError> {
        let row: Option<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', id,
                'name', name,
                'description', description,
                'category', category,
                'metadata', metadata,
                'isPaperclipManaged', is_paperclip_managed,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            FROM skill_catalogs
            WHERE id = $1
            "#,
        )
        .bind(catalog_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
    }

    async fn get_catalog_files(&self) -> Result<Vec<JsonValue>, RepositoryError> {
        // Return metadata files from catalogs
        let rows: Vec<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'catalogId', id,
                'name', name,
                'files', metadata->'files'
            )
            FROM skill_catalogs
            WHERE metadata ? 'files'
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }
}

// ─── PgCompanySkillRepository ─────────────────────────────────

pub struct PgCompanySkillRepository {
    pool: PgPool,
}

impl PgCompanySkillRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// `iconUrl`/`color`/`tagline`/`authorName`/`homepageUrl` are not modelled as
/// dedicated columns in Parrot, so they persist into `company_skills.metadata`.
/// Only keys the caller actually supplied are included, which keeps this patch
/// safe to `||` onto an existing metadata object.
fn company_skill_metadata_patch(data: &JsonValue) -> JsonValue {
    let mut patch = serde_json::Map::new();
    for key in ["iconUrl", "color", "tagline", "authorName", "homepageUrl"] {
        if let Some(value) = data.get(key) {
            patch.insert(key.to_string(), value.clone());
        }
    }
    JsonValue::Object(patch)
}

/// Paperclip source badge derived from the persisted source metadata.
fn company_skill_source_badge(alias: &str) -> String {
    format!(
        "CASE \
           WHEN {alias}.source_type = 'catalog' THEN 'catalog' \
           WHEN {alias}.is_paperclip_managed THEN 'paperclip' \
           WHEN {alias}.source_type = 'github' \
             OR position('github.com' in COALESCE({alias}.source_ref, '')) > 0 THEN 'github' \
           WHEN {alias}.source_type = 'url' THEN 'url' \
           WHEN {alias}.source_type = 'skills_sh' THEN 'skills_sh' \
           ELSE 'local' \
         END"
    )
}

/// Human-readable provenance label, mirroring Paperclip's `deriveSkillSourceInfo`.
fn company_skill_source_label(alias: &str) -> String {
    format!(
        "CASE \
           WHEN {alias}.source_type = 'catalog' THEN 'Catalog' \
           WHEN {alias}.is_paperclip_managed THEN 'Paperclip' \
           WHEN {alias}.source_type = 'github' \
             OR position('github.com' in COALESCE({alias}.source_ref, '')) > 0 THEN 'GitHub' \
           WHEN {alias}.source_type = 'url' THEN 'URL' \
           WHEN {alias}.source_type = 'skills_sh' THEN 'skills.sh' \
           ELSE COALESCE({alias}.source_locator, 'Local folder') \
         END"
    )
}

/// Paperclip-managed skills are read-only until they are forked.
fn company_skill_editable(alias: &str) -> String {
    format!("((NOT {alias}.is_paperclip_managed) OR {alias}.is_fork)")
}

/// Newest version row, mirroring Paperclip's `current_version_id` column.
///
/// `revision_number` is the monotonic identity, so it orders head correctly even
/// when two revisions share a `created_at` timestamp.
fn company_skill_current_version_id(alias: &str) -> String {
    format!(
        "(SELECT sv.id FROM skill_versions sv \
         WHERE sv.company_id = {alias}.company_id AND sv.skill_id = {alias}.id \
         ORDER BY sv.revision_number DESC LIMIT 1)"
    )
}

/// Whether an agent's `desired_skills` entry refers to this skill.
///
/// Entries are either a bare key string or a `{key, versionId}` object
/// (`sync_agent_skills` normalizes both), so the match has to consider both
/// the canonical `key` and the `slug`.
fn company_skill_desired_entry_matches(alias: &str, entry: &str) -> String {
    format!(
        "(CASE jsonb_typeof({entry}) \
           WHEN 'string' THEN {entry} #>> '{{}}' \
           ELSE {entry}->>'key' END) IN ({alias}.key, {alias}.slug) \
         OR {entry}->>'key' = {alias}.slug"
    )
}

/// An agent's configured skill set, normalized to an array.
///
/// `adapter_config` is `snake_case` at rest (`desired_skills`), unlike the API
/// projection which serializes it as `desiredSkills`.
fn desired_skills_array(agent: &str) -> String {
    format!(
        "(CASE WHEN jsonb_typeof({agent}.adapter_config->'desired_skills') = 'array' \
               THEN {agent}.adapter_config->'desired_skills' ELSE '[]'::jsonb END)"
    )
}

/// Key/value pairs per `jsonb_build_object` call.
///
/// PostgreSQL rejects functions with more than 100 arguments, so the company
/// skill projection has to be assembled from chunked objects merged with `||`.
const JSONB_OBJECT_PAIR_LIMIT: usize = 50;

fn jsonb_object_from_pairs(pairs: &[(String, String)]) -> String {
    pairs
        .chunks(JSONB_OBJECT_PAIR_LIMIT)
        .map(|chunk| {
            let args = chunk
                .iter()
                .map(|(key, expr)| format!("'{key}', {expr}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("jsonb_build_object({args})")
        })
        .collect::<Vec<_>>()
        .join(" || ")
}

fn jsonb_pair(key: &str, expr: impl Into<String>) -> (String, String) {
    (key.to_string(), expr.into())
}

/// Full `CompanySkill` projection shared by the list, detail, create and update
/// reads.
///
/// Columns Parrot does not persist as dedicated fields are projected from
/// `metadata` when an importer captured them, and as JSON `null` otherwise, so
/// the response always satisfies the Paperclip company-skill contract.
fn company_skill_fields(alias: &str) -> Vec<(String, String)> {
    let desired = desired_skills_array("agent");
    let matches = company_skill_desired_entry_matches(alias, "desired");
    let editable = company_skill_editable(alias);
    let metadata_kind =
        format!("COALESCE({alias}.metadata->>'catalogKind', {alias}.metadata->>'sourceKind')");
    vec![
        jsonb_pair("id", format!("{alias}.id")),
        jsonb_pair("companyId", format!("{alias}.company_id")),
        jsonb_pair("key", format!("{alias}.key")),
        jsonb_pair("slug", format!("{alias}.slug")),
        jsonb_pair("name", format!("{alias}.name")),
        jsonb_pair("description", format!("NULLIF({alias}.description, '')")),
        jsonb_pair("markdown", format!("{alias}.markdown")),
        jsonb_pair("sourceType", format!("{alias}.source_type")),
        jsonb_pair("sourceLocator", format!("{alias}.source_locator")),
        jsonb_pair("sourceRef", format!("{alias}.source_ref")),
        jsonb_pair("trustLevel", format!("{alias}.trust_level")),
        jsonb_pair("compatibility", format!("{alias}.compatibility")),
        jsonb_pair("fileInventory", format!("{alias}.file_inventory")),
        jsonb_pair("iconUrl", format!("{alias}.metadata->>'iconUrl'")),
        jsonb_pair("color", format!("{alias}.metadata->>'color'")),
        jsonb_pair("tagline", format!("{alias}.metadata->>'tagline'")),
        jsonb_pair("authorName", format!("{alias}.metadata->>'authorName'")),
        jsonb_pair("homepageUrl", format!("{alias}.metadata->>'homepageUrl'")),
        jsonb_pair("categories", format!("{alias}.categories")),
        jsonb_pair("sharingScope", format!("{alias}.sharing_scope")),
        jsonb_pair("publicShareToken", "NULL"),
        jsonb_pair("forkedFromSkillId", format!("{alias}.forked_from_skill_id")),
        jsonb_pair(
            "forkedFromCompanyId",
            format!(
                "(SELECT src.company_id FROM company_skills src \
                 WHERE src.id = {alias}.forked_from_skill_id)"
            ),
        ),
        jsonb_pair(
            "starCount",
            format!(
                "(SELECT COUNT(*) FROM skill_stars ss \
                 WHERE ss.company_id = {alias}.company_id AND ss.skill_id = {alias}.id)"
            ),
        ),
        jsonb_pair("installCount", format!("{alias}.install_count")),
        jsonb_pair(
            "forkCount",
            format!(
                "(SELECT COUNT(*) FROM company_skills child \
                 WHERE child.company_id = {alias}.company_id \
                   AND child.forked_from_skill_id = {alias}.id)"
            ),
        ),
        jsonb_pair("currentVersionId", company_skill_current_version_id(alias)),
        jsonb_pair("metadata", format!("{alias}.metadata")),
        jsonb_pair("createdAt", format!("{alias}.created_at")),
        jsonb_pair("updatedAt", format!("{alias}.updated_at")),
        jsonb_pair(
            "attachedAgentCount",
            format!(
                "(SELECT COUNT(DISTINCT agent.id) FROM agents agent \
                 WHERE agent.company_id = {alias}.company_id AND agent.status <> 'terminated' \
                   AND EXISTS (SELECT 1 FROM jsonb_array_elements({desired}) AS desired \
                               WHERE {matches}))"
            ),
        ),
        jsonb_pair("editable", editable.clone()),
        jsonb_pair(
            "editableReason",
            format!(
                "CASE WHEN {editable} THEN NULL \
                 ELSE 'Managed skills are read-only. Fork this skill to edit it.' END"
            ),
        ),
        jsonb_pair("sourceBadge", company_skill_source_badge(alias)),
        jsonb_pair("sourceLabel", company_skill_source_label(alias)),
        jsonb_pair(
            "sourcePath",
            format!("COALESCE({alias}.source_locator, {alias}.metadata->>'directoryRoot')"),
        ),
        jsonb_pair(
            "catalogKind",
            format!(
                "CASE WHEN {alias}.source_type = 'catalog' AND {metadata_kind} IN ('bundled', 'optional') \
                 THEN {metadata_kind} END"
            ),
        ),
        jsonb_pair(
            "originHash",
            format!("CASE WHEN {alias}.source_type = 'catalog' THEN {alias}.metadata->>'originHash' END"),
        ),
        jsonb_pair(
            "packageName",
            format!("CASE WHEN {alias}.source_type = 'catalog' THEN {alias}.metadata->>'packageName' END"),
        ),
        jsonb_pair(
            "packageVersion",
            format!("CASE WHEN {alias}.source_type = 'catalog' THEN {alias}.metadata->>'packageVersion' END"),
        ),
        jsonb_pair("catalogId", format!("{alias}.catalog_id")),
        jsonb_pair("category", format!("{alias}.category")),
        jsonb_pair("version", format!("{alias}.version")),
        jsonb_pair("tags", format!("{alias}.tags")),
        jsonb_pair("config", format!("{alias}.config")),
        jsonb_pair("isPaperclipManaged", format!("{alias}.is_paperclip_managed")),
        jsonb_pair("isFork", format!("{alias}.is_fork")),
        jsonb_pair("forkedFromCatalogId", format!("{alias}.forked_from_catalog_id")),
        jsonb_pair("status", format!("{alias}.status")),
        jsonb_pair("updateAvailable", format!("{alias}.update_available")),
        jsonb_pair("latestVersion", format!("{alias}.latest_version")),
    ]
}

/// `CompanySkillDetail`-only pairs, layered onto [`company_skill_fields`].
fn company_skill_detail_fields(alias: &str) -> Vec<(String, String)> {
    let desired = desired_skills_array("agent");
    let matches = company_skill_desired_entry_matches(alias, "desired");
    let usage_matches = company_skill_desired_entry_matches(alias, "desired");
    // `usage` is the outer subquery alias — `agent` is only in scope inside it.
    let url_key = models::agent_url_key::agent_url_key_sql("usage");
    vec![
        jsonb_pair(
            "usedByAgents",
            format!(
                "COALESCE((SELECT jsonb_agg(jsonb_build_object( \
                    'id', usage.id, 'name', usage.name, 'urlKey', {url_key}, \
                    'adapterType', usage.adapter_type, 'desired', true, 'actualState', NULL, \
                    'versionId', usage.version_id) ORDER BY usage.name) \
                  FROM (SELECT DISTINCT agent.id, agent.name, agent.adapter_type, \
                          (SELECT NULLIF(desired->>'versionId', '') \
                           FROM jsonb_array_elements({desired}) AS desired \
                           WHERE {matches} LIMIT 1) AS version_id \
                        FROM agents agent \
                        WHERE agent.company_id = {alias}.company_id \
                          AND agent.status <> 'terminated' \
                          AND EXISTS (SELECT 1 FROM jsonb_array_elements({desired}) AS desired \
                                      WHERE {usage_matches})) usage), '[]'::jsonb)"
            ),
        ),
        jsonb_pair(
            "existingForks",
            format!(
                "COALESCE((SELECT jsonb_agg(jsonb_build_object( \
                    'id', fork.id, 'name', fork.name, 'slug', fork.slug, \
                    'sourceType', fork.source_type, 'sourceLocator', fork.source_locator, \
                    'sourceRef', fork.source_ref, 'key', fork.key, \
                    'forkedFromSkillId', fork.forked_from_skill_id, \
                    'forkedFromCompanyId', fork.company_id, \
                    'currentVersionId', (SELECT sv.id FROM skill_versions sv \
                        WHERE sv.company_id = fork.company_id AND sv.skill_id = fork.id \
                        ORDER BY sv.created_at DESC, sv.id DESC LIMIT 1), \
                    'createdByCurrentActor', false, \
                    'diverged', (fork.markdown IS DISTINCT FROM {alias}.markdown) \
                      OR (SELECT COUNT(*) FROM skill_versions sv \
                          WHERE sv.company_id = fork.company_id AND sv.skill_id = fork.id) > 1, \
                    'createdAt', fork.created_at, 'updatedAt', fork.updated_at) \
                    ORDER BY fork.updated_at DESC, fork.name) \
                  FROM company_skills fork \
                  WHERE fork.company_id = {alias}.company_id \
                    AND fork.forked_from_skill_id = {alias}.id), '[]'::jsonb)"
            ),
        ),
        jsonb_pair(
            "currentVersion",
            format!(
                "(SELECT {} FROM skill_versions sv \
                  WHERE sv.company_id = {alias}.company_id AND sv.skill_id = {alias}.id \
                  ORDER BY sv.revision_number DESC LIMIT 1)",
                skill_version_object("sv")
            ),
        ),
        jsonb_pair("starredByCurrentActor", "false"),
    ]
}

/// `CompanySkill` JSON object for a `company_skills` row, extended with the
/// detail-only pairs when requested.
fn company_skill_object(alias: &str, extra: &[(String, String)]) -> String {
    let mut pairs = company_skill_fields(alias);
    pairs.extend_from_slice(extra);
    jsonb_object_from_pairs(&pairs)
}

/// `CompanySkillVersion` projection.
///
/// `file_inventory` carries `{path, kind, content}` for the revision, so the
/// history, diff and restore surfaces keep resolving after the live files move
/// on. `label` is the author's note and is independent of `version`.
fn skill_version_object(alias: &str) -> String {
    jsonb_object_from_pairs(&[
        jsonb_pair("id", format!("{alias}.id")),
        jsonb_pair("companyId", format!("{alias}.company_id")),
        jsonb_pair("companySkillId", format!("{alias}.skill_id")),
        jsonb_pair("revisionNumber", format!("{alias}.revision_number")),
        jsonb_pair("version", format!("{alias}.version")),
        jsonb_pair("label", format!("{alias}.label")),
        jsonb_pair("releaseId", format!("{alias}.release_id")),
        jsonb_pair("releaseName", format!("{alias}.release_name")),
        jsonb_pair("releasedAt", format!("{alias}.released_at")),
        jsonb_pair("fileInventory", format!("{alias}.file_inventory")),
        jsonb_pair("authorAgentId", format!("{alias}.created_by_agent_id")),
        jsonb_pair("authorUserId", format!("{alias}.created_by_user_id")),
        jsonb_pair("createdAt", format!("{alias}.created_at")),
    ])
}

/// `CompanySkillTestRun` projection, including the per-run cost rollup and the
/// retention flag the Studio uses to disable the harness deep link.
///
/// `status` is normalized on read: rows written before this projection existed
/// carry the legacy `pending` default, which the Studio does not recognize.
fn skill_test_run_object(alias: &str) -> String {
    jsonb_object_from_pairs(&[
        jsonb_pair("id", format!("{alias}.id")),
        jsonb_pair("companyId", format!("{alias}.company_id")),
        jsonb_pair("skillId", format!("{alias}.skill_id")),
        jsonb_pair("inputId", format!("{alias}.input_id")),
        jsonb_pair("inputSnapshot", format!("{alias}.input_snapshot")),
        jsonb_pair("skillVersionId", format!("{alias}.skill_version_id")),
        jsonb_pair("agentId", format!("{alias}.agent_id")),
        jsonb_pair(
            "agentConfigSnapshot",
            format!("{alias}.agent_config_snapshot"),
        ),
        jsonb_pair("issueId", format!("{alias}.issue_id")),
        jsonb_pair("templateId", format!("{alias}.template_id")),
        jsonb_pair("templateName", format!("{alias}.template_name")),
        jsonb_pair("templateBody", format!("{alias}.template_body")),
        jsonb_pair(
            "renderedTemplateBody",
            format!("{alias}.rendered_template_body"),
        ),
        jsonb_pair(
            "harnessIssueDescription",
            format!(
                "COALESCE(NULLIF({alias}.harness_issue_description, ''), {alias}.input_snapshot)"
            ),
        ),
        jsonb_pair(
            "status",
            format!(
                "CASE WHEN {alias}.status IN ('running','succeeded','failed','cancelled') \
                 THEN {alias}.status ELSE 'queued' END"
            ),
        ),
        jsonb_pair(
            "outputDocumentKey",
            format!("COALESCE(NULLIF({alias}.output_document_key, ''), 'output')"),
        ),
        jsonb_pair("outputSnapshot", format!("{alias}.output_snapshot")),
        jsonb_pair("error", format!("{alias}.error")),
        jsonb_pair("deletedAt", format!("{alias}.deleted_at")),
        jsonb_pair("supersededAt", format!("{alias}.superseded_at")),
        jsonb_pair(
            "harnessIssueExpiresAt",
            format!("{alias}.harness_issue_expires_at"),
        ),
        jsonb_pair(
            "harnessIssueDeletedAt",
            format!("{alias}.harness_issue_deleted_at"),
        ),
        jsonb_pair("createdAt", format!("{alias}.created_at")),
        jsonb_pair("updatedAt", format!("{alias}.updated_at")),
        jsonb_pair(
            "cost",
            format!(
                "(SELECT jsonb_build_object( \
                    'costCents', COALESCE(SUM(ce.cost_cents), 0), \
                    'inputTokens', COALESCE(SUM(ce.input_tokens), 0), \
                    'cachedInputTokens', COALESCE(SUM(ce.cached_input_tokens), 0), \
                    'outputTokens', COALESCE(SUM(ce.output_tokens), 0)) \
                  FROM cost_events ce \
                  WHERE ce.company_id = {alias}.company_id AND ce.issue_id = {alias}.issue_id)"
            ),
        ),
        jsonb_pair(
            "taskExpired",
            format!("({alias}.harness_issue_deleted_at IS NOT NULL)"),
        ),
    ])
}


#[async_trait]
impl CompanySkillRepository for PgCompanySkillRepository {
    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError> {
        let mut rows: Vec<JsonValue> = sqlx::query_scalar(&format!(
            "SELECT {} FROM company_skills cs \
             WHERE cs.company_id = $1 ORDER BY cs.name",
            company_skill_object("cs", &[])
        ))
        .bind(company_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        let mut files = crate::skill_inventory::load_company_skill_files(&self.pool, company_id)
            .await
            .map_err(RepositoryError::DatabaseError)?;
        for row in &mut rows {
            let skill_files = row
                .get("id")
                .and_then(|id| id.as_str())
                .and_then(|id| id.parse::<Uuid>().ok())
                .and_then(|id| files.remove(&id))
                .unwrap_or_default();
            crate::skill_inventory::attach_file_inventory(row, &skill_files);
        }

        Ok(rows)
    }

    async fn get_by_id(&self, company_id: Uuid, skill_id: Uuid) -> Result<Option<JsonValue>, RepositoryError> {
        let row: Option<JsonValue> = sqlx::query_scalar(&format!(
            "SELECT {} FROM company_skills cs \
             WHERE cs.id = $1 AND cs.company_id = $2",
            company_skill_object("cs", &company_skill_detail_fields("cs"))
        ))
        .bind(skill_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        let Some(mut row) = row else {
            return Ok(None);
        };
        let files = crate::skill_inventory::load_skill_files(&self.pool, company_id, skill_id)
            .await
            .map_err(RepositoryError::DatabaseError)?;
        crate::skill_inventory::attach_file_inventory(&mut row, &files);

        Ok(Some(row))
    }

    async fn create(&self, company_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed");
        let slug = data.get("slug").and_then(|v| v.as_str()).unwrap_or(name);
        let description = data.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let category = data.get("category").and_then(|v| v.as_str());
        let catalog_id: Option<Uuid> = data.get("catalogId").and_then(|v| v.as_str()).and_then(|s| s.parse().ok());
        let is_paperclip_managed = data.get("isPaperclipManaged").and_then(|v| v.as_bool()).unwrap_or(false);
        let version = data.get("version").and_then(|v| v.as_str()).unwrap_or("1.0.0");
        let tags = data.get("tags").cloned().unwrap_or(JsonValue::Array(vec![]));
        let config = data.get("config").cloned().unwrap_or(JsonValue::Object(serde_json::Map::new()));
        let status = data.get("status").and_then(|v| v.as_str()).unwrap_or("active");
        let markdown = data.get("markdown").and_then(|v| v.as_str()).unwrap_or("");
        let categories = data.get("categories").cloned().unwrap_or(JsonValue::Array(vec![]));
        let sharing_scope = data.get("sharingScope").and_then(|v| v.as_str()).unwrap_or("company");
        let metadata = company_skill_metadata_patch(&data);
        let forked_from_skill_id: Option<Uuid> = data
            .get("forkedFromSkillId")
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse().ok());

        let skill_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO company_skills (
                company_id, key, catalog_id, name, slug, description, category,
                version, tags, config, status, is_paperclip_managed,
                markdown, categories, sharing_scope, metadata, forked_from_skill_id, is_fork
            )
            VALUES ($1, format('company/%s/%s', $1, $4), $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                    $12, $13, $14, $15, $16, $17)
            RETURNING id
            "#,
        )
        .bind(company_id)
        .bind(catalog_id)
        .bind(name)
        .bind(slug)
        .bind(description)
        .bind(category)
        .bind(version)
        .bind(&tags)
        .bind(&config)
        .bind(status)
        .bind(is_paperclip_managed)
        .bind(markdown)
        .bind(&categories)
        .bind(sharing_scope)
        .bind(&metadata)
        .bind(forked_from_skill_id)
        .bind(forked_from_skill_id.is_some())
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        // Re-read so the response carries the full Paperclip company-skill
        // contract (derived badges, stats, version pointers) rather than the
        // narrow set of columns the INSERT itself supplies.
        self.get_by_id(company_id, skill_id)
            .await?
            .ok_or_else(|| RepositoryError::NotFound(skill_id))
    }

    async fn update(&self, company_id: Uuid, skill_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        let name = data.get("name").and_then(|v| v.as_str());
        let description = data.get("description").and_then(|v| v.as_str());
        let category = data.get("category").and_then(|v| v.as_str());
        let status = data.get("status").and_then(|v| v.as_str());
        let markdown = data.get("markdown").and_then(|v| v.as_str());
        let categories = data.get("categories").cloned();
        let sharing_scope = data.get("sharingScope").and_then(|v| v.as_str());
        let metadata = company_skill_metadata_patch(&data);
        let forked_from_skill_id: Option<Uuid> = data
            .get("forkedFromSkillId")
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse().ok());

        let updated: Uuid = sqlx::query_scalar(
            r#"
            UPDATE company_skills
            SET
                name = COALESCE($4, name),
                description = COALESCE($5, description),
                category = COALESCE($6, category),
                status = COALESCE($7, status),
                markdown = COALESCE($8, markdown),
                categories = COALESCE($9, categories),
                sharing_scope = COALESCE($10, sharing_scope),
                forked_from_skill_id = COALESCE($11, forked_from_skill_id),
                is_fork = COALESCE($11, forked_from_skill_id) IS NOT NULL,
                metadata = metadata || $3::jsonb,
                updated_at = NOW()
            WHERE id = $1 AND company_id = $2
            RETURNING id
            "#,
        )
        .bind(skill_id)
        .bind(company_id)
        .bind(&metadata)
        .bind(name)
        .bind(description)
        .bind(category)
        .bind(status)
        .bind(markdown)
        .bind(&categories)
        .bind(sharing_scope)
        .bind(forked_from_skill_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or_else(|| RepositoryError::NotFound(skill_id))?;

        self.get_by_id(company_id, updated)
            .await?
            .ok_or_else(|| RepositoryError::NotFound(updated))
    }

    async fn delete(&self, company_id: Uuid, skill_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM company_skills
            WHERE id = $1 AND company_id = $2
            "#,
        )
        .bind(skill_id)
        .bind(company_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }

    async fn get_categories(&self, company_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError> {
        let rows: Vec<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', COALESCE(category, 'uncategorized'),
                'name', COALESCE(category, 'Uncategorized'),
                'count', COUNT(*)
            )
            FROM company_skills
            WHERE company_id = $1
            GROUP BY category
            ORDER BY category
            "#,
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn rename(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        new_name: &str,
        new_slug: &str,
        new_key: &str,
        markdown: &str,
    ) -> Result<JsonValue, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(RepositoryError::DatabaseError)?;

        // Lock the row first: the caller has already scanned for slug/key
        // conflicts, so a concurrent rename must not slip between that scan and
        // this write.
        let locked: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM company_skills WHERE id = $1 AND company_id = $2 FOR UPDATE",
        )
        .bind(skill_id)
        .bind(company_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        if locked.is_none() {
            return Err(RepositoryError::NotFound(skill_id));
        }

        sqlx::query(
            "UPDATE company_skills \
             SET name = $3, slug = $4, key = $5, markdown = $6, updated_at = NOW() \
             WHERE id = $1 AND company_id = $2",
        )
        .bind(skill_id)
        .bind(company_id)
        .bind(new_name)
        .bind(new_slug)
        .bind(new_key)
        .bind(markdown)
        .execute(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        tx.commit().await.map_err(RepositoryError::DatabaseError)?;

        self.get_by_id(company_id, skill_id)
            .await?
            .ok_or(RepositoryError::NotFound(skill_id))
    }

    async fn fork_precheck(&self, company_id: Uuid, skill_id: Uuid) -> Result<JsonValue, RepositoryError> {
        let skill = self.get_by_id(company_id, skill_id).await?;
        match skill {
            Some(s) => Ok(serde_json::json!({
                "skillId": skill_id,
                "canFork": true,
                "reason": null,
                "skill": s,
            })),
            None => Ok(serde_json::json!({
                "skillId": skill_id,
                "canFork": false,
                "reason": "Skill not found",
            })),
        }
    }

    async fn fork_skill(&self, company_id: Uuid, skill_id: Uuid, new_owner_company_id: Uuid) -> Result<JsonValue, RepositoryError> {
        let original = self.get_by_id(company_id, skill_id).await?
            .ok_or_else(|| RepositoryError::NotFound(skill_id))?;

        let name = original.get("name").and_then(|v| v.as_str()).unwrap_or("forked");
        let slug = format!("{}-fork-{}", name, Uuid::new_v4().to_string().chars().take(8).collect::<String>());
        let description = original.get("description").and_then(|v| v.as_str()).unwrap_or("");

        let row: JsonValue = sqlx::query_scalar(
            r#"
            INSERT INTO company_skills (company_id, key, name, slug, description, is_fork, forked_from_skill_id, is_paperclip_managed)
            VALUES ($1, format('company/%s/%s', $1, $3), $2, $3, $4, true, $5, false)
            RETURNING jsonb_build_object(
                'id', id,
                'originalSkillId', $6::uuid,
                'forkedSkillId', id,
                'forked', true,
                'name', name,
                'slug', slug
            )
            "#,
        )
        .bind(new_owner_company_id)
        .bind(name)
        .bind(&slug)
        .bind(description)
        .bind(skill_id)
        .bind(skill_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
    }

    async fn check_update_status(&self, company_id: Uuid, skill_id: Uuid) -> Result<JsonValue, RepositoryError> {
        let row: Option<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'skillId', id,
                'updateAvailable', update_available,
                'currentVersion', version,
                'latestVersion', latest_version
            )
            FROM company_skills
            WHERE id = $1 AND company_id = $2
            "#,
        )
        .bind(skill_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row.unwrap_or(serde_json::json!({
            "skillId": skill_id,
            "updateAvailable": false,
            "currentVersion": "unknown",
            "latestVersion": null,
        })))
    }

    async fn install_update(&self, company_id: Uuid, skill_id: Uuid) -> Result<JsonValue, RepositoryError> {
        let row: JsonValue = sqlx::query_scalar(
            r#"
            UPDATE company_skills
            SET
                version = COALESCE(latest_version, version),
                update_available = false,
                updated_at = NOW()
            WHERE id = $1 AND company_id = $2
            RETURNING jsonb_build_object(
                'skillId', id,
                'updated', true,
                'version', version,
                'previousVersion', version
            )
            "#,
        )
        .bind(skill_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or_else(|| RepositoryError::NotFound(skill_id))?;

        Ok(row)
    }

    async fn reset_skill(&self, company_id: Uuid, skill_id: Uuid) -> Result<JsonValue, RepositoryError> {
        let row: JsonValue = sqlx::query_scalar(
            r#"
            UPDATE company_skills
            SET
                update_available = false,
                latest_version = NULL,
                updated_at = NOW()
            WHERE id = $1 AND company_id = $2
            RETURNING jsonb_build_object(
                'skillId', id,
                'reset', true,
                'version', version
            )
            "#,
        )
        .bind(skill_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or_else(|| RepositoryError::NotFound(skill_id))?;

        Ok(row)
    }

    async fn import_skill(&self, company_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        // Import creates a new skill from external data
        self.create(company_id, data).await
    }

    async fn install_catalog(&self, company_id: Uuid, catalog_id: Uuid) -> Result<JsonValue, RepositoryError> {
        // Install all skills from a catalog into the company
        let rows: Vec<JsonValue> = sqlx::query_scalar(
            r#"
            INSERT INTO company_skills (company_id, key, catalog_id, name, slug, description, category, is_paperclip_managed)
            SELECT $1, format('company/%s/%s', $1, LOWER(REPLACE(sc.name, ' ', '-'))), sc.id, sc.name, LOWER(REPLACE(sc.name, ' ', '-')), sc.description, sc.category, sc.is_paperclip_managed
            FROM skill_catalogs sc
            WHERE sc.id = $2
            AND NOT EXISTS (
                SELECT 1 FROM company_skills cs
                WHERE cs.company_id = $1 AND cs.catalog_id = sc.id
            )
            ON CONFLICT (company_id, slug) DO NOTHING
            RETURNING jsonb_build_object('id', id, 'name', name)
            "#,
        )
        .bind(company_id)
        .bind(catalog_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(serde_json::json!({
            "companyId": company_id,
            "catalogInstalled": true,
            "skillsInstalled": rows.len(),
        }))
    }
}

// ─── PgSkillVersionRepository ─────────────────────────────────

pub struct PgSkillVersionRepository {
    pool: PgPool,
}

impl PgSkillVersionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SkillVersionRepository for PgSkillVersionRepository {
    async fn list_versions(&self, company_id: Uuid, skill_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError> {
        let rows: Vec<JsonValue> = sqlx::query_scalar(&format!(
            "SELECT {} FROM skill_versions sv \
             JOIN company_skills cs ON cs.id = sv.skill_id AND cs.company_id = $1 \
             WHERE sv.skill_id = $2 \
             ORDER BY sv.revision_number DESC",
            skill_version_object("sv")
        ))
        .bind(company_id)
        .bind(skill_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn get_version(&self, company_id: Uuid, skill_id: Uuid, version_id: Uuid) -> Result<Option<JsonValue>, RepositoryError> {
        let row: Option<JsonValue> = sqlx::query_scalar(&format!(
            "SELECT {} FROM skill_versions sv \
             JOIN company_skills cs ON cs.id = sv.skill_id AND cs.company_id = $1 \
             WHERE sv.id = $2 AND sv.skill_id = $3",
            skill_version_object("sv")
        ))
        .bind(company_id)
        .bind(version_id)
        .bind(skill_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
    }

    async fn create_version(
        &self,
        company_id: Uuid,
        skill_id: Uuid,
        label: Option<String>,
        author_agent_id: Option<Uuid>,
        author_user_id: Option<Uuid>,
    ) -> Result<JsonValue, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(RepositoryError::DatabaseError)?;

        // Lock the skill row first: the revision number is allocated with
        // `MAX + 1`, so two concurrent restores of the same skill must serialize.
        let locked: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM company_skills WHERE id = $1 AND company_id = $2 FOR UPDATE",
        )
        .bind(skill_id)
        .bind(company_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        if locked.is_none() {
            return Err(RepositoryError::NotFound(skill_id));
        }

        let files = crate::skill_inventory::load_skill_files_in_tx(&mut tx, company_id, skill_id)
            .await
            .map_err(RepositoryError::DatabaseError)?;
        let file_inventory = crate::skill_inventory::skill_file_version_inventory(&files);

        let next_revision: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(revision_number), 0) + 1 FROM skill_versions \
             WHERE company_id = $1 AND skill_id = $2",
        )
        .bind(company_id)
        .bind(skill_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        let version_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO skill_versions (
                company_id, skill_id, version, revision_number, label, file_inventory,
                created_by_agent_id, created_by_user_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .bind(format!("v{next_revision}"))
        .bind(next_revision)
        .bind(&label)
        .bind(JsonValue::Array(file_inventory))
        .bind(author_agent_id)
        .bind(author_user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        sqlx::query("UPDATE company_skills SET updated_at = NOW() WHERE id = $1 AND company_id = $2")
            .bind(skill_id)
            .bind(company_id)
            .execute(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        let row: Option<JsonValue> = sqlx::query_scalar(&format!(
            "SELECT {} FROM skill_versions sv WHERE sv.id = $1",
            skill_version_object("sv")
        ))
        .bind(version_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        tx.commit().await.map_err(RepositoryError::DatabaseError)?;

        row.ok_or(RepositoryError::NotFound(version_id))
    }
}

// ─── PgSkillTestInputRepository ───────────────────────────────

pub struct PgSkillTestInputRepository {
    pool: PgPool,
}

impl PgSkillTestInputRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SkillTestInputRepository for PgSkillTestInputRepository {
    async fn list(&self, company_id: Uuid, skill_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError> {
        let rows: Vec<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', sti.id,
                'skillId', sti.skill_id,
                'name', sti.name,
                'content', sti.content,
                'createdAt', sti.created_at,
                'updatedAt', sti.updated_at
            )
            FROM skill_test_inputs sti
            JOIN company_skills cs ON cs.id = sti.skill_id AND cs.company_id = $1
            WHERE sti.skill_id = $2
            ORDER BY sti.name
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn create(&self, company_id: Uuid, skill_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("test-input");
        let content = data.get("content").cloned().unwrap_or(JsonValue::Null);

        let row: JsonValue = sqlx::query_scalar(
            r#"
            INSERT INTO skill_test_inputs (company_id, skill_id, name, content)
            SELECT $1, $2, $3, $4
            WHERE EXISTS (SELECT 1 FROM company_skills WHERE id = $2 AND company_id = $1)
            RETURNING jsonb_build_object(
                'id', id,
                'skillId', skill_id,
                'name', name,
                'content', content,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .bind(name)
        .bind(&content)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or_else(|| RepositoryError::NotFound(skill_id))?;

        Ok(row)
    }

    async fn update(&self, company_id: Uuid, skill_id: Uuid, input_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        let name = data.get("name").and_then(|v| v.as_str());
        let content = data.get("content");

        let row: JsonValue = sqlx::query_scalar(
            r#"
            UPDATE skill_test_inputs sti
            SET
                name = COALESCE($4, sti.name),
                content = COALESCE($5, sti.content),
                updated_at = NOW()
            FROM company_skills cs
            WHERE sti.id = $1 AND sti.skill_id = $2 AND cs.id = sti.skill_id AND cs.company_id = $3
            RETURNING jsonb_build_object(
                'id', sti.id,
                'skillId', sti.skill_id,
                'name', sti.name,
                'content', sti.content,
                'createdAt', sti.created_at,
                'updatedAt', sti.updated_at
            )
            "#,
        )
        .bind(input_id)
        .bind(skill_id)
        .bind(company_id)
        .bind(name)
        .bind(content)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or_else(|| RepositoryError::NotFound(input_id))?;

        Ok(row)
    }

    async fn delete(&self, company_id: Uuid, skill_id: Uuid, input_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM skill_test_inputs sti
            USING company_skills cs
            WHERE sti.id = $1 AND sti.skill_id = $2 AND cs.id = sti.skill_id AND cs.company_id = $3
            "#,
        )
        .bind(input_id)
        .bind(skill_id)
        .bind(company_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }
}

// ─── PgSkillTestRunTemplateRepository ─────────────────────────

pub struct PgSkillTestRunTemplateRepository {
    pool: PgPool,
}

impl PgSkillTestRunTemplateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SkillTestRunTemplateRepository for PgSkillTestRunTemplateRepository {
    async fn list(&self, company_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError> {
        let rows: Vec<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', id,
                'companyId', company_id,
                'name', name,
                'description', description,
                'body', body,
                'builtIn', false,
                'config', config,
                'createdByAgentId', created_by_agent_id,
                'createdByUserId', created_by_user_id,
                'updatedByAgentId', updated_by_agent_id,
                'updatedByUserId', updated_by_user_id,
                'deletedAt', deleted_at,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            FROM skill_test_run_templates
            WHERE company_id = $1 AND deleted_at IS NULL
            ORDER BY name ASC, created_at ASC
            "#,
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn create(&self, company_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("template");
        let description = data.get("description").and_then(|v| v.as_str());
        let body = data.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let config = data.get("config").cloned().unwrap_or(JsonValue::Null);
        let created_by_agent_id = data
            .get("createdByAgentId")
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<Uuid>().ok());
        let created_by_user_id = data
            .get("createdByUserId")
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<Uuid>().ok());

        let row: JsonValue = sqlx::query_scalar(
            r#"
            INSERT INTO skill_test_run_templates (
                company_id, name, description, body, config,
                created_by_agent_id, created_by_user_id,
                updated_by_agent_id, updated_by_user_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $6, $7)
            RETURNING jsonb_build_object(
                'id', id,
                'companyId', company_id,
                'name', name,
                'description', description,
                'body', body,
                'builtIn', false,
                'config', config,
                'createdByAgentId', created_by_agent_id,
                'createdByUserId', created_by_user_id,
                'updatedByAgentId', updated_by_agent_id,
                'updatedByUserId', updated_by_user_id,
                'deletedAt', deleted_at,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            "#,
        )
        .bind(company_id)
        .bind(name)
        .bind(description)
        .bind(body)
        .bind(&config)
        .bind(created_by_agent_id)
        .bind(created_by_user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
    }

    async fn update(&self, company_id: Uuid, template_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        let name = data.get("name").and_then(|v| v.as_str());
        let description = data.get("description").and_then(|v| v.as_str());
        let body = data.get("body").and_then(|v| v.as_str());
        let config = data.get("config");
        let updated_by_agent_id = data
            .get("updatedByAgentId")
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<Uuid>().ok());
        let updated_by_user_id = data
            .get("updatedByUserId")
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<Uuid>().ok());

        let row: JsonValue = sqlx::query_scalar(
            r#"
            UPDATE skill_test_run_templates
            SET
                name = COALESCE($3, name),
                description = COALESCE($4, description),
                body = COALESCE($5, body),
                config = COALESCE($6, config),
                updated_by_agent_id = COALESCE($7, updated_by_agent_id),
                updated_by_user_id = COALESCE($8, updated_by_user_id),
                updated_at = NOW()
            WHERE id = $1 AND company_id = $2 AND deleted_at IS NULL
            RETURNING jsonb_build_object(
                'id', id,
                'companyId', company_id,
                'name', name,
                'description', description,
                'body', body,
                'builtIn', false,
                'config', config,
                'createdByAgentId', created_by_agent_id,
                'createdByUserId', created_by_user_id,
                'updatedByAgentId', updated_by_agent_id,
                'updatedByUserId', updated_by_user_id,
                'deletedAt', deleted_at,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            "#,
        )
        .bind(template_id)
        .bind(company_id)
        .bind(name)
        .bind(description)
        .bind(body)
        .bind(config)
        .bind(updated_by_agent_id)
        .bind(updated_by_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or(RepositoryError::NotFound(template_id))?;

        Ok(row)
    }

    async fn delete(&self, company_id: Uuid, template_id: Uuid) -> Result<(), RepositoryError> {
        // Soft delete: runs snapshot the template they used, but the template list
        // is still the only place a deleted template's name can be resolved.
        let affected = sqlx::query(
            r#"
            UPDATE skill_test_run_templates
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND company_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(template_id)
        .bind(company_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .rows_affected();

        if affected == 0 {
            return Err(RepositoryError::NotFound(template_id));
        }

        Ok(())
    }
}

// ─── PgSkillTestRunRepository ─────────────────────────────────

pub struct PgSkillTestRunRepository {
    pool: PgPool,
}

impl PgSkillTestRunRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SkillTestRunRepository for PgSkillTestRunRepository {
    async fn list(&self, company_id: Uuid, skill_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError> {
        let rows: Vec<JsonValue> = sqlx::query_scalar(&format!(
            "SELECT {} FROM skill_test_runs str \
             WHERE str.company_id = $1 AND str.skill_id = $2 AND str.deleted_at IS NULL \
             ORDER BY str.created_at DESC",
            skill_test_run_object("str")
        ))
        .bind(company_id)
        .bind(skill_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn get(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> Result<Option<JsonValue>, RepositoryError> {
        let row: Option<JsonValue> = sqlx::query_scalar(&format!(
            "SELECT {} FROM skill_test_runs str \
             WHERE str.id = $1 AND str.skill_id = $2 AND str.company_id = $3",
            skill_test_run_object("str")
        ))
        .bind(run_id)
        .bind(skill_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
    }

    async fn cancel(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> Result<JsonValue, RepositoryError> {
        // Terminal runs are returned untouched: cancelling is idempotent, and the
        // Studio re-reads the row it already has rather than a rewritten outcome.
        let row: Option<JsonValue> = sqlx::query_scalar(&format!(
            "UPDATE skill_test_runs str \
             SET status = 'cancelled', \
                 error = COALESCE(str.error, 'Cancelled by operator'), \
                 completed_at = COALESCE(str.completed_at, NOW()), \
                 updated_at = NOW() \
             WHERE str.id = $1 AND str.skill_id = $2 AND str.company_id = $3 \
               AND str.status NOT IN ('succeeded', 'failed', 'cancelled') \
             RETURNING {}",
            skill_test_run_object("str")
        ))
        .bind(run_id)
        .bind(skill_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        if let Some(row) = row {
            return Ok(row);
        }

        self.get(company_id, skill_id, run_id)
            .await?
            .ok_or(RepositoryError::NotFound(run_id))
    }

    async fn delete(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> Result<(), RepositoryError> {
        // Soft delete: the harness issue keeps its own retention deadline, and a
        // hard delete would orphan the run the issue still points at.
        let affected = sqlx::query(
            r#"
            UPDATE skill_test_runs str
            SET deleted_at = NOW(),
                harness_issue_deleted_at = COALESCE(str.harness_issue_deleted_at, NOW()),
                updated_at = NOW()
            WHERE str.id = $1 AND str.skill_id = $2 AND str.company_id = $3
            "#,
        )
        .bind(run_id)
        .bind(skill_id)
        .bind(company_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .rows_affected();

        if affected == 0 {
            return Err(RepositoryError::NotFound(run_id));
        }

        Ok(())
    }

    async fn latest_active_run(&self, company_id: Uuid, skill_id: Uuid) -> Result<Option<JsonValue>, RepositoryError> {
        let row: Option<JsonValue> = sqlx::query_scalar(&format!(
            "SELECT {} FROM skill_test_runs str \
             WHERE str.company_id = $1 AND str.skill_id = $2 \
               AND str.superseded_at IS NULL AND str.deleted_at IS NULL \
             ORDER BY str.created_at DESC, str.id DESC LIMIT 1",
            skill_test_run_object("str")
        ))
        .bind(company_id)
        .bind(skill_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
    }

    async fn create(&self, company_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        let skill_id: Uuid = data
            .get("skillId")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse().ok())
            .ok_or(RepositoryError::NotFound(Uuid::nil()))?;
        let input_id: Option<Uuid> = data
            .get("inputId")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse().ok());
        let skill_version_id: Option<Uuid> = data
            .get("skillVersionId")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse().ok());
        let agent_id: Option<Uuid> = data
            .get("agentId")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse().ok());
        let issue_id: Option<Uuid> = data
            .get("issueId")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse().ok());
        let started_by_agent_id: Option<Uuid> = data
            .get("startedByAgentId")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse().ok());
        let started_by_user_id: Option<Uuid> = data
            .get("startedByUserId")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse().ok());
        let text = |key: &str| -> Option<String> {
            data.get(key).and_then(|value| value.as_str()).map(str::to_string)
        };

        let mut tx = self.pool.begin().await.map_err(RepositoryError::DatabaseError)?;

        // One live run per (skill, input) slot: the newer run supersedes the older
        // one instead of racing it, and the older run's harness task is given a
        // retention deadline so it can be reclaimed.
        sqlx::query(
            r#"
            UPDATE skill_test_runs
            SET superseded_at = NOW(),
                status = CASE WHEN status IN ('queued', 'running') THEN 'cancelled' ELSE status END,
                error = CASE WHEN status IN ('queued', 'running')
                             THEN COALESCE(error, 'Superseded by newer run') ELSE error END,
                harness_issue_expires_at = NOW() + INTERVAL '7 days',
                updated_at = NOW()
            WHERE company_id = $1 AND skill_id = $2
              AND (input_id = $3 OR ($3::uuid IS NULL AND input_id IS NULL))
              AND superseded_at IS NULL
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .bind(input_id)
        .execute(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        let row: Option<JsonValue> = sqlx::query_scalar(&format!(
            r#"
            INSERT INTO skill_test_runs (
                company_id, skill_id, input_id, input_snapshot, skill_version_id, agent_id,
                agent_config_snapshot, issue_id, template_id, template_name, template_body,
                rendered_template_body, harness_issue_description, output_document_key,
                status, started_by_agent_id, started_by_user_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, 'queued', $15, $16)
            RETURNING {}
            "#,
            skill_test_run_object("skill_test_runs")
        ))
        .bind(company_id)
        .bind(skill_id)
        .bind(input_id)
        .bind(text("inputSnapshot").unwrap_or_default())
        .bind(skill_version_id)
        .bind(agent_id)
        .bind(
            data.get("agentConfigSnapshot")
                .cloned()
                .unwrap_or_else(|| JsonValue::Object(serde_json::Map::new())),
        )
        .bind(issue_id)
        .bind(text("templateId"))
        .bind(text("templateName"))
        .bind(text("templateBody"))
        .bind(text("renderedTemplateBody"))
        .bind(text("harnessIssueDescription").unwrap_or_default())
        .bind(text("outputDocumentKey").unwrap_or_else(|| "output".to_string()))
        .bind(started_by_agent_id)
        .bind(started_by_user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        tx.commit().await.map_err(RepositoryError::DatabaseError)?;

        row.ok_or(RepositoryError::NotFound(skill_id))
    }

    async fn mark_running(&self, company_id: Uuid, issue_id: Uuid) -> Result<Option<JsonValue>, RepositoryError> {
        let row: Option<JsonValue> = sqlx::query_scalar(&format!(
            "UPDATE skill_test_runs str \
             SET status = 'running', started_at = COALESCE(str.started_at, NOW()), updated_at = NOW() \
             WHERE str.company_id = $1 AND str.issue_id = $2 AND str.status = 'queued' \
               AND str.deleted_at IS NULL AND str.superseded_at IS NULL \
             RETURNING {}",
            skill_test_run_object("str")
        ))
        .bind(company_id)
        .bind(issue_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
    }

    async fn complete_for_issue(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        outcome: &str,
        error: Option<String>,
    ) -> Result<Option<JsonValue>, RepositoryError> {
        let existing: Option<JsonValue> = sqlx::query_scalar(&format!(
            "SELECT {} FROM skill_test_runs str \
             WHERE str.company_id = $1 AND str.issue_id = $2 \
               AND str.deleted_at IS NULL AND str.superseded_at IS NULL \
             ORDER BY str.created_at DESC, str.id DESC LIMIT 1",
            skill_test_run_object("str")
        ))
        .bind(company_id)
        .bind(issue_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        let Some(existing) = existing else {
            return Ok(None);
        };
        let run_id: Uuid = existing
            .get("id")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse().ok())
            .ok_or(RepositoryError::NotFound(Uuid::nil()))?;

        // A run that already settled keeps its outcome: the issue can be closed
        // more than once, and the first verdict is the real one.
        let status = existing.get("status").and_then(|value| value.as_str()).unwrap_or_default();
        if matches!(status, "succeeded" | "failed" | "cancelled") {
            return Ok(Some(existing));
        }

        let output_document_key = existing
            .get("outputDocumentKey")
            .and_then(|value| value.as_str())
            .unwrap_or("output")
            .to_string();
        let output_body: Option<String> = sqlx::query_scalar(
            r#"
            SELECT d.content
            FROM issue_documents idoc
            JOIN documents d ON d.id = idoc.document_id
            WHERE idoc.company_id = $1 AND idoc.issue_id = $2 AND idoc.key = $3
            LIMIT 1
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .bind(&output_document_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        let previous_snapshot = existing
            .get("outputSnapshot")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();

        let row: Option<JsonValue> = sqlx::query_scalar(&format!(
            "UPDATE skill_test_runs str \
             SET status = $3, output_snapshot = $4, error = $5, \
                 completed_at = NOW(), updated_at = NOW() \
             WHERE str.id = $1 AND str.company_id = $2 \
             RETURNING {}",
            skill_test_run_object("str")
        ))
        .bind(run_id)
        .bind(company_id)
        .bind(outcome)
        .bind(output_body.unwrap_or(previous_snapshot))
        .bind(error)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
    }
}

// ─── PgSkillStarRepository ────────────────────────────────────

pub struct PgSkillStarRepository {
    pool: PgPool,
}

impl PgSkillStarRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SkillStarRepository for PgSkillStarRepository {
    async fn star(&self, company_id: Uuid, skill_id: Uuid, user_id: Uuid) -> Result<JsonValue, RepositoryError> {
        let row: JsonValue = sqlx::query_scalar(
            r#"
            INSERT INTO skill_stars (company_id, skill_id, user_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (company_id, skill_id, user_id) DO NOTHING
            RETURNING jsonb_build_object(
                'skillId', skill_id,
                'starred', true,
                'createdAt', created_at
            )
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .unwrap_or(serde_json::json!({
            "skillId": skill_id,
            "starred": true,
        }));

        Ok(row)
    }

    async fn unstar(&self, company_id: Uuid, skill_id: Uuid, user_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM skill_stars
            WHERE company_id = $1 AND skill_id = $2 AND user_id = $3
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }
}

// ─── PgSkillCommentRepository ─────────────────────────────────

pub struct PgSkillCommentRepository {
    pool: PgPool,
}

impl PgSkillCommentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SkillCommentRepository for PgSkillCommentRepository {
    async fn list(&self, company_id: Uuid, skill_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError> {
        let rows: Vec<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', sc.id,
                'skillId', sc.skill_id,
                'parentCommentId', sc.parent_comment_id,
                'body', sc.body,
                'authorAgentId', sc.author_agent_id,
                'authorUserId', sc.author_user_id,
                'createdAt', sc.created_at,
                'updatedAt', sc.updated_at
            )
            FROM skill_comments sc
            JOIN company_skills cs ON cs.id = sc.skill_id AND cs.company_id = $1
            WHERE sc.skill_id = $2
            ORDER BY sc.created_at ASC
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn create(&self, company_id: Uuid, skill_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        let body = data.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let parent_comment_id: Option<Uuid> = data.get("parentCommentId")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok());
        let author_user_id: Option<Uuid> = data.get("authorUserId")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok());
        let author_agent_id: Option<Uuid> = data.get("authorAgentId")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok());

        let row: JsonValue = sqlx::query_scalar(
            r#"
            INSERT INTO skill_comments (company_id, skill_id, parent_comment_id, body, author_agent_id, author_user_id)
            SELECT $1, $2, $3, $4, $5, $6
            WHERE EXISTS (SELECT 1 FROM company_skills WHERE id = $2 AND company_id = $1)
            RETURNING jsonb_build_object(
                'id', id,
                'skillId', skill_id,
                'parentCommentId', parent_comment_id,
                'body', body,
                'authorAgentId', author_agent_id,
                'authorUserId', author_user_id,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .bind(parent_comment_id)
        .bind(body)
        .bind(author_agent_id)
        .bind(author_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or_else(|| RepositoryError::NotFound(skill_id))?;

        Ok(row)
    }

    async fn update(&self, company_id: Uuid, skill_id: Uuid, comment_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        let body = data.get("body").and_then(|v| v.as_str());

        let row: JsonValue = sqlx::query_scalar(
            r#"
            UPDATE skill_comments sc
            SET
                body = COALESCE($4, sc.body),
                updated_at = NOW()
            FROM company_skills cs
            WHERE sc.id = $1 AND sc.skill_id = $2 AND cs.id = sc.skill_id AND cs.company_id = $3
            RETURNING jsonb_build_object(
                'id', sc.id,
                'skillId', sc.skill_id,
                'parentCommentId', sc.parent_comment_id,
                'body', sc.body,
                'authorAgentId', sc.author_agent_id,
                'authorUserId', sc.author_user_id,
                'createdAt', sc.created_at,
                'updatedAt', sc.updated_at
            )
            "#,
        )
        .bind(comment_id)
        .bind(skill_id)
        .bind(company_id)
        .bind(body)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or_else(|| RepositoryError::NotFound(comment_id))?;

        Ok(row)
    }

    async fn delete(&self, company_id: Uuid, skill_id: Uuid, comment_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM skill_comments sc
            USING company_skills cs
            WHERE sc.id = $1 AND sc.skill_id = $2 AND cs.id = sc.skill_id AND cs.company_id = $3
            "#,
        )
        .bind(comment_id)
        .bind(skill_id)
        .bind(company_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }
}

// ─── PgSkillFileRepository ────────────────────────────────────

pub struct PgSkillFileRepository {
    pool: PgPool,
}

impl PgSkillFileRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SkillFileRepository for PgSkillFileRepository {
    async fn list(&self, company_id: Uuid, skill_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError> {
        let rows: Vec<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', sf.id,
                'skillId', sf.skill_id,
                'path', sf.path,
                'content', sf.content,
                'mimeType', sf.mime_type,
                'sizeBytes', sf.size_bytes,
                'createdAt', sf.created_at,
                'updatedAt', sf.updated_at
            )
            FROM skill_files sf
            JOIN company_skills cs ON cs.id = sf.skill_id AND cs.company_id = $1
            WHERE sf.skill_id = $2
            ORDER BY sf.path
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn update(&self, company_id: Uuid, skill_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        // Upsert: insert or update files
        let files = data.get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut results = Vec::new();

        for file in &files {
            let path = file.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = file.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let mime_type = file.get("mimeType").and_then(|v| v.as_str());

            let row: Option<JsonValue> = sqlx::query_scalar(
                r#"
                INSERT INTO skill_files (company_id, skill_id, path, content, mime_type, size_bytes)
                SELECT $1, $2, $3, $4, $5, LENGTH($4)
                WHERE EXISTS (SELECT 1 FROM company_skills WHERE id = $2 AND company_id = $1)
                ON CONFLICT (skill_id, path) DO UPDATE SET
                    content = EXCLUDED.content,
                    mime_type = COALESCE(EXCLUDED.mime_type, skill_files.mime_type),
                    size_bytes = LENGTH(EXCLUDED.content),
                    updated_at = NOW()
                RETURNING jsonb_build_object(
                    'id', id,
                    'skillId', skill_id,
                    'path', path,
                    'content', content,
                    'mimeType', mime_type,
                    'sizeBytes', size_bytes,
                    'createdAt', created_at,
                    'updatedAt', updated_at
                )
                "#,
            )
            .bind(company_id)
            .bind(skill_id)
            .bind(path)
            .bind(content)
            .bind(mime_type)
            .fetch_optional(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

            if let Some(r) = row {
                results.push(r);
            }
        }

        Ok(serde_json::json!({
            "skillId": skill_id,
            "files": results,
            "updated": true,
        }))
    }

    async fn delete(&self, company_id: Uuid, skill_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM skill_files sf
            USING company_skills cs
            WHERE sf.skill_id = $1 AND cs.id = sf.skill_id AND cs.company_id = $2
            "#,
        )
        .bind(skill_id)
        .bind(company_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }
}
