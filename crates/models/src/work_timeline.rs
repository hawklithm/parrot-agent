use serde::{Deserialize, Serialize};

/// Timeline actor types - agent, user, system, or plugin
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum TimelineActorType {
    Agent,
    User,
    System,
    Plugin,
}

/// Timeline event kinds
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TimelineEventKind {
    Created,
    Commented,
    Approved,
    Delegated,
    Assigned,
}

/// Timeline edge kinds - representing relationships between actors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TimelineEdgeKind {
    Delegation,
    Assignment,
    Mention,
}

/// Actor in the timeline - agent, user, system, or plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkTimelineActor {
    /// Namespaced id, e.g. `agent:<uuid>`, `user:<uuid>`, `system:<id>`
    pub id: String,
    #[serde(rename = "type")]
    pub actor_type: TimelineActorType,
    pub name: String,
    pub avatar: Option<String>,
}

/// Token usage statistics for a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunUsage {
    #[serde(rename = "inputTokens")]
    pub input_tokens: i64,
    #[serde(rename = "cachedInputTokens")]
    pub cached_input_tokens: i64,
    #[serde(rename = "outputTokens")]
    pub output_tokens: i64,
    #[serde(rename = "totalTokens")]
    pub total_tokens: i64,
}

/// Span representing an agent run (the most important visual element - the bars in the Gantt chart)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkTimelineSpan {
    #[serde(rename = "actorId")]
    pub actor_id: String,
    #[serde(rename = "laneHint")]
    pub lane_hint: Option<String>,
    #[serde(rename = "runId")]
    pub run_id: String,
    #[serde(rename = "issueId")]
    pub issue_id: String,
    #[serde(rename = "issueIdentifier")]
    pub issue_identifier: Option<String>,
    #[serde(rename = "issueTitle")]
    pub issue_title: Option<String>,
    /// ISO timestamp of run start
    pub start: String,
    /// ISO timestamp of run finish, or null when the run is still in progress
    pub end: Option<String>,
    pub status: String,
    #[serde(rename = "retryOfRunId", skip_serializing_if = "Option::is_none")]
    pub retry_of_run_id: Option<String>,
    #[serde(rename = "continuationAttempt", skip_serializing_if = "Option::is_none")]
    pub continuation_attempt: Option<i32>,
    #[serde(rename = "invocationSource", skip_serializing_if = "Option::is_none")]
    pub invocation_source: Option<String>,
    pub usage: Option<RunUsage>,
}

/// Event point in the timeline (creation, comment, approval, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkTimelineEvent {
    #[serde(rename = "actorId")]
    pub actor_id: String,
    pub kind: TimelineEventKind,
    #[serde(rename = "issueId")]
    pub issue_id: String,
    /// ISO timestamp
    pub at: String,
}

/// Edge representing a relationship between actors (delegation, assignment, mention)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkTimelineEdge {
    #[serde(rename = "fromActorId")]
    pub from_actor_id: String,
    #[serde(rename = "toActorId")]
    pub to_actor_id: String,
    #[serde(rename = "issueId")]
    pub issue_id: String,
    /// ISO timestamp
    pub at: String,
    pub kind: TimelineEdgeKind,
}

/// Pagination metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelinePagination {
    pub limit: usize,
    pub offset: usize,
    #[serde(rename = "totalIssues")]
    pub total_issues: usize,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
}

/// Time window metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineWindow {
    /// ISO timestamp of the window start
    pub from: String,
    /// ISO timestamp of the window end
    pub to: String,
    /// Whether the window was capped to MAX_WINDOW_MS
    pub capped: bool,
}

/// Complete timeline result returned by the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkTimelineResult {
    pub actors: Vec<WorkTimelineActor>,
    pub spans: Vec<WorkTimelineSpan>,
    pub events: Vec<WorkTimelineEvent>,
    pub edges: Vec<WorkTimelineEdge>,
    pub pagination: TimelinePagination,
    pub window: TimelineWindow,
}
