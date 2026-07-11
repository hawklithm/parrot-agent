use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::sync::Arc;

use models::{IssueComment, IssueCommentAuthorType, IssueThreadInteraction, ThreadInteractionKind, ThreadInteractionStatus};
use repositories::IssueRepository;
use crate::{ServiceError, issue_comment_service::IssueCommentService};

/// Add comment input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCommentInput {
    pub body: String,
    #[serde(default)]
    pub reopen_requested: bool,
    #[serde(default)]
    pub interrupt: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Add comment actor
#[derive(Debug, Clone)]
pub struct CommentActor {
    pub agent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
}

/// Create thread interaction input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInteractionInput {
    pub kind: ThreadInteractionKind,
    pub question: String,
    pub agent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
}

/// Resolve interaction input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveInteractionInput {
    pub status: ThreadInteractionStatus,
    pub response: Option<String>,
    pub resolver_agent_id: Option<Uuid>,
    pub resolver_user_id: Option<Uuid>,
}

/// Default Issue Comment Service Implementation
pub struct DefaultIssueCommentService {
    issue_repo: Arc<dyn IssueRepository>,
}

impl DefaultIssueCommentService {
    pub fn new(issue_repo: Arc<dyn IssueRepository>) -> Self {
        Self { issue_repo }
    }

    /// Verify actor is allowed to comment on issue
    async fn verify_comment_permission(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        actor: &CommentActor,
    ) -> Result<(), ServiceError> {
        // Get issue to verify company isolation
        let issue = self.issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get issue: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Issue {} not found", issue_id)))?;

        if issue.company_id != company_id {
            return Err(ServiceError::Forbidden(
                "Cannot comment on issue from different company".to_string()
            ));
        }

        // Basic permission check: agent or user must be specified
        if actor.agent_id.is_none() && actor.user_id.is_none() {
            return Err(ServiceError::Forbidden(
                "Either agent_id or user_id must be provided".to_string()
            ));
        }

        Ok(())
    }

    /// Determine author type from actor
    fn determine_author_type(actor: &CommentActor) -> IssueCommentAuthorType {
        if actor.agent_id.is_some() {
            IssueCommentAuthorType::Agent
        } else if actor.user_id.is_some() {
            IssueCommentAuthorType::User
        } else {
            IssueCommentAuthorType::System
        }
    }

    /// Handle reopen requested logic
    async fn handle_reopen_if_requested(
        &self,
        issue_id: Uuid,
        reopen_requested: bool,
    ) -> Result<(), ServiceError> {
        if !reopen_requested {
            return Ok(());
        }

        // Get issue and check if it's done or cancelled
        let issue = self.issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get issue: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Issue {} not found", issue_id)))?;

        if issue.status == "done" || issue.status == "cancelled" {
            // Reopen to todo
            let mut reopened_issue = issue;
            reopened_issue.status = "todo".to_string();
            reopened_issue.updated_at = chrono::Utc::now();

            self.issue_repo
                .update(reopened_issue)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to reopen issue: {}", e)))?;
        }

        Ok(())
    }
}

#[async_trait]
impl IssueCommentService for DefaultIssueCommentService {
    async fn add_comment(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        actor: CommentActor,
        input: AddCommentInput,
    ) -> Result<IssueComment, ServiceError> {
        // Verify permission
        self.verify_comment_permission(issue_id, company_id, &actor).await?;

        // Handle reopen if requested
        self.handle_reopen_if_requested(issue_id, input.reopen_requested).await?;

        // Create comment
        let comment_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let author_type = Self::determine_author_type(&actor);

        let comment = IssueComment {
            id: comment_id,
            company_id,
            issue_id,
            author_type,
            author_agent_id: actor.agent_id,
            author_user_id: actor.user_id,
            created_by_run_id: actor.run_id,
            body: input.body,
            presentation: input.presentation.and_then(|v| serde_json::from_value(v).ok()),
            metadata: input.metadata.and_then(|v| serde_json::from_value(v).ok()),
            deleted_at: None,
            deleted_by_type: None,
            deleted_by_agent_id: None,
            deleted_by_user_id: None,
            deleted_by_run_id: None,
            follow_up_requested: input.interrupt,
            created_at: now,
            updated_at: now,
        };

        // TODO: Persist to database via CommentRepository
        // For now, return the created comment
        Ok(comment)
    }

    async fn list_comments(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<Vec<IssueComment>, ServiceError> {
        // Verify issue exists and company access
        let issue = self.issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get issue: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Issue {} not found", issue_id)))?;

        if issue.company_id != company_id {
            return Err(ServiceError::Forbidden(
                "Cannot list comments from issue in different company".to_string()
            ));
        }

        // TODO: Load from CommentRepository
        Ok(vec![])
    }

    async fn delete_comment(
        &self,
        comment_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        deleter: CommentActor,
    ) -> Result<IssueComment, ServiceError> {
        // TODO: Verify deleter is author or admin
        // TODO: Soft delete comment (set deleted_at, deleted_by fields)
        Err(ServiceError::NotImplemented("delete_comment not yet implemented".to_string()))
    }

    async fn create_interaction(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: CreateInteractionInput,
    ) -> Result<IssueThreadInteraction, ServiceError> {
        // Verify issue exists
        let issue = self.issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get issue: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Issue {} not found", issue_id)))?;

        if issue.company_id != company_id {
            return Err(ServiceError::Forbidden(
                "Cannot create interaction for issue in different company".to_string()
            ));
        }

        let interaction = IssueThreadInteraction {
            id: Uuid::new_v4(),
            company_id,
            issue_id,
            kind: input.kind,
            status: ThreadInteractionStatus::Pending,
            question: input.question,
            response: None,
            created_by_agent_id: input.agent_id,
            created_by_user_id: input.user_id,
            resolved_by_agent_id: None,
            resolved_by_user_id: None,
            created_at: chrono::Utc::now(),
            resolved_at: None,
        };

        // TODO: Persist to database via InteractionRepository
        Ok(interaction)
    }

    async fn resolve_interaction(
        &self,
        interaction_id: Uuid,
        company_id: Uuid,
        input: ResolveInteractionInput,
    ) -> Result<IssueThreadInteraction, ServiceError> {
        // TODO: Load interaction from repository
        // TODO: Update status, response, resolved_by, resolved_at
        Err(ServiceError::NotImplemented("resolve_interaction not yet implemented".to_string()))
    }

    async fn list_interactions(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<Vec<IssueThreadInteraction>, ServiceError> {
        // Verify issue access
        let issue = self.issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get issue: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Issue {} not found", issue_id)))?;

        if issue.company_id != company_id {
            return Err(ServiceError::Forbidden(
                "Cannot list interactions from issue in different company".to_string()
            ));
        }

        // TODO: Load from InteractionRepository
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_author_type() {
        let agent_actor = CommentActor {
            agent_id: Some(Uuid::new_v4()),
            user_id: None,
            run_id: None,
        };
        assert_eq!(
            DefaultIssueCommentService::determine_author_type(&agent_actor),
            IssueCommentAuthorType::Agent
        );

        let user_actor = CommentActor {
            agent_id: None,
            user_id: Some(Uuid::new_v4()),
            run_id: None,
        };
        assert_eq!(
            DefaultIssueCommentService::determine_author_type(&user_actor),
            IssueCommentAuthorType::User
        );

        let system_actor = CommentActor {
            agent_id: None,
            user_id: None,
            run_id: None,
        };
        assert_eq!(
            DefaultIssueCommentService::determine_author_type(&system_actor),
            IssueCommentAuthorType::System
        );
    }
}
