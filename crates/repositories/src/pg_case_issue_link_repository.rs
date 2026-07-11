use async_trait::async_trait;
use sqlx::PgPool;
use models::{CaseIssueLink, CaseIssueLinkRole};
use uuid::Uuid;
use crate::{
    case_issue_link_repository::{CaseIssueLinkRepository, CreateCaseIssueLinkInput},
    RepositoryError,
};

pub struct PgCaseIssueLinkRepository {
    pool: PgPool,
}

impl PgCaseIssueLinkRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CaseIssueLinkRepository for PgCaseIssueLinkRepository {
    async fn create(&self, input: CreateCaseIssueLinkInput) -> Result<CaseIssueLink, RepositoryError> {
        let link = sqlx::query_as::<_, CaseIssueLink>(
            r#"
            INSERT INTO case_issue_links (
                company_id, case_id, issue_id, role, created_by_run_id
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(input.company_id)
        .bind(input.case_id)
        .bind(input.issue_id)
        .bind(input.role)
        .bind(input.created_by_run_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(link)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<CaseIssueLink>, RepositoryError> {
        let link = sqlx::query_as::<_, CaseIssueLink>(
            r#"
            SELECT * FROM case_issue_links WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(link)
    }

    async fn list_by_case(&self, case_id: Uuid) -> Result<Vec<CaseIssueLink>, RepositoryError> {
        let links = sqlx::query_as::<_, CaseIssueLink>(
            r#"
            SELECT * FROM case_issue_links
            WHERE case_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(links)
    }

    async fn list_by_issue(&self, issue_id: Uuid) -> Result<Vec<CaseIssueLink>, RepositoryError> {
        let links = sqlx::query_as::<_, CaseIssueLink>(
            r#"
            SELECT * FROM case_issue_links
            WHERE issue_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(links)
    }

    async fn find_by_case_issue_role(
        &self,
        case_id: Uuid,
        issue_id: Uuid,
        role: CaseIssueLinkRole,
    ) -> Result<Option<CaseIssueLink>, RepositoryError> {
        let link = sqlx::query_as::<_, CaseIssueLink>(
            r#"
            SELECT * FROM case_issue_links
            WHERE case_id = $1 AND issue_id = $2 AND role = $3
            "#,
        )
        .bind(case_id)
        .bind(issue_id)
        .bind(role)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(link)
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM case_issue_links WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }

    async fn delete_by_case_and_issue(&self, case_id: Uuid, issue_id: Uuid) -> Result<u64, RepositoryError> {
        let result = sqlx::query(
            r#"
            DELETE FROM case_issue_links
            WHERE case_id = $1 AND issue_id = $2
            "#,
        )
        .bind(case_id)
        .bind(issue_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(result.rows_affected())
    }
}
