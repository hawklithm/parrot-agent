use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// 决策训练数据源类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionTrainingSourceKind {
    IssueExecutionDecision,
    IssueApproval,
    IssueThreadInteraction,
    HeartbeatDecision,
}

/// 决策训练快照（版本1）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTrainingSnapshotV1 {
    pub version: String,
    pub decision_id: Uuid,
    pub source_kind: DecisionTrainingSourceKind,
    pub source_id: String,
    pub company_id: Uuid,
    pub issue_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub decision_spec: serde_json::Value,
    pub decision_outcome: Option<String>,
    pub context: DecisionContext,
    pub captured_at: DateTime<Utc>,
    pub commit_sha: Option<String>,
}

/// 决策上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionContext {
    pub issue_title: Option<String>,
    pub issue_status: Option<String>,
    pub workspace_type: Option<String>,
    pub agent_role: Option<String>,
    pub thread_messages: Vec<ThreadMessage>,
    pub related_approvals: Vec<RelatedApproval>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMessage {
    pub id: Uuid,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedApproval {
    pub id: Uuid,
    pub approval_type: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// 决策训练notes历史条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTrainingNotesHistoryEntry {
    pub notes: String,
    pub updated_by_user_id: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
}

/// 决策训练样本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTrainingExample {
    pub id: Uuid,
    pub company_id: Uuid,
    pub source_kind: DecisionTrainingSourceKind,
    pub source_id: String,
    pub snapshot: DecisionTrainingSnapshotV1,
    pub notes: Option<String>,
    pub notes_history: Vec<DecisionTrainingNotesHistoryEntry>,
    pub tags: Vec<String>,
    pub quality_score: Option<f32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub cutoff_at: DateTime<Utc>,
}

/// 捕获输入
#[derive(Debug, Clone)]
pub struct CaptureInput {
    pub company_id: Uuid,
    pub source_kind: DecisionTrainingSourceKind,
    pub source_id: String,
}

/// 列表输入
#[derive(Debug, Clone)]
pub struct ListInput {
    pub company_id: Uuid,
    pub source_kind: Option<DecisionTrainingSourceKind>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// 决策训练服务
#[async_trait]
pub trait DecisionTrainingService: Send + Sync {
    /// 捕获决策训练快照
    async fn capture_snapshot(
        &self,
        input: CaptureInput,
    ) -> Result<DecisionTrainingExample, TrainingError>;

    /// 获取训练样本
    async fn get_example(
        &self,
        example_id: Uuid,
    ) -> Result<Option<DecisionTrainingExample>, TrainingError>;

    /// 列出训练样本
    async fn list_examples(
        &self,
        input: ListInput,
    ) -> Result<Vec<DecisionTrainingExample>, TrainingError>;

    /// 更新训练样本的notes
    async fn update_notes(
        &self,
        example_id: Uuid,
        notes: String,
        updated_by_user_id: Uuid,
    ) -> Result<DecisionTrainingExample, TrainingError>;

    /// 添加标签到训练样本
    async fn add_tags(
        &self,
        example_id: Uuid,
        tags: Vec<String>,
    ) -> Result<DecisionTrainingExample, TrainingError>;

    /// 设置质量分数
    async fn set_quality_score(
        &self,
        example_id: Uuid,
        score: f32,
    ) -> Result<DecisionTrainingExample, TrainingError>;

    /// 删除过期的训练数据
    async fn scrub_deleted_comments(
        &self,
        company_id: Uuid,
        older_than_days: i64,
    ) -> Result<usize, TrainingError>;
}

#[derive(Debug, thiserror::Error)]
pub enum TrainingError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Source not found: {kind:?} {id}")]
    SourceNotFound {
        kind: DecisionTrainingSourceKind,
        id: String,
    },

    #[error("Example not found: {0}")]
    ExampleNotFound(Uuid),

    #[error("Invalid snapshot data: {0}")]
    InvalidSnapshot(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// 默认决策训练服务实现
pub struct DefaultDecisionTrainingService {
    examples: Arc<Mutex<HashMap<Uuid, DecisionTrainingExample>>>,
}

impl DefaultDecisionTrainingService {
    pub fn new() -> Self {
        Self {
            examples: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 从决策数据中提取commit SHA
    fn extract_commit_sha(value: &serde_json::Value) -> Option<String> {
        // 尝试从不同可能的路径提取commit SHA
        value
            .get("context")
            .and_then(|ctx| ctx.get("commit_sha"))
            .or_else(|| value.get("commit_sha"))
            .or_else(|| value.get("workspace")?.get("commit_sha"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

impl Default for DefaultDecisionTrainingService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DecisionTrainingService for DefaultDecisionTrainingService {
    async fn capture_snapshot(
        &self,
        input: CaptureInput,
    ) -> Result<DecisionTrainingExample, TrainingError> {
        if input.source_id.trim().is_empty() {
            return Err(TrainingError::InvalidSnapshot(
                "source_id must not be empty".to_string(),
            ));
        }

        let now = Utc::now();
        let example_id = Uuid::new_v4();
        let snapshot = DecisionTrainingSnapshotV1 {
            version: "v1".to_string(),
            decision_id: example_id,
            source_kind: input.source_kind.clone(),
            source_id: input.source_id.clone(),
            company_id: input.company_id,
            issue_id: None,
            agent_id: None,
            decision_spec: serde_json::json!({}),
            decision_outcome: None,
            context: DecisionContext {
                issue_title: None,
                issue_status: None,
                workspace_type: None,
                agent_role: None,
                thread_messages: vec![],
                related_approvals: vec![],
            },
            captured_at: now,
            commit_sha: None,
        };

        let example = DecisionTrainingExample {
            id: example_id,
            company_id: input.company_id,
            source_kind: input.source_kind,
            source_id: input.source_id,
            snapshot,
            notes: None,
            notes_history: vec![],
            tags: vec![],
            quality_score: None,
            created_at: now,
            updated_at: now,
            cutoff_at: now,
        };
        self.examples
            .lock()
            .await
            .insert(example.id, example.clone());
        Ok(example)
    }

    async fn get_example(
        &self,
        example_id: Uuid,
    ) -> Result<Option<DecisionTrainingExample>, TrainingError> {
        Ok(self.examples.lock().await.get(&example_id).cloned())
    }

    async fn list_examples(
        &self,
        input: ListInput,
    ) -> Result<Vec<DecisionTrainingExample>, TrainingError> {
        let mut examples: Vec<_> = self
            .examples
            .lock()
            .await
            .values()
            .filter(|example| example.company_id == input.company_id)
            .filter(|example| {
                input
                    .source_kind
                    .as_ref()
                    .map_or(true, |kind| &example.source_kind == kind)
            })
            .cloned()
            .collect();
        examples.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        let offset = input.offset.unwrap_or(0).max(0) as usize;
        let limit = input.limit.unwrap_or(100).clamp(1, 1000) as usize;
        Ok(examples.into_iter().skip(offset).take(limit).collect())
    }

    async fn update_notes(
        &self,
        example_id: Uuid,
        notes: String,
        updated_by_user_id: Uuid,
    ) -> Result<DecisionTrainingExample, TrainingError> {
        let now = Utc::now();
        let history_entry = DecisionTrainingNotesHistoryEntry {
            notes: notes.clone(),
            updated_by_user_id: Some(updated_by_user_id),
            updated_at: now,
        };
        let mut examples = self.examples.lock().await;
        let example = examples
            .get_mut(&example_id)
            .ok_or(TrainingError::ExampleNotFound(example_id))?;
        example.notes = Some(notes);
        example.notes_history.push(history_entry);
        example.updated_at = now;
        Ok(example.clone())
    }

    async fn add_tags(
        &self,
        example_id: Uuid,
        tags: Vec<String>,
    ) -> Result<DecisionTrainingExample, TrainingError> {
        let mut examples = self.examples.lock().await;
        let example = examples
            .get_mut(&example_id)
            .ok_or(TrainingError::ExampleNotFound(example_id))?;
        for tag in tags {
            let tag = tag.trim();
            if !tag.is_empty() && !example.tags.iter().any(|existing| existing == tag) {
                example.tags.push(tag.to_string());
            }
        }
        example.tags.sort();
        example.updated_at = Utc::now();
        Ok(example.clone())
    }

    async fn set_quality_score(
        &self,
        example_id: Uuid,
        score: f32,
    ) -> Result<DecisionTrainingExample, TrainingError> {
        if !score.is_finite() || !(0.0..=1.0).contains(&score) {
            return Err(TrainingError::InvalidSnapshot(
                "quality score must be between 0 and 1".to_string(),
            ));
        }
        let mut examples = self.examples.lock().await;
        let example = examples
            .get_mut(&example_id)
            .ok_or(TrainingError::ExampleNotFound(example_id))?;
        example.quality_score = Some(score);
        example.updated_at = Utc::now();
        Ok(example.clone())
    }

    async fn scrub_deleted_comments(
        &self,
        company_id: Uuid,
        older_than_days: i64,
    ) -> Result<usize, TrainingError> {
        let cutoff = Utc::now() - chrono::Duration::days(older_than_days.max(0));
        let mut scrubbed = 0;
        let mut examples = self.examples.lock().await;
        for example in examples
            .values_mut()
            .filter(|example| example.company_id == company_id && example.cutoff_at <= cutoff)
        {
            let before = example.snapshot.context.thread_messages.len();
            example
                .snapshot
                .context
                .thread_messages
                .retain(|message| !message.content.trim().is_empty());
            if before != example.snapshot.context.thread_messages.len() {
                scrubbed += before - example.snapshot.context.thread_messages.len();
                example.updated_at = Utc::now();
            }
        }
        Ok(scrubbed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_capture_snapshot() {
        let service = DefaultDecisionTrainingService::new();
        let company_id = Uuid::new_v4();
        let input = CaptureInput {
            company_id,
            source_kind: DecisionTrainingSourceKind::IssueExecutionDecision,
            source_id: "test-123".to_string(),
        };

        let result = service.capture_snapshot(input).await.unwrap();
        assert_eq!(
            service.get_example(result.id).await.unwrap().unwrap().id,
            result.id
        );
        assert_eq!(
            service
                .list_examples(ListInput {
                    company_id,
                    source_kind: None,
                    limit: None,
                    offset: None,
                })
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn test_update_tags_and_quality_are_idempotent_and_validated() {
        let service = DefaultDecisionTrainingService::new();
        let example = service
            .capture_snapshot(CaptureInput {
                company_id: Uuid::new_v4(),
                source_kind: DecisionTrainingSourceKind::IssueApproval,
                source_id: "approval-1".to_string(),
            })
            .await
            .unwrap();

        let updated = service
            .update_notes(example.id, "reviewed".to_string(), Uuid::new_v4())
            .await
            .unwrap();
        assert_eq!(updated.notes.as_deref(), Some("reviewed"));
        assert_eq!(updated.notes_history.len(), 1);

        let tagged = service
            .add_tags(
                example.id,
                vec![" important ".to_string(), "important".to_string()],
            )
            .await
            .unwrap();
        assert_eq!(tagged.tags, vec!["important"]);
        assert!(service.set_quality_score(example.id, 1.1).await.is_err());
        assert_eq!(
            service
                .set_quality_score(example.id, 0.75)
                .await
                .unwrap()
                .quality_score,
            Some(0.75)
        );
    }

    #[test]
    fn test_extract_commit_sha() {
        let value = serde_json::json!({
            "context": {
                "commit_sha": "abc123"
            }
        });

        let sha = DefaultDecisionTrainingService::extract_commit_sha(&value);
        assert_eq!(sha, Some("abc123".to_string()));
    }
}
