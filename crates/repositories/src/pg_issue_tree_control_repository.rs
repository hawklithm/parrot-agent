use async_trait::async_trait;
use sqlx::PgPool;
use models::{
    IssueTreeHold, IssueTreeHoldMember, IssueTreeHoldStatus,
};
use uuid::Uuid;
use crate::{
    issue_tree_control_repository::{IssueTreeHoldRepository, CreateTreeHoldInput},
    RepositoryError,
};

pub struct PgIssueTreeHoldRepository {
    pool: PgPool,
}

impl PgIssueTreeHoldRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IssueTreeHoldRepository for PgIssueTreeHoldRepository {
    async fn create(&self, input: CreateTreeHoldInput) -> Result<IssueTreeHold, RepositoryError> {
        let hold = sqlx::query_as::<_, IssueTreeHold>(
            r#"
            INSERT INTO issue_tree_holds (
                company_id, root_issue_id, mode, status, reason,
                release_policy, metadata, actor_type, actor_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(input.company_id)
        .bind(input.root_issue_id)
        .bind(input.mode)
        .bind(IssueTreeHoldStatus::Active)
        .bind(input.reason.as_ref())
        .bind(&input.release_policy)
        .bind(&input.metadata)
        .bind(input.actor_type.as_ref())
        .bind(input.actor_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(hold)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<IssueTreeHold>, RepositoryError> {
        let hold = sqlx::query_as::<_, IssueTreeHold>(
            r#"
            SELECT * FROM issue_tree_holds WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(hold)
    }

    async fn list_active_for_issue(&self, issue_id: Uuid) -> Result<Vec<IssueTreeHold>, RepositoryError> {
        let holds = sqlx::query_as::<_, IssueTreeHold>(
            r#"
            SELECT DISTINCT h.*
            FROM issue_tree_holds h
            JOIN issue_tree_hold_members m ON h.id = m.hold_id
            WHERE m.issue_id = $1 AND h.status = 'active'
            ORDER BY h.created_at DESC
            "#,
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(holds)
    }

    async fn list_by_root_issue(&self, root_issue_id: Uuid) -> Result<Vec<IssueTreeHold>, RepositoryError> {
        let holds = sqlx::query_as::<_, IssueTreeHold>(
            r#"
            SELECT * FROM issue_tree_holds
            WHERE root_issue_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(root_issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(holds)
    }

    async fn release(
        &self,
        hold_id: Uuid,
        released_by_type: Option<String>,
        released_by_id: Option<Uuid>,
    ) -> Result<IssueTreeHold, RepositoryError> {
        let hold = sqlx::query_as::<_, IssueTreeHold>(
            r#"
            UPDATE issue_tree_holds
            SET status = 'released',
                released_at = NOW(),
                released_by_type = $2,
                released_by_id = $3
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(hold_id)
        .bind(released_by_type.as_ref())
        .bind(released_by_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(hold)
    }

    async fn get_members(&self, hold_id: Uuid) -> Result<Vec<IssueTreeHoldMember>, RepositoryError> {
        let members = sqlx::query_as::<_, IssueTreeHoldMember>(
            r#"
            SELECT * FROM issue_tree_hold_members
            WHERE hold_id = $1
            ORDER BY depth ASC, created_at ASC
            "#,
        )
        .bind(hold_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(members)
    }

    async fn create_members(&self, members: Vec<IssueTreeHoldMember>) -> Result<(), RepositoryError> {
        if members.is_empty() {
            return Ok(());
        }

        // Batch insert members
        let mut tx = self.pool.begin().await.map_err(RepositoryError::DatabaseError)?;

        for member in members {
            sqlx::query(
                r#"
                INSERT INTO issue_tree_hold_members (
                    company_id, hold_id, issue_id, parent_issue_id, depth,
                    issue_identifier, issue_title, issue_status,
                    assignee_agent_id, assignee_user_id, active_run_id,
                    active_run_status, skipped, skip_reason
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                "#,
            )
            .bind(member.company_id)
            .bind(member.hold_id)
            .bind(member.issue_id)
            .bind(member.parent_issue_id)
            .bind(member.depth)
            .bind(member.issue_identifier.as_ref())
            .bind(&member.issue_title)
            .bind(&member.issue_status)
            .bind(member.assignee_agent_id)
            .bind(member.assignee_user_id)
            .bind(member.active_run_id)
            .bind(member.active_run_status.as_ref())
            .bind(member.skipped)
            .bind(member.skip_reason.as_ref())
            .execute(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;
        }

        tx.commit().await.map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }
}
