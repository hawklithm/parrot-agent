/// Project URL key utilities (migrated from paperclip)
/// Source: paperclip/packages/shared/src/project-url-key.ts

use regex::Regex;
use uuid::Uuid;
pub fn normalize_project_url_key(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Replace non-alphanumeric with dashes
    let delim_re = Regex::new(r"[^a-z0-9]+").unwrap();
    let lowercase = trimmed.to_lowercase();  // Store in variable to extend lifetime
    let normalized = delim_re.replace_all(&lowercase, "-");
    
    // Trim leading/trailing dashes
    let trim_re = Regex::new(r"^-+|-+$").unwrap();
    let result = trim_re.replace_all(&normalized, "").to_string();
    
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Check if string contains non-ASCII characters
pub fn has_non_ascii_content(value: &str) -> bool {
    value.chars().any(|c| !c.is_ascii())
}

/// Extract first 8 hex chars from a UUID
fn short_id_from_uuid(value: &str) -> Option<String> {
    if let Ok(uuid) = Uuid::parse_str(value.trim()) {
        let hex = uuid.as_simple().to_string();
        Some(hex[..8].to_lowercase())
    } else {
        None
    }
}

/// Derive URL key from project name and optional UUID fallback
/// Examples:
///   ("My Project", None) -> "my-project"
///   ("你好", Some(uuid)) -> "12abcd34" (first 8 chars of UUID)
///   ("Test 你好", Some(uuid)) -> "test-12abcd34"
pub fn derive_project_url_key(name: Option<&str>, fallback_uuid: Option<Uuid>) -> String {
    let base = name.and_then(normalize_project_url_key);
    let has_non_ascii = name.map(has_non_ascii_content).unwrap_or(false);
    
    // If base is clean ASCII, use it directly
    if let Some(ref base_key) = base {
        if !has_non_ascii {
            return base_key.clone();
        }
    }
    
    // Non-ASCII was stripped, append short UUID for uniqueness
    let short_id = fallback_uuid
        .map(|uuid| uuid.to_string())
        .as_deref()
        .and_then(short_id_from_uuid);
    
    match (base, short_id) {
        (Some(base_key), Some(short)) => format!("{}-{}", base_key, short),
        (None, Some(short)) => short,
        (Some(base_key), None) => base_key,
        (None, None) => "project".to_string(),
    }
}

/// Check if a string looks like a UUID
pub fn is_uuid_like(value: &str) -> bool {
    Uuid::parse_str(value.trim()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_project_url_key() {
        assert_eq!(normalize_project_url_key("My Project"), Some("my-project".to_string()));
        assert_eq!(normalize_project_url_key("Test@123"), Some("test-123".to_string()));
        assert_eq!(normalize_project_url_key("  spaces  "), Some("spaces".to_string()));
        assert_eq!(normalize_project_url_key("---trim---"), Some("trim".to_string()));
        assert_eq!(normalize_project_url_key(""), None);
        assert_eq!(normalize_project_url_key("   "), None);
    }

    #[test]
    fn test_has_non_ascii() {
        assert!(!has_non_ascii_content("hello"));
        assert!(has_non_ascii_content("你好"));
        assert!(has_non_ascii_content("test你好"));
    }

    #[test]
    fn test_derive_project_url_key() {
        let uuid = Uuid::parse_str("12345678-1234-1234-1234-123456789abc").unwrap();
        
        // ASCII name
        assert_eq!(derive_project_url_key(Some("My Project"), None), "my-project");
        
        // Non-ASCII with UUID
        assert_eq!(derive_project_url_key(Some("你好"), Some(uuid)), "12345678");
        
        // Mixed ASCII + non-ASCII with UUID
        let result = derive_project_url_key(Some("Test 你好"), Some(uuid));
        assert_eq!(result, "test-12345678");
        
        // No name, with UUID
        assert_eq!(derive_project_url_key(None, Some(uuid)), "12345678");
        
        // No name, no UUID
        assert_eq!(derive_project_url_key(None, None), "project");
    }

    #[test]
    fn test_is_uuid_like() {
        assert!(is_uuid_like("12345678-1234-1234-1234-123456789abc"));
        assert!(is_uuid_like("  12345678-1234-1234-1234-123456789abc  "));
        assert!(!is_uuid_like("test"));
        assert!(!is_uuid_like("not-a-uuid"));
    }
}
