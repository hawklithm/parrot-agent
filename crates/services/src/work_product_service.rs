use async_trait::async_trait;
use models::issue_auxiliary::{CreateWorkProductInput, UpdateWorkProductInput, WorkProduct};
use uuid::Uuid;

use crate::errors::ServiceResult;
use sqlx::PgPool;

/// Work product service trait
#[async_trait]
pub trait WorkProductService: Send + Sync {
    /// List work products for an issue
    async fn list_work_products(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<Vec<WorkProduct>>;

    /// Create a work product for an issue
    async fn create_work_product(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: CreateWorkProductInput,
    ) -> ServiceResult<WorkProduct>;

    /// Update a work product by id
    async fn update_work_product(
        &self,
        product_id: Uuid,
        company_id: Uuid,
        input: UpdateWorkProductInput,
    ) -> ServiceResult<WorkProduct>;

    /// Delete a work product by id
    async fn delete_work_product(&self, product_id: Uuid, company_id: Uuid) -> ServiceResult<()>;
}

/// Mock implementation
pub struct MockWorkProductService;

/// PostgreSQL-backed work-product service.  Paperclip stores these records as
/// issue work products; keep the service scoped by company on every query.
pub struct PgWorkProductService {
    pool: PgPool,
}

impl PgWorkProductService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn validate_work_product_value(
    work_product_type: Option<&str>,
    provider: Option<&str>,
    title: Option<&str>,
    url: Option<&str>,
    status: Option<&str>,
    review_state: Option<&str>,
    health_status: Option<&str>,
) -> ServiceResult<()> {
    const TYPES: [&str; 7] = [
        "preview_url",
        "runtime_service",
        "pull_request",
        "branch",
        "commit",
        "artifact",
        "document",
    ];
    const STATUSES: [&str; 9] = [
        "active",
        "ready_for_review",
        "approved",
        "changes_requested",
        "merged",
        "closed",
        "failed",
        "archived",
        "draft",
    ];
    const REVIEWS: [&str; 4] = [
        "none",
        "needs_board_review",
        "approved",
        "changes_requested",
    ];
    const HEALTH: [&str; 3] = ["unknown", "healthy", "unhealthy"];

    if let Some(value) = work_product_type.filter(|value| !TYPES.contains(value)) {
        return Err(crate::errors::ServiceError::Validation(format!(
            "unsupported work product type: {value}"
        )));
    }
    if let Some(value) = provider.filter(|value| value.trim().is_empty()) {
        return Err(crate::errors::ServiceError::Validation(
            "provider must not be empty".into(),
        ));
    }
    if let Some(value) = title.filter(|value| value.trim().is_empty()) {
        return Err(crate::errors::ServiceError::Validation(
            "title must not be empty".into(),
        ));
    }
    if let Some(value) =
        url.filter(|value| !(value.starts_with("https://") || value.starts_with("http://")))
    {
        return Err(crate::errors::ServiceError::Validation(format!(
            "invalid work product URL: {value}"
        )));
    }
    if let Some(value) = status.filter(|value| !STATUSES.contains(value)) {
        return Err(crate::errors::ServiceError::Validation(format!(
            "unsupported work product status: {value}"
        )));
    }
    if let Some(value) = review_state.filter(|value| !REVIEWS.contains(value)) {
        return Err(crate::errors::ServiceError::Validation(format!(
            "unsupported work product review state: {value}"
        )));
    }
    if let Some(value) = health_status.filter(|value| !HEALTH.contains(value)) {
        return Err(crate::errors::ServiceError::Validation(format!(
            "unsupported work product health status: {value}"
        )));
    }
    Ok(())
}

#[async_trait]
impl WorkProductService for PgWorkProductService {
    async fn list_work_products(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<Vec<WorkProduct>> {
        sqlx::query_as::<_, WorkProduct>(
            "SELECT id, issue_id, company_id, project_id, execution_workspace_id, runtime_service_id,
                    name, description, artifact, type AS work_product_type, provider, external_id,
                    title, url, status, review_state, is_primary, health_status, summary, metadata,
                    source_trust, created_by_run_id, created_at, updated_at
             FROM issue_work_products WHERE issue_id = $1 AND company_id = $2
             ORDER BY is_primary DESC, updated_at DESC, id DESC")
            .bind(issue_id).bind(company_id).fetch_all(&self.pool).await.map_err(Into::into)
    }

    async fn create_work_product(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: CreateWorkProductInput,
    ) -> ServiceResult<WorkProduct> {
        validate_work_product_value(
            input.work_product_type.as_deref(),
            input.provider.as_deref(),
            input.title.as_deref().or(input.name.as_deref()),
            input.url.as_deref(),
            input.status.as_deref(),
            input.review_state.as_deref(),
            input.health_status.as_deref(),
        )?;
        let title = input.title.clone().or(input.name.clone()).ok_or_else(|| {
            crate::errors::ServiceError::Validation("title or name is required".into())
        })?;
        let artifact = input
            .artifact
            .clone()
            .or(input.metadata.clone())
            .unwrap_or_else(|| serde_json::json!({}));
        let summary = input.summary.clone().or(input.description.clone());
        let work_product_type = input
            .work_product_type
            .clone()
            .unwrap_or_else(|| "artifact".into());
        let mut tx = self.pool.begin().await?;
        if input.is_primary.unwrap_or(false) {
            sqlx::query(
                "UPDATE issue_work_products SET is_primary = FALSE, updated_at = NOW()
                 WHERE company_id = $1 AND issue_id = $2 AND type = $3",
            )
            .bind(company_id)
            .bind(issue_id)
            .bind(&work_product_type)
            .execute(&mut *tx)
            .await?;
        }
        let product = sqlx::query_as::<_, WorkProduct>(
            "INSERT INTO issue_work_products
             (issue_id, company_id, project_id, execution_workspace_id, runtime_service_id,
              name, description, artifact, type, provider, external_id, title, url, status,
              review_state, is_primary, health_status, summary, metadata, source_trust, created_by_run_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)
             RETURNING id, issue_id, company_id, project_id, execution_workspace_id, runtime_service_id,
                       name, description, artifact, type AS work_product_type, provider, external_id,
                       title, url, status, review_state, is_primary, health_status, summary, metadata,
                       source_trust, created_by_run_id, created_at, updated_at")
            .bind(issue_id).bind(company_id).bind(input.project_id).bind(input.execution_workspace_id)
            .bind(input.runtime_service_id).bind(input.name.clone().unwrap_or_else(|| title.clone()))
            .bind(input.description).bind(artifact).bind(work_product_type)
            .bind(input.provider.unwrap_or_else(|| "parrot".into())).bind(input.external_id).bind(title)
            .bind(input.url).bind(input.status.unwrap_or_else(|| "active".into()))
            .bind(input.review_state.unwrap_or_else(|| "none".into())).bind(input.is_primary.unwrap_or(false))
            .bind(input.health_status.unwrap_or_else(|| "unknown".into())).bind(summary)
            .bind(input.metadata).bind(input.source_trust).bind(input.created_by_run_id)
            .fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(product)
    }

    async fn update_work_product(
        &self,
        product_id: Uuid,
        company_id: Uuid,
        input: UpdateWorkProductInput,
    ) -> ServiceResult<WorkProduct> {
        validate_work_product_value(
            input.work_product_type.as_deref(),
            input.provider.as_deref(),
            input.title.as_deref().or(input.name.as_deref()),
            input.url.as_deref(),
            input.status.as_deref(),
            input.review_state.as_deref(),
            input.health_status.as_deref(),
        )?;
        let mut tx = self.pool.begin().await?;
        if input.is_primary.unwrap_or(false) {
            let work_product_type: String = if let Some(value) = input.work_product_type.clone() {
                value
            } else {
                sqlx::query_scalar(
                    "SELECT type FROM issue_work_products WHERE id = $1 AND company_id = $2",
                )
                .bind(product_id)
                .bind(company_id)
                .fetch_one(&mut *tx)
                .await?
            };
            sqlx::query(
                "UPDATE issue_work_products SET is_primary = FALSE, updated_at = NOW()
                 WHERE company_id = $1 AND type = $2 AND id <> $3
                   AND issue_id = (SELECT issue_id FROM issue_work_products WHERE id = $3 AND company_id = $1)",
            )
            .bind(company_id)
            .bind(work_product_type)
            .bind(product_id)
            .execute(&mut *tx)
            .await?;
        }
        let product = sqlx::query_as::<_, WorkProduct>(
            "UPDATE issue_work_products SET
             project_id=COALESCE($3,project_id), execution_workspace_id=COALESCE($4,execution_workspace_id),
             runtime_service_id=COALESCE($5,runtime_service_id), name=COALESCE($6,name),
             description=COALESCE($7,description), artifact=COALESCE($8,artifact),
             type=COALESCE($9,type), provider=COALESCE($10,provider), external_id=COALESCE($11,external_id),
             title=COALESCE($12,title), url=COALESCE($13,url), status=COALESCE($14,status),
             review_state=COALESCE($15,review_state), is_primary=COALESCE($16,is_primary),
             health_status=COALESCE($17,health_status), summary=COALESCE($18,summary),
             metadata=COALESCE($19,metadata), source_trust=COALESCE($20,source_trust),
             created_by_run_id=COALESCE($21,created_by_run_id), updated_at=NOW()
             WHERE id = $1 AND company_id = $2
             RETURNING id, issue_id, company_id, project_id, execution_workspace_id, runtime_service_id,
                       name, description, artifact, type AS work_product_type, provider, external_id,
                       title, url, status, review_state, is_primary, health_status, summary, metadata,
                       source_trust, created_by_run_id, created_at, updated_at")
            .bind(product_id).bind(company_id).bind(input.project_id).bind(input.execution_workspace_id)
            .bind(input.runtime_service_id).bind(input.name).bind(input.description).bind(input.artifact)
            .bind(input.work_product_type).bind(input.provider).bind(input.external_id).bind(input.title)
            .bind(input.url).bind(input.status).bind(input.review_state).bind(input.is_primary)
            .bind(input.health_status).bind(input.summary).bind(input.metadata).bind(input.source_trust)
            .bind(input.created_by_run_id)
            .fetch_optional(&mut *tx).await.map_err(ServiceErrorFromSql::into_service)?
            .ok_or_else(|| crate::errors::ServiceError::NotFound("work product not found".into()))?;
        tx.commit().await?;
        Ok(product)
    }

    async fn delete_work_product(&self, product_id: Uuid, company_id: Uuid) -> ServiceResult<()> {
        let result =
            sqlx::query("DELETE FROM issue_work_products WHERE id = $1 AND company_id = $2")
                .bind(product_id)
                .bind(company_id)
                .execute(&self.pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(crate::errors::ServiceError::NotFound(
                "work product not found".into(),
            ));
        }
        Ok(())
    }
}

struct ServiceErrorFromSql;
impl ServiceErrorFromSql {
    fn into_service(e: sqlx::Error) -> crate::errors::ServiceError {
        e.into()
    }
}

impl MockWorkProductService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl WorkProductService for MockWorkProductService {
    async fn list_work_products(
        &self,
        _issue_id: Uuid,
        _company_id: Uuid,
    ) -> ServiceResult<Vec<WorkProduct>> {
        Ok(Vec::new())
    }

    async fn create_work_product(
        &self,
        _issue_id: Uuid,
        _company_id: Uuid,
        _input: CreateWorkProductInput,
    ) -> ServiceResult<WorkProduct> {
        Err(crate::errors::ServiceError::NotImplemented(
            "WorkProductService::create_work_product not implemented".to_string(),
        ))
    }

    async fn update_work_product(
        &self,
        _product_id: Uuid,
        _company_id: Uuid,
        _input: UpdateWorkProductInput,
    ) -> ServiceResult<WorkProduct> {
        Err(crate::errors::ServiceError::NotImplemented(
            "WorkProductService::update_work_product not implemented".to_string(),
        ))
    }

    async fn delete_work_product(&self, _product_id: Uuid, _company_id: Uuid) -> ServiceResult<()> {
        Err(crate::errors::ServiceError::NotImplemented(
            "WorkProductService::delete_work_product not implemented".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::validate_work_product_value;

    #[test]
    fn accepts_paperclip_work_product_values() {
        validate_work_product_value(
            Some("pull_request"),
            Some("github"),
            Some("Review PR"),
            Some("https://github.com/example/repo/pull/1"),
            Some("ready_for_review"),
            Some("needs_board_review"),
            Some("healthy"),
        )
        .expect("valid work product should pass");
    }

    #[test]
    fn rejects_invalid_work_product_enum_and_url() {
        assert!(validate_work_product_value(
            Some("unknown"),
            Some("provider"),
            Some("title"),
            None,
            None,
            None,
            None,
        )
        .is_err());
        assert!(validate_work_product_value(
            None,
            Some("provider"),
            Some("title"),
            Some("file:///tmp/output"),
            None,
            None,
            None,
        )
        .is_err());
    }
}
