//! Text utilities for safe Unicode string manipulation
//! 
//! This module provides utilities for safely truncating strings that may contain
//! multi-byte UTF-8 characters (e.g., Chinese, emoji, combining characters).

use unicode_segmentation::UnicodeSegmentation;

/// Safely truncate a string to the last N bytes, ensuring character boundaries.
/// 
/// This is a simple byte-based truncation that adjusts to the nearest character boundary.
/// Useful when you need to limit memory usage but don't care about exact character count.
/// 
/// # Example
/// ```
/// let text = "Hello 你好世界";
/// let truncated = truncate_suffix_bytes(text, 10);
/// // Returns "你好世界" or similar, never panics on character boundary
/// ```
pub fn truncate_suffix_bytes(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    
    let mut start = s.len() - max_bytes;
    // Move forward to the nearest character boundary
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    
    &s[start..]
}

/// Truncate a string to the last N Unicode scalar values (Rust chars).
/// 
/// This treats each Unicode scalar value as one unit. For most text this works well,
/// but note that some emoji and combining characters may still span multiple scalars.
/// 
/// # Example
/// ```
/// let text = "Hello world! 你好世界";
/// let truncated = truncate_suffix_chars(text, 5);
/// // Returns "好世界" (last 5 chars)
/// ```
pub fn truncate_suffix_chars(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    
    s.chars()
        .skip(char_count - max_chars)
        .collect()
}

/// Truncate a string to the last N grapheme clusters (user-perceived characters).
/// 
/// This is the most correct way to count "characters" as users perceive them.
/// It handles emoji, combining characters, and other complex Unicode correctly.
/// 
/// # Example
/// ```
/// let text = "Hello 👨‍👩‍👧‍👦 你好";
/// let truncated = truncate_suffix_graphemes(text, 3);
/// // Returns "👨‍👩‍👧‍👦 你好" (3 graphemes: emoji + space + 你 + 好)
/// ```
pub fn truncate_suffix_graphemes(s: &str, max_graphemes: usize) -> String {
    let graphemes: Vec<&str> = s.graphemes(true).collect();
    
    if graphemes.len() <= max_graphemes {
        return s.to_string();
    }
    
    graphemes[graphemes.len() - max_graphemes..]
        .concat()
}

/// Truncate text from the beginning (like Paperclip's approach) with a marker.
/// 
/// This truncates from the start and adds "[truncated]" marker, similar to
/// Paperclip's `truncateText` function.
/// 
/// # Example
/// ```
/// let text = "Very long text that needs truncation";
/// let truncated = truncate_with_marker(text, 20);
/// // Returns something like "Ve\n[truncated]"
/// ```
pub fn truncate_with_marker(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    let char_count = trimmed.chars().count();
    
    if char_count <= max_chars {
        return trimmed.to_string();
    }
    
    // Reserve space for the marker
    let take = max_chars.saturating_sub(20);
    let truncated: String = trimmed.chars().take(take).collect();
    
    format!("{}\n[truncated]", truncated.trim_end())
}

/// Truncate text from the beginning using grapheme clusters with a marker.
/// 
/// Most robust truncation that respects user-perceived characters.
pub fn truncate_graphemes_with_marker(s: &str, max_graphemes: usize) -> String {
    let trimmed = s.trim();
    let graphemes: Vec<&str> = trimmed.graphemes(true).collect();
    
    if graphemes.len() <= max_graphemes {
        return trimmed.to_string();
    }
    
    // Reserve space for the marker
    let take = max_graphemes.saturating_sub(20);
    let truncated = graphemes[..take].concat();
    
    format!("{}\n[truncated]", truncated.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_suffix_bytes_ascii() {
        let text = "Hello, world!";
        assert_eq!(truncate_suffix_bytes(text, 6), "world!");
        assert_eq!(truncate_suffix_bytes(text, 100), text);
    }

    #[test]
    fn test_truncate_suffix_bytes_chinese() {
        let text = "Hello 你好世界";
        // "你" = 3 bytes, "好" = 3 bytes, "世" = 3 bytes, "界" = 3 bytes
        let result = truncate_suffix_bytes(text, 10);
        // Should not panic and should end with valid UTF-8
        assert!(result.chars().all(|c| c != '\u{FFFD}')); // No replacement char
    }

    #[test]
    fn test_truncate_suffix_chars() {
        let text = "你好世界ABC";
        assert_eq!(truncate_suffix_chars(text, 3), "ABC");
        assert_eq!(truncate_suffix_chars(text, 5), "世界ABC");
        assert_eq!(truncate_suffix_chars(text, 100), text);
    }

    #[test]
    fn test_truncate_suffix_graphemes() {
        let text = "Hello 👨‍👩‍👧‍👦 世界";
        // text 包含: "Hello" (5个) + " " (1个) + "👨‍👩‍👧‍👦" (1个 grapheme) + " " (1个) + "世" (1个) + "界" (1个)
        // 总共约 10 个 graphemes
        let result = truncate_suffix_graphemes(text, 3);
        // 最后 3 个 graphemes 应该是: " " + "世" + "界"
        assert_eq!(result, " 世界");
        
        // 测试包含 emoji 的情况
        let result_with_emoji = truncate_suffix_graphemes(text, 4);
        // 最后 4 个: "👨‍👩‍👧‍👦" + " " + "世" + "界"
        assert!(result_with_emoji.contains("👨‍👩‍👧‍👦"));
    }

    #[test]
    fn test_truncate_with_marker() {
        let text = "This is a very long text that needs to be truncated";
        let result = truncate_with_marker(text, 20);
        assert!(result.contains("[truncated]"));
        assert!(result.len() <= 30); // Rough estimate
    }

    #[test]
    fn test_truncate_with_marker_short() {
        let text = "Short";
        let result = truncate_with_marker(text, 100);
        assert_eq!(result, "Short");
        assert!(!result.contains("[truncated]"));
    }

    #[test]
    fn test_chinese_character_boundary() {
        // This is the actual panic case from the issue
        let text = "制定技术栈与开发工具";
        // "制" is at bytes 0-2, if we try to split at byte 1, it would panic
        let result = truncate_suffix_bytes(text, 10);
        assert!(!result.is_empty());
        // Verify no invalid UTF-8
        assert!(result.chars().all(|c| c != '\u{FFFD}'));
    }
}
