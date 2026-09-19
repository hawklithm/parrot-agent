//! Paperclip-compatible wake payload construction and prompt rendering.
//!
//! A wakeup only carries comment **ids**; the comment bodies are loaded from
//! the database while the run is being prepared. This is what lets a resumed
//! Claude session receive the operator's latest feedback: `--resume` restores
//! the provider's history, but the new comment itself has to be delivered as
//! prompt text on stdin.
//!
//! Ports (Paperclip → Parrot):
//! - `server/src/services/heartbeat.ts` `buildPaperclipWakePayload`
//! - `server/src/services/heartbeat.ts` `extractWakeCommentIds` / `mergeWakeCommentIds`
//! - `packages/adapter-utils/src/server-utils.ts` `renderPaperclipWakePrompt`

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// Wake reason recorded when a comment merely adds to a running/finished task.
pub const WAKE_REASON_ISSUE_COMMENTED: &str = "issue_commented";
/// Wake reason recorded when a comment (re)starts work on a terminal issue.
pub const WAKE_REASON_ISSUE_REOPENED_VIA_COMMENT: &str = "issue_reopened_via_comment";
/// `context_snapshot.source` for an ordinary issue comment wake.
pub const WAKE_SOURCE_ISSUE_COMMENT: &str = "issue.comment";
/// `context_snapshot.source` for a comment that reopened a terminal issue.
pub const WAKE_SOURCE_ISSUE_COMMENT_REOPEN: &str = "issue.comment.reopen";

/// Maximum number of comments inlined into one wake payload.
pub const MAX_INLINE_WAKE_COMMENTS: usize = 8;
/// Maximum characters taken from a single comment body.
pub const MAX_INLINE_WAKE_COMMENT_BODY_CHARS: usize = 4_000;
/// Maximum characters taken across all inlined comment bodies.
pub const MAX_INLINE_WAKE_COMMENT_BODY_TOTAL_CHARS: usize = 12_000;

/// Wake reasons that (re)start work on an issue, where a resuming session may
/// not have seen the task brief yet even though the provider session resumes.
/// Paperclip keeps this list in `server-utils.ts`.
pub const ASSIGNMENT_SHAPED_WAKE_REASONS: [&str; 4] = [
    "issue_assigned",
    "issue_reopened_via_comment",
    "issue_recovery_action_restored",
    "issue_tree_restored",
];

/// Whether this wake reason must be delivered as a full task brief rather than
/// a resume delta. Mirrors `isAssignmentShapedPaperclipWakeReason`.
pub fn is_assignment_shaped_wake_reason(reason: Option<&str>) -> bool {
    reason.is_some_and(|reason| ASSIGNMENT_SHAPED_WAKE_REASONS.contains(&reason))
}

/// Wake reasons that exist only because an operator wrote a comment.
///
/// Both producers (`issue_comments.rs`) always attach `wakeCommentIds`, so a
/// wake carrying one of these reasons with no comment id cannot deliver the
/// feedback it was raised for.
pub fn is_comment_shaped_wake_reason(reason: Option<&str>) -> bool {
    matches!(
        reason,
        Some(WAKE_REASON_ISSUE_COMMENTED | WAKE_REASON_ISSUE_REOPENED_VIA_COMMENT)
    )
}

/// One comment resolved for the wake payload.
#[derive(Debug, Clone)]
pub struct WakeComment {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub author_type: String,
    pub author_id: Option<Uuid>,
    /// Body as delivered to the prompt; empty when the comment is deleted.
    pub body: String,
    pub body_truncated: bool,
    pub deleted: bool,
    pub created_at: DateTime<Utc>,
}

/// Extract ordered, de-duplicated wake comment ids from a run context snapshot.
///
/// Order is the comment order (oldest first), so the last id is the latest
/// comment. Mirrors Paperclip's `extractWakeCommentIds`.
pub fn extract_wake_comment_ids(snapshot: &Value) -> Vec<Uuid> {
    let mut ids: Vec<Uuid> = Vec::new();
    if let Some(raw) = snapshot.get("wakeCommentIds").and_then(Value::as_array) {
        for entry in raw {
            if let Some(id) = entry.as_str().and_then(|value| Uuid::parse_str(value).ok()) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    // Older contexts (and the reopen wake) only carry a single comment.
    if ids.is_empty() {
        for key in ["commentId", "wakeCommentId"] {
            if let Some(id) = snapshot
                .get(key)
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
            {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    ids
}

/// Merge comment ids from an existing snapshot with an incoming snapshot,
/// preserving oldest-first order and dropping duplicates.
pub fn merge_wake_comment_ids(existing: &Value, incoming: &Value) -> Vec<Uuid> {
    let mut ids = extract_wake_comment_ids(existing);
    for id in extract_wake_comment_ids(incoming) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// Merge an incoming wake context into an existing run's context snapshot.
///
/// Comment ids accumulate oldest-first and the latest id becomes `commentId` /
/// `wakeCommentId`. Non-wake fields are filled only when absent.
///
/// Deviation from Paperclip's `mergeCoalescedContextSnapshot`: when the
/// *incoming* wake carries comment ids, its `wakeReason` / `source` / comment
/// ids replace the existing ones. Parrot's `LegacyIssueService::update` spawns a
/// context-less `issue_reopened` assignment wake on every comment against a
/// terminal issue, and that wake can commit before the comment wake. Without
/// this precedence the run would be mislabelled as a plain reassignment and a
/// resuming session would lose the comment delta.
pub fn merge_wake_context(existing: &Value, incoming: &Value) -> Value {
    let mut merged = existing.as_object().cloned().unwrap_or_default();
    let incoming_comment_ids = extract_wake_comment_ids(incoming);
    let incoming_carries_comment = !incoming_comment_ids.is_empty();
    if let Some(incoming_object) = incoming.as_object() {
        for (key, value) in incoming_object {
            // Comment ids are merged below, not copied verbatim.
            if key == "wakeCommentIds" {
                continue;
            }
            let promote = incoming_carries_comment
                && matches!(key.as_str(), "wakeReason" | "source" | "commentId" | "wakeCommentId");
            if promote {
                merged.insert(key.clone(), value.clone());
                continue;
            }
            let absent = merged
                .get(key)
                .map(|current| current.is_null())
                .unwrap_or(true);
            if absent {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    let ids = merge_wake_comment_ids(existing, incoming);
    if !ids.is_empty() {
        merged.insert(
            "wakeCommentIds".to_string(),
            serde_json::json!(ids.iter().map(Uuid::to_string).collect::<Vec<_>>()),
        );
        if let Some(latest) = ids.last() {
            merged.insert(
                "commentId".to_string(),
                serde_json::json!(latest.to_string()),
            );
            merged.insert(
                "wakeCommentId".to_string(),
                serde_json::json!(latest.to_string()),
            );
        }
    }
    // Drop any cached payload so it is rebuilt from the merged ids.
    merged.remove("wakePayload");
    Value::Object(merged)
}

/// Load the comment bodies referenced by a wake payload.
///
/// Missing ids are simply omitted; the caller renders an explicit note so the
/// agent knows the batch was incomplete rather than silently seeing nothing.
pub async fn load_wake_comments(
    pool: &PgPool,
    company_id: Uuid,
    comment_ids: &[Uuid],
) -> Result<Vec<WakeComment>, sqlx::Error> {
    if comment_ids.is_empty() {
        return Ok(Vec::new());
    }
    // Raw rows so deleted comments stay visible as tombstones, matching
    // Paperclip's behavior of inlining a deleted comment with an empty body.
    let rows: Vec<(Uuid, Uuid, String, Option<Uuid>, Option<String>, Option<DateTime<Utc>>)> =
        sqlx::query_as(
            "SELECT id, issue_id, body, actor_id, author_type, deleted_at
             FROM issue_comments
             WHERE company_id = $1 AND id = ANY($2)",
        )
        .bind(company_id)
        .bind(comment_ids)
        .fetch_all(pool)
        .await?;

    let mut remaining_body_chars = MAX_INLINE_WAKE_COMMENT_BODY_TOTAL_CHARS;
    let mut comments = Vec::new();
    // Iterate the id list, not the row list, so payload order is the wake order.
    for comment_id in comment_ids {
        if comments.len() >= MAX_INLINE_WAKE_COMMENTS {
            break;
        }
        let Some(row) = rows.iter().find(|row| row.0 == *comment_id) else {
            continue;
        };
        let allowed_body_chars = remaining_body_chars.min(MAX_INLINE_WAKE_COMMENT_BODY_CHARS);
        if allowed_body_chars == 0 {
            break;
        }
        let deleted = row.5.is_some();
        let full_body = if deleted { "" } else { row.2.as_str() };
        let body: String = full_body.chars().take(allowed_body_chars).collect();
        let body_truncated = body.chars().count() < full_body.chars().count();
        remaining_body_chars -= body.chars().count();
        comments.push(WakeComment {
            id: row.0,
            issue_id: row.1,
            author_type: row
                .4
                .clone()
                .unwrap_or_else(|| if row.3.is_some() { "agent".into() } else { "system".into() }),
            author_id: row.3,
            body,
            body_truncated,
            deleted,
            created_at: row.5.unwrap_or_else(Utc::now),
        });
    }
    Ok(comments)
}

/// Render the inline comment batch shared by both prompt variants.
///
/// Format is byte-for-byte Paperclip's `renderPaperclipWakePrompt` comment loop:
///
/// ```text
/// New comments in order:
/// 1. comment <id> at <iso> by <authorType> <authorId>
/// <body>
/// ```
fn render_comment_batch(lines: &mut Vec<String>, comments: &[WakeComment]) {
    if comments.is_empty() {
        return;
    }
    lines.push("New comments in order:".to_string());
    for (index, comment) in comments.iter().enumerate() {
        let author_label = match comment.author_id {
            Some(id) => format!("{} {}", comment.author_type, id),
            None => comment.author_type.clone(),
        };
        lines.push(format!(
            "{}. comment {} at {} by {}",
            index + 1,
            comment.id,
            comment.created_at.to_rfc3339(),
            author_label
        ));
        if comment.body.is_empty() {
            lines.push("(deleted comment)".to_string());
        } else {
            lines.push(comment.body.clone());
        }
        if comment.body_truncated {
            lines.push("[comment body truncated]".to_string());
        }
        lines.push(String::new());
    }
}

/// Summary bullets describing the wake, shared by both prompt variants.
fn render_wake_summary(
    reason: Option<&str>,
    issue_identifier: Option<&str>,
    issue_title: &str,
    requested_count: usize,
    comments: &[WakeComment],
    latest_comment_id: Option<Uuid>,
    missing_count: usize,
) -> Vec<String> {
    let issue_label = issue_identifier.unwrap_or("unknown");
    let mut lines = vec![
        format!("- reason: {}", reason.unwrap_or("unknown")),
        format!("- issue: {issue_label} {issue_title}").trim_end().to_string(),
    ];
    if requested_count > 0 {
        lines.push(format!(
            "- pending comments: {}/{}",
            comments.len(),
            requested_count
        ));
        lines.push(format!(
            "- latest comment id: {}",
            latest_comment_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
    }
    lines.push(format!(
        "- fallback fetch needed: {}",
        if missing_count > 0 { "yes" } else { "no" }
    ));
    lines
}

/// Render the resume delta delivered to a session that is being resumed.
///
/// Paperclip deliberately replaces the task template here: re-sending the
/// original brief would replay the task as a brand-new user message and bury
/// the operator's actual feedback.
pub fn render_resume_delta_prompt(
    reason: Option<&str>,
    issue_identifier: Option<&str>,
    issue_title: &str,
    comments: &[WakeComment],
    requested_count: usize,
    missing_count: usize,
) -> String {
    let mut lines = vec![
        "## Parrot Resume Delta".to_string(),
        String::new(),
        "You are resuming an existing Parrot session.".to_string(),
        "This heartbeat is scoped to the issue below. Do not switch to another issue until you have handled this wake.".to_string(),
        "Focus on the new wake delta below and continue the current task without restating the full heartbeat boilerplate.".to_string(),
        "Fetch the API thread only when `fallbackFetchNeeded` is true or you need broader history than this batch.".to_string(),
        String::new(),
    ];
    lines.extend(render_wake_summary(
        reason,
        issue_identifier,
        issue_title,
        requested_count,
        comments,
        comments.last().map(|comment| comment.id),
        missing_count,
    ));
    lines.push("- issue description: omitted from this resume delta; fetch the issue if you need the latest brief".to_string());
    lines.push(String::new());
    render_comment_batch(&mut lines, comments);
    lines.join("\n").trim().to_string()
}

/// Render the wake payload appended to a full task brief.
///
/// Used for fresh sessions and assignment-shaped wakes, where the session may
/// still need the complete brief but must also see the new comments.
pub fn render_wake_payload_prompt(
    reason: Option<&str>,
    issue_identifier: Option<&str>,
    issue_title: &str,
    comments: &[WakeComment],
    requested_count: usize,
    missing_count: usize,
) -> String {
    let mut lines = vec![
        "## Parrot Wake Payload".to_string(),
        String::new(),
        "Treat this wake payload as the highest-priority change for the current heartbeat.".to_string(),
        "This heartbeat is scoped to the issue below. Do not switch to another issue until you have handled this wake.".to_string(),
    ];
    if !comments.is_empty() {
        lines.push("Before generic repo exploration or boilerplate heartbeat updates, acknowledge the latest comment and explain how it changes your next action.".to_string());
    }
    lines.push("Use this inline wake data first before refetching the issue thread.".to_string());
    lines.push(String::new());
    lines.extend(render_wake_summary(
        reason,
        issue_identifier,
        issue_title,
        requested_count,
        comments,
        comments.last().map(|comment| comment.id),
        missing_count,
    ));
    lines.push(String::new());
    render_comment_batch(&mut lines, comments);
    lines.join("\n").trim().to_string()
}

/// Render the prompt for a comment wake whose comment bodies could not be read.
///
/// Sending the task template here would replay the original task as if it were
/// the operator's message — the exact defect. The agent is told what happened
/// and given the ids so it can decide to refetch; the template is dropped
/// either way.
pub fn render_empty_comment_delta_prompt(
    reason: Option<&str>,
    issue_identifier: Option<&str>,
    issue_title: &str,
    requested_count: usize,
) -> String {
    let mut lines = vec![
        "## Parrot Wake Payload".to_string(),
        String::new(),
        "A new comment was left on this issue, but its body could not be loaded (the comment counted below has no readable row).".to_string(),
        "This heartbeat is scoped to the issue below. Do not switch to another issue until you have handled this wake.".to_string(),
        "This is not a new task assignment: do not re-run the original task from scratch.".to_string(),
        "Fetch the issue thread through the API and act on the comment you find there.".to_string(),
        String::new(),
    ];
    lines.extend(render_wake_summary(
        reason,
        issue_identifier,
        issue_title,
        requested_count,
        &[],
        None,
        requested_count,
    ));
    lines.join("\n").trim().to_string()
}

/// Metrics recorded alongside the final prompt so a run's input can be audited
/// without logging the (potentially sensitive) prompt body itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptSource {
    /// The original task template, with no wake delta.
    TaskTemplate,
    /// Task template plus an inline wake payload.
    WakePayload,
    /// Resume delta replacing the task template.
    ResumeDelta,
    /// A comment wake whose comments could not be loaded. Never the bare task
    /// template: that is the replay defect, and it must be distinguishable in
    /// the logs rather than silently shipped as a normal heartbeat.
    EmptyCommentDelta,
}

impl PromptSource {
    pub fn as_str(self) -> &'static str {
        match self {
            PromptSource::TaskTemplate => "task_template",
            PromptSource::WakePayload => "wake_payload",
            PromptSource::ResumeDelta => "resume_delta",
            PromptSource::EmptyCommentDelta => "empty_comment_delta",
        }
    }
}

/// The prompt to send on stdin plus its provenance.
#[derive(Debug, Clone)]
pub struct RenderedPrompt {
    pub prompt: String,
    pub source: PromptSource,
    pub comment_ids: Vec<Uuid>,
    /// Comment ids requested by the wake that had no row (deleted or absent).
    pub missing_comment_count: usize,
}

/// Choose and render the prompt for a run.
///
/// `resumed_session` is true when a provider session was found and `--resume`
/// was appended, which is exactly the case where the original template must be
/// dropped in favor of the delta.
pub fn render_run_prompt(
    base_prompt: String,
    resumed_session: bool,
    reason: Option<&str>,
    issue_identifier: Option<&str>,
    issue_title: &str,
    comments: &[WakeComment],
    requested_comment_ids: &[Uuid],
) -> RenderedPrompt {
    // The ids the wake asked for, not the ids that loaded: a wake whose
    // comments were deleted still carries ids, and that is what distinguishes
    // it from a wake that never mentioned a comment at all.
    let comment_ids = requested_comment_ids.to_vec();
    let requested_count = requested_comment_ids.len();
    let missing_comment_count = requested_count.saturating_sub(comments.len());

    // A fresh session still needs the brief even when comments are present;
    // only an ordinary (non-assignment-shaped) resume drops it.
    if resumed_session && !is_assignment_shaped_wake_reason(reason) && !comments.is_empty() {
        return RenderedPrompt {
            prompt: render_resume_delta_prompt(
                reason,
                issue_identifier,
                issue_title,
                comments,
                requested_count,
                missing_comment_count,
            ),
            source: PromptSource::ResumeDelta,
            comment_ids,
            missing_comment_count,
        };
    }

    if comments.is_empty() {
        // A wake carrying comment ids that produced no bodies is the replay
        // defect: the turn would otherwise be sent the original task template
        // and nothing else. Replace the template with an explicit instruction
        // to refetch the thread, and mark the provenance so the case is
        // visible in the run log instead of looking like a fresh heartbeat.
        if !comment_ids.is_empty() {
            return RenderedPrompt {
                prompt: render_empty_comment_delta_prompt(
                    reason,
                    issue_identifier,
                    issue_title,
                    requested_count,
                ),
                source: PromptSource::EmptyCommentDelta,
                comment_ids,
                missing_comment_count,
            };
        }
        return RenderedPrompt {
            prompt: base_prompt,
            source: PromptSource::TaskTemplate,
            comment_ids,
            missing_comment_count,
        };
    }

    let payload = render_wake_payload_prompt(
        reason,
        issue_identifier,
        issue_title,
        comments,
        requested_count,
        missing_comment_count,
    );
    RenderedPrompt {
        prompt: format!("{base_prompt}\n\n{payload}"),
        source: PromptSource::WakePayload,
        comment_ids,
        missing_comment_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn comment(id: u128, body: &str) -> WakeComment {
        WakeComment {
            id: Uuid::from_u128(id),
            issue_id: Uuid::from_u128(0x10),
            author_type: "user".to_string(),
            author_id: Some(Uuid::from_u128(0x20)),
            body: body.to_string(),
            body_truncated: false,
            deleted: false,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn extracts_single_comment_key_when_no_batch_is_present() {
        let snapshot = json!({ "commentId": "00000000-0000-0000-0000-0000000000aa" });
        assert_eq!(
            extract_wake_comment_ids(&snapshot),
            vec![Uuid::from_u128(0xaa)]
        );
    }

    #[test]
    fn merges_batches_without_duplicating_or_reordering() {
        let existing = json!({ "wakeCommentIds": [
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
        ] });
        let incoming = json!({ "wakeCommentIds": [
            "00000000-0000-0000-0000-000000000002",
            "00000000-0000-0000-0000-000000000003",
        ] });
        assert_eq!(
            merge_wake_comment_ids(&existing, &incoming),
            vec![
                Uuid::from_u128(1),
                Uuid::from_u128(2),
                Uuid::from_u128(3)
            ]
        );
    }

    #[test]
    fn resume_delta_replaces_the_task_template() {
        let rendered = render_run_prompt(
            "Task: Create a new agent".to_string(),
            true,
            Some(WAKE_REASON_ISSUE_COMMENTED),
            Some("MAR-12"),
            "Create a new agent",
            &[comment(2, "但是agent并没有按照预期正常创建，所以需要重新创建")],
            &[Uuid::from_u128(2)],
        );
        assert_eq!(rendered.source, PromptSource::ResumeDelta);
        assert!(rendered.prompt.contains("## Parrot Resume Delta"));
        assert!(rendered.prompt.contains("但是agent并没有按照预期正常创建，所以需要重新创建"));
        assert!(
            !rendered.prompt.contains("Task: Create a new agent"),
            "a resumed session must not replay the original brief"
        );
        // The comment batch keeps Paperclip's exact numbered form.
        assert!(rendered.prompt.contains("1. comment 00000000-0000-0000-0000-000000000002 at"));
    }

    #[test]
    fn reopened_comment_keeps_the_full_brief_on_resume() {
        let rendered = render_run_prompt(
            "Task: Create a new agent".to_string(),
            true,
            Some(WAKE_REASON_ISSUE_REOPENED_VIA_COMMENT),
            Some("MAR-12"),
            "Create a new agent",
            &[comment(2, "recreate it")],
            &[Uuid::from_u128(2)],
        );
        assert_eq!(rendered.source, PromptSource::WakePayload);
        assert!(rendered.prompt.starts_with("Task: Create a new agent"));
        assert!(rendered.prompt.contains("recreate it"));
    }

    #[test]
    fn fresh_session_without_comments_keeps_the_template() {
        let rendered = render_run_prompt(
            "Task: Create a new agent".to_string(),
            false,
            Some("issue_assigned"),
            Some("MAR-12"),
            "Create a new agent",
            &[],
            &[],
        );
        assert_eq!(rendered.source, PromptSource::TaskTemplate);
        assert_eq!(rendered.prompt, "Task: Create a new agent");
    }

    #[test]
    fn missing_comments_are_reported_to_the_agent() {
        let rendered = render_run_prompt(
            "Task".to_string(),
            true,
            Some(WAKE_REASON_ISSUE_COMMENTED),
            None,
            "Title",
            &[comment(1, "hello")],
            &[Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)],
        );
        assert_eq!(rendered.missing_comment_count, 2);
        assert!(rendered.prompt.contains("- fallback fetch needed: yes"));
        assert!(rendered.prompt.contains("- pending comments: 1/3"));
    }

    #[test]
    fn comment_wake_whose_bodies_are_gone_does_not_replay_the_template() {
        // Deleted or unreadable comment bodies must not degrade the run into a
        // bare task-template replay: that is the defect being fixed.
        let rendered = render_run_prompt(
            "Task: Create a new agent".to_string(),
            true,
            Some(WAKE_REASON_ISSUE_COMMENTED),
            Some("MAR-12"),
            "Create a new agent",
            &[],
            &[Uuid::from_u128(1)],
        );
        assert_eq!(rendered.source, PromptSource::EmptyCommentDelta);
        assert!(rendered.prompt.contains("could not be loaded"));
        assert!(rendered.prompt.contains("- fallback fetch needed: yes"));
        assert!(!rendered.prompt.contains("Task: Create a new agent"));
    }

    #[test]
    fn template_replay_is_reserved_for_wakes_without_comment_ids() {
        let rendered = render_run_prompt(
            "Task: Create a new agent".to_string(),
            false,
            None,
            None,
            "Create a new agent",
            &[],
            &[],
        );
        assert_eq!(rendered.source, PromptSource::TaskTemplate);
        assert_eq!(rendered.prompt, "Task: Create a new agent");
    }

    #[test]
    fn incoming_comment_wake_wins_over_a_raceless_context_less_assignment() {
        // The reopen assignment wake (no comment) can commit first; the comment
        // wake that follows must re-label the run so the delta is delivered.
        let existing = json!({
            "issueId": "e7817290-1df9-4d9f-bc09-421af107063f",
            "taskKey": "issue:e7817290-1df9-4d9f-bc09-421af107063f",
            "wakeReason": "issue_reopened",
            "source": "assignment",
        });
        let incoming = json!({
            "issueId": "e7817290-1df9-4d9f-bc09-421af107063f",
            "taskId": "e7817290-1df9-4d9f-bc09-421af107063f",
            "source": "issue.comment",
            "wakeReason": "issue_commented",
            "commentId": "00000000-0000-0000-0000-0000000000c1",
            "wakeCommentId": "00000000-0000-0000-0000-0000000000c1",
            "wakeCommentIds": ["00000000-0000-0000-0000-0000000000c1"],
        });
        let merged = merge_wake_context(&existing, &incoming);
        assert_eq!(merged["wakeReason"], json!("issue_commented"));
        assert_eq!(merged["source"], json!("issue.comment"));
        assert_eq!(merged["taskKey"], existing["taskKey"]);
        assert_eq!(
            merged["commentId"],
            json!("00000000-0000-0000-0000-0000000000c1")
        );
    }

    #[test]
    fn later_comment_appends_to_the_batch_and_becomes_latest() {
        let existing = json!({
            "wakeCommentIds": ["00000000-0000-0000-0000-0000000000c1"],
            "commentId": "00000000-0000-0000-0000-0000000000c1",
        });
        let incoming = json!({
            "commentId": "00000000-0000-0000-0000-0000000000c2",
            "wakeCommentId": "00000000-0000-0000-0000-0000000000c2",
            "wakeCommentIds": ["00000000-0000-0000-0000-0000000000c2"],
        });
        let merged = merge_wake_context(&existing, &incoming);
        assert_eq!(
            merged["wakeCommentIds"],
            json!([
                "00000000-0000-0000-0000-0000000000c1",
                "00000000-0000-0000-0000-0000000000c2"
            ])
        );
        assert_eq!(
            merged["commentId"],
            json!("00000000-0000-0000-0000-0000000000c2")
        );
    }

    #[test]
    fn assignment_wake_does_not_clobber_an_existing_comment_wake() {
        // Reverse ordering: the comment wake landed first, then the
        // context-less assignment wake arrived. It must add nothing.
        let existing = json!({
            "wakeCommentIds": ["00000000-0000-0000-0000-0000000000c1"],
            "commentId": "00000000-0000-0000-0000-0000000000c1",
            "wakeReason": "issue_commented",
            "source": "issue.comment",
        });
        let incoming = json!({ "taskKey": "issue:abc" });
        let merged = merge_wake_context(&existing, &incoming);
        assert_eq!(merged["wakeReason"], json!("issue_commented"));
        assert_eq!(merged["source"], json!("issue.comment"));
        assert_eq!(
            merged["commentId"],
            json!("00000000-0000-0000-0000-0000000000c1")
        );
        assert_eq!(merged["taskKey"], json!("issue:abc"));
    }

    #[test]
    fn comment_shaped_reasons_require_a_comment_id() {
        assert!(is_comment_shaped_wake_reason(Some(WAKE_REASON_ISSUE_COMMENTED)));
        assert!(is_comment_shaped_wake_reason(Some(
            WAKE_REASON_ISSUE_REOPENED_VIA_COMMENT
        )));
        assert!(!is_comment_shaped_wake_reason(Some("issue_assigned")));
        assert!(!is_comment_shaped_wake_reason(None));
    }

    #[test]
    fn assignment_shaped_reasons_are_recognised() {
        assert!(is_assignment_shaped_wake_reason(Some("issue_assigned")));
        assert!(is_assignment_shaped_wake_reason(Some(
            "issue_reopened_via_comment"
        )));
        assert!(!is_assignment_shaped_wake_reason(Some("issue_commented")));
        assert!(!is_assignment_shaped_wake_reason(None));
    }

    #[tokio::test]
    async fn wake_payload_inlines_comments_in_wake_order() {
        // load_wake_comments issues SQL, so this exercises rendering order
        // through the pure renderer instead of a live database.
        let comments = vec![comment(1, "first"), comment(2, "second")];
        let payload = render_wake_payload_prompt(
            Some(WAKE_REASON_ISSUE_COMMENTED),
            Some("MAR-12"),
            "Create a new agent",
            &comments,
            2,
            0,
        );
        let first = payload.find("first").expect("first comment rendered");
        let second = payload.find("second").expect("second comment rendered");
        assert!(first < second);
    }
}
