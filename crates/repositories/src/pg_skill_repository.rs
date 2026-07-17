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

#[async_trait]
impl CompanySkillRepository for PgCompanySkillRepository {
    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<JsonValue>, RepositoryError> {
        let rows: Vec<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', cs.id,
                'companyId', cs.company_id,
                'catalogId', cs.catalog_id,
                'name', cs.name,
                'slug', cs.slug,
                'description', cs.description,
                'category', cs.category,
                'version', cs.version,
                'tags', cs.tags,
                'config', cs.config,
                'isPaperclipManaged', cs.is_paperclip_managed,
                'isFork', cs.is_fork,
                'forkedFromSkillId', cs.forked_from_skill_id,
                'forkedFromCatalogId', cs.forked_from_catalog_id,
                'status', cs.status,
                'updateAvailable', cs.update_available,
                'latestVersion', cs.latest_version,
                'createdAt', cs.created_at,
                'updatedAt', cs.updated_at
            )
            FROM company_skills cs
            WHERE cs.company_id = $1
            ORDER BY cs.name
            "#,
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn get_by_id(&self, company_id: Uuid, skill_id: Uuid) -> Result<Option<JsonValue>, RepositoryError> {
        let row: Option<JsonValue> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'id', cs.id,
                'companyId', cs.company_id,
                'catalogId', cs.catalog_id,
                'name', cs.name,
                'slug', cs.slug,
                'description', cs.description,
                'category', cs.category,
                'version', cs.version,
                'tags', cs.tags,
                'config', cs.config,
                'isPaperclipManaged', cs.is_paperclip_managed,
                'isFork', cs.is_fork,
                'forkedFromSkillId', cs.forked_from_skill_id,
                'forkedFromCatalogId', cs.forked_from_catalog_id,
                'status', cs.status,
                'updateAvailable', cs.update_available,
                'latestVersion', cs.latest_version,
                'createdAt', cs.created_at,
                'updatedAt', cs.updated_at
            )
            FROM company_skills cs
            WHERE cs.id = $1 AND cs.company_id = $2
            "#,
        )
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

        let row: JsonValue = sqlx::query_scalar(
            r#"
            INSERT INTO company_skills (company_id, catalog_id, name, slug, description, category, is_paperclip_managed)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING jsonb_build_object(
                'id', id,
                'companyId', company_id,
                'catalogId', catalog_id,
                'name', name,
                'slug', slug,
                'description', description,
                'category', category,
                'version', version,
                'tags', tags,
                'config', config,
                'isPaperclipManaged', is_paperclip_managed,
                'isFork', is_fork,
                'status', status,
                'updateAvailable', update_available,
                'latestVersion', latest_version,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            "#,
        )
        .bind(company_id)
        .bind(catalog_id)
        .bind(name)
        .bind(slug)
        .bind(description)
        .bind(category)
        .bind(is_paperclip_managed)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row)
    }

    async fn update(&self, company_id: Uuid, skill_id: Uuid, data: JsonValue) -> Result<JsonValue, RepositoryError> {
        let name = data.get("name").and_then(|v| v.as_str());
        let description = data.get("description").and_then(|v| v.as_str());
        let category = data.get("category").and_then(|v| v.as_str());
        let status = data.get("status").and_then(|v| v.as_str());

        let row: JsonValue = sqlx::query_scalar(
            r#"
            UPDATE company_skills
            SET
                name = COALESCE($3, name),
                description = COALESCE($4, description),
                category = COALESCE($5, category),
                status = COALESCE($6, status),
                updated_at = NOW()
            WHERE id = $1 AND company_id = $2
            RETURNING jsonb_build_object(
                'id', id,
                'companyId', company_id,
                'catalogId', catalog_id,
                'name', name,
                'slug', slug,
                'description', description,
                'category', category,
                'version', version,
                'tags', tags,
                'config', config,
                'isPaperclipManaged', is_paperclip_managed,
                'isFork', is_fork,
                'status', status,
                'updateAvailable', update_available,
                'latestVersion', latest_version,
                'createdAt', created_at,
                'updatedAt', updated_at
            )
            "#,
        )
        .bind(skill_id)
        .bind(company_id)
        .bind(name)
        .bind(description)
        .bind(category)
        .bind(status)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or_else(|| RepositoryError::NotFound(skill_id))?;

        Ok(row)
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
            INSERT INTO company_skills (company_id, name, slug, description, is_fork, forked_from_skill_id, is_paperclip_managed)
            VALUES ($1, $2, $3, $4, true, $5, false)
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
            INSERT INTO company_skills (company_id, catalog_id, name, slug, description, category, is_paperclip_managed)
            SELECT $1, sc.id, sc.name, LOWER(REPLACE(sc.name, ' ', '-')), sc.description, sc.category, sc.is_paperclip_managed
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
                'skillId', sv.skill_id,
                'version', sv.version,
                'files', sv.files,
                'metadata', sv.metadata,
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
                'skillId', sv.skill_id,
                'version', sv.version,
                'files', sv.files,
                'metadata', sv.metadata,
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
