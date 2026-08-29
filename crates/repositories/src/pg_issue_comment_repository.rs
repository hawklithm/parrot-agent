use async_trait::async_trait;
use sqlx::PgPool;
use models::{IssueComment, IssueCommentAuthorType, Pagination};
use uuid::Uuid;
use crate::{
    issue_comment_repository::{IssueCommentRepository, CreateIssueCommentInput, UpdateIssueCommentInput},
    RepositoryError,
};

pub struct PgIssueCommentRepository {
    pool: PgPool,
}

pub const ISSUE_COMMENT_COLUMNS: &str = "id, company_id, issue_id, actor_type AS author_type, actor_id, CASE WHEN actor_type = 'agent'::comment_actor_type THEN actor_id ELSE NULL END AS author_agent_id, CASE WHEN actor_type = 'user'::comment_actor_type THEN actor_id ELSE NULL END AS author_user_id, actor_run_id AS created_by_run_id, body, NULL::jsonb AS presentation, metadata, deleted_at, deleted_by_type, deleted_by_agent_id, deleted_by_user_id, deleted_by_run_id, false AS follow_up_requested, created_at, updated_at";

impl PgIssueCommentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IssueCommentRepository for PgIssueCommentRepository {
    async fn create(&self, input: CreateIssueCommentInput) -> Result<IssueComment, RepositoryError> {
        let comment = sqlx::query_as::<_, IssueComment>(&format!(
            "INSERT INTO issue_comments (company_id, issue_id, body, actor_type, actor_id, actor_run_id, metadata) VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {ISSUE_COMMENT_COLUMNS}"
        ))
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
        let comment = sqlx::query_as::<_, IssueComment>(&format!("SELECT {ISSUE_COMMENT_COLUMNS} FROM issue_comments WHERE id = $1"))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(comment)
    }

    async fn list_by_issue(&self, issue_id: Uuid, pagination: &Pagination) -> Result<Vec<IssueComment>, RepositoryError> {
        let comments = sqlx::query_as::<_, IssueComment>(&format!("SELECT {ISSUE_COMMENT_COLUMNS} FROM issue_comments WHERE issue_id = $1 ORDER BY created_at ASC LIMIT $2 OFFSET $3"))
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
            "UPDATE issue_comments SET {} WHERE id = $1 AND deleted_at IS NULL RETURNING {}",
            updates.join(", ")
            , ISSUE_COMMENT_COLUMNS
        );

        let mut q = sqlx::query_as::<_, IssueComment>(&query).bind(id);

        if let Some(ref body) = input.body {
            q = q.bind(body);
        }
        if let Some(ref metadata) = input.metadata {
            q = q.bind(metadata);
        }

        let comment = q.fetch_optional(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?
            .ok_or(RepositoryError::NotFound(id))?;

        Ok(comment)
    }

    async fn tombstone(
        &self,
        id: Uuid,
        deleted_by_type: IssueCommentAuthorType,
        deleted_by_id: Uuid,
        deleted_by_run_id: Option<Uuid>,
    ) -> Result<Option<IssueComment>, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(RepositoryError::DatabaseError)?;
        let deleted_by_type = match deleted_by_type {
            IssueCommentAuthorType::Agent => "agent",
            IssueCommentAuthorType::User => "user",
            IssueCommentAuthorType::System => "system",
        };
        let deleted_by_agent_id = (deleted_by_type == "agent").then_some(deleted_by_id);
        let deleted_by_user_id = (deleted_by_type == "user").then(|| deleted_by_id.to_string());

        let comment = sqlx::query_as::<_, IssueComment>(&format!(
            "UPDATE issue_comments
             SET body = '', metadata = NULL, deleted_at = NOW(), deleted_by_type = $2,
                 deleted_by_agent_id = $3, deleted_by_user_id = $4, deleted_by_run_id = $5,
                 updated_at = NOW()
             WHERE id = $1 AND deleted_at IS NULL
             RETURNING {ISSUE_COMMENT_COLUMNS}"
        ))
        .bind(id)
        .bind(deleted_by_type)
        .bind(deleted_by_agent_id)
        .bind(deleted_by_user_id.as_deref())
        .bind(deleted_by_run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        if let Some(ref comment) = comment {
            sqlx::query("UPDATE issues SET updated_at = NOW() WHERE id = $1")
                .bind(comment.issue_id)
                .execute(&mut *tx)
                .await
                .map_err(RepositoryError::DatabaseError)?;
        }

        tx.commit().await.map_err(RepositoryError::DatabaseError)?;
        Ok(comment)
    }
}
