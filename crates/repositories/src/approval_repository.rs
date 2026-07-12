use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::RepositoryResult;
use models::approval::{Approval, ApprovalStatus};

#[async_trait]
pub trait ApprovalRepository: Send + Sync {
    async fn create(&self, approval: Approval) -> RepositoryResult<Approval>;
    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<Approval>>;
    async fn find_by_company_id(
        &self,
        company_id: Uuid,
        status: Option<ApprovalStatus>,
    ) -> RepositoryResult<Vec<Approval>>;
    async fn find_by_issue_id(&self, issue_id: Uuid) -> RepositoryResult<Vec<Approval>>;
    async fn find_linked_issues(&self, approval_id: Uuid) -> RepositoryResult<Vec<Uuid>>;
    async fn find_pending_for_reviewer(&self, user_id: Uuid) -> RepositoryResult<Vec<Approval>>;
    async fn link_to_issue(&self, approval_id: Uuid, issue_id: Uuid) -> RepositoryResult<()>;
    async fn update(&self, approval: Approval) -> RepositoryResult<Approval>;
}

pub struct PostgresApprovalRepository {
    pool: PgPool,
}

impl PostgresApprovalRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const APPROVAL_COLS: &str = "id, company_id, approval_type, requested_by_agent_id, requested_by_user_id, \
    status, payload, decision_note, decided_by_user_id, decided_at, created_at, updated_at";

#[async_trait]
impl ApprovalRepository for PostgresApprovalRepository {
    async fn create(&self, approval: Approval) -> RepositoryResult<Approval> {
        sqlx::query(
            r#"INSERT INTO approvals
               (id, company_id, approval_type, requested_by_agent_id, requested_by_user_id,
                status, payload, decision_note, decided_by_user_id, decided_at, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#
        )
        .bind(approval.id)
        .bind(approval.company_id)
        .bind(approval.approval_type)
        .bind(approval.requested_by_agent_id)
        .bind(approval.requested_by_user_id)
        .bind(approval.status)
        .bind(&approval.payload)
        .bind(&approval.decision_note)
        .bind(approval.decided_by_user_id)
        .bind(approval.decided_at)
        .bind(approval.created_at)
        .bind(approval.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(approval)
    }

    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<Approval>> {
        let approval = sqlx::query_as::<_, Approval>(
            &format!("SELECT {} FROM approvals WHERE id = $1", APPROVAL_COLS)
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(approval)
    }

    async fn find_by_company_id(
        &self,
        company_id: Uuid,
        status: Option<ApprovalStatus>,
    ) -> RepositoryResult<Vec<Approval>> {
        let approvals = if let Some(status) = status {
            sqlx::query_as::<_, Approval>(
                &format!("SELECT {} FROM approvals WHERE company_id = $1 AND status = $2 ORDER BY created_at DESC", APPROVAL_COLS)
            )
            .bind(company_id)
            .bind(status)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Approval>(
                &format!("SELECT {} FROM approvals WHERE company_id = $1 ORDER BY created_at DESC", APPROVAL_COLS)
            )
            .bind(company_id)
            .fetch_all(&self.pool)
            .await?
        };
        Ok(approvals)
    }

    async fn find_by_issue_id(&self, issue_id: Uuid) -> RepositoryResult<Vec<Approval>> {
        let approvals = sqlx::query_as::<_, Approval>(
            &format!("SELECT a.{} FROM approvals a
                      INNER JOIN issue_approvals ia ON a.id = ia.approval_id
                      WHERE ia.issue_id = $1 ORDER BY a.created_at DESC", APPROVAL_COLS)
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(approvals)
    }

    async fn find_linked_issues(&self, approval_id: Uuid) -> RepositoryResult<Vec<Uuid>> {
        let rows = sqlx::query("SELECT issue_id FROM issue_approvals WHERE approval_id = $1")
            .bind(approval_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|r| r.get("issue_id")).collect())
    }

    async fn find_pending_for_reviewer(&self, _user_id: Uuid) -> RepositoryResult<Vec<Approval>> {
        let approvals = sqlx::query_as::<_, Approval>(
            &format!("SELECT {} FROM approvals WHERE status = $1 ORDER BY created_at DESC", APPROVAL_COLS)
        )
        .bind(ApprovalStatus::Pending)
        .fetch_all(&self.pool)
        .await?;
        Ok(approvals)
    }

    async fn link_to_issue(&self, approval_id: Uuid, issue_id: Uuid) -> RepositoryResult<()> {
        sqlx::query(
            "INSERT INTO issue_approvals (id, approval_id, issue_id) VALUES ($1, $2, $3)
             ON CONFLICT (approval_id, issue_id) DO NOTHING"
        )
        .bind(Uuid::new_v4())
        .bind(approval_id)
        .bind(issue_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update(&self, approval: Approval) -> RepositoryResult<Approval> {
        sqlx::query(
            r#"UPDATE approvals
               SET approval_type = $2, requested_by_agent_id = $3, requested_by_user_id = $4,
                   status = $5, payload = $6, decision_note = $7, decided_by_user_id = $8,
                   decided_at = $9, updated_at = $10
               WHERE id = $1"#
        )
        .bind(approval.id)
        .bind(approval.approval_type)
        .bind(approval.requested_by_agent_id)
        .bind(approval.requested_by_user_id)
        .bind(approval.status)
        .bind(&approval.payload)
        .bind(&approval.decision_note)
        .bind(approval.decided_by_user_id)
        .bind(approval.decided_at)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(approval)
    }
}
