use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
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
    #[serde(rename = "body", alias = "notes")]
    pub notes: String,
    #[serde(rename = "author", alias = "updated_by_user_id")]
    pub updated_by_user_id: Option<Uuid>,
    #[serde(rename = "at", alias = "updated_at")]
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
    pub retention_policy: String,
    pub created_by_user_id: String,
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
    pub project_id: Option<Uuid>,
    pub author_id: Option<String>,
    pub query: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Persistence input for an already captured API snapshot.
#[derive(Debug, Clone)]
pub struct PersistSnapshotInput {
    pub company_id: Uuid,
    pub source_kind: DecisionTrainingSourceKind,
    pub source_id: Uuid,
    pub issue_id: Uuid,
    pub cutoff_at: DateTime<Utc>,
    pub notes: String,
    pub tags: Vec<String>,
    pub quality_score: Option<f32>,
    pub decision_outcome: Option<String>,
    pub retention_policy: String,
    pub snapshot: Value,
    pub created_by_user_id: String,
}

#[derive(Debug, Clone)]
pub struct UpdateInput {
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
    pub quality_score: Option<f32>,
    pub updated_by_user_id: Uuid,
}

/// 决策训练服务
#[async_trait]
pub trait DecisionTrainingService: Send + Sync {
    /// Persist a snapshot assembled by an API or another trusted caller.
    async fn persist_snapshot(&self, input: PersistSnapshotInput) -> Result<Uuid, TrainingError>;

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

    /// Delete a training example and report whether it existed.
    async fn delete_example(&self, example_id: Uuid) -> Result<bool, TrainingError>;

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

    /// Atomically update notes, tags, and quality metadata.
    async fn update_example(
        &self,
        example_id: Uuid,
        input: UpdateInput,
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

/// PostgreSQL-backed implementation used by durable workers and service callers.
///
/// The API has a richer HTTP envelope, but the persistence contract is shared by
/// both paths through `decision_training_examples`.
pub struct PgDecisionTrainingService {
    pool: PgPool,
}

impl PgDecisionTrainingService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn source_kind_name(kind: &DecisionTrainingSourceKind) -> &'static str {
        match kind {
            DecisionTrainingSourceKind::IssueExecutionDecision => "execution_decision",
            DecisionTrainingSourceKind::IssueApproval => "approval",
            DecisionTrainingSourceKind::IssueThreadInteraction => "interaction",
            DecisionTrainingSourceKind::HeartbeatDecision => "interaction",
        }
    }

    fn parse_source_kind(value: &str) -> Option<DecisionTrainingSourceKind> {
        match value {
            "execution_decision" => Some(DecisionTrainingSourceKind::IssueExecutionDecision),
            "approval" => Some(DecisionTrainingSourceKind::IssueApproval),
            "interaction" => Some(DecisionTrainingSourceKind::IssueThreadInteraction),
            _ => None,
        }
    }

    fn db_error(error: sqlx::Error) -> TrainingError {
        TrainingError::DatabaseError(error.to_string())
    }

    fn parse_snapshot(
        value: Value,
        row_source_kind: &str,
        source_id: Uuid,
        company_id: Uuid,
        issue_id: Uuid,
        outcome: Option<String>,
        captured_at: DateTime<Utc>,
    ) -> Result<DecisionTrainingSnapshotV1, TrainingError> {
        if let Ok(snapshot) = serde_json::from_value::<DecisionTrainingSnapshotV1>(value.clone()) {
            return Ok(snapshot);
        }

        // Normalize the API snapshot shape (`issue/comments/runs/decision/code`)
        // into the reusable service shape without discarding retained context.
        let issue = value.get("issue").cloned().unwrap_or(Value::Null);
        let decision = value.get("decision").cloned().unwrap_or(Value::Null);
        let actor = decision.get("actor").cloned().unwrap_or(Value::Null);
        let messages = value
            .get("comments")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|comment| {
                Some(ThreadMessage {
                    id: comment.get("id").and_then(Value::as_str)?.parse().ok()?,
                    role: comment
                        .get("actorType")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string(),
                    content: comment
                        .get("body")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    created_at: comment
                        .get("createdAt")
                        .and_then(Value::as_str)
                        .and_then(|raw| raw.parse().ok())
                        .unwrap_or(captured_at),
                })
            })
            .collect();
        let captured_at = value
            .get("capturedAt")
            .and_then(Value::as_str)
            .and_then(|raw| raw.parse().ok())
            .unwrap_or(captured_at);
        let source_kind = Self::parse_source_kind(row_source_kind).ok_or_else(|| {
            TrainingError::InvalidSnapshot(format!("unsupported source kind {row_source_kind}"))
        })?;

        Ok(DecisionTrainingSnapshotV1 {
            version: "v1".to_string(),
            decision_id: source_id,
            source_kind,
            source_id: source_id.to_string(),
            company_id,
            issue_id: Some(issue_id),
            agent_id: actor
                .get("agentId")
                .and_then(Value::as_str)
                .and_then(|raw| raw.parse().ok()),
            decision_spec: decision.get("payload").cloned().unwrap_or(decision),
            decision_outcome: outcome,
            context: DecisionContext {
                issue_title: issue.get("title").and_then(Value::as_str).map(str::to_string),
                issue_status: issue.get("status").and_then(Value::as_str).map(str::to_string),
                workspace_type: None,
                agent_role: None,
                thread_messages: messages,
                related_approvals: vec![],
            },
            captured_at,
            commit_sha: value
                .pointer("/code/commitSha")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    async fn load_example(&self, example_id: Uuid) -> Result<Option<DecisionTrainingExample>, TrainingError> {
        let row = sqlx::query(
            "SELECT id, company_id, source_kind, source_id, issue_id, cutoff_at, notes,
                    notes_history, tags, quality_score, decision_outcome, retention_policy,
                    snapshot, created_by_user_id, created_at, updated_at
               FROM decision_training_examples WHERE id = $1",
        )
        .bind(example_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::db_error)?;
        row.map(|row| self.row_to_example(row)).transpose()
    }

    async fn source_context(
        &self,
        input: &CaptureInput,
    ) -> Result<(Uuid, Option<Uuid>, Value, Option<String>, DateTime<Utc>), TrainingError> {
        let kind = Self::source_kind_name(&input.source_kind);
        let source_id = Uuid::parse_str(&input.source_id).map_err(|_| TrainingError::SourceNotFound {
            kind: input.source_kind.clone(),
            id: input.source_id.clone(),
        })?;
        let source = match kind {
            "execution_decision" => sqlx::query(
                "SELECT origin_issue_id AS issue_id, origin_agent_id AS agent_id,
                        to_jsonb(d) AS payload, status::text AS outcome,
                        COALESCE(decided_at, NOW()) AS cutoff_at
                   FROM decisions d WHERE id = $1 AND company_id = $2",
            )
            .bind(source_id)
            .bind(input.company_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::db_error)?,
            "approval" => sqlx::query(
                "SELECT ia.issue_id, a.requested_by_agent_id AS agent_id,
                        to_jsonb(a) AS payload, a.status::text AS outcome,
                        COALESCE(a.decided_at, NOW()) AS cutoff_at
                   FROM approvals a
                   JOIN issue_approvals ia ON ia.approval_id = a.id
                  WHERE a.id = $1 AND a.company_id = $2 LIMIT 1",
            )
            .bind(source_id)
            .bind(input.company_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::db_error)?,
            _ => sqlx::query(
                "SELECT issue_id, created_by_agent_id AS agent_id,
                        to_jsonb(i) AS payload, i.status::text AS outcome,
                        COALESCE(i.resolved_at, NOW()) AS cutoff_at
                   FROM issue_thread_interactions i
                  WHERE id = $1 AND company_id = $2",
            )
            .bind(source_id)
            .bind(input.company_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::db_error)?,
        };
        let row = source.ok_or_else(|| TrainingError::SourceNotFound {
            kind: input.source_kind.clone(),
            id: input.source_id.clone(),
        })?;
        Ok((
            row.get("issue_id"),
            row.try_get("agent_id").unwrap_or(None),
            row.get("payload"),
            row.try_get("outcome").unwrap_or(None),
            row.get("cutoff_at"),
        ))
    }

    fn row_to_example(&self, row: sqlx::postgres::PgRow) -> Result<DecisionTrainingExample, TrainingError> {
        let source_kind_name: String = row.get("source_kind");
        let source_kind = Self::parse_source_kind(&source_kind_name).ok_or_else(|| {
            TrainingError::InvalidSnapshot(format!("unsupported source kind {source_kind_name}"))
        })?;
        let source_id: Uuid = row.get("source_id");
        let company_id: Uuid = row.get("company_id");
        let issue_id: Uuid = row.get("issue_id");
        let snapshot = Self::parse_snapshot(
            row.get("snapshot"),
            &source_kind_name,
            source_id,
            company_id,
            issue_id,
            row.try_get("decision_outcome").unwrap_or(None),
            row.get("created_at"),
        )?;
        Ok(DecisionTrainingExample {
            id: row.get("id"),
            company_id,
            source_kind,
            source_id: source_id.to_string(),
            snapshot,
            notes: row.get("notes"),
            notes_history: serde_json::from_value(row.get("notes_history"))
                .map_err(|e| TrainingError::SerializationError(e.to_string()))?,
            tags: serde_json::from_value(row.get("tags"))
                .map_err(|e| TrainingError::SerializationError(e.to_string()))?,
            quality_score: row.try_get("quality_score").unwrap_or(None),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            cutoff_at: row.get("cutoff_at"),
            retention_policy: row.get("retention_policy"),
            created_by_user_id: row.get("created_by_user_id"),
        })
    }
}

fn normalize_update_tags(tags: Vec<String>) -> Result<Vec<String>, TrainingError> {
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        if tag.len() > 64 {
            return Err(TrainingError::InvalidSnapshot(
                "training tag is too long".to_string(),
            ));
        }
        if !normalized.iter().any(|existing| existing == tag) {
            normalized.push(tag.to_string());
        }
    }
    normalized.sort();
    Ok(normalized)
}

#[async_trait]
impl DecisionTrainingService for PgDecisionTrainingService {
    async fn persist_snapshot(&self, input: PersistSnapshotInput) -> Result<Uuid, TrainingError> {
        if input.notes.len() > 100_000 {
            return Err(TrainingError::InvalidSnapshot(
                "notes must be at most 100000 characters".to_string(),
            ));
        }
        if input.tags.iter().any(|tag| tag.len() > 64) {
            return Err(TrainingError::InvalidSnapshot(
                "training tag is too long".to_string(),
            ));
        }
        if input
            .quality_score
            .is_some_and(|score| !score.is_finite() || !(0.0..=1.0).contains(&score))
        {
            return Err(TrainingError::InvalidSnapshot(
                "quality score must be between 0 and 1".to_string(),
            ));
        }

        sqlx::query_scalar(
            "INSERT INTO decision_training_examples
                (company_id, source_kind, source_id, issue_id, cutoff_at, notes,
                 notes_history, tags, quality_score, decision_outcome, retention_policy,
                 snapshot, created_by_user_id)
             VALUES ($1, $2, $3, $4, $5, $6, '[]'::jsonb, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (source_kind, source_id, created_by_user_id) DO NOTHING
             RETURNING id",
        )
        .bind(input.company_id)
        .bind(Self::source_kind_name(&input.source_kind))
        .bind(input.source_id)
        .bind(input.issue_id)
        .bind(input.cutoff_at)
        .bind(input.notes)
        .bind(serde_json::to_value(input.tags).map_err(|error| TrainingError::SerializationError(error.to_string()))?)
        .bind(input.quality_score)
        .bind(input.decision_outcome)
        .bind(input.retention_policy)
        .bind(input.snapshot)
        .bind(input.created_by_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::db_error)?
        .ok_or_else(|| TrainingError::InvalidSnapshot("duplicate decision training example".to_string()))
    }

    async fn capture_snapshot(&self, input: CaptureInput) -> Result<DecisionTrainingExample, TrainingError> {
        let source_id = Uuid::parse_str(&input.source_id).map_err(|_| TrainingError::SourceNotFound {
            kind: input.source_kind.clone(),
            id: input.source_id.clone(),
        })?;
        let existing = sqlx::query(
            "SELECT id FROM decision_training_examples
              WHERE company_id = $1 AND source_kind = $2 AND source_id = $3
                AND created_by_user_id = 'system' LIMIT 1",
        )
        .bind(input.company_id)
        .bind(Self::source_kind_name(&input.source_kind))
        .bind(source_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::db_error)?;
        if let Some(row) = existing {
            return self
                .load_example(row.get("id"))
                .await?
                .ok_or_else(|| TrainingError::ExampleNotFound(row.get("id")));
        }

        let (issue_id, agent_id, source_payload, outcome, cutoff_at) = self.source_context(&input).await?;
        let now = Utc::now();
        let issue = sqlx::query("SELECT title, status::text AS status FROM issues WHERE id = $1 AND company_id = $2")
            .bind(issue_id)
            .bind(input.company_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::db_error)?
            .ok_or(TrainingError::SourceNotFound {
                kind: input.source_kind.clone(),
                id: input.source_id.clone(),
            })?;
        let comments = sqlx::query(
            "SELECT id, actor_type::text AS actor_type, body, created_at
               FROM issue_comments WHERE issue_id = $1 AND created_at <= $2
              ORDER BY created_at ASC, id ASC",
        )
        .bind(issue_id)
        .bind(cutoff_at)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::db_error)?;
        let thread_messages: Vec<ThreadMessage> = comments
            .into_iter()
            .filter_map(|row| {
                Some(ThreadMessage {
                    id: row.try_get("id").ok()?,
                    role: row.try_get::<String, _>("actor_type").ok()?,
                    content: row.try_get::<Option<String>, _>("body").ok()?.unwrap_or_default(),
                    created_at: row.try_get("created_at").ok()?,
                })
            })
            .collect();
        let snapshot = serde_json::to_value(DecisionTrainingSnapshotV1 {
            version: "v1".to_string(),
            decision_id: source_id,
            source_kind: input.source_kind.clone(),
            source_id: input.source_id.clone(),
            company_id: input.company_id,
            issue_id: Some(issue_id),
            agent_id,
            decision_spec: source_payload,
            decision_outcome: outcome.clone(),
            context: DecisionContext {
                issue_title: issue.try_get("title").unwrap_or(None),
                issue_status: issue.try_get("status").unwrap_or(None),
                workspace_type: None,
                agent_role: None,
                thread_messages,
                related_approvals: vec![],
            },
            captured_at: now,
            commit_sha: None,
        })
        .map_err(|e| TrainingError::SerializationError(e.to_string()))?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO decision_training_examples
                (company_id, source_kind, source_id, issue_id, cutoff_at, notes,
                 notes_history, decision_outcome, snapshot, created_by_user_id)
             VALUES ($1, $2, $3, $4, $5, '', '[]'::jsonb, $6, $7, 'system')
             RETURNING id",
        )
        .bind(input.company_id)
        .bind(Self::source_kind_name(&input.source_kind))
        .bind(source_id)
        .bind(issue_id)
        .bind(cutoff_at)
        .bind(outcome)
        .bind(snapshot)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::db_error)?;
        self.load_example(id)
            .await?
            .ok_or(TrainingError::ExampleNotFound(id))
    }

    async fn get_example(&self, example_id: Uuid) -> Result<Option<DecisionTrainingExample>, TrainingError> {
        self.load_example(example_id).await
    }

    async fn delete_example(&self, example_id: Uuid) -> Result<bool, TrainingError> {
        let deleted: Option<Uuid> = sqlx::query_scalar(
            "DELETE FROM decision_training_examples WHERE id = $1 RETURNING id",
        )
        .bind(example_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::db_error)?;
        Ok(deleted.is_some())
    }

    async fn list_examples(&self, input: ListInput) -> Result<Vec<DecisionTrainingExample>, TrainingError> {
        let limit = input.limit.unwrap_or(100).clamp(1, 1000);
        let offset = input.offset.unwrap_or(0).max(0);
        let pattern = input
            .query
            .as_deref()
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .map(|query| format!("%{query}%"));
        let rows = sqlx::query(
            "SELECT e.id, e.company_id, e.source_kind, e.source_id, e.issue_id, e.cutoff_at, e.notes,
                    e.notes_history, e.tags, e.quality_score, e.decision_outcome, e.retention_policy,
                    e.snapshot, e.created_by_user_id, e.created_at, e.updated_at
               FROM decision_training_examples e
               JOIN issues i ON i.id = e.issue_id
              WHERE e.company_id = $1
                AND ($2::uuid IS NULL OR i.project_id = $2)
                AND ($3::text IS NULL OR e.source_kind = $3)
                AND ($4::text IS NULL OR e.created_by_user_id = $4)
                AND ($5::text IS NULL OR e.notes ILIKE $5 OR i.title ILIKE $5 OR COALESCE(i.identifier, '') ILIKE $5)
              ORDER BY e.created_at DESC, e.id DESC
              LIMIT $6 OFFSET $7",
        )
        .bind(input.company_id)
        .bind(input.project_id)
        .bind(input.source_kind.as_ref().map(Self::source_kind_name))
        .bind(input.author_id.as_deref())
        .bind(pattern.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::db_error)?;
        rows.into_iter().map(|row| self.row_to_example(row)).collect()
    }

    async fn update_example(
        &self,
        example_id: Uuid,
        input: UpdateInput,
    ) -> Result<DecisionTrainingExample, TrainingError> {
        let mut tx = self.pool.begin().await.map_err(Self::db_error)?;
        let current = sqlx::query(
            "SELECT notes, notes_history FROM decision_training_examples WHERE id = $1 FOR UPDATE",
        )
        .bind(example_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Self::db_error)?
        .ok_or(TrainingError::ExampleNotFound(example_id))?;
        let previous_notes: String = current.get("notes");
        let notes = input.notes.unwrap_or_else(|| previous_notes.clone());
        if notes.len() > 100_000 {
            return Err(TrainingError::InvalidSnapshot(
                "notes must be at most 100000 characters".to_string(),
            ));
        }
        let tags = input.tags.map(normalize_update_tags).transpose()?;
        if input
            .quality_score
            .is_some_and(|score| !score.is_finite() || !(0.0..=1.0).contains(&score))
        {
            return Err(TrainingError::InvalidSnapshot(
                "quality score must be between 0 and 1".to_string(),
            ));
        }
        if notes == previous_notes && tags.is_none() && input.quality_score.is_none() {
            tx.commit().await.map_err(Self::db_error)?;
            return self
                .load_example(example_id)
                .await?
                .ok_or(TrainingError::ExampleNotFound(example_id));
        }

        let mut history: Value = current.get("notes_history");
        if !history.is_array() {
            history = Value::Array(vec![]);
        }
        if notes != previous_notes {
            history.as_array_mut().expect("history normalized").push(serde_json::json!({
                "author": input.updated_by_user_id,
                "at": Utc::now(),
                "body": previous_notes,
            }));
        }
        sqlx::query(
            "UPDATE decision_training_examples
                SET notes = $1, notes_history = $2,
                    tags = COALESCE($3, tags), quality_score = COALESCE($4, quality_score),
                    updated_at = NOW()
              WHERE id = $5",
        )
        .bind(notes)
        .bind(history)
        .bind(tags.map(|value| serde_json::to_value(value)).transpose().map_err(|error| TrainingError::SerializationError(error.to_string()))?)
        .bind(input.quality_score)
        .bind(example_id)
        .execute(&mut *tx)
        .await
        .map_err(Self::db_error)?;
        tx.commit().await.map_err(Self::db_error)?;
        self.load_example(example_id)
            .await?
            .ok_or(TrainingError::ExampleNotFound(example_id))
    }

    async fn update_notes(&self, example_id: Uuid, notes: String, updated_by_user_id: Uuid) -> Result<DecisionTrainingExample, TrainingError> {
        let entry = serde_json::json!({
            "notes": notes,
            "updated_by_user_id": updated_by_user_id,
            "updated_at": Utc::now(),
        });
        let row = sqlx::query(
            "UPDATE decision_training_examples
                SET notes = $2, notes_history = notes_history || jsonb_build_array($3::jsonb), updated_at = NOW()
              WHERE id = $1 RETURNING id",
        )
        .bind(example_id)
        .bind(notes)
        .bind(entry)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::db_error)?
        .ok_or(TrainingError::ExampleNotFound(example_id))?;
        self.load_example(row.get("id")).await?.ok_or(TrainingError::ExampleNotFound(example_id))
    }

    async fn add_tags(&self, example_id: Uuid, tags: Vec<String>) -> Result<DecisionTrainingExample, TrainingError> {
        let mut example = self.load_example(example_id).await?.ok_or(TrainingError::ExampleNotFound(example_id))?;
        for tag in tags.into_iter().map(|tag| tag.trim().to_string()).filter(|tag| !tag.is_empty()) {
            if !example.tags.contains(&tag) {
                example.tags.push(tag);
            }
        }
        example.tags.sort();
        let tags = serde_json::to_value(&example.tags)
            .map_err(|e| TrainingError::SerializationError(e.to_string()))?;
        sqlx::query("UPDATE decision_training_examples SET tags = $2, updated_at = NOW() WHERE id = $1")
            .bind(example_id)
            .bind(tags)
            .execute(&self.pool)
            .await
            .map_err(Self::db_error)?;
        self.load_example(example_id).await?.ok_or(TrainingError::ExampleNotFound(example_id))
    }

    async fn set_quality_score(&self, example_id: Uuid, score: f32) -> Result<DecisionTrainingExample, TrainingError> {
        if !score.is_finite() || !(0.0..=1.0).contains(&score) {
            return Err(TrainingError::InvalidSnapshot("quality score must be between 0 and 1".to_string()));
        }
        let row = sqlx::query("UPDATE decision_training_examples SET quality_score = $2, updated_at = NOW() WHERE id = $1 RETURNING id")
            .bind(example_id)
            .bind(score)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::db_error)?
            .ok_or(TrainingError::ExampleNotFound(example_id))?;
        self.load_example(row.get("id")).await?.ok_or(TrainingError::ExampleNotFound(example_id))
    }

    async fn scrub_deleted_comments(&self, company_id: Uuid, older_than_days: i64) -> Result<usize, TrainingError> {
        let cutoff = Utc::now() - chrono::Duration::days(older_than_days.max(0));
        let rows = sqlx::query("SELECT id, snapshot FROM decision_training_examples WHERE company_id = $1 AND cutoff_at <= $2")
            .bind(company_id)
            .bind(cutoff)
            .fetch_all(&self.pool)
            .await
            .map_err(Self::db_error)?;
        let mut scrubbed = 0;
        for row in rows {
            let id: Uuid = row.get("id");
            let mut snapshot: Value = row.get("snapshot");
            let Some(comments) = snapshot.get_mut("comments").and_then(Value::as_array_mut) else { continue };
            let ids: Vec<Uuid> = comments.iter().filter_map(|comment| comment.get("id").and_then(Value::as_str)?.parse().ok()).collect();
            if ids.is_empty() { continue; }
            let existing: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM issue_comments WHERE id = ANY($1)")
                .bind(&ids)
                .fetch_all(&self.pool)
                .await
                .map_err(Self::db_error)?;
            let before = comments.len();
            comments.retain(|comment| comment.get("id").and_then(Value::as_str).and_then(|id| id.parse::<Uuid>().ok()).is_some_and(|id| existing.contains(&id)));
            scrubbed += before - comments.len();
            if before != comments.len() {
                sqlx::query("UPDATE decision_training_examples SET snapshot = $2, updated_at = NOW() WHERE id = $1")
                    .bind(id)
                    .bind(snapshot)
                    .execute(&self.pool)
                    .await
                    .map_err(Self::db_error)?;
            }
        }
        Ok(scrubbed)
    }
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
    async fn persist_snapshot(&self, input: PersistSnapshotInput) -> Result<Uuid, TrainingError> {
        if input.notes.len() > 100_000 {
            return Err(TrainingError::InvalidSnapshot(
                "notes must be at most 100000 characters".to_string(),
            ));
        }
        if input.tags.iter().any(|tag| tag.len() > 64) {
            return Err(TrainingError::InvalidSnapshot(
                "training tag is too long".to_string(),
            ));
        }
        if input
            .quality_score
            .is_some_and(|score| !score.is_finite() || !(0.0..=1.0).contains(&score))
        {
            return Err(TrainingError::InvalidSnapshot(
                "quality score must be between 0 and 1".to_string(),
            ));
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        let snapshot = DecisionTrainingSnapshotV1 {
            version: "v1".to_string(),
            decision_id: input.source_id,
            source_kind: input.source_kind.clone(),
            source_id: input.source_id.to_string(),
            company_id: input.company_id,
            issue_id: Some(input.issue_id),
            agent_id: None,
            decision_spec: input.snapshot.clone(),
            decision_outcome: input.decision_outcome,
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
        self.examples.lock().await.insert(
            id,
            DecisionTrainingExample {
                id,
                company_id: input.company_id,
                source_kind: input.source_kind,
                source_id: input.source_id.to_string(),
                snapshot,
                notes: Some(input.notes),
                notes_history: vec![],
                tags: input.tags,
                quality_score: input.quality_score,
                created_at: now,
                updated_at: now,
                cutoff_at: input.cutoff_at,
                retention_policy: input.retention_policy,
                created_by_user_id: input.created_by_user_id,
            },
        );
        Ok(id)
    }

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
            retention_policy: "scrub_deleted_comments_v1".to_string(),
            created_by_user_id: "system".to_string(),
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

    async fn delete_example(&self, example_id: Uuid) -> Result<bool, TrainingError> {
        Ok(self.examples.lock().await.remove(&example_id).is_some())
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
            .filter(|example| {
                input
                    .author_id
                    .as_ref()
                    .map_or(true, |author| &example.created_by_user_id == author)
            })
            .filter(|example| {
                input.query.as_ref().map_or(true, |query| {
                    let query = query.trim().to_ascii_lowercase();
                    example.notes.as_deref().unwrap_or_default().to_ascii_lowercase().contains(&query)
                        || example.snapshot.context.issue_title.as_deref().unwrap_or_default().to_ascii_lowercase().contains(&query)
                })
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

    async fn update_example(
        &self,
        example_id: Uuid,
        input: UpdateInput,
    ) -> Result<DecisionTrainingExample, TrainingError> {
        if input.notes.as_ref().is_some_and(|notes| notes.len() > 100_000) {
            return Err(TrainingError::InvalidSnapshot(
                "notes must be at most 100000 characters".to_string(),
            ));
        }
        let tags = input.tags.map(normalize_update_tags).transpose()?;
        if input
            .quality_score
            .is_some_and(|score| !score.is_finite() || !(0.0..=1.0).contains(&score))
        {
            return Err(TrainingError::InvalidSnapshot(
                "quality score must be between 0 and 1".to_string(),
            ));
        }
        let mut examples = self.examples.lock().await;
        let example = examples
            .get_mut(&example_id)
            .ok_or(TrainingError::ExampleNotFound(example_id))?;
        let previous_notes = example.notes.clone().unwrap_or_default();
        let notes = input.notes.unwrap_or_else(|| previous_notes.clone());
        if notes != previous_notes {
            example.notes_history.push(DecisionTrainingNotesHistoryEntry {
                notes: previous_notes,
                updated_by_user_id: Some(input.updated_by_user_id),
                updated_at: Utc::now(),
            });
        }
        example.notes = Some(notes);
        if let Some(tags) = tags {
            example.tags = tags;
        }
        if let Some(score) = input.quality_score {
            example.quality_score = Some(score);
        }
        example.updated_at = Utc::now();
        Ok(example.clone())
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
                    project_id: None,
                    author_id: None,
                    query: None,
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

    #[tokio::test]
    async fn test_postgres_service_reads_durable_table() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&database_url).await.unwrap();
        let service = PgDecisionTrainingService::new(pool);
        assert!(service.get_example(Uuid::new_v4()).await.unwrap().is_none());
    }
}
