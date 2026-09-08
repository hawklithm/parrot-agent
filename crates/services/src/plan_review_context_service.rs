//! Plan Review Context service — aggregates plan document threads/comments for Agent wakeup.
//!
//! Paperclip source: server/src/services/plan-review-context.ts
//! Ported to Rust with raw SQL, matching limits and structure.
//!
//! Used by heartbeat runs in `planning` work mode to include review context
//! in Agent wakeup payloads, enabling continuation of interrupted plan reviews.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Context limits — matches Paperclip PLAN_REVIEW_CONTEXT_LIMITS.
pub const MAX_THREADS: usize = 20;
pub const MAX_COMMENTS: usize = 80;
pub const MAX_BODY_CHARS: usize = 1_200;
pub const MAX_TOTAL_BODY_CHARS: usize = 12_000;
pub const MAX_ANCHOR_TEXT_CHARS: usize = 500;

/// Input for `build_plan_review_context`.
#[derive(Debug, Clone)]
pub struct BuildPlanReviewContextInput {
    pub company_id: Uuid,
    pub issue_id: Uuid,
    /// Optional interaction ID to enrich context with the latest plan interaction.
    pub interaction_id: Option<Uuid>,
    /// Include context even when no plan document exists (e.g. annotation delta).
    pub include_for_issue_comment: bool,
    /// Include context for annotation delta changes.
    pub include_for_annotation_delta: bool,
    /// Issue is in `planning` work mode — Paperclip always includes context then.
    pub issue_work_mode: Option<String>,
}

/// The result — serialised into `heartbeat_runs.context_snapshot`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReviewContext {
    pub issue_id: Uuid,
    pub latest_revision_id: Option<Uuid>,
    pub latest_revision_number: Option<i64>,
    pub threads: Vec<PlanReviewContextThread>,
    pub interaction: Option<PlanReviewInteractionContext>,
    pub totals: PlanReviewContextTotals,
    pub limits: PlanReviewContextLimits,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReviewContextLimits {
    pub max_threads: usize,
    pub max_comments: usize,
    pub max_body_chars: usize,
    pub max_total_body_chars: usize,
    pub max_anchor_text_chars: usize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReviewContextTotals {
    pub open_thread_count: u64,
    pub included_thread_count: u64,
    pub omitted_thread_count: u64,
    pub comment_count: u64,
    pub included_comment_count: u64,
    pub omitted_comment_count: u64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReviewContextThread {
    pub id: Uuid,
    pub document_key: String,
    pub document_id: Uuid,
    pub status: String,
    pub revision_id: Option<Uuid>,
    pub revision_number: i64,
    pub anchor_state: String,
    pub anchor_confidence: String,
    pub selected_text: String,
    pub selected_text_truncated: bool,
    pub prefix_text: String,
    pub prefix_text_truncated: bool,
    pub suffix_text: String,
    pub suffix_text_truncated: bool,
    pub author: Option<String>,
    pub comment_count: u64,
    pub comments: Vec<PlanReviewContextComment>,
    pub comments_truncated: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReviewContextComment {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub body: String,
    pub body_truncated: bool,
    pub author: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReviewInteractionContext {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub continuation_policy: Option<String>,
    pub source_comment_id: Option<Uuid>,
    pub source_run_id: Option<Uuid>,
    pub target: Option<PlanReviewInteractionTarget>,
    pub accepted_target_revision: Option<PlanReviewInteractionTarget>,
    pub result: Option<PlanReviewInteractionResult>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReviewInteractionTarget {
    pub issue_id: Uuid,
    pub document_id: Option<Uuid>,
    pub key: String,
    pub revision_id: Option<Uuid>,
    pub revision_number: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReviewInteractionResult {
    pub outcome: Option<String>,
    pub reason: Option<String>,
    pub comment_id: Option<Uuid>,
}

// ─── Internal row types ────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct PlanDocRow {
    document_id: Uuid,
}

#[derive(sqlx::FromRow)]
struct PlanThreadRow {
    id: Uuid,
    document_key: String,
    document_id: Uuid,
    status: String,
    revision_id: Option<Uuid>,
    revision_number: i64,
    anchor_state: String,
    anchor_confidence: String,
    selected_text: String,
    prefix_text: String,
    suffix_text: String,
    author: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow, Clone)]
struct PlanCommentRow {
    id: Uuid,
    thread_id: Uuid,
    body: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct PlanInteractionRow {
    id: Uuid,
    kind: String,
    status: String,
    continuation_policy: Option<String>,
    source_comment_id: Option<Uuid>,
    source_run_id: Option<Uuid>,
    payload: Option<serde_json::Value>,
    result: Option<serde_json::Value>,
    resolved_at: Option<DateTime<Utc>>,
}

// ─── Public API ────────────────────────────────────────────────────────────────

/// Build plan review context for an issue. Returns `None` when context is not needed.
///
/// Called from heartbeat service during planning-mode wakes to enrich Agent context
/// with pending thread reviews and interaction history.
pub async fn build_plan_review_context(
    db: &PgPool,
    input: &BuildPlanReviewContextInput,
) -> anyhow::Result<Option<PlanReviewContext>> {
    // `shouldInclude` (planning work mode, issue comment, annotation delta, or a
    // resolvable interaction).
    let planning = input.issue_work_mode.as_deref() == Some("planning");
    let should_include = planning
        || input.include_for_issue_comment
        || input.include_for_annotation_delta
        || input.interaction_id.is_some();
    if !should_include {
        return Ok(None);
    }

    // 1. Fetch interaction context if provided
    let interaction = match input.interaction_id {
        Some(iid) => {
            fetch_interaction_context(db, input.company_id, input.issue_id, &iid).await?
        }
        None => None,
    };

    // 2. Find plan document (key = "plan"). Paperclip returns `null` when the
    // issue has no plan document — context is only produced from real threads.
    match fetch_plan_document(db, input.company_id, input.issue_id).await? {
        Some(doc_id) => build_from_plan_document(db, input, doc_id, interaction).await,
        None => Ok(None),
    }
}

async fn fetch_plan_document(
    db: &PgPool,
    company_id: Uuid,
    issue_id: Uuid,
) -> anyhow::Result<Option<Uuid>> {
    let row = sqlx::query_as!(
        PlanDocRow,
        r#"SELECT d.id AS document_id
           FROM issue_documents idoc
           JOIN documents d ON d.id = idoc.document_id
           WHERE idoc.company_id = $1 AND idoc.issue_id = $2 AND idoc.key = 'plan'
           LIMIT 1"#,
        company_id,
        issue_id,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| r.document_id))
}

async fn fetch_interaction_context(
    db: &PgPool,
    company_id: Uuid,
    issue_id: Uuid,
    interaction_id: &Uuid,
) -> anyhow::Result<Option<PlanReviewInteractionContext>> {
    let row = sqlx::query_as!(
        PlanInteractionRow,
        r#"SELECT id, kind, status, continuation_policy,
                  source_comment_id, source_run_id, payload, result, resolved_at
           FROM issue_thread_interactions
           WHERE id = $1 AND company_id = $2 AND issue_id = $3
           LIMIT 1"#,
        interaction_id,
        company_id,
        issue_id,
    )
    .fetch_optional(db)
    .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    let target = parse_plan_target(&row.payload, issue_id);
    let result = parse_plan_result(&row.result);
    let status = row.status.clone();

    Ok(Some(PlanReviewInteractionContext {
        id: row.id,
        kind: row.kind,
        status,
        continuation_policy: row.continuation_policy,
        source_comment_id: row.source_comment_id,
        source_run_id: row.source_run_id,
        target: target.clone(),
        accepted_target_revision: if row.status == "accepted" { target } else { None },
        result,
        resolved_at: row.resolved_at,
    }))
}

async fn build_from_plan_document(
    db: &PgPool,
    input: &BuildPlanReviewContextInput,
    doc_id: Uuid,
    interaction: Option<PlanReviewInteractionContext>,
) -> anyhow::Result<Option<PlanReviewContext>> {
    // Count open threads for this plan document
    let open_thread_count: Option<i32> = sqlx::query_scalar!(
        r#"SELECT count(*)::int FROM document_annotation_threads
           WHERE company_id = $1 AND document_id = $2 AND document_key = 'plan' AND status = 'open'"#,
        input.company_id,
        doc_id
    )
    .fetch_one(db)
    .await?;

    // Fetch open threads (limited)
    let thread_rows: Vec<PlanThreadRow> = sqlx::query_as!(
        PlanThreadRow,
        r#"SELECT id, document_key, document_id, status, current_revision_id AS revision_id,
                  current_revision_number AS revision_number, anchor_state, anchor_confidence,
                  selected_text, prefix_text, suffix_text,
                  COALESCE(created_by_agent_id::text, created_by_user_id) AS author,
                  created_at, updated_at
           FROM document_annotation_threads
           WHERE company_id = $1 AND document_id = $2 AND document_key = 'plan' AND status = 'open'
           ORDER BY updated_at DESC, id DESC
           LIMIT $3"#,
        input.company_id,
        doc_id,
        MAX_THREADS as i32,
    )
    .fetch_all(db)
    .await?;

    let thread_ids: Vec<Uuid> = thread_rows.iter().map(|t| t.id).collect();

    // Fetch comments for these threads
    let comment_rows: Vec<PlanCommentRow> = if thread_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as!(
            PlanCommentRow,
            r#"SELECT id, thread_id, body, created_at, updated_at
               FROM document_annotation_comments
               WHERE company_id = $1 AND thread_id = ANY($2)
               ORDER BY created_at ASC, id ASC
               LIMIT $3"#,
            input.company_id,
            &thread_ids,
            MAX_COMMENTS as i32,
        )
        .fetch_all(db)
        .await?
    };

    // Count total comments (for truncation detection)
    let comment_count: Option<i32> = if thread_ids.is_empty() {
        None
    } else {
        sqlx::query_scalar!(
            r#"SELECT count(*)::int FROM document_annotation_comments c
               JOIN document_annotation_threads t ON c.thread_id = t.id
               WHERE c.company_id = $1 AND t.document_key = 'plan' AND t.status = 'open'
                 AND c.thread_id = ANY($2)"#,
            input.company_id,
            &thread_ids,
        )
        .fetch_one(db)
        .await?
    };

    // Group comments by thread
    let mut comments_by_thread: std::collections::HashMap<Uuid, Vec<PlanCommentRow>> =
        std::collections::HashMap::new();
    for row in &comment_rows {
        comments_by_thread.entry(row.thread_id).or_default().push(row.clone());
    }

    // Build thread context with comments
    let mut truncated = open_thread_count.unwrap_or(0) as usize > thread_rows.len();
    let mut remaining_body_chars = MAX_TOTAL_BODY_CHARS;
    let mut included_comment_count = 0u64;

    let mut result_threads = Vec::with_capacity(thread_rows.len());
    for thread in &thread_rows {
        let selected = truncate_text(&thread.selected_text, MAX_ANCHOR_TEXT_CHARS);
        let prefix = truncate_text(&thread.prefix_text, MAX_ANCHOR_TEXT_CHARS);
        let suffix = truncate_text(&thread.suffix_text, MAX_ANCHOR_TEXT_CHARS);
        if selected.1 || prefix.1 || suffix.1 {
            truncated = true;
        }

        let thread_comments = comments_by_thread.get(&thread.id).cloned().unwrap_or_default();
        let mut comments = Vec::new();
        let mut comments_truncated = false;

        for comment in &thread_comments {
            if included_comment_count >= MAX_COMMENTS as u64 || remaining_body_chars <= 0 {
                truncated = true;
                comments_truncated = true;
                break;
            }
            let allowed = std::cmp::min(MAX_BODY_CHARS, remaining_body_chars);
            let (body, body_truncated) = truncate_text(&comment.body, allowed);
            if body_truncated {
                truncated = true;
            }
            remaining_body_chars -= body.chars().count();
            included_comment_count += 1;
            comments.push(PlanReviewContextComment {
                id: comment.id,
                thread_id: comment.thread_id,
                body,
                body_truncated,
                author: None,
                created_at: comment.created_at,
                updated_at: comment.updated_at,
            });
            comments_truncated = true;
            truncated = true;
        }

        result_threads.push(PlanReviewContextThread {
            id: thread.id,
            document_key: thread.document_key.clone(),
            document_id: thread.document_id,
            status: thread.status.clone(),
            revision_id: thread.revision_id,
            revision_number: thread.revision_number,
            anchor_state: thread.anchor_state.clone(),
            anchor_confidence: thread.anchor_confidence.clone(),
            selected_text: selected.0,
            selected_text_truncated: selected.1,
            prefix_text: prefix.0,
            prefix_text_truncated: prefix.1,
            suffix_text: suffix.0,
            suffix_text_truncated: suffix.1,
            author: thread.author.clone(),
            comment_count: thread_comments.len() as u64,
            comments,
            comments_truncated,
            created_at: thread.created_at,
            updated_at: thread.updated_at,
        });
    }

    let omitted_comment_count = (comment_count.unwrap_or(0) as usize).saturating_sub(included_comment_count as usize);
    if omitted_comment_count > 0 {
        truncated = true;
    }
    let included_thread_count = result_threads.len();
    let omitted_thread_count = open_thread_count.unwrap_or(0).saturating_sub(included_thread_count as i32);
    let comment_count = comment_count.unwrap_or(0);
    let included_comment_count = included_comment_count;
    let omitted_comment_count = (comment_count as usize).saturating_sub(included_comment_count as usize);

    Ok(Some(PlanReviewContext {
        issue_id: input.issue_id,
        latest_revision_id: None,
        latest_revision_number: None,
        threads: result_threads,
        interaction,
        totals: PlanReviewContextTotals {
            open_thread_count: open_thread_count.unwrap_or(0) as u64,
            included_thread_count: included_thread_count as u64,
            omitted_thread_count: omitted_thread_count as u64,
            comment_count: comment_count as u64,
            included_comment_count: included_comment_count,
            omitted_comment_count: omitted_comment_count as u64,
        },
        limits: limits(),
        truncated,
    }))
}

fn parse_plan_target(value: &Option<serde_json::Value>, issue_id: Uuid) -> Option<PlanReviewInteractionTarget> {
    let value = value.as_ref()?;
    let v = value.as_object()?;
    Some(PlanReviewInteractionTarget {
        issue_id,
        document_id: v.get("documentId").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()),
        key: v.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        revision_id: v.get("revisionId").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()),
        revision_number: v.get("revisionNumber").and_then(|v| v.as_i64()),
    })
}

fn parse_plan_result(value: &Option<serde_json::Value>) -> Option<PlanReviewInteractionResult> {
    let value = value.as_ref()?;
    let v = value.as_object()?;
    Some(PlanReviewInteractionResult {
        outcome: v.get("outcome").and_then(|v| v.as_str()).map(|s| s.to_string()),
        reason: v.get("reason").and_then(|v| v.as_str()).map(|s| s.to_string()),
        comment_id: v.get("commentId").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()),
    })
}

fn truncate_text(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        (text.to_string(), false)
    } else {
        (text.chars().take(max_chars).collect(), true)
    }
}

fn limits() -> PlanReviewContextLimits {
    PlanReviewContextLimits {
        max_threads: MAX_THREADS,
        max_comments: MAX_COMMENTS,
        max_body_chars: MAX_BODY_CHARS,
        max_total_body_chars: MAX_TOTAL_BODY_CHARS,
        max_anchor_text_chars: MAX_ANCHOR_TEXT_CHARS,
    }
}

impl PlanReviewContextTotals {
    fn empty() -> Self {
        Self {
            open_thread_count: 0,
            included_thread_count: 0,
            omitted_thread_count: 0,
            comment_count: 0,
            included_comment_count: 0,
            omitted_comment_count: 0,
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_text_exact() {
        let (result, truncated) = truncate_text("hello", 5);
        assert_eq!(result, "hello");
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_text_over() {
        let (result, truncated) = truncate_text("hello world", 5);
        assert_eq!(result, "hello");
        assert!(truncated);
    }

    #[test]
    fn test_truncate_text_under() {
        let (result, truncated) = truncate_text("hi", 5);
        assert_eq!(result, "hi");
        assert!(!truncated);
    }

    #[test]
    fn test_limits_match_paperclip() {
        let limits = limits();
        assert_eq!(limits.max_threads, 20);
        assert_eq!(limits.max_comments, 80);
        assert_eq!(limits.max_body_chars, 1200);
        assert_eq!(limits.max_total_body_chars, 12000);
        assert_eq!(limits.max_anchor_text_chars, 500);
    }

    #[test]
    fn test_parse_plan_target() {
        let json = serde_json::json!({
            "issueId": "11111111-1111-1111-1111-111111111111",
            "documentId": "22222222-2222-2222-2222-222222222222",
            "key": "plan",
            "revisionId": "33333333-3333-3333-3333-333333333333",
            "revisionNumber": 5
        });
        let issue_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let target = parse_plan_target(&Some(json), issue_id);
        assert!(target.is_some());
        let target = target.unwrap();
        assert_eq!(target.issue_id, issue_id);
        assert_eq!(target.key, "plan");
        assert_eq!(target.revision_number, Some(5));
    }

    #[test]
    fn test_parse_plan_result() {
        let json = serde_json::json!({
            "outcome": "accepted",
            "reason": "Looks good",
            "commentId": "44444444-4444-4444-4444-444444444444"
        });
        let result = parse_plan_result(&Some(json));
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.outcome, Some("accepted".to_string()));
        assert_eq!(result.reason, Some("Looks good".to_string()));
        assert_eq!(result.comment_id, Some(Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap()));
    }

    #[test]
    fn test_empty_totals() {
        let totals = PlanReviewContextTotals::empty();
        assert_eq!(totals.open_thread_count, 0);
        assert_eq!(totals.included_thread_count, 0);
        assert_eq!(totals.omitted_thread_count, 0);
        assert_eq!(totals.comment_count, 0);
        assert_eq!(totals.included_comment_count, 0);
        assert_eq!(totals.omitted_comment_count, 0);
    }
}
