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
fn company_skill_current_version_id(alias: &str) -> String {
    format!(
        "(SELECT sv.id FROM skill_versions sv \
         WHERE sv.company_id = {alias}.company_id AND sv.skill_id = {alias}.id \
         ORDER BY sv.created_at DESC, sv.id DESC LIMIT 1)"
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
                "(SELECT jsonb_build_object( \
                    'id', sv.id, 'companyId', sv.company_id, 'companySkillId', sv.skill_id, \
                    'revisionNumber', (SELECT COUNT(*) FROM skill_versions prior \
                        WHERE prior.skill_id = sv.skill_id \
                          AND (prior.created_at, prior.id) <= (sv.created_at, sv.id)), \
                    'label', sv.version, 'fileInventory', '[]'::jsonb, \
                    'authorAgentId', sv.created_by_agent_id, \
                    'authorUserId', sv.created_by_user_id, \
                    'createdAt', sv.created_at) \
                  FROM skill_versions sv \
                  WHERE sv.company_id = {alias}.company_id AND sv.skill_id = {alias}.id \
                  ORDER BY sv.created_at DESC, sv.id DESC LIMIT 1)"
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


#[async_trait]
impl CompanySkillRepository for PgCompanySkillRepository {
    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError> {
        let rows: Vec<JsonValue> = sqlx::query_scalar(&format!(
            "SELECT {} FROM company_skills cs \
             WHERE cs.company_id = $1 ORDER BY cs.name",
            company_skill_object("cs", &[])
        ))
        .bind(company_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

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

        Ok(row)
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
        let rows: Vec<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', sv.id,
                'companyId', sv.company_id,
                'companySkillId', sv.skill_id,
                'revisionNumber', ROW_NUMBER() OVER (PARTITION BY sv.skill_id ORDER BY sv.created_at, sv.id),
                'label', sv.version,
                'releaseId', sv.release_id,
                'releaseName', sv.release_name,
                'releasedAt', sv.released_at,
                'files', sv.files,
                'fileInventory', '[]'::jsonb,
                'authorAgentId', sv.created_by_agent_id,
                'authorUserId', sv.created_by_user_id,
                'createdAt', sv.created_at
            )
            FROM skill_versions sv
            JOIN company_skills cs ON cs.id = sv.skill_id AND cs.company_id = $1
            WHERE sv.skill_id = $2
            ORDER BY sv.created_at DESC
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn get_version(&self, company_id: Uuid, skill_id: Uuid, version_id: Uuid) -> Result<Option<JsonValue>, RepositoryError> {
        let row: Option<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', sv.id,
                'companyId', sv.company_id,
                'companySkillId', sv.skill_id,
                'revisionNumber', (
                    SELECT COUNT(*) + 1
                    FROM skill_versions previous
                    WHERE previous.skill_id = sv.skill_id
                      AND (previous.created_at, previous.id) < (sv.created_at, sv.id)
                ),
                'label', sv.version,
                'releaseId', sv.release_id,
                'releaseName', sv.release_name,
                'releasedAt', sv.released_at,
                'files', sv.files,
                'fileInventory', '[]'::jsonb,
                'authorAgentId', sv.created_by_agent_id,
                'authorUserId', sv.created_by_user_id,
                'createdAt', sv.created_at
            )
            FROM skill_versions sv
            JOIN company_skills cs ON cs.id = sv.skill_id AND cs.company_id = $1
            WHERE sv.id = $2 AND sv.skill_id = $3
            "#,
        )
        .bind(company_id)
        .bind(version_id)
        .bind(skill_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
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
                'config', config,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            FROM skill_test_run_templates
            WHERE company_id = $1
            ORDER BY name
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
        let config = data.get("config").cloned().unwrap_or(JsonValue::Null);

        let row: JsonValue = sqlx::query_scalar(
            r#"
            INSERT INTO skill_test_run_templates (company_id, name, config)
            VALUES ($1, $2, $3)
            RETURNING jsonb_build_object(
                'id', id,
                'companyId', company_id,
                'name', name,
                'config', config,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            "#,
        )
        .bind(company_id)
        .bind(name)
        .bind(&config)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
    }

    async fn update(&self, company_id: Uuid, template_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        let name = data.get("name").and_then(|v| v.as_str());
        let config = data.get("config");

        let row: JsonValue = sqlx::query_scalar(
            r#"
            UPDATE skill_test_run_templates
            SET
                name = COALESCE($3, name),
                config = COALESCE($4, config),
                updated_at = NOW()
            WHERE id = $1 AND company_id = $2
            RETURNING jsonb_build_object(
                'id', id,
                'companyId', company_id,
                'name', name,
                'config', config,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            "#,
        )
        .bind(template_id)
        .bind(company_id)
        .bind(name)
        .bind(config)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or_else(|| RepositoryError::NotFound(template_id))?;

        Ok(row)
    }

    async fn delete(&self, company_id: Uuid, template_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM skill_test_run_templates
            WHERE id = $1 AND company_id = $2
            "#,
        )
        .bind(template_id)
        .bind(company_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

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
        let rows: Vec<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', str.id,
                'skillId', str.skill_id,
                'templateId', str.template_id,
                'status', str.status,
                'result', str.result,
                'startedAt', str.started_at,
                'completedAt', str.completed_at,
                'createdAt', str.created_at
            )
            FROM skill_test_runs str
            JOIN company_skills cs ON cs.id = str.skill_id AND cs.company_id = $1
            WHERE str.skill_id = $2
            ORDER BY str.created_at DESC
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn get(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> Result<Option<JsonValue>, RepositoryError> {
        let row: Option<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', str.id,
                'skillId', str.skill_id,
                'templateId', str.template_id,
                'status', str.status,
                'result', str.result,
                'startedAt', str.started_at,
                'completedAt', str.completed_at,
                'createdAt', str.created_at
            )
            FROM skill_test_runs str
            JOIN company_skills cs ON cs.id = str.skill_id AND cs.company_id = $1
            WHERE str.id = $2 AND str.skill_id = $3
            "#,
        )
        .bind(company_id)
        .bind(run_id)
        .bind(skill_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
    }

    async fn cancel(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> Result<JsonValue, RepositoryError> {
        let row: JsonValue = sqlx::query_scalar(
            r#"
            UPDATE skill_test_runs str
            SET status = 'cancelled', updated_at = NOW()
            FROM company_skills cs
            WHERE str.id = $1 AND str.skill_id = $2 AND cs.id = str.skill_id AND cs.company_id = $3
            RETURNING jsonb_build_object(
                'id', str.id,
                'skillId', str.skill_id,
                'status', str.status,
                'result', str.result,
                'startedAt', str.started_at,
                'completedAt', str.completed_at
            )
            "#,
        )
        .bind(run_id)
        .bind(skill_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or_else(|| RepositoryError::NotFound(run_id))?;

        Ok(row)
    }

    async fn delete(&self, company_id: Uuid, skill_id: Uuid, run_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM skill_test_runs str
            USING company_skills cs
            WHERE str.id = $1 AND str.skill_id = $2 AND cs.id = str.skill_id AND cs.company_id = $3
            "#,
        )
        .bind(run_id)
        .bind(skill_id)
        .bind(company_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(())
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
