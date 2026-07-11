use async_trait::async_trait;
use sqlx::PgPool;
use models::{Document, IssueDocument, UpsertDocumentInput, LockDocumentInput};
use uuid::Uuid;
use crate::{
    issue_document_repository::IssueDocumentRepository,
    RepositoryError,
};

pub struct PgIssueDocumentRepository {
    pool: PgPool,
}

impl PgIssueDocumentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IssueDocumentRepository for PgIssueDocumentRepository {
    async fn list_by_issue(&self, issue_id: Uuid) -> Result<Vec<(IssueDocument, Document)>, RepositoryError> {
        let links = sqlx::query_as::<_, IssueDocument>(
            r#"
            SELECT * FROM issue_documents WHERE issue_id = $1 ORDER BY created_at DESC
            "#,
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        let mut results = Vec::new();
        for link in links {
            let document = sqlx::query_as::<_, Document>(
                r#"
                SELECT * FROM documents WHERE id = $1
                "#,
            )
            .bind(link.document_id)
            .fetch_one(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

            results.push((link, document));
        }

        Ok(results)
    }

    async fn get_by_key(&self, issue_id: Uuid, key: &str) -> Result<Option<(IssueDocument, Document)>, RepositoryError> {
        let link = sqlx::query_as::<_, IssueDocument>(
            r#"
            SELECT * FROM issue_documents WHERE issue_id = $1 AND key = $2
            "#,
        )
        .bind(issue_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        if let Some(link) = link {
            let document = sqlx::query_as::<_, Document>(
                r#"
                SELECT * FROM documents WHERE id = $1
                "#,
            )
            .bind(link.document_id)
            .fetch_one(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

            Ok(Some((link, document)))
        } else {
            Ok(None)
        }
    }

    async fn upsert(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: UpsertDocumentInput,
    ) -> Result<(IssueDocument, Document, bool), RepositoryError> {
        // Start a transaction
        let mut tx = self.pool.begin().await.map_err(RepositoryError::DatabaseError)?;

        // Check if link already exists
        let existing_link: Option<IssueDocument> = sqlx::query_as(
            r#"
            SELECT * FROM issue_documents
            WHERE issue_id = $1 AND key = $2
            "#,
        )
        .bind(issue_id)
        .bind(&input.key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        let (link, document, created) = if let Some(link) = existing_link {
            // Update existing document
            let document: Document = sqlx::query_as(
                r#"
                UPDATE documents
                SET content = $1, content_type = $2, updated_at = NOW()
                WHERE id = $3
                RETURNING *
                "#,
            )
            .bind(&input.content)
            .bind(&input.content_type)
            .bind(link.document_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;

            // Update link timestamp
            let updated_link: IssueDocument = sqlx::query_as(
                r#"
                UPDATE issue_documents
                SET updated_at = NOW()
                WHERE id = $1
                RETURNING *
                "#,
            )
            .bind(link.id)
            .fetch_one(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;

            (updated_link, document, false)
        } else {
            // Create new document
            let document: Document = sqlx::query_as(
                r#"
                INSERT INTO documents (company_id, content, content_type)
                VALUES ($1, $2, $3)
                RETURNING *
                "#,
            )
            .bind(company_id)
            .bind(&input.content)
            .bind(&input.content_type)
            .fetch_one(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;

            // Create link
            let link: IssueDocument = sqlx::query_as(
                r#"
                INSERT INTO issue_documents (company_id, issue_id, document_id, key)
                VALUES ($1, $2, $3, $4)
                RETURNING *
                "#,
            )
            .bind(company_id)
            .bind(issue_id)
            .bind(document.id)
            .bind(&input.key)
            .fetch_one(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;

            (link, document, true)
        };

        tx.commit().await.map_err(RepositoryError::DatabaseError)?;

        Ok((link, document, created))
    }

    async fn lock_document(
        &self,
        document_id: Uuid,
        input: LockDocumentInput,
    ) -> Result<Document, RepositoryError> {
        let document = sqlx::query_as::<_, Document>(
            r#"
            UPDATE documents
            SET locked_by_type = $1,
                locked_by_id = $2,
                locked_run_id = $3,
                locked_at = NOW(),
                updated_at = NOW()
            WHERE id = $4
            RETURNING *
            "#,
        )
        .bind(input.locked_by_type)
        .bind(input.locked_by_id)
        .bind(input.run_id)
        .bind(document_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(document)
    }

    async fn unlock_document(&self, document_id: Uuid) -> Result<Document, RepositoryError> {
        let document = sqlx::query_as::<_, Document>(
            r#"
            UPDATE documents
            SET locked_by_type = NULL,
                locked_by_id = NULL,
                locked_run_id = NULL,
                locked_at = NULL,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(document_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(document)
    }

    async fn delete(&self, issue_id: Uuid, key: &str) -> Result<(), RepositoryError> {
        // Start transaction
        let mut tx = self.pool.begin().await.map_err(RepositoryError::DatabaseError)?;

        // Get the document_id first
        let link: Option<IssueDocument> = sqlx::query_as(
            r#"
            SELECT * FROM issue_documents
            WHERE issue_id = $1 AND key = $2
            "#,
        )
        .bind(issue_id)
        .bind(key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        if let Some(link) = link {
            // Delete the link
            sqlx::query(
                r#"
                DELETE FROM issue_documents WHERE id = $1
                "#,
            )
            .bind(link.id)
            .execute(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;

            // Check if document is still referenced
            let ref_count: i64 = sqlx::query_scalar(
                r#"
                SELECT COUNT(*) FROM issue_documents WHERE document_id = $1
                UNION ALL
                SELECT COUNT(*) FROM case_documents WHERE document_id = $1
                "#,
            )
            .bind(link.document_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;

            // If no more references, delete the document
            if ref_count == 0 {
                sqlx::query(
                    r#"
                    DELETE FROM documents WHERE id = $1
                    "#,
                )
                .bind(link.document_id)
                .execute(&mut *tx)
                .await
                .map_err(RepositoryError::DatabaseError)?;
            }
        }

        tx.commit().await.map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }
}
