use async_trait::async_trait;
use sqlx::PgPool;
use models::{
    IssueTreeHold, IssueTreeHoldMember, IssueTreeHoldStatus,
};
use uuid::Uuid;
use crate::{
    issue_tree_control_repository::{
        CreateTreeHoldInput, IssueTreeHoldRepository, ReleaseTreeHoldInput,
    },
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
        let actor_type = input
            .created_by_actor_type
            .clone()
            .unwrap_or_else(|| "system".to_string());
        let hold = sqlx::query_as::<_, IssueTreeHold>(
            r#"
            INSERT INTO issue_tree_holds (
                company_id, root_issue_id, mode, status, reason,
                release_policy, metadata,
                created_by_actor_type, created_by_agent_id, created_by_user_id,
                created_by_run_id, actor_type, actor_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
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
        .bind(&actor_type)
        .bind(input.created_by_agent_id)
        .bind(input.created_by_user_id.as_ref())
        .bind(input.created_by_run_id)
        // Legacy columns retained for readers pinned to the old shape.
        .bind(&actor_type)
        .bind(
            input.created_by_agent_id
                .or_else(|| input.created_by_user_id.as_ref().and_then(|id| Uuid::parse_str(id).ok())),
        )
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
        let (agent_id, user_id) = match released_by_type.as_deref() {
            Some("agent") => (released_by_id, None),
            Some(_) => (None, released_by_id.map(|id| id.to_string())),
            None => (None, None),
        };
        self.release_with_actor(
            hold_id,
            ReleaseTreeHoldInput {
                released_by_actor_type: released_by_type,
                released_by_agent_id: agent_id,
                released_by_user_id: user_id,
                ..Default::default()
            },
        )
        .await
    }

    async fn release_with_actor(
        &self,
        hold_id: Uuid,
        input: ReleaseTreeHoldInput,
    ) -> Result<IssueTreeHold, RepositoryError> {
        let hold = sqlx::query_as::<_, IssueTreeHold>(
            r#"
            UPDATE issue_tree_holds
            SET status = 'released',
                released_at = NOW(),
                updated_at = NOW(),
                released_by_actor_type = $2,
                released_by_agent_id = $3,
                released_by_user_id = $4,
                released_by_run_id = $5,
                release_reason = $6,
                release_metadata = $7,
                released_by_type = $2,
                released_by_id = COALESCE($3, $4::uuid)
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(hold_id)
        .bind(input.released_by_actor_type.as_ref())
        .bind(input.released_by_agent_id)
        .bind(input.released_by_user_id.as_ref())
        .bind(input.released_by_run_id)
        .bind(input.release_reason.as_ref())
        .bind(&input.release_metadata)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(hold)
    }

    async fn mark_member_restored(
        &self,
        hold_id: Uuid,
        issue_id: Uuid,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            UPDATE issue_tree_hold_members
            SET restored_at = NOW()
            WHERE hold_id = $1 AND issue_id = $2
            "#,
        )
        .bind(hold_id)
        .bind(issue_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        Ok(())
    }

    async fn set_apply_error(
        &self,
        hold_id: Uuid,
        error: Option<String>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            UPDATE issue_tree_holds
            SET apply_error = $2, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(hold_id)
        .bind(error.as_ref())
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;
        Ok(())
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
                    active_run_status, skipped, skip_reason, previous_status
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
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
            .bind(member.previous_status.as_ref())
            .execute(&mut *tx)
            .await
            .map_err(RepositoryError::DatabaseError)?;
        }

        tx.commit().await.map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }
}
