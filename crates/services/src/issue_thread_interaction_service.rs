use std::collections::{HashMap, HashSet};

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
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                addressee_agent_id, resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
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
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                addressee_agent_id, resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
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
        let requested = input.clone();
        if let Some(addressee_agent_id) = input.addressee_agent_id {
            if creator.agent_id == Some(addressee_agent_id) {
                return Err("Agents cannot address issue-thread interactions to themselves".to_string());
            }
            if input.kind == "request_confirmation" && input.payload.get("toolAction").is_some() {
                return Err("Tool-action confirmations cannot be addressed to agents".to_string());
            }

            #[derive(sqlx::FromRow)]
            struct AddresseeAgent {
                company_id: Uuid,
                status: String,
            }
            let addressee = sqlx::query_as::<_, AddresseeAgent>(
                "SELECT company_id, status::text AS status FROM agents WHERE id = $1",
            )
            .bind(addressee_agent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| format!("Failed to validate interaction addressee: {}", e))?
            .ok_or_else(|| "addresseeAgentId must belong to the same company".to_string())?;
            if addressee.company_id != issue.company_id {
                return Err("addresseeAgentId must belong to the same company".to_string());
            }
            if matches!(addressee.status.as_str(), "paused" | "terminated" | "pending_approval") {
                return Err(format!(
                    "addresseeAgentId must reference an invokable agent (status: {})",
                    addressee.status
                ));
            }
        }

        if let Some(source_comment_id) = input.source_comment_id {
            #[derive(sqlx::FromRow)]
            struct SourceComment {
                company_id: Uuid,
                issue_id: Uuid,
            }
            let source_comment = sqlx::query_as::<_, SourceComment>(
                "SELECT company_id, issue_id FROM issue_comments WHERE id = $1",
            )
            .bind(source_comment_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| format!("Failed to validate interaction source comment: {}", e))?
            .ok_or_else(|| "sourceCommentId must belong to the same issue and company".to_string())?;
            if source_comment.company_id != issue.company_id || source_comment.issue_id != issue.id {
                return Err("sourceCommentId must belong to the same issue and company".to_string());
            }
        }

        if let Some(source_run_id) = input.source_run_id {
            let source_run_company: Option<Uuid> = sqlx::query_scalar(
                "SELECT company_id FROM heartbeat_runs WHERE id = $1",
            )
            .bind(source_run_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| format!("Failed to validate interaction source run: {}", e))?;
            if source_run_company != Some(issue.company_id) {
                return Err("sourceRunId must belong to the same company".to_string());
            }
        }

        let governance: serde_json::Value = sqlx::query_scalar(
            "SELECT interaction_resolver_governance FROM companies WHERE id = $1",
        )
        .bind(issue.company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("Failed to load interaction resolver governance: {}", e))?
        .unwrap_or_else(|| serde_json::json!({}));
        let requested_policy_input = input
            .resolver_policy
            .as_deref()
            .or_else(|| {
                governance
                    .get(&input.kind)
                    .and_then(|rule| rule.get("defaultPolicy"))
                    .and_then(serde_json::Value::as_str)
            })
            .unwrap_or_else(|| default_resolver_policy_for_kind(&input.kind));
        let resolver_policy_provenance = if input.resolver_policy.is_some() {
            "explicit"
        } else {
            "inherited"
        };
        let (requested_policy, effective_policy, effective_policy_source) = resolve_resolver_policies(
            &input.kind,
            &input.payload,
            requested_policy_input,
            &governance,
        )?;
        let now = Utc::now();
        let id = Uuid::new_v4();
        let interaction = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            INSERT INTO issue_thread_interactions (
                id, company_id, issue_id, kind, status,
                continuation_policy, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                addressee_agent_id, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, payload, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, 'pending', $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $19)
            ON CONFLICT (company_id, issue_id, idempotency_key)
                WHERE idempotency_key IS NOT NULL
            DO UPDATE SET updated_at = issue_thread_interactions.updated_at
            RETURNING 
                id, company_id, issue_id, kind, status,
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, created_by_user_id,
                addressee_agent_id, resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
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
        .bind(creator.user_id.map(|id| id.to_string()))
        .bind(input.addressee_agent_id)
        .bind(requested_policy.clone())
        .bind(effective_policy)
        .bind(resolver_policy_provenance)
        .bind(effective_policy_source)
        .bind(input.idempotency_key)
        .bind(input.payload)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| format!("Failed to create interaction: {}", e))?;

        if requested.idempotency_key.is_some()
            && !is_equivalent_create_request(&interaction, &requested, &creator, &requested_policy)
        {
            return Err("Idempotency key conflicts with a different interaction".to_string());
        }

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

        if interaction.issue_id != issue.id || interaction.company_id != issue.company_id {
            return Err("Interaction not found".to_string());
        }

        if interaction.status != "pending" {
            return Err(format!("Interaction is not pending (status: {})", interaction.status));
        }
        assert_resolver_allowed(&interaction, &resolver)?;

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
                id, company_id, issue_id, kind, status,
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
                payload, result, resolved_at,
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

        let selected_keys = input.selected_client_keys.as_ref().map(|keys| {
            keys.iter().cloned().collect::<HashSet<_>>()
        });
        if let Some(keys) = &selected_keys {
            if keys.len() != input.selected_client_keys.as_ref().map_or(0, Vec::len) {
                return Err("selectedClientKeys must be unique".to_string());
            }
            for key in keys {
                if !tasks.iter().any(|task| task.get("clientKey").and_then(|v| v.as_str()) == Some(key)) {
                    return Err(format!("Unknown suggested task client key: {key}"));
                }
            }
        }
        let selected_tasks: Vec<&serde_json::Value> = tasks
            .iter()
            .filter(|task| {
                selected_keys.as_ref().is_none_or(|keys| {
                    task.get("clientKey").and_then(|v| v.as_str()).is_some_and(|key| keys.contains(key))
                })
            })
            .collect();
        if selected_tasks.is_empty() {
            return Err("At least one suggested task must be selected".to_string());
        }
        let skipped_client_keys: Vec<String> = if selected_keys.is_some() {
            tasks.iter()
                .filter_map(|task| task.get("clientKey").and_then(|v| v.as_str()))
                .filter(|key| !selected_keys.as_ref().is_some_and(|keys| keys.contains(*key)))
                .map(str::to_owned)
                .collect()
        } else {
            vec![]
        };

        let mut created_issues = Vec::new();

        // Create child issues for each suggested task
        for task in selected_tasks {
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
            Some({
                let mut result = serde_json::json!({
                    "version": 1,
                    "outcome": "accepted",
                    "createdTasks": created_issues.iter().map(|i| i.id).collect::<Vec<_>>(),
                });
                if !skipped_client_keys.is_empty() {
                    result["skippedClientKeys"] = serde_json::json!(skipped_client_keys);
                }
                result
            }),
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
        let selected_option_ids = if interaction.kind == "request_checkbox_confirmation" {
            Some(resolve_checkbox_selection(interaction, input)?)
        } else {
            None
        };
        // Update interaction status to accepted
        let updated_interaction = self.mark_interaction_accepted(
            tx,
            interaction.id,
            input.response.clone(),
            Some({
                let mut result = serde_json::json!({
                    "version": 1,
                    "outcome": "accepted",
                });
                if let Some(selected) = selected_option_ids {
                    result["selectedOptionIds"] = serde_json::json!(selected);
                }
                result
            }),
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

        let resolved_result = match (result, response) {
            (Some(mut result), Some(response)) => {
                if let Some(object) = result.as_object_mut() {
                    object.insert("response".to_string(), response);
                    Some(result)
                } else {
                    Some(serde_json::json!({ "response": response, "value": result }))
                }
            }
            (Some(result), None) => Some(result),
            (None, Some(response)) => Some(serde_json::json!({ "response": response })),
            (None, None) => None,
        };

        let interaction = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            UPDATE issue_thread_interactions
            SET status = 'accepted',
                result = COALESCE($2, result),
                resolved_by_agent_id = $3,
                resolved_by_run_id = $4,
                resolved_by_user_id = $5,
                resolved_at = $6,
                updated_at = $6
            WHERE id = $1 AND status = 'pending'
            RETURNING 
                id, company_id, issue_id, kind, status,
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
                payload, result, resolved_at,
                created_at, updated_at
            "#,
        )
        .bind(interaction_id)
        .bind(resolved_result)
        .bind(resolver_agent_id(resolver))
        .bind(resolver.run_id)
        .bind(resolver_user_id(resolver))
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
        assert_resolver_allowed(&interaction, &resolver)?;

        if interaction.kind != "ask_user_questions" {
            return Err("Only ask_user_questions interactions can be answered".to_string());
        }

        if interaction.status != "pending" {
            return Err(format!("Interaction has already been resolved (status: {})", interaction.status));
        }

        let answers = normalize_question_answers(&interaction, &input.answers)?;

        let now = Utc::now();
        let updated = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            UPDATE issue_thread_interactions
            SET 
                status = 'answered',
                result = $1,
                resolved_by_agent_id = $2,
                resolved_by_run_id = $3,
                resolved_by_user_id = $4,
                resolved_at = $5,
                updated_at = $5
            WHERE id = $6 AND status = 'pending'
            RETURNING 
                id, company_id, issue_id, kind, status,
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
                payload, result, resolved_at, created_at, updated_at
            "#,
        )
        .bind(serde_json::json!({
            "version": 1,
            "answers": answers,
            "summaryMarkdown": input.summary_markdown,
        }))
        .bind(if resolver.resolver_type == "agent" { 
            Some(resolver.resolver_id.parse::<Uuid>().ok()) 
        } else { 
            None 
        })
        .bind(resolver.run_id)
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
        assert_resolver_allowed(&interaction, &resolver)?;

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
                resolved_by_run_id = $3,
                resolved_by_user_id = $4,
                resolved_at = $5,
                updated_at = $5
            WHERE id = $6 AND status = 'pending'
            RETURNING 
                id, company_id, issue_id, kind, status,
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
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
        .bind(resolver.run_id)
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
        let mut tx = self.pool.begin().await
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;
        let now = Utc::now();

        let current = self.get_interaction_for_update(&mut tx, interaction_id).await?;
        if current.issue_id != issue.id {
            return Err("Interaction not found".to_string());
        }
        if current.status != "pending" {
            return Err(format!("Interaction is not pending (status: {})", current.status));
        }
        if !matches!(current.kind.as_str(), "suggest_tasks" | "request_confirmation" | "request_checkbox_confirmation") {
            return Err(format!("Interactions of kind {} cannot be rejected", current.kind));
        }
        assert_resolver_allowed(&current, &resolver)?;

        let reason = input
            .reason
            .clone()
            .or_else(|| input.response.as_ref().and_then(serde_json::Value::as_str).map(str::to_owned))
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if current
            .payload
            .get("rejectRequiresReason")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
            && reason.is_none()
        {
            return Err("A decline reason is required for this confirmation".to_string());
        }

        let interaction = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            UPDATE issue_thread_interactions
            SET status = 'rejected',
                result = $2,
                resolved_by_agent_id = $3,
                resolved_by_run_id = $4,
                resolved_by_user_id = $5,
                resolved_at = $6,
                updated_at = $6
            WHERE id = $1 AND issue_id = $7 AND status = 'pending'
            RETURNING 
                id, company_id, issue_id, kind, status,
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
                payload, result, resolved_at,
                created_at, updated_at
            "#,
        )
        .bind(interaction_id)
        .bind(serde_json::json!({
            "version": 1,
            "outcome": "rejected",
            "reason": reason,
        }))
        .bind(resolver_agent_id(&resolver))
        .bind(resolver.run_id)
        .bind(resolver_user_id(&resolver))
        .bind(now)
        .bind(issue.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("Failed to reject interaction: {}", e))?
        .ok_or_else(|| "Interaction not found".to_string())?;

        sqlx::query("UPDATE issues SET updated_at = $2 WHERE id = $1")
            .bind(issue.id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to touch issue: {}", e))?;

        tx.commit().await
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;

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
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
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
                    resolved_by_run_id = $3,
                    resolved_by_user_id = $4,
                    resolved_at = $5,
                    updated_at = $5
                WHERE id = $6 AND status = 'pending'
                RETURNING 
                    id, company_id, issue_id, kind, status,
                    continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                    idempotency_key, source_comment_id, source_run_id,
                    title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                    resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
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
            .bind(resolver.run_id)
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
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
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
                    resolved_by_run_id = $3,
                    resolved_by_user_id = $4,
                    resolved_at = $5,
                    updated_at = $5
                WHERE id = $6 AND status = 'pending'
                RETURNING 
                    id, company_id, issue_id, kind, status,
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
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
            .bind(resolver.run_id)
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

    /// Withdraw a pending interaction (sets status to cancelled with outcome=withdrawn)
    pub async fn withdraw_interaction(
        &self,
        issue_id: Uuid,
        interaction_id: Uuid,
        input: models::WithdrawInteractionInput,
        resolver: InteractionResolver,
    ) -> Result<IssueThreadInteraction, String> {
        let mut tx = self.pool.begin().await
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;
        let now = Utc::now();

        let current = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            SELECT
                id, company_id, issue_id, kind::text as kind, status::text as status,
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
                payload, result, resolved_at,
                created_at, updated_at
            FROM issue_thread_interactions
            WHERE id = $1 AND issue_id = $2
            FOR UPDATE
            "#,
        )
        .bind(interaction_id)
        .bind(issue_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("Failed to load interaction for withdrawal: {}", e))?
        .ok_or_else(|| "Interaction not found".to_string())?;

        if current.status != "pending" {
            return Err(format!("Interaction is not pending (status: {})", current.status));
        }
        assert_resolver_allowed(&current, &resolver)?;

        let interaction = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            UPDATE issue_thread_interactions
            SET status = 'cancelled',
                result = $2,
                resolved_by_agent_id = $3,
                resolved_by_run_id = $4,
                resolved_by_user_id = $5,
                resolved_at = $6,
                updated_at = $6
            WHERE id = $1 AND issue_id = $7 AND status = 'pending'
            RETURNING
                id, company_id, issue_id, kind, status,
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
                payload, result, resolved_at,
                created_at, updated_at
            "#,
        )
        .bind(interaction_id)
        .bind(serde_json::json!({
            "version": 1,
            "outcome": "withdrawn",
            "reason": input.reason.as_deref().map(str::trim).filter(|reason| !reason.is_empty()),
            "withdrawnAt": now,
        }))
        .bind(resolver_agent_id(&resolver))
        .bind(resolver.run_id)
        .bind(resolver_user_id(&resolver))
        .bind(now)
        .bind(issue_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("Failed to withdraw interaction: {}", e))?
        .ok_or_else(|| "Interaction not found or already resolved".to_string())?;

        sqlx::query("UPDATE issues SET updated_at = $2 WHERE id = $1")
            .bind(issue_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to touch issue: {}", e))?;

        tx.commit().await
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;

        Ok(interaction)
    }

    /// Submit item verdicts for a request_item_verdicts interaction
    pub async fn submit_item_verdicts(
        &self,
        issue_id: Uuid,
        interaction_id: Uuid,
        input: models::SubmitItemVerdictsInput,
        resolver: InteractionResolver,
    ) -> Result<IssueThreadInteraction, String> {
        let mut tx = self.pool.begin().await
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;

        // Load interaction with FOR UPDATE lock
        let interaction = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            SELECT 
                id, company_id, issue_id, kind, status,
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
                payload, result, resolved_at,
                created_at, updated_at
            FROM issue_thread_interactions
            WHERE id = $1 AND issue_id = $2
            FOR UPDATE
            "#,
        )
        .bind(interaction_id)
        .bind(issue_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("Failed to load interaction: {}", e))?
        .ok_or_else(|| "Interaction not found".to_string())?;

        if interaction.kind != "request_item_verdicts" {
            return Err("Only request_item_verdicts interactions accept verdicts".to_string());
        }

        let payload_items = interaction.payload.get("items")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "request_item_verdicts payload is missing an items array".to_string())?;
        let payload_ids: HashSet<String> = payload_items.iter()
            .filter_map(|item| item.get("id").and_then(serde_json::Value::as_str).map(str::to_owned))
            .collect();
        if payload_ids.len() != payload_items.len() {
            return Err("request_item_verdicts payload contains an item without a unique id".to_string());
        }

        let previous_items = interaction.result.as_ref()
            .and_then(|result| result.get("items"))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut resolved_by_id: HashMap<String, serde_json::Value> = previous_items.iter()
            .filter_map(|item| {
                Some((item.get("id")?.as_str()?.to_owned(), item.clone()))
            })
            .collect();
        if resolved_by_id.keys().any(|id| !payload_ids.contains(id)) {
            return Err("Interaction result contains an unknown item id".to_string());
        }

        let mut newly_resolved = 0usize;
        let mut submitted_ids = HashSet::new();
        let enabled_verdicts: HashSet<String> = interaction
            .payload
            .get("verdicts")
            .and_then(serde_json::Value::as_array)
            .map(|values| values.iter().filter_map(serde_json::Value::as_str).map(str::to_owned).collect())
            .unwrap_or_else(|| ["approve".to_string(), "reject".to_string()].into_iter().collect());
        let require_reason_on: HashSet<String> = interaction
            .payload
            .get("requireReasonOn")
            .and_then(serde_json::Value::as_array)
            .map(|values| values.iter().filter_map(serde_json::Value::as_str).map(str::to_owned).collect())
            .unwrap_or_else(|| ["reject".to_string()].into_iter().collect());
        let now = Utc::now();
        for verdict in &input.verdicts {
            if !payload_ids.contains(&verdict.item_id) {
                return Err(format!("Unknown item verdict id: {}", verdict.item_id));
            }
            if !submitted_ids.insert(verdict.item_id.clone()) {
                return Err(format!("Duplicate item verdict id: {}", verdict.item_id));
            }
            if !enabled_verdicts.contains(&verdict.verdict) {
                return Err(format!("Verdict {} is not enabled for this item verdict request", verdict.verdict));
            }
            if require_reason_on.contains(&verdict.verdict)
                && verdict.reason.as_deref().map(str::trim).filter(|reason| !reason.is_empty()).is_none()
            {
                return Err(format!("A reason is required when verdict is {}", verdict.verdict));
            }
            if !resolved_by_id.contains_key(&verdict.item_id) {
                newly_resolved += 1;
                resolved_by_id.insert(
                    verdict.item_id.clone(),
                    serde_json::json!({
                        "id": verdict.item_id,
                        "verdict": verdict.verdict,
                        "reason": verdict.reason,
                        "resolvedByUserId": resolver_user_id(&resolver),
                        "resolvedByAgentId": resolver_agent_id(&resolver),
                        "resolvedByRunId": resolver.run_id,
                        "resolvedAt": now,
                    }),
                );
            }
        }

        if interaction.status != "pending" {
            if interaction.status == "answered"
                && input.verdicts.iter().all(|verdict| resolved_by_id.contains_key(&verdict.item_id))
            {
                tx.commit().await
                    .map_err(|e| format!("Failed to commit idempotent verdict request: {}", e))?;
                return Ok(interaction);
            }
            return Err(format!("Interaction is not pending (status: {})", interaction.status));
        }
        assert_resolver_allowed(&interaction, &resolver)?;
        if newly_resolved == 0 {
            tx.commit().await
                .map_err(|e| format!("Failed to commit unchanged verdict request: {}", e))?;
            return Ok(interaction);
        }

        let items: Vec<serde_json::Value> = payload_items.iter()
            .filter_map(|item| item.get("id").and_then(serde_json::Value::as_str))
            .filter_map(|id| resolved_by_id.get(id).cloned())
            .collect();
        let total_items = payload_ids.len();
        let complete = total_items > 0 && items.len() == total_items;

        let updated = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"
            UPDATE issue_thread_interactions
            SET status = CASE WHEN $3 THEN 'answered' ELSE 'pending' END,
                result = $2,
                resolved_by_agent_id = CASE WHEN $3 THEN $4 ELSE NULL END,
                resolved_by_run_id = CASE WHEN $3 THEN $5 ELSE NULL END,
                resolved_by_user_id = CASE WHEN $3 THEN $6 ELSE NULL END,
                resolved_at = CASE WHEN $3 THEN $7 ELSE NULL END,
                updated_at = $7
            WHERE id = $1 AND status = 'pending'
            RETURNING 
                id, company_id, issue_id, kind, status,
                continuation_policy, requested_resolver_policy, effective_resolver_policy,
                resolver_policy_provenance, effective_resolver_policy_source,
                idempotency_key, source_comment_id, source_run_id,
                title, summary, created_by_agent_id, addressee_agent_id, created_by_user_id,
                resolved_by_agent_id, resolved_by_run_id, resolved_by_user_id,
                payload, result, resolved_at,
                created_at, updated_at
            "#,
        )
        .bind(interaction_id)
        .bind(serde_json::json!({
            "version": 1,
            "outcome": "resolved",
            "complete": complete,
            "items": items,
            "summaryMarkdown": input.summary_markdown,
            "submittedAt": now,
        }))
        .bind(complete)
        .bind(resolver_agent_id(&resolver))
        .bind(resolver.run_id)
        .bind(resolver_user_id(&resolver))
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("Failed to update interaction verdicts: {}", e))?
        .ok_or_else(|| "Interaction has already been resolved".to_string())?;

        // Touch the issue
        sqlx::query("UPDATE issues SET updated_at = NOW() WHERE id = $1")
            .bind(issue_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to touch issue: {}", e))?;

        tx.commit().await
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;

        Ok(updated)
    }
}
/// Creator of an interaction (agent or user)
#[derive(Debug, Clone)]
pub struct InteractionCreator {
    pub agent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
}

fn is_equivalent_create_request(
    existing: &IssueThreadInteraction,
    requested: &models::CreateThreadInteractionInput,
    creator: &InteractionCreator,
    requested_policy: &str,
) -> bool {
    let requested_policy = canonical_resolver_policy(requested_policy).ok();
    existing.kind == requested.kind
        && existing.continuation_policy == requested.continuation_policy
        && requested_policy.as_deref() == Some(existing.requested_resolver_policy.as_str())
        && existing.idempotency_key == requested.idempotency_key
        && existing.source_comment_id == requested.source_comment_id
        && existing.source_run_id == requested.source_run_id
        && existing.title == requested.title
        && existing.summary == requested.summary
        && existing.created_by_agent_id == creator.agent_id
        && existing.created_by_user_id == creator.user_id.map(|id| id.to_string())
        && existing.addressee_agent_id == requested.addressee_agent_id
        && existing.payload == requested.payload
}

fn default_resolver_policy_for_kind(kind: &str) -> &'static str {
    match kind {
        "suggest_tasks"
        | "ask_user_questions"
        | "request_confirmation"
        | "request_checkbox_confirmation"
        | "request_item_verdicts"
        | "question"
        | "approval"
        | "review"
        | "item_verdict" => "anyone",
        _ => "anyone",
    }
}

fn canonical_resolver_policy(policy: &str) -> Result<String, String> {
    match policy {
        "anyone" | "not_creator" | "human_only" => Ok(policy.to_string()),
        // Keep the old API spellings accepted while storing the Paperclip
        // canonical audience in the interaction row.
        "board_or_agents" => Ok("anyone".to_string()),
        "board_only" => Ok("human_only".to_string()),
        _ => Err(format!("Unsupported interaction resolver policy: {policy}")),
    }
}

fn resolver_policy_rank(policy: &str) -> u8 {
    match policy {
        "anyone" => 0,
        "not_creator" => 1,
        "human_only" => 2,
        _ => u8::MAX,
    }
}

fn resolve_resolver_policies(
    kind: &str,
    payload: &serde_json::Value,
    requested: &str,
    governance: &serde_json::Value,
) -> Result<(String, String, String), String> {
    let requested = canonical_resolver_policy(requested)?;
    let mut effective = requested.clone();
    let mut effective_source = "requested";

    if kind == "request_confirmation" && payload.get("toolAction").is_some() {
        effective = "human_only".to_string();
        effective_source = "governed_action";
    } else if let Some(cap) = governance
        .get(kind)
        .and_then(|rule| rule.get("cap"))
        .and_then(serde_json::Value::as_str)
    {
        let cap = canonical_resolver_policy(cap)?;
        if resolver_policy_rank(&cap) > resolver_policy_rank(&effective) {
            effective = cap;
            effective_source = "company_cap";
        }
    }

    Ok((requested, effective, effective_source.to_string()))
}

fn resolve_checkbox_selection(
    interaction: &IssueThreadInteraction,
    input: &AcceptThreadInteractionInput,
) -> Result<Vec<String>, String> {
    let options = interaction
        .payload
        .get("options")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "request_checkbox_confirmation payload is missing an options array".to_string())?;
    let valid_ids: HashSet<String> = options
        .iter()
        .filter_map(|option| option.get("id").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect();
    let selected = input
        .selected_option_ids
        .clone()
        .or_else(|| {
            interaction
                .payload
                .get("defaultSelectedOptionIds")
                .and_then(serde_json::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
        })
        .unwrap_or_default();
    let mut unique = HashSet::new();
    for option_id in &selected {
        if !valid_ids.contains(option_id) {
            return Err(format!("Unknown selected option id: {option_id}"));
        }
        if !unique.insert(option_id) {
            return Err(format!("Duplicate selected option id: {option_id}"));
        }
    }
    let min_selected = interaction
        .payload
        .get("minSelected")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let max_selected = interaction
        .payload
        .get("maxSelected")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize);
    if selected.len() < min_selected {
        return Err(format!("At least {min_selected} options must be selected"));
    }
    if max_selected.is_some_and(|max| selected.len() > max) {
        return Err(format!("At most {} options may be selected", max_selected.unwrap()));
    }
    Ok(selected)
}

fn normalize_question_answers(
    interaction: &IssueThreadInteraction,
    answers: &[models::QuestionAnswer],
) -> Result<Vec<serde_json::Value>, String> {
    let questions = interaction
        .payload
        .get("questions")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "ask_user_questions payload is missing a questions array".to_string())?;

    let mut question_by_id = HashMap::new();
    for question in questions {
        let id = question
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "ask_user_questions payload contains a question without an id".to_string())?;
        if question_by_id.insert(id.to_owned(), question).is_some() {
            return Err(format!("ask_user_questions payload contains duplicate questionId: {id}"));
        }
    }

    let mut answer_by_question_id = HashMap::new();
    for answer in answers {
        let question = question_by_id
            .get(&answer.question_id)
            .ok_or_else(|| format!("Unknown questionId: {}", answer.question_id))?;
        if answer_by_question_id.contains_key(&answer.question_id) {
            return Err(format!("Duplicate answer for questionId: {}", answer.question_id));
        }

        let mut option_ids = Vec::new();
        let mut seen_option_ids = HashSet::new();
        for option_id in answer.option_ids.as_deref().unwrap_or_default() {
            if seen_option_ids.insert(option_id.clone()) {
                option_ids.push(option_id.clone());
            }
        }

        let valid_option_ids: HashSet<String> = question
            .get("options")
            .and_then(serde_json::Value::as_array)
            .map(|options| {
                options
                    .iter()
                    .filter_map(|option| option.get("id").and_then(serde_json::Value::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        for option_id in &option_ids {
            if !valid_option_ids.contains(option_id) {
                return Err(format!(
                    "Unknown optionId for question {}: {}",
                    answer.question_id, option_id
                ));
            }
        }

        if question
            .get("selectionMode")
            .and_then(serde_json::Value::as_str)
            == Some("single")
            && option_ids.len() > 1
        {
            return Err(format!(
                "Question {} only allows one answer",
                answer.question_id
            ));
        }

        let other_text = answer
            .other_text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let normalized = if let Some(other_text) = other_text {
            serde_json::json!({
                "questionId": answer.question_id,
                "optionIds": option_ids,
                "otherText": other_text,
            })
        } else {
            serde_json::json!({
                "questionId": answer.question_id,
                "optionIds": option_ids,
            })
        };
        answer_by_question_id.insert(answer.question_id.clone(), normalized);
    }

    for question in questions {
        let id = question
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "ask_user_questions payload contains a question without an id".to_string())?;
        let required = question
            .get("required")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if required {
            let answered = answer_by_question_id.get(id).is_some_and(|answer| {
                answer
                    .get("optionIds")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|options| !options.is_empty())
                    || answer
                        .get("otherText")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|text| !text.is_empty())
            });
            if !answered {
                return Err(format!("Question {id} requires an answer"));
            }
        }
    }

    Ok(questions
        .iter()
        .filter_map(|question| question.get("id").and_then(serde_json::Value::as_str))
        .filter_map(|id| answer_by_question_id.get(id).cloned())
        .collect())
}

/// Resolver of an interaction (user or system)
#[derive(Debug, Clone)]
pub struct InteractionResolver {
    pub resolver_type: String, // "user" | "agent" | "system"
    pub resolver_id: String,
    pub run_id: Option<Uuid>,
}

fn resolver_agent_id(resolver: &InteractionResolver) -> Option<Uuid> {
    (resolver.resolver_type == "agent")
        .then(|| resolver.resolver_id.parse::<Uuid>().ok())
        .flatten()
}

fn resolver_user_id(resolver: &InteractionResolver) -> Option<String> {
    (resolver.resolver_type == "user").then(|| resolver.resolver_id.clone())
}

fn assert_resolver_allowed(
    interaction: &IssueThreadInteraction,
    resolver: &InteractionResolver,
) -> Result<(), String> {
    if resolver.resolver_type == "system" {
        return Ok(());
    }

    let effective_policy = canonical_resolver_policy(&interaction.effective_resolver_policy)
        .unwrap_or_else(|_| interaction.effective_resolver_policy.clone());

    if resolver.resolver_type == "user" {
        if effective_policy == "not_creator"
            && interaction.created_by_user_id.as_deref() == Some(resolver.resolver_id.as_str())
        {
            return Err("This interaction requires a resolver other than its creator".to_string());
        }
        return Ok(());
    }

    if effective_policy == "human_only" {
        return Err("This interaction is human-only".to_string());
    }
    if !matches!(effective_policy.as_str(), "anyone" | "not_creator") {
        return Err("This interaction is not resolvable by an agent".to_string());
    }
    let run_id = resolver
        .run_id
        .ok_or_else(|| "A valid authenticated agent run is required to resolve this interaction".to_string())?;
    let resolver_id = resolver.resolver_id.parse::<Uuid>()
        .map_err(|_| "A valid agent resolver id is required".to_string())?;
    if interaction.addressee_agent_id.is_some_and(|id| id != resolver_id) {
        return Err("Only the addressed agent or an authorized human may resolve this interaction".to_string());
    }
    if effective_policy == "not_creator"
        && (interaction.created_by_agent_id == Some(resolver_id)
            || interaction.source_run_id == Some(run_id))
    {
        return Err("This interaction requires a resolver other than its creator".to_string());
    }
    Ok(())
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
