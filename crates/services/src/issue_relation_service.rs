use serde::{Deserialize, Serialize};
use sqlx::{PgPool, FromRow};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IssueRelation {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub related_issue_id: Uuid,
    pub relation_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub created_by_agent_id: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IssueRelationSummary {
    pub id: Uuid,
    pub identifier: String,
    pub title: String,
    pub status: String,
    pub relation_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueRelationsSummary {
    pub blocked_by: Vec<IssueRelationSummary>,
    pub blocks: Vec<IssueRelationSummary>,
}

pub struct IssueRelationService {
    pool: PgPool,
}

impl IssueRelationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get all relation summaries for an issue
    pub async fn get_relation_summaries(
        &self,
        issue_id: Uuid,
    ) -> Result<IssueRelationsSummary, sqlx::Error> {
        // Get all "blocks" relations where this issue blocks others
        let blocks: Vec<IssueRelationSummary> = sqlx::query_as(
            r#"
            SELECT 
                i.id,
                COALESCE(i.identifier, i.id::text) as identifier,
                i.title,
                i.status,
                'blocks' as relation_type
            FROM issue_relations ir
            INNER JOIN issues i ON ir.related_issue_id = i.id
            WHERE ir.issue_id = $1 AND ir.relation_type = 'blocks'
            ORDER BY i.created_at DESC
            "#,
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await?;

        // Get all "blocks" relations where this issue is blocked by others
        let blocked_by: Vec<IssueRelationSummary> = sqlx::query_as(
            r#"
            SELECT 
                i.id,
                COALESCE(i.identifier, i.id::text) as identifier,
                i.title,
                i.status,
                'blocked_by' as relation_type
            FROM issue_relations ir
            INNER JOIN issues i ON ir.issue_id = i.id
            WHERE ir.related_issue_id = $1 AND ir.relation_type = 'blocks'
            ORDER BY i.created_at DESC
            "#,
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(IssueRelationsSummary {
            blocked_by,
            blocks,
        })
    }

    /// Add a relation between two issues
    pub async fn add_relation(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        related_issue_id: Uuid,
        relation_type: &str,
        created_by_agent_id: Option<Uuid>,
    ) -> Result<IssueRelation, sqlx::Error> {
        let relation: IssueRelation = sqlx::query_as(
            r#"
            INSERT INTO issue_relations (
                company_id, issue_id, related_issue_id, relation_type, created_by_agent_id
            ) VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .bind(related_issue_id)
        .bind(relation_type)
        .bind(created_by_agent_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(relation)
    }

    /// Remove a relation between two issues
    pub async fn remove_relation(
        &self,
        issue_id: Uuid,
        related_issue_id: Uuid,
        relation_type: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM issue_relations
            WHERE issue_id = $1 AND related_issue_id = $2 AND relation_type = $3
            "#,
        )
        .bind(issue_id)
        .bind(related_issue_id)
        .bind(relation_type)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Update blocked_by relations for an issue (bulk operation)
    pub async fn update_blocked_by_relations(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        blocked_by_issue_ids: Vec<Uuid>,
        created_by_agent_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        // Remove existing blocked_by relations
        sqlx::query(
            r#"
            DELETE FROM issue_relations
            WHERE related_issue_id = $1 AND relation_type = 'blocks'
            "#,
        )
        .bind(issue_id)
        .execute(&self.pool)
        .await?;

        // Add new blocked_by relations
        for blocker_id in blocked_by_issue_ids {
            self.add_relation(company_id, blocker_id, issue_id, "blocks", created_by_agent_id)
                .await?;
        }

        Ok(())
    }
}
