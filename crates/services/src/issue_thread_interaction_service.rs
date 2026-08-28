use models::{
    AcceptInteractionResult, AcceptThreadInteractionInput, AnswerQuestionsInput,
    CancelQuestionsInput, CreateThreadInteractionInput, Issue, IssueThreadInteraction,
    RejectThreadInteractionInput,
};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use chrono::Utc;
use crate::issue_workspace_validation::assert_issue_workspace_finalized_for_accept;

/// Issue thread interaction service for managing interactions (questions, approvals, etc.)
pub struct IssueThreadInteractionService {
    pool: PgPool,
}

impl IssueThreadInteractionService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List all interactions for an issue
    pub async fn list_for_issue(&self, issue_id: Uuid) -> Result<Vec<IssueThreadInteraction>, String> {
        let interactions = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            SELECT 
                id, company_id, issue_id, kind, status,
                continuation_policy, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_user_id,
                payload, result, resolved_at, created_at, updated_at
            FROM issue_thread_interactions
            WHERE issue_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to list interactions: {}", e))?;

        Ok(interactions)
    }

    /// Get a single interaction by ID
    /// Returns None if not found
    pub async fn get_by_id(&self, interaction_id: Uuid) -> Result<Option<IssueThreadInteraction>, String> {
        let interaction = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            SELECT 
                id, company_id, issue_id, kind, status,
                continuation_policy, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_user_id,
                payload, result, resolved_at, created_at, updated_at
            FROM issue_thread_interactions
            WHERE id = $1
            "#,
        )
        .bind(interaction_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("Failed to get interaction: {}", e))?;

        Ok(interaction)
    }

    /// Create a new interaction
    pub async fn create(
        &self,
        issue: &Issue,
        input: CreateThreadInteractionInput,
        creator: InteractionCreator,
    ) -> Result<IssueThreadInteraction, String> {
        let now = Utc::now();
        let id = Uuid::new_v4();
        let interaction = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            INSERT INTO issue_thread_interactions (
                id, company_id, issue_id, kind, status,
                continuation_policy, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                payload, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, 'pending', $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
            RETURNING 
                id, company_id, issue_id, kind, status,
                continuation_policy, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_user_id,
                payload, result, resolved_at, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(issue.company_id)
        .bind(issue.id)
        .bind(input.kind)
        .bind(input.continuation_policy)
        .bind(input.source_comment_id)
        .bind(input.source_run_id)
        .bind(input.title)
        .bind(input.summary)
        .bind(creator.agent_id)
        .bind(creator.user_id)
        .bind(input.payload)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| format!("Failed to create interaction: {}", e))?;

        Ok(interaction)
    }

    /// Accept an interaction
    /// This is the main entry point for handling user acceptance
    pub async fn accept_interaction(
        &self,
        issue: &Issue,
        interaction_id: Uuid,
        input: AcceptThreadInteractionInput,
        resolver: InteractionResolver,
    ) -> Result<AcceptInteractionResult, String> {
        let mut tx = self.pool.begin().await
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;

        // Load the interaction
        let interaction = self.get_interaction_for_update(&mut tx, interaction_id).await?;

        if interaction.status != "pending" {
            return Err(format!("Interaction is not pending (status: {})", interaction.status));
        }

        // For request_confirmation interactions, verify workspace is finalized
        // This prevents users from making decisions based on stale workspace state
        if matches!(interaction.kind.as_str(), "request_confirmation" | "request_checkbox_confirmation") {
            assert_issue_workspace_finalized_for_accept(
                &self.pool,
                issue.id,
                interaction.source_run_id,
            ).await?;
        }

        // Handle different interaction kinds
        let result = match interaction.kind.as_str() {
            "suggest_tasks" => {
                self.accept_suggest_tasks(&mut tx, issue, &interaction, &input, &resolver).await?
            }
            "request_confirmation" | "request_checkbox_confirmation" => {
                self.accept_request_confirmation(&mut tx, issue, &interaction, &input, &resolver).await?
            }
            "question" | "approval" | "review" | "item_verdict" => {
                self.accept_simple_interaction(&mut tx, issue, &interaction, &input, &resolver).await?
            }
            "withdraw" => {
                // Withdraw is a cancellation, not an acceptance
                return Err("Cannot accept a withdraw interaction; use reject instead".to_string());
            }
            _ => {
                return Err(format!("Unsupported interaction kind: {}", interaction.kind));
            }
        };

        tx.commit().await
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;

        Ok(result)
    }

    /// Get interaction with FOR UPDATE lock
    async fn get_interaction_for_update(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        interaction_id: Uuid,
    ) -> Result<IssueThreadInteraction, String> {
        let interaction = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            SELECT 
                id, company_id, issue_id, kind::text as kind, status::text as status,
                source_run_id, source_comment_id, payload, idempotency_key,
                continuation_policy, question, response, result,
                resolved_by_type, resolved_by_id, expires_at,
                created_at, updated_at
            FROM issue_thread_interactions
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(interaction_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| format!("Failed to load interaction: {}", e))?
        .ok_or_else(|| "Interaction not found".to_string())?;

        Ok(interaction)
    }

    /// Accept suggest_tasks interaction
    /// Creates multiple child issues from the suggested tasks
    async fn accept_suggest_tasks(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        issue: &Issue,
     interaction: &IssueThreadInteraction,
        input: &AcceptThreadInteractionInput,
        resolver: &InteractionResolver,
    ) -> Result<AcceptInteractionResult, String> {
        // Extract suggested tasks from payload
        let tasks = interaction.payload.get("tasks")
            .and_then(|t| t.as_array())
            .ok_or_else(|| "suggest_tasks payload missing tasks array".to_string())?;

        let mut created_issues = Vec::new();

        // Create child issues for each suggested task
        for task in tasks {
            let title = task.get("title")
                .and_then(|t| t.as_str())
                .ok_or_else(|| "Task missing title".to_string())?;

            let description = task.get("description")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());

            let assignee_agent_id = task.get("assigneeAgentId")
                .and_then(|a| a.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            // Note: We should use IssueService here, but to avoid circular dependency
            // we insert directly for now

            // Note: We need to use the issue service to create the child
            // For now, we'll insert directly to avoid circular dependency
            let child_id = Uuid::new_v4();
            let child = sqlx::query_as::<_, Issue>(
                r#"
                INSERT INTO issues (
                    id, company_id, project_id, parent_id, identifier,
                    title, description, assignee_agent_id, status, created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, 
                     (SELECT COALESCE(MAX(CAST(SUBSTRING(identifier FROM '[0-9]+$') AS INTEGER)), 0) + 1 
                         FROM issues WHERE company_id = $2),
                        $5, $6, $7, $8::issue_status, NOW(), NOW())
                RETURNING *
                "#,
            )
            .bind(child_id)
            .bind(issue.company_id)
            .bind(issue.project_id)
            .bind(issue.id)
            .bind(title)
            .bind(description)
            .bind(assignee_agent_id)
            .bind("todo")
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| format!("Failed to create child issue: {}", e))?;

            created_issues.push(child);
        }

        // Update interaction status
        let updated_interaction = self.mark_interaction_accepted(
            tx,
            interaction.id,
            input.response.clone(),
            Some(serde_json::json!({
                "createdTasks": created_issues.iter().map(|i| i.id).collect::<Vec<_>>(),
            })),
            resolver,
        ).await?;

        Ok(AcceptInteractionResult {
            interaction: updated_interaction,
            created_issues,
            continuation_issue: None,
        })
    }

    /// Accept request_confirmation interaction
    /// May update the source issue's assignee or status
    async fn accept_request_confirmation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        issue: &Issue,
        interaction: &IssueThreadInteraction,
        input: &AcceptThreadInteractionInput,
        resolver: &InteractionResolver,
    ) -> Result<AcceptInteractionResult, String> {
        // Update interaction status to accepted
        let updated_interaction = self.mark_interaction_accepted(
            tx,
            interaction.id,
            input.response.clone(),
            None,
            resolver,
        ).await?;

        // Check if we should return the issue to the creator agent
        let continuation_issue = if should_return_accepted_confirmation_to_creator_agent(
            issue,
            interaction,
            resolver,
        ) {
            // Determine return status: keep "blocked" as blocked, otherwise "todo"
            let return_status = if issue.status == models::IssueStatus::Blocked {
                models::IssueStatus::Blocked
            } else {
                models::IssueStatus::Todo
            };

            // Update issue: assign back to the agent who created the interaction
            let updated_issue = sqlx::query_as::<_, Issue>(
                r#"
                UPDATE issues
                SET 
                    status = $1,
                    assignee_agent_id = $2,
                    assignee_user_id = NULL,
                    updated_at = NOW()
                WHERE id = $3
                RETURNING *
                "#,
            )
            .bind(return_status)
            .bind(interaction.created_by_agent_id)
            .bind(issue.id)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| format!("Failed to update issue: {}", e))?;

            Some(updated_issue)
        } else {
            // Just touch the issue to update its timestamp
            sqlx::query("UPDATE issues SET updated_at = NOW() WHERE id = $1")
                .bind(issue.id)
                .execute(&mut **tx)
                .await
                .map_err(|e| format!("Failed to touch issue: {}", e))?;
            
            None
        };

        Ok(AcceptInteractionResult {
            interaction: updated_interaction,
            created_issues: vec![],
            continuation_issue,
        })
    }

    /// Accept simple interaction (question, approval, review)
    async fn accept_simple_interaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        issue: &Issue,
        interaction: &IssueThreadInteraction,
        input: &AcceptThreadInteractionInput,
        resolver: &InteractionResolver,
    ) -> Result<AcceptInteractionResult, String> {
        let updated_interaction = self.mark_interaction_accepted(
            tx,
            interaction.id,
            input.response.clone(),
            None,
            resolver,
        ).await?;

        // Check if we should return the issue to the creator agent
        let continuation_issue = if should_return_accepted_confirmation_to_creator_agent(
            issue,
            interaction,
            resolver,
        ) {
            let return_status = if issue.status == models::IssueStatus::Blocked {
                models::IssueStatus::Blocked
            } else {
                models::IssueStatus::Todo
            };

            let updated_issue = sqlx::query_as::<_, Issue>(
                r#"
                UPDATE issues
                SET 
                    status = $1,
                    assignee_agent_id = $2,
                    assignee_user_id = NULL,
                    updated_at = NOW()
                WHERE id = $3
                RETURNING *
                "#,
            )
            .bind(return_status)
            .bind(interaction.created_by_agent_id)
            .bind(issue.id)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| format!("Failed to update issue: {}", e))?;

            Some(updated_issue)
        } else {
            sqlx::query("UPDATE issues SET updated_at = NOW() WHERE id = $1")
                .bind(issue.id)
                .execute(&mut **tx)
                .await
                .map_err(|e| format!("Failed to touch issue: {}", e))?;
            
            None
        };

        Ok(AcceptInteractionResult {
            interaction: updated_interaction,
            created_issues: vec![],
            continuation_issue,
        })
    }

    /// Mark interaction as accepted
    async fn mark_interaction_accepted(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        interaction_id: Uuid,
        response: Option<serde_json::Value>,
        result: Option<serde_json::Value>,
        resolver: &InteractionResolver,
    ) -> Result<IssueThreadInteraction, String> {
        let now = Utc::now();

        let interaction = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            UPDATE issue_thread_interactions
            SET status = 'accepted'::issue_thread_interaction_status,
                response = COALESCE($2, response),
                result = COALESCE($3, result),
                resolved_by_type = $4,
                resolved_by_id = $5,
                updated_at = $6
            WHERE id = $1
            RETURNING 
                id, company_id, issue_id, kind::text as kind, status::text as status,
                source_run_id, source_comment_id, payload, idempotency_key,
                continuation_policy, question, response, result,
                resolved_by_type, resolved_by_id, expires_at,
                created_at, updated_at
            "#,
        )
        .bind(interaction_id)
        .bind(response)
        .bind(result)
        .bind(resolver.resolver_type.clone())
        .bind(resolver.resolver_id.clone())
        .bind(now)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to update interaction: {}", e))?;

        Ok(interaction)
    }

    /// Reject an interaction

    /// Answer ask_user_questions interaction
    /// 
    /// Validates answers against the questions in the payload and marks the interaction as answered.
    /// Reference: Paperclip issue-thread-interactions.ts:1577-1634
    pub async fn answer_questions(
        &self,
        issue_id: Uuid,
        interaction_id: Uuid,
        input: AnswerQuestionsInput,
        resolver: InteractionResolver,
    ) -> Result<IssueThreadInteraction, String> {
        let mut tx = self.pool.begin().await
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;

        // Load and validate the interaction
        let interaction = self.get_interaction_for_update(&mut tx, interaction_id).await?;

        if interaction.issue_id != issue_id {
            return Err("Interaction does not belong to this issue".to_string());
        }

        if interaction.kind != "ask_user_questions" {
            return Err("Only ask_user_questions interactions can be answered".to_string());
        }

        if interaction.status != "pending" {
            return Err(format!("Interaction has already been resolved (status: {})", interaction.status));
        }

        // TODO: Validate answers against questions in payload
        // For now, accept answers as-is

        let now = Utc::now();
        let updated = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            UPDATE issue_thread_interactions
            SET 
                status = 'answered',
                result = $1,
                resolved_by_agent_id = $2,
                resolved_by_user_id = $3,
                resolved_at = $4,
                updated_at = $4
            WHERE id = $5 AND status = 'pending'
            RETURNING 
                id, company_id, issue_id, kind, status,
                continuation_policy, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_user_id,
                payload, result, resolved_at, created_at, updated_at
            "#,
        )
        .bind(serde_json::json!({
            "version": 1,
            "answers": input.answers,
            "summaryMarkdown": input.summary_markdown,
        }))
        .bind(if resolver.resolver_type == "agent" { 
            Some(resolver.resolver_id.parse::<Uuid>().ok()) 
        } else { 
            None 
        })
        .bind(if resolver.resolver_type == "user" { 
            Some(resolver.resolver_id.clone()) 
        } else { 
            None 
        })
        .bind(now)
        .bind(interaction_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("Failed to update interaction: {}", e))?
        .ok_or_else(|| "Interaction has already been resolved".to_string())?;

        // Touch the issue to update its timestamp
        sqlx::query("UPDATE issues SET updated_at = NOW() WHERE id = $1")
            .bind(issue_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to touch issue: {}", e))?;

        tx.commit().await
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;

        Ok(updated)
    }

    /// Cancel ask_user_questions interaction
    /// 
    /// Marks the interaction as cancelled, typically when the agent no longer needs the answer.
    /// Reference: Paperclip issue-thread-interactions.ts:1636-1691
    pub async fn cancel_questions(
        &self,
        issue_id: Uuid,
        interaction_id: Uuid,
        input: CancelQuestionsInput,
        resolver: InteractionResolver,
    ) -> Result<IssueThreadInteraction, String> {
        let mut tx = self.pool.begin().await
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;

        // Load and validate the interaction
        let interaction = self.get_interaction_for_update(&mut tx, interaction_id).await?;

        if interaction.issue_id != issue_id {
            return Err("Interaction does not belong to this issue".to_string());
        }

        if interaction.kind != "ask_user_questions" {
            return Err("Only ask_user_questions interactions can be cancelled".to_string());
        }

        if interaction.status != "pending" {
            return Err(format!("Interaction has already been resolved (status: {})", interaction.status));
        }

        let now = Utc::now();
        let updated = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            UPDATE issue_thread_interactions
            SET 
                status = 'cancelled',
                result = $1,
                resolved_by_agent_id = $2,
                resolved_by_user_id = $3,
                resolved_at = $4,
                updated_at = $4
            WHERE id = $5 AND status = 'pending'
            RETURNING 
                id, company_id, issue_id, kind, status,
                continuation_policy, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_user_id,
                payload, result, resolved_at, created_at, updated_at
            "#,
        )
        .bind(serde_json::json!({
            "version": 1,
            "answers": [],
            "cancelled": true,
            "cancellationReason": input.reason,
            "summaryMarkdown": null,
        }))
        .bind(if resolver.resolver_type == "agent" { 
            Some(resolver.resolver_id.parse::<Uuid>().ok()) 
        } else { 
            None 
        })
        .bind(if resolver.resolver_type == "user" { 
            Some(resolver.resolver_id.clone()) 
        } else { 
            None 
        })
        .bind(now)
        .bind(interaction_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("Failed to update interaction: {}", e))?
        .ok_or_else(|| "Interaction has already been resolved".to_string())?;

        // Touch the issue to update its timestamp
        sqlx::query("UPDATE issues SET updated_at = NOW() WHERE id = $1")
            .bind(issue_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to touch issue: {}", e))?;

        tx.commit().await
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;

        Ok(updated)
    }

    /// Reject an interaction
    pub async fn reject_interaction(
        &self,
        issue: &Issue,
        interaction_id: Uuid,
        input: RejectThreadInteractionInput,
        resolver: InteractionResolver,
    ) -> Result<IssueThreadInteraction, String> {
        let now = Utc::now();

        let interaction = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            UPDATE issue_thread_interactions
            SET status = 'rejected'::issue_thread_interaction_status,
                response = COALESCE($2, response),
                resolved_by_type = $3,
                resolved_by_id = $4,
                updated_at = $5
            WHERE id = $1 AND issue_id = $6
            RETURNING 
                id, company_id, issue_id, kind::text as kind, status::text as status,
                source_run_id, source_comment_id, payload, idempotency_key,
                continuation_policy, question, response, result,
                resolved_by_type, resolved_by_id, expires_at,
                created_at, updated_at
            "#,
        )
        .bind(interaction_id)
        .bind(input.response)
        .bind(resolver.resolver_type)
        .bind(resolver.resolver_id)
        .bind(now)
        .bind(issue.id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("Failed to reject interaction: {}", e))?
        .ok_or_else(|| "Interaction not found".to_string())?;

        Ok(interaction)
    }

    /// Expire request_confirmation interactions that are superseded by a user comment
    /// 
    /// When a user comments on an issue, any pending confirmation requests that have
    /// `supersedeOnUserComment: true` and were created before or at the same time as 
    /// the comment should be automatically expired.
    /// 
    /// Reference: Paperclip issue-thread-interactions.ts:1323-1380
    pub async fn expire_request_confirmations_superseded_by_comment(
        &self,
        issue_id: Uuid,
        comment_created_at: chrono::DateTime<Utc>,
        comment_author_user_id: Option<String>,
        resolver: InteractionResolver,
    ) -> Result<Vec<IssueThreadInteraction>, String> {
        // Only user comments can supersede interactions
        if comment_author_user_id.is_none() {
            return Ok(vec![]);
        }

        let mut tx = self.pool.begin().await
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;

        // Find all pending supersedable interactions for this issue
        let pending_interactions = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            SELECT 
                id, company_id, issue_id, kind, status,
                continuation_policy, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_user_id,
                payload, result, resolved_at, created_at, updated_at
            FROM issue_thread_interactions
            WHERE issue_id = $1 
              AND status = 'pending'
              AND kind IN ('request_confirmation', 'request_checkbox_confirmation', 'ask_user_questions')
              AND created_at <= $2
            "#,
        )
        .bind(issue_id)
        .bind(comment_created_at)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("Failed to query interactions: {}", e))?;

        // Filter interactions that should be superseded
        let to_expire: Vec<_> = pending_interactions
            .into_iter()
            .filter(|interaction| should_supersede_interaction_on_user_comment(interaction))
            .collect();

        if to_expire.is_empty() {
            return Ok(vec![]);
        }

        let mut expired = Vec::new();
        let now = Utc::now();

        for interaction in to_expire {
            let updated = sqlx::query_as::<_, IssueThreadInteraction>(
                r#"
                UPDATE issue_thread_interactions
                SET 
                    status = 'expired',
                    result = $1,
                    resolved_by_agent_id = $2,
                    resolved_by_user_id = $3,
                    resolved_at = $4,
                    updated_at = $4
                WHERE id = $5 AND status = 'pending'
                RETURNING 
                    id, company_id, issue_id, kind, status,
                    continuation_policy, source_comment_id, source_run_id,
                    title, summary, created_by_agent_id, created_by_user_id,
                    resolved_by_agent_id, resolved_by_user_id,
                    payload, result, resolved_at, created_at, updated_at
                "#,
            )
            .bind(serde_json::json!({
                "version": 1,
                "expirationReason": "superseded_by_comment"
            }))
            .bind(if resolver.resolver_type == "agent" { 
                Some(resolver.resolver_id.parse::<Uuid>().ok()) 
            } else { 
                None 
            })
            .bind(if resolver.resolver_type == "user" { 
                Some(resolver.resolver_id.clone()) 
            } else { 
                None 
            })
            .bind(now)
            .bind(interaction.id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("Failed to expire interaction: {}", e))?;

            if let Some(updated_interaction) = updated {
                expired.push(updated_interaction);
            }
        }

        if !expired.is_empty() {
            // Touch the issue to update its timestamp
            sqlx::query("UPDATE issues SET updated_at = NOW() WHERE id = $1")
                .bind(issue_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("Failed to touch issue: {}", e))?;
        }

        tx.commit().await
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;

        Ok(expired)
    }


    /// Expire request_confirmation interactions for a document that has been updated
    /// 
    /// When a document is updated, any pending confirmation requests targeting the old
    /// revision should be expired because the user would be making decisions based on
    /// stale content.
    /// 
    /// Reference: Paperclip issue-thread-interactions.ts:1501-1575
    pub async fn expire_stale_request_confirmations_for_issue_document(
        &self,
        issue_id: Uuid,
        document_key: Option<String>,
        current_revision_id: Option<Uuid>,
        resolver: InteractionResolver,
    ) -> Result<Vec<IssueThreadInteraction>, String> {
        let mut tx = self.pool.begin().await
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;

        // Find all pending request_confirmation interactions for this issue
        let pending_interactions = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            SELECT 
                id, company_id, issue_id, kind, status,
                continuation_policy, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_user_id,
                payload, result, resolved_at, created_at, updated_at
            FROM issue_thread_interactions
            WHERE issue_id = $1 
              AND status = 'pending'
              AND kind IN ('request_confirmation', 'request_checkbox_confirmation')
            "#,
        )
        .bind(issue_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("Failed to query interactions: {}", e))?;

        // Filter interactions with stale document targets
        let to_expire: Vec<_> = pending_interactions
            .into_iter()
            .filter(|interaction| {
                is_stale_document_confirmation(
                    interaction,
                    issue_id,
                    document_key.as_deref(),
                    current_revision_id,
                )
            })
            .collect();

        if to_expire.is_empty() {
            return Ok(vec![]);
        }

        let mut expired = Vec::new();
        let now = Utc::now();

        for interaction in to_expire {
            let stale_target = interaction.payload.get("target").cloned();

            let updated = sqlx::query_as::<_, IssueThreadInteraction>(
                r#"
                UPDATE issue_thread_interactions
                SET 
                    status = 'expired',
                    result = $1,
                    resolved_by_agent_id = $2,
                    resolved_by_user_id = $3,
                    resolved_at = $4,
                    updated_at = $4
                WHERE id = $5 AND status = 'pending'
                RETURNING 
                    id, company_id, issue_id, kind, status,
                    continuation_policy, source_comment_id, source_run_id,
                    title, summary, created_by_agent_id, created_by_user_id,
                    resolved_by_agent_id, resolved_by_user_id,
                    payload, result, resolved_at, created_at, updated_at
                "#,
            )
            .bind(serde_json::json!({
                "version": 1,
                "outcome": "stale_target",
                "staleTarget": stale_target
            }))
            .bind(if resolver.resolver_type == "agent" { 
                Some(resolver.resolver_id.parse::<Uuid>().ok()) 
            } else { 
                None 
            })
            .bind(if resolver.resolver_type == "user" { 
                Some(resolver.resolver_id.clone()) 
            } else { 
                None 
            })
            .bind(now)
            .bind(interaction.id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("Failed to expire interaction: {}", e))?;

            if let Some(updated_interaction) = updated {
                expired.push(updated_interaction);
            }
        }

        if !expired.is_empty() {
            // Touch the issue to update its timestamp
            sqlx::query("UPDATE issues SET updated_at = NOW() WHERE id = $1")
                .bind(issue_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("Failed to touch issue: {}", e))?;
        }

        tx.commit().await
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;

        Ok(expired)
    }
}

/// Creator of an interaction (agent or user)
#[derive(Debug, Clone)]
pub struct InteractionCreator {
    pub agent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
}

/// Resolver of an interaction (user or system)
#[derive(Debug, Clone)]
pub struct InteractionResolver {
    pub resolver_type: String, // "user" | "agent" | "system"
    pub resolver_id: String,
}


/// Helper function to determine if an issue status is terminal
fn is_terminal_issue_status(status: &models::IssueStatus) -> bool {
    matches!(status, models::IssueStatus::Done | models::IssueStatus::Cancelled)
}

/// Determine if an accepted confirmation should return the issue to the creator agent
/// 
/// Returns true when:
/// - Interaction kind is request_confirmation or request_checkbox_confirmation
/// - Interaction was created by an agent
/// - Resolver is a user (not the creating agent accepting their own request)
/// - Issue is currently assigned to a user (not an agent)
/// - Issue is not in a terminal status (done/cancelled)
///
/// Reference: Paperclip issue-thread-interactions.ts:186-198
fn should_return_accepted_confirmation_to_creator_agent(
    issue: &Issue,
    interaction: &IssueThreadInteraction,
    resolver: &InteractionResolver,
) -> bool {
    // Only applies to request_confirmation interactions
    let is_request_confirmation = interaction.kind == "request_confirmation" 
        || interaction.kind == "request_checkbox_confirmation";
    if !is_request_confirmation {
        return false;
    }

    // Must be created by an agent
    if interaction.created_by_agent_id.is_none() {
        return false;
    }

    // Resolver must be a user
    if resolver.resolver_type != "user" {
        return false;
    }

    // Issue must be currently assigned to a user (not an agent)
    if issue.assignee_user_id.is_none() {
        return false;
    }
    if issue.assignee_agent_id.is_some() {
        return false;
    }

    // Issue must not be in terminal status
    if is_terminal_issue_status(&issue.status) {
        return false;
    }

    true
}

/// Check if an interaction should be superseded when a user comments
/// Reference: Paperclip issue-thread-interactions.ts:200-202
fn should_supersede_interaction_on_user_comment(interaction: &IssueThreadInteraction) -> bool {
    interaction
        .payload
        .get("supersedeOnUserComment")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Check if a request_confirmation interaction targets a stale document revision
/// Reference: Paperclip issue-thread-interactions.ts:1516-1529
fn is_stale_document_confirmation(
    interaction: &IssueThreadInteraction,
    issue_id: Uuid,
    document_key: Option<&str>,
    current_revision_id: Option<Uuid>,
) -> bool {
    let Some(target) = interaction.payload.get("target") else {
        return false;
    };

    // Target must be an issue_document
    let target_type = target.get("type").and_then(|v| v.as_str());
    if target_type != Some("issue_document") {
        return false;
    }

    // Check if target belongs to this issue
    let target_issue_id = target
        .get("issueId")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Uuid>().ok())
        .unwrap_or(issue_id);
    
    if target_issue_id != issue_id {
        return false;
    }

    // If document_key is provided, verify it matches
    if let Some(doc_key) = document_key {
        let target_key = target.get("key").and_then(|v| v.as_str());
        if target_key != Some(doc_key) {
            return false;
        }
    }

    // If no current revision provided, all confirmations for this document are stale
    let Some(current_rev_id) = current_revision_id else {
        return true;
    };

    // Check if the target revision matches the current revision
    let target_revision_id = target
        .get("revisionId")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Uuid>().ok());

    target_revision_id != Some(current_rev_id)
}