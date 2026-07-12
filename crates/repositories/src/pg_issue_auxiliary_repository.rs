use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use models::{
    IssueReadStatus, MarkIssueReadInput,
    IssueInboxArchive, ArchiveIssueInput,
    FeedbackVote, CreateFeedbackVoteInput,
    FeedbackTrace, FeedbackTraceBundle,
    RecoveryAction, CreateRecoveryActionInput, ResolveRecoveryActionInput,
    PlanDecomposition, CreatePlanDecompositionInput, AcceptPlanDecompositionInput,
};
use crate::issue_auxiliary_repository::{
    IssueReadStatusRepository, IssueInboxArchiveRepository,
    FeedbackVoteRepository, FeedbackTraceRepository,
    RecoveryActionRepository, PlanDecompositionRepository,
};
use crate::RepositoryError;

// ─── PostgreSQL: Issue Read Status ─────────────────────────────

pub struct PgIssueReadStatusRepository {
    pool: PgPool,
}

impl PgIssueReadStatusRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IssueReadStatusRepository for PgIssueReadStatusRepository {
    async fn mark_read(&self, company_id: Uuid, issue_id: Uuid, input: &MarkIssueReadInput) -> Result<IssueReadStatus, RepositoryError> {
        sqlx::query_as::<_, IssueReadStatus>(
            r#"
            INSERT INTO issue_read_status (company_id, issue_id, user_id, read_at)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (issue_id, user_id)
            DO UPDATE SET read_at = NOW(), updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .bind(input.user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn unmark_read(&self, _company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM issue_read_status
            WHERE issue_id = $1 AND user_id = $2
            "#,
        )
        .bind(issue_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        Ok(())
    }

    async fn get_read_status(&self, _company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<Option<IssueReadStatus>, RepositoryError> {
        sqlx::query_as::<_, IssueReadStatus>(
            r#"
            SELECT * FROM issue_read_status
            WHERE issue_id = $1 AND user_id = $2
            "#,
        )
        .bind(issue_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn list_read_statuses(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<IssueReadStatus>, RepositoryError> {
        sqlx::query_as::<_, IssueReadStatus>(
            r#"
            SELECT * FROM issue_read_status
            WHERE company_id = $1 AND issue_id = $2
            ORDER BY read_at DESC
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn is_read(&self, _company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<bool, RepositoryError> {
        let row: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM issue_read_status
            WHERE issue_id = $1 AND user_id = $2
            "#,
        )
        .bind(issue_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        Ok(row.map(|r| r.0 > 0).unwrap_or(false))
    }
}

// ─── PostgreSQL: Issue Inbox Archive ───────────────────────────

pub struct PgIssueInboxArchiveRepository {
    pool: PgPool,
}

impl PgIssueInboxArchiveRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IssueInboxArchiveRepository for PgIssueInboxArchiveRepository {
    async fn archive(&self, company_id: Uuid, issue_id: Uuid, input: &ArchiveIssueInput) -> Result<IssueInboxArchive, RepositoryError> {
        sqlx::query_as::<_, IssueInboxArchive>(
            r#"
            INSERT INTO issue_inbox_archives (company_id, issue_id, user_id, archived_at)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (issue_id, user_id)
            DO UPDATE SET archived_at = NOW(), updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .bind(input.user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn unarchive(&self, _company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM issue_inbox_archives
            WHERE issue_id = $1 AND user_id = $2
            "#,
        )
        .bind(issue_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        Ok(())
    }

    async fn get_archive(&self, _company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<Option<IssueInboxArchive>, RepositoryError> {
        sqlx::query_as::<_, IssueInboxArchive>(
            r#"
            SELECT * FROM issue_inbox_archives
            WHERE issue_id = $1 AND user_id = $2
            "#,
        )
        .bind(issue_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn list_archived(&self, company_id: Uuid, user_id: Uuid) -> Result<Vec<IssueInboxArchive>, RepositoryError> {
        sqlx::query_as::<_, IssueInboxArchive>(
            r#"
            SELECT * FROM issue_inbox_archives
            WHERE company_id = $1 AND user_id = $2
            ORDER BY archived_at DESC
            "#,
        )
        .bind(company_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn is_archived(&self, _company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<bool, RepositoryError> {
        let row: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM issue_inbox_archives
            WHERE issue_id = $1 AND user_id = $2
            "#,
        )
        .bind(issue_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        Ok(row.map(|r| r.0 > 0).unwrap_or(false))
    }
}

// ─── PostgreSQL: Feedback Vote ─────────────────────────────────

pub struct PgFeedbackVoteRepository {
    pool: PgPool,
}

impl PgFeedbackVoteRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FeedbackVoteRepository for PgFeedbackVoteRepository {
    async fn upsert_vote(&self, company_id: Uuid, issue_id: Uuid, input: &CreateFeedbackVoteInput) -> Result<FeedbackVote, RepositoryError> {
        let voter_type = input.voter_type.clone().unwrap_or_else(|| "user".to_string());
        let shared_with_labs = input.shared_with_labs.unwrap_or(false);
        let vote_str = input.vote.to_string();

        sqlx::query_as::<_, FeedbackVote>(
            r#"
            INSERT INTO feedback_votes (company_id, issue_id, voter_id, voter_type, vote, reason, shared_with_labs)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (issue_id, voter_id, voter_type)
            DO UPDATE SET vote = $5, reason = $6, shared_with_labs = $7, updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .bind(input.voter_id)
        .bind(&voter_type)
        .bind(&vote_str)
        .bind(&input.reason)
        .bind(shared_with_labs)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn list_votes(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<FeedbackVote>, RepositoryError> {
        sqlx::query_as::<_, FeedbackVote>(
            r#"
            SELECT * FROM feedback_votes
            WHERE company_id = $1 AND issue_id = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn get_vote(&self, vote_id: Uuid) -> Result<Option<FeedbackVote>, RepositoryError> {
        sqlx::query_as::<_, FeedbackVote>(
            r#"
            SELECT * FROM feedback_votes WHERE id = $1
            "#,
        )
        .bind(vote_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn delete_vote(&self, vote_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM feedback_votes WHERE id = $1")
            .bind(vote_id)
            .execute(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;
        Ok(())
    }
}

// ─── PostgreSQL: Feedback Trace ────────────────────────────────

pub struct PgFeedbackTraceRepository {
    pool: PgPool,
}

impl PgFeedbackTraceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FeedbackTraceRepository for PgFeedbackTraceRepository {
    async fn create_trace(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        vote_id: Uuid,
        target_type: &str,
        target_id: Option<Uuid>,
        payload: &serde_json::Value,
    ) -> Result<FeedbackTrace, RepositoryError> {
        sqlx::query_as::<_, FeedbackTrace>(
            r#"
            INSERT INTO feedback_traces (company_id, issue_id, vote_id, target_type, target_id, payload)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .bind(vote_id)
        .bind(target_type)
        .bind(target_id)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn list_traces(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<FeedbackTrace>, RepositoryError> {
        sqlx::query_as::<_, FeedbackTrace>(
            r#"
            SELECT * FROM feedback_traces
            WHERE company_id = $1 AND issue_id = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn get_trace_bundle(&self, trace_id: Uuid) -> Result<Option<FeedbackTraceBundle>, RepositoryError> {
        // Fetch trace with vote and issue info in one query
        let row = sqlx::query_as::<_, FeedbackTrace>(
            r#"SELECT * FROM feedback_traces WHERE id = $1"#,
        )
        .bind(trace_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        match row {
            Some(trace) => {
                let vote: Option<FeedbackVote> = sqlx::query_as(
                    r#"SELECT * FROM feedback_votes WHERE id = $1"#,
                )
                .bind(trace.vote_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(RepositoryError::DatabaseError)?;

                let issue_info: Option<(String, Option<String>)> = sqlx::query_as(
                    r#"
                    SELECT title, identifier FROM issues WHERE id = $1
                    "#,
                )
                .bind(trace.issue_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(RepositoryError::DatabaseError)?;

                let (issue_title, issue_identifier) = match issue_info {
                    Some((t, id)) => (Some(t), id),
                    None => (None, None),
                };

                Ok(Some(FeedbackTraceBundle {
                    trace,
                    vote,
                    issue_title,
                    issue_identifier,
                }))
            }
            None => Ok(None),
        }
    }

    async fn update_trace_status(&self, trace_id: Uuid, status: &str, failure_reason: Option<&str>) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            UPDATE feedback_traces
            SET status = $1, failure_reason = $2, updated_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind(status)
        .bind(failure_reason)
        .bind(trace_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        Ok(())
    }
}

// ─── PostgreSQL: Recovery Action ───────────────────────────────

pub struct PgRecoveryActionRepository {
    pool: PgPool,
}

impl PgRecoveryActionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RecoveryActionRepository for PgRecoveryActionRepository {
    async fn create(&self, company_id: Uuid, issue_id: Uuid, input: &CreateRecoveryActionInput) -> Result<RecoveryAction, RepositoryError> {
        sqlx::query_as::<_, RecoveryAction>(
            r#"
            INSERT INTO recovery_actions (company_id, issue_id, action_type, description, metadata, triggered_by_issue_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .bind(&input.action_type)
        .bind(&input.description)
        .bind(&input.metadata)
        .bind(input.triggered_by_issue_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn list_by_issue(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<RecoveryAction>, RepositoryError> {
        sqlx::query_as::<_, RecoveryAction>(
            r#"
            SELECT * FROM recovery_actions
            WHERE company_id = $1 AND issue_id = $2
            ORDER BY triggered_at DESC
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn list_pending(&self, company_id: Uuid, limit: i64) -> Result<Vec<RecoveryAction>, RepositoryError> {
        sqlx::query_as::<_, RecoveryAction>(
            r#"
            SELECT * FROM recovery_actions
            WHERE company_id = $1 AND status IN ('pending', 'in_progress')
            ORDER BY triggered_at ASC
            LIMIT $2
            "#,
        )
        .bind(company_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn resolve(&self, action_id: Uuid, input: &ResolveRecoveryActionInput) -> Result<RecoveryAction, RepositoryError> {
        let resolved_at = input.resolved_at.unwrap_or_else(chrono::Utc::now);
        sqlx::query_as::<_, RecoveryAction>(
            r#"
            UPDATE recovery_actions
            SET status = 'resolved', resolved_at = $1, updated_at = NOW()
            WHERE id = $2
            RETURNING *
            "#,
        )
        .bind(resolved_at)
        .bind(action_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn reconcile_for_issue_and_ancestors(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<RecoveryAction>, RepositoryError> {
        // Recursive CTE to find all ancestors + the issue itself
        sqlx::query_as::<_, RecoveryAction>(
            r#"
            WITH RECURSIVE issue_tree AS (
                SELECT id, parent_id FROM issues WHERE id = $2
                UNION ALL
                SELECT i.id, i.parent_id FROM issues i
                INNER JOIN issue_tree t ON i.id = t.parent_id
            )
            UPDATE recovery_actions ra
            SET status = 'resolved', resolved_at = NOW(), updated_at = NOW()
            FROM issue_tree it
            WHERE ra.issue_id = it.id
              AND ra.company_id = $1
              AND ra.status IN ('pending', 'in_progress')
            RETURNING ra.*
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn resolve_active_for_issue(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<RecoveryAction>, RepositoryError> {
        sqlx::query_as::<_, RecoveryAction>(
            r#"
            UPDATE recovery_actions
            SET status = 'resolved', resolved_at = NOW(), updated_at = NOW()
            WHERE company_id = $1 AND issue_id = $2
              AND status IN ('pending', 'in_progress')
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }
}

// ─── PostgreSQL: Plan Decomposition ────────────────────────────

pub struct PgPlanDecompositionRepository {
    pool: PgPool,
}

impl PgPlanDecompositionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PlanDecompositionRepository for PgPlanDecompositionRepository {
    async fn create(&self, company_id: Uuid, issue_id: Uuid, input: &CreatePlanDecompositionInput) -> Result<PlanDecomposition, RepositoryError> {
        sqlx::query_as::<_, PlanDecomposition>(
            r#"
            INSERT INTO plan_decompositions (company_id, issue_id, plan)
            VALUES ($1, $2, $3)
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .bind(&input.plan)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn list_by_issue(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<PlanDecomposition>, RepositoryError> {
        sqlx::query_as::<_, PlanDecomposition>(
            r#"
            SELECT * FROM plan_decompositions
            WHERE company_id = $1 AND issue_id = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn accept(&self, id: Uuid, input: &AcceptPlanDecompositionInput) -> Result<PlanDecomposition, RepositoryError> {
        sqlx::query_as::<_, PlanDecomposition>(
            r#"
            UPDATE plan_decompositions
            SET accepted_at = NOW(), accepted_by_type = $1, accepted_by_id = $2, updated_at = NOW()
            WHERE id = $3
            RETURNING *
            "#,
        )
        .bind(&input.accepted_by_type)
        .bind(input.accepted_by_id)
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<PlanDecomposition>, RepositoryError> {
        sqlx::query_as::<_, PlanDecomposition>(
            r#"SELECT * FROM plan_decompositions WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM plan_decompositions WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;
        Ok(())
    }
}
