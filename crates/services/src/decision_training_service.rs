use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    // TODO: 添加必要的依赖（database pool, repositories）
}

impl DefaultDecisionTrainingService {
    pub fn new() -> Self {
        Self {}
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
        // TODO: 实现快照捕获逻辑
        // 1. 根据source_kind和source_id查找源决策
        // 2. 加载相关的issue、agent、workspace上下文
        // 3. 收集thread messages和approvals
        // 4. 提取commit SHA
        // 5. 构建snapshot并保存

        let now = Utc::now();
        let snapshot = DecisionTrainingSnapshotV1 {
            version: "v1".to_string(),
            decision_id: Uuid::new_v4(),
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

        Ok(DecisionTrainingExample {
            id: Uuid::new_v4(),
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
        })
    }

    async fn get_example(
        &self,
        _example_id: Uuid,
    ) -> Result<Option<DecisionTrainingExample>, TrainingError> {
        // TODO: 实现查询逻辑
        Ok(None)
    }

    async fn list_examples(
        &self,
        _input: ListInput,
    ) -> Result<Vec<DecisionTrainingExample>, TrainingError> {
        // TODO: 实现列表查询逻辑
        Ok(vec![])
    }

    async fn update_notes(
        &self,
        example_id: Uuid,
        notes: String,
        updated_by_user_id: Uuid,
    ) -> Result<DecisionTrainingExample, TrainingError> {
        // TODO: 实现notes更新逻辑
        // 1. 查找example
        // 2. 添加新的notes历史条目
        // 3. 更新notes字段
        // 4. 保存

        let now = Utc::now();
        let history_entry = DecisionTrainingNotesHistoryEntry {
            notes,
            updated_by_user_id: Some(updated_by_user_id),
            updated_at: now,
        };

        // TODO: 实际更新数据库
        Err(TrainingError::ExampleNotFound(example_id))
    }

    async fn add_tags(
        &self,
        example_id: Uuid,
        _tags: Vec<String>,
    ) -> Result<DecisionTrainingExample, TrainingError> {
        // TODO: 实现标签添加逻辑
        Err(TrainingError::ExampleNotFound(example_id))
    }

    async fn set_quality_score(
        &self,
        example_id: Uuid,
        _score: f32,
    ) -> Result<DecisionTrainingExample, TrainingError> {
        // TODO: 实现质量分数设置逻辑
        Err(TrainingError::ExampleNotFound(example_id))
    }

    async fn scrub_deleted_comments(
        &self,
        _company_id: Uuid,
        _older_than_days: i64,
    ) -> Result<usize, TrainingError> {
        // TODO: 实现清理逻辑
        // 1. 找到所有包含已删除评论的训练样本
        // 2. 从snapshot中移除这些评论
        // 3. 返回清理数量
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_capture_snapshot() {
        let service = DefaultDecisionTrainingService::new();
        let input = CaptureInput {
            company_id: Uuid::new_v4(),
            source_kind: DecisionTrainingSourceKind::IssueExecutionDecision,
            source_id: "test-123".to_string(),
        };

        let result = service.capture_snapshot(input).await;
        assert!(result.is_ok());
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
