//! Rust port of Paperclip `packages/shared/src/document-anchors.ts`.
//!
//! Recomputes annotation anchor state when a document revision changes, so
//! open annotation threads track the current revision instead of going stale
//! silently. Mirrors `remapDocumentAnchor` + `projectMarkdownToText`.

use serde_json::Value;

const DEFAULT_CONTEXT_LENGTH: usize = 48;

#[derive(Debug, Clone)]
struct TextPosition {
    source_start: usize,
    source_end: usize,
}

#[derive(Debug, Clone)]
struct Projection {
    text: String,
    positions: Vec<TextPosition>,
}

fn normalize_anchor_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Project a markdown body to plain text with source offsets, stripping
/// structural markdown (fences, headings, lists, blockquotes, inline
/// emphasis) but keeping the literal copy text. Faithful to `projectMarkdownToText`.
fn project_markdown_to_text(markdown: &str) -> Projection {
    let mut text = String::new();
    let mut positions: Vec<TextPosition> = Vec::new();
    let mut pending_space: Option<TextPosition> = None;

    let lines: Vec<&str> = markdown.split('\n').collect();
    let mut offset: usize = 0;
    let mut in_fence = false;

    for (line_idx, raw_line) in lines.iter().enumerate() {
        if raw_line.is_empty() {
            // A blank line still advances the source offset.
            offset += raw_line.len() + 1; // +1 for the newline consumed by split
            if line_idx == lines.len() - 1 {
                break;
            }
            continue;
        }
        let fence_match = regex_syntax_fence(raw_line);
        if fence_match {
            in_fence = !in_fence;
            offset += raw_line.len() + 1;
            add_char(
                &mut text,
                &mut positions,
                &mut pending_space,
                ' ',
                offset - 1,
                offset,
            );
            continue;
        }
        if in_fence {
            add_inline_markdown_text(
                &mut text,
                &mut positions,
                &mut pending_space,
                raw_line,
                offset,
            );
            offset += raw_line.len() + 1;
            add_char(
                &mut text,
                &mut positions,
                &mut pending_space,
                ' ',
                offset - 1,
                offset,
            );
            continue;
        }
        let (stripped, source_offset) = strip_block_syntax(raw_line, offset);
        add_inline_markdown_text(
            &mut text,
            &mut positions,
            &mut pending_space,
            stripped,
            source_offset,
        );
        offset += raw_line.len() + 1;
        add_char(
            &mut text,
            &mut positions,
            &mut pending_space,
            ' ',
            offset - 1,
            offset,
        );
    }

    Projection { text, positions }
}

fn regex_syntax_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
        && trimmed.chars().take_while(|c| *c == '`' || *c == '~').count() >= 3
}

fn strip_block_syntax(line: &str, absolute_offset: usize) -> (&str, usize) {
    let block = line
        .trim_start_matches(' ')
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| i)
        .unwrap_or(0);
    let rest = &line[block..];
    let m = regex_block_leading(rest);
    if let Some(len) = m {
        let stripped = &rest[len..];
        (stripped, absolute_offset + block + len)
    } else {
        (line, absolute_offset)
    }
}

fn regex_block_leading(rest: &str) -> Option<usize> {
    // headings `#{1,6}\s+`
    let heading = rest.find(char::is_whitespace).map(|_| 0);
    if rest.starts_with('#') {
        let hashes = rest.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && rest[hashes..].starts_with(' ') {
            return Some(hashes + 1);
        }
    }
    if heading.is_some() {
        return None;
    }
    // list markers `[-+*]\s+` or `\d+[.)]\s+`
    if (rest.starts_with('-') || rest.starts_with('+') || rest.starts_with('*'))
        && rest.len() > 1
        && rest[1..].starts_with(' ')
    {
        return Some(2);
    }
    if let Some(dot) = rest.find(|c: char| c == '.' || c == ')') {
        if dot > 0 && rest[..dot].chars().all(|c| c.is_ascii_digit()) {
            return Some(dot + 1);
        }
    }
    // blockquote `>\s?`
    if rest.starts_with('>') {
        if rest.len() > 1 && rest[1..].starts_with(' ') {
            return Some(2);
        }
        return Some(1);
    }
    None
}

fn add_inline_markdown_text(
    text: &mut String,
    positions: &mut Vec<TextPosition>,
    pending_space: &mut Option<TextPosition>,
    s: &str,
    source_offset: usize,
) {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        // image ![alt](url)
        if s[i..].starts_with("![") {
            if let Some(close) = s[i..].find("](") {
                let after_paren = &s[i + close + 2..];
                if let Some(end) = after_paren.find(')') {
                    let alt = &s[i + 2..i + close];
                    let span_start = source_offset + i + 2;
                    add_text_span(text, positions, pending_space, alt, span_start);
                    i = i + close + 2 + end + 1;
                    continue;
                }
            }
        }
        // link [label](url)
        if s[i..].starts_with('[') {
            if let Some(close) = s[i..].find("](") {
                let after_paren = &s[i + close + 2..];
                if let Some(end) = after_paren.find(')') {
                    let label = &s[i + 1..i + close];
                    let span_start = source_offset + i + 1;
                    add_text_span(text, positions, pending_space, label, span_start);
                    i = i + close + 2 + end + 1;
                    continue;
                }
            }
        }
        let ch = s.as_bytes()[i] as char;
        if ch == '`' {
            let closing = s[i + 1..].find('`');
            if let Some(c) = closing {
                if c > 0 {
                    let inner = &s[i + 1..i + 1 + c];
                    let span_start = source_offset + i + 1;
                    add_text_span(text, positions, pending_space, inner, span_start);
                    i = i + 1 + c + 1;
                    continue;
                }
            }
        }
        if ch == '|' || ch == '\t' {
            add_char(text, positions, pending_space, ' ', source_offset + i, source_offset + i + 1);
            i += 1;
            continue;
        }
        if is_markdown_formatting(ch, s, i) {
            i += 1;
            continue;
        }
        add_char(text, positions, pending_space, ch, source_offset + i, source_offset + i + 1);
        i += 1;
    }
    let _ = bytes;
}

#[allow(clippy::too_many_arguments)]
fn add_text_span(
    text: &mut String,
    positions: &mut Vec<TextPosition>,
    pending_space: &mut Option<TextPosition>,
    s: &str,
    source_offset: usize,
) {
    for (i, ch) in s.char_indices() {
        add_char(
            text,
            positions,
            pending_space,
            ch,
            source_offset + i,
            source_offset + i + 1,
        );
    }
}

fn is_markdown_formatting(ch: char, _s: &str, _i: usize) -> bool {
    ch == '*' || ch == '_' || ch == '~' || (ch == '\\')
}

fn add_char(
    text: &mut String,
    positions: &mut Vec<TextPosition>,
    pending_space: &mut Option<TextPosition>,
    ch: char,
    source_start: usize,
    source_end: usize,
) {
    if ch.is_whitespace() {
        if !text.is_empty() && pending_space.is_none() {
            pending_space.replace(TextPosition {
                source_start,
                source_end,
            });
        }
        return;
    }
    if let Some(space) = pending_space.take() {
        if !text.is_empty() {
            text.push(' ');
            positions.push(space);
        }
    }
    text.push(ch);
    positions.push(TextPosition {
        source_start,
        source_end,
    });
}

#[derive(Debug, Clone)]
pub struct AnchorSnapshot {
    selected_text: String,
    prefix_text: String,
    suffix_text: String,
    normalized_start: i32,
    normalized_end: i32,
    markdown_start: i32,
    markdown_end: i32,
}

impl AnchorSnapshot {
    fn from_thread(thread: &Value) -> AnchorSnapshot {
        AnchorSnapshot {
            selected_text: thread
                .get("selectedText")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            prefix_text: thread
                .get("prefixText")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            suffix_text: thread
                .get("suffixText")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            normalized_start: thread
                .get("normalizedStart")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            normalized_end: thread
                .get("normalizedEnd")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            markdown_start: thread
                .get("markdownStart")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            markdown_end: thread
                .get("markdownEnd")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnchorState {
    Active,
    Stale,
    Orphaned,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Confidence {
    Exact,
    Duplicate,
    Ambiguous,
    Fuzzy,
    Missing,
}

#[derive(Debug, Clone)]
pub struct RemapResult {
    anchor_state: AnchorState,
    confidence: Confidence,
    anchor: Option<AnchorSnapshot>,
    reason: &'static str,
}
#[derive(Debug, Clone)]
struct Candidate {
    start: usize,
    end: usize,
    score: f64,
    reason: &'static str,
}

fn find_occurrences(text: &str, quote: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    if quote.is_empty() {
        return starts;
    }
    let mut from = 0;
    while let Some(idx) = text[from..].find(quote) {
        let absolute = from + idx;
        starts.push(absolute);
        from = absolute + quote.len();
    }
    starts
}

fn score_candidate(
    projection: &Projection,
    start: usize,
    end: usize,
    previous: &AnchorSnapshot,
    reason: &'static str,
    context_length: usize,
) -> Candidate {
    let before = &projection.text[..start].chars().rev().take(context_length).collect::<String>();
    let before: String = before.chars().rev().collect();
    let after = projection.text[end..]
        .chars()
        .take(context_length)
        .collect::<String>();
    let prefix_score = suffix_overlap_score(&previous.prefix_text, &before);
    let suffix_score = prefix_overlap_score(&previous.suffix_text, &after);
    let distance = (start as i64 - previous.normalized_start as i64).unsigned_abs() as f64;
    let proximity = 1.0 / (1.0 + distance / 200.0);
    Candidate {
        start,
        end,
        score: prefix_score * 0.35 + suffix_score * 0.35 + proximity * 0.3,
        reason,
    }
}

fn find_fuzzy_candidate(
    projection: &Projection,
    previous: &AnchorSnapshot,
    context_length: usize,
) -> Option<Candidate> {
    let normalized = normalize_anchor_text(&previous.selected_text);
    let words: Vec<&str> = normalized
        .split(' ')
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return None;
    }
    let text_words: Vec<(String, usize, usize)> = projection
        .text
        .split_whitespace()
        .enumerate()
        .map(|(i, w)| {
            // Approximate source span from first char of the word.
            let start = projection
                .text
                .char_indices()
                .filter(|(ci, _)| {
                    projection.text[..*ci].split_whitespace().count() == i
                })
                .map(|(ci, _)| ci)
                .next()
                .unwrap_or(0);
            let end = start + w.len();
            (w.to_string(), start, end)
        })
        .collect();
    let window_sizes: Vec<usize> = [words.len() as i64 - 1, words.len() as i64, words.len() as i64 + 1, words.len() as i64 + 2]
        .into_iter()
        .filter(|n| *n > 0)
        .map(|n| n as usize)
        .collect();
    let mut best: Option<Candidate> = None;
    for size in window_sizes {
        if size > text_words.len() {
            continue;
        }
        for index in 0..=(text_words.len() - size) {
            let window = &text_words[index..index + size];
            let candidate_text = window
                .iter()
                .map(|w| w.0.clone())
                .collect::<Vec<_>>()
                .join(" ");
            let similarity = similarity_score(&normalize_anchor_text(&previous.selected_text), &candidate_text);
            if similarity < 0.45 {
                continue;
            }
            let mut scored = score_candidate(
                projection,
                window[0].1,
                window[window.len() - 1].2,
                previous,
                "fuzzy",
                context_length,
            );
            scored.score = scored.score * 0.35 + similarity * 0.65;
            if best.as_ref().map(|b| scored.score > b.score).unwrap_or(true) {
                best = Some(scored);
            }
        }
    }
    best
}

fn build_anchor_snapshot(projection: &Projection, normalized_start: usize, normalized_end: usize, context_length: usize) -> AnchorSnapshot {
    let range = resolve_projection_range(projection, normalized_start, normalized_end);
    let Some(range) = range else {
        return AnchorSnapshot {
            selected_text: String::new(),
            prefix_text: String::new(),
            suffix_text: String::new(),
            normalized_start: normalized_start as i32,
            normalized_end: normalized_end as i32,
            markdown_start: 0,
            markdown_end: 0,
        };
    };
    let prefix = projection
        .text
        .chars()
        .take(range.normalized_start)
        .collect::<String>()
        .chars()
        .rev()
        .take(context_length)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let suffix = projection
        .text
        .chars()
        .skip(range.normalized_end)
        .take(context_length)
        .collect::<String>();
    AnchorSnapshot {
        selected_text: range.text,
        prefix_text: prefix,
        suffix_text: suffix,
        normalized_start: range.normalized_start as i32,
        normalized_end: range.normalized_end as i32,
        markdown_start: range.markdown_start,
        markdown_end: range.markdown_end,
    }
}

struct ProjectionRange {
    text: String,
    normalized_start: usize,
    normalized_end: usize,
    markdown_start: i32,
    markdown_end: i32,
}

fn resolve_projection_range(projection: &Projection, normalized_start: usize, normalized_end: usize) -> Option<ProjectionRange> {
    if normalized_start >= normalized_end || normalized_end > projection.text.len() {
        return None;
    }
    if normalized_start >= projection.positions.len()
        || normalized_end.saturating_sub(1) >= projection.positions.len()
    {
        return None;
    }
    Some(ProjectionRange {
        text: projection.text[normalized_start..normalized_end].to_string(),
        normalized_start,
        normalized_end,
        markdown_start: projection.positions[normalized_start].source_start as i32,
        markdown_end: projection.positions[normalized_end - 1].source_end as i32,
    })
}

fn prefix_overlap_score(expected_prefix: &str, actual_prefix: &str) -> f64 {
    let expected = normalize_anchor_text(expected_prefix);
    let actual = normalize_anchor_text(actual_prefix);
    if expected.is_empty() {
        return 0.5;
    }
    for size in (1..=expected.len().min(actual.len())).rev() {
        if expected[..size] == actual[..size] {
            return size as f64 / expected.len() as f64;
        }
    }
    0.0
}

fn suffix_overlap_score(expected_suffix: &str, actual_suffix: &str) -> f64 {
    let expected = normalize_anchor_text(expected_suffix);
    let actual = normalize_anchor_text(actual_suffix);
    if expected.is_empty() {
        return 0.5;
    }
    for size in (1..=expected.len().min(actual.len())).rev() {
        if expected[expected.len() - size..] == actual[actual.len() - size..] {
            return size as f64 / expected.len() as f64;
        }
    }
    0.0
}

fn similarity_score(left: &str, right: &str) -> f64 {
    if left == right {
        return 1.0;
    }
    let left_words: std::collections::HashSet<String> =
        normalize_anchor_text(left).to_lowercase().split(' ').filter(|w| !w.is_empty()).map(|w| w.to_string()).collect();
    let right_words: std::collections::HashSet<String> =
        normalize_anchor_text(right).to_lowercase().split(' ').filter(|w| !w.is_empty()).map(|w| w.to_string()).collect();
    let intersection = left_words.intersection(&right_words).count();
    let union = left_words.union(&right_words).count().max(1);
    let jaccard = intersection as f64 / union as f64;
    let length_ratio = (left.len().min(right.len())) as f64 / (left.len().max(right.len()).max(1)) as f64;
    jaccard * 0.75 + length_ratio * 0.25
}

/// Rust port of `remapDocumentAnchor`. Given the previous anchor snapshot and
/// the next revision's markdown body, recompute where the quote now lands.
pub fn remap_document_anchor(previous: &AnchorSnapshot, next_markdown: &str) -> RemapResult {
    let projection = project_markdown_to_text(next_markdown);
    let context_length = DEFAULT_CONTEXT_LENGTH;
    let quote = normalize_anchor_text(&previous.selected_text);
    if quote.is_empty() {
        return RemapResult {
            anchor_state: AnchorState::Orphaned,
            confidence: Confidence::Missing,
            anchor: None,
            reason: "missing",
        };
    }

    let exact_starts = find_occurrences(&projection.text, &quote);
    if !exact_starts.is_empty() {
        let mut candidates: Vec<Candidate> = exact_starts
            .iter()
            .map(|&start| score_candidate(&projection, start, start + quote.len(), previous, "exact", context_length))
            .collect();
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        let best = candidates[0].clone();
        if candidates.len() > 1 {
            let second = candidates[1].clone();
            if (best.score - second.score).abs() < 0.05 {
                return RemapResult {
                    anchor_state: AnchorState::Stale,
                    confidence: Confidence::Ambiguous,
                    anchor: Some(build_anchor_snapshot(&projection, best.start, best.end, context_length)),
                    reason: "ambiguous",
                };
            }
        }
        let confidence = if candidates.len() == 1 {
            Confidence::Exact
        } else {
            Confidence::Duplicate
        };
        return RemapResult {
            anchor_state: AnchorState::Active,
            confidence,
            anchor: Some(build_anchor_snapshot(&projection, best.start, best.end, context_length)),
            reason: if candidates.len() == 1 { "exact" } else { "duplicate" },
        };
    }

    if let Some(fuzzy) = find_fuzzy_candidate(&projection, previous, context_length) {
        if fuzzy.score >= 0.58 {
            return RemapResult {
                anchor_state: AnchorState::Stale,
                confidence: Confidence::Fuzzy,
                anchor: Some(build_anchor_snapshot(&projection, fuzzy.start, fuzzy.end, context_length)),
                reason: "fuzzy",
            };
        }
    }

    RemapResult {
        anchor_state: AnchorState::Orphaned,
        confidence: Confidence::Missing,
        anchor: None,
        reason: "missing",
    }
}

/// Serialize a remap result into the JSON columns updated on the thread row.
pub fn remap_result_to_patch(result: &RemapResult) -> Value {
    let state = match result.anchor_state {
        AnchorState::Active => "active",
        AnchorState::Stale => "stale",
        AnchorState::Orphaned => "orphaned",
    };
    let confidence = match result.confidence {
        Confidence::Exact => "exact",
        Confidence::Duplicate => "duplicate",
        Confidence::Ambiguous => "ambiguous",
        Confidence::Fuzzy => "fuzzy",
        Confidence::Missing => "missing",
    };
    let mut patch = serde_json::json!({
        "anchorState": state,
        "anchorConfidence": confidence,
        "anchorSelector": {},
    });
    if let Some(anchor) = &result.anchor {
        patch["selectedText"] = Value::String(anchor.selected_text.clone());
        patch["prefixText"] = Value::String(anchor.prefix_text.clone());
        patch["suffixText"] = Value::String(anchor.suffix_text.clone());
        patch["normalizedStart"] = Value::from(anchor.normalized_start);
        patch["normalizedEnd"] = Value::from(anchor.normalized_end);
        patch["markdownStart"] = Value::from(anchor.markdown_start);
        patch["markdownEnd"] = Value::from(anchor.markdown_end);
        patch["anchorSelector"] = serde_json::json!({
            "quote": {
                "exact": anchor.selected_text,
                "prefix": anchor.prefix_text,
                "suffix": anchor.suffix_text,
            },
            "position": {
                "normalizedStart": anchor.normalized_start,
                "normalizedEnd": anchor.normalized_end,
                "markdownStart": anchor.markdown_start,
                "markdownEnd": anchor.markdown_end,
            },
        });
    }
    patch
}

/// Convenience: remap a thread JSON (as stored) against the next markdown body.
pub fn remap_thread(thread: &Value, next_markdown: &str) -> RemapResult {
    let previous = AnchorSnapshot::from_thread(thread);
    remap_document_anchor(&previous, next_markdown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread_with(selected: &str, prefix: &str, suffix: &str) -> Value {
        serde_json::json!({
            "selectedText": selected,
            "prefixText": prefix,
            "suffixText": suffix,
            "normalizedStart": 0,
            "normalizedEnd": selected.len() as i64,
            "markdownStart": 0,
            "markdownEnd": selected.len() as i64,
        })
    }

    #[test]
    fn exact_remap_stays_active() {
        let thread = thread_with("the quick brown fox", "before ", " after");
        let result = remap_thread(&thread, "before the quick brown fox after");
        assert_eq!(result.anchor_state, AnchorState::Active);
        assert_eq!(result.confidence, Confidence::Exact);
    }

    #[test]
    fn missing_quote_becomes_orphaned() {
        let thread = thread_with("the quick brown fox", "", "");
        let result = remap_thread(&thread, "completely different paragraph here");
        assert_eq!(result.anchor_state, AnchorState::Orphaned);
        assert_eq!(result.confidence, Confidence::Missing);
    }

    #[test]
    fn duplicate_quote_is_ambiguous_stale() {
        let thread = thread_with("repeat", "x ", " y");
        let result = remap_thread(&thread, "x repeat y and also x repeat y again");
        assert_eq!(result.anchor_state, AnchorState::Stale);
        assert_eq!(result.confidence, Confidence::Ambiguous);
    }
}
