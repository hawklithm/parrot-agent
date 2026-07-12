use async_trait::async_trait;
use uuid::Uuid;
use models::{IssueComment, AddCommentInput, CommentActorType};

/// Comment service trait for issue comments
#[async_trait]
pub trait CommentService: Send + Sync {
    /// List comments for an issue
    async fn list_comments(&self, issue_id: Uuid, company_id: Uuid) -> Result<Vec<IssueComment>, String>;
    
    /// Add comment to an issue
    async fn add_comment(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: AddCommentInput,
        agent_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<IssueComment, String>;
    
    /// Delete a comment
    async fn delete_comment(
        &self,
        comment_id: Uuid,
        company_id: Uuid,
        agent_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<bool, String>;
}

/// Mock implementation of CommentService
pub struct MockCommentService;

impl MockCommentService {
    pub fn new() -> Self {
        Self
    }
    
    fn create_mock_comment(id: Uuid, issue_id: Uuid, company_id: Uuid, body: String) -> IssueComment {
        IssueComment {
            id,
            issue_id,
            company_id,
            body,
            author_type: "user".to_string(),
            actor_id: None,
            author_agent_id: None,
            author_user_id: Some(Uuid::new_v4()),
            created_by_run_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}

#[async_trait]
impl CommentService for MockCommentService {
    async fn list_comments(&self, issue_id: Uuid, company_id: Uuid) -> Result<Vec<IssueComment>, String> {
        Ok(vec![
            Self::create_mock_comment(Uuid::new_v4(), issue_id, company_id, "First comment".to_string()),
            Self::create_mock_comment(Uuid::new_v4(), issue_id, company_id, "Second comment".to_string()),
        ])
    }
    
    async fn add_comment(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: AddCommentInput,
        _agent_id: Option<Uuid>,
        _user_id: Option<Uuid>,
    ) -> Result<IssueComment, String> {
        let mut comment = Self::create_mock_comment(Uuid::new_v4(), issue_id, company_id, input.body);
        if input.reopen_requested == Some(true) {
            // TODO: Trigger issue reopen logic
        }
        Ok(comment)
    }
    
    async fn delete_comment(
        &self,
        _comment_id: Uuid,
        _company_id: Uuid,
        _agent_id: Option<Uuid>,
        _user_id: Option<Uuid>,
    ) -> Result<bool, String> {
        Ok(true)
    }
}
