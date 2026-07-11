use async_trait::async_trait;
use sqlx::PgPool;
use models::{IssueComment, Pagination};
use uuid::Uuid;
use crate::{
    issue_comment_repository::{IssueCommentRepository, CreateIssueCommentInput, UpdateIssueCommentInput},
    RepositoryError,
};

pub struct PgIssueCommentRepository {
    pool: PgPool,
}

impl PgIssueCommentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IssueCommentRepository for PgIssueCommentRepository {
    async fn create(&self, input: CreateIssueCommentInput) -> Result<IssueComment, RepositoryError> {
        let comment = sqlx::query_as::<_, IssueComment>(
            r#"
            INSERT INTO issue_comments (
                company_id, issue_id, body, actor_type, actor_id, actor_run_id, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(input.company_id)
        .bind(input.issue_id)
        .bind(&input.body)
        .bind(input.actor_type)
        .bind(input.actor_id)
        .bind(input.actor_run_id)
        .bind(&input.metadata)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(comment)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<IssueComment>, RepositoryError> {
        let comment = sqlx::query_as::<_, IssueComment>(
            r#"
            SELECT * FROM issue_comments WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(comment)
    }

    async fn list_by_issue(&self, issue_id: Uuid, pagination: &Pagination) -> Result<Vec<IssueComment>, RepositoryError> {
        let comments = sqlx::query_as::<_, IssueComment>(
            r#"
            SELECT * FROM issue_comments
            WHERE issue_id = $1
            ORDER BY created_at ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(issue_id)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(comments)
    }

    async fn count_by_issue(&self, issue_id: Uuid) -> Result<i64, RepositoryError> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM issue_comments WHERE issue_id = $1
            "#,
        )
        .bind(issue_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(count)
    }

    async fn update(&self, id: Uuid, input: UpdateIssueCommentInput) -> Result<IssueComment, RepositoryError> {
        let mut updates = Vec::new();
        let mut param_count = 1;

        if input.body.is_some() {
            param_count += 1;
            updates.push(format!("body = ${}", param_count));
        }
        if input.metadata.is_some() {
            param_count += 1;
            updates.push(format!("metadata = ${}", param_count));
        }

        if updates.is_empty() {
            return self.get_by_id(id).await?.ok_or_else(|| RepositoryError::NotFound(id));
        }

        updates.push("updated_at = NOW()".to_string());

        let query = format!(
            "UPDATE issue_comments SET {} WHERE id = $1 RETURNING *",
            updates.join(", ")
        );

        let mut q = sqlx::query_as::<_, IssueComment>(&query).bind(id);

        if let Some(ref body) = input.body {
            q = q.bind(body);
        }
        if let Some(ref metadata) = input.metadata {
            q = q.bind(metadata);
        }

        let comment = q.fetch_one(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        Ok(comment)
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM issue_comments WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }
}
