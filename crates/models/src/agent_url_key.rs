/// Agent URL key utilities (migrated from paperclip).
/// Source: paperclip/packages/shared/src/agent-url-key.ts
///
/// Paperclip persists no `url_key` column on `agents`: the key is derived from
/// the agent name on every read, falling back to the agent UUID when the name
/// retains no URL-safe characters. Parrot previously had no such projection at
/// all, plus a divergent `name-<id8>` helper in the heartbeat scheduler whose
/// output the route resolver could not resolve.

use regex::Regex;
use uuid::Uuid;

/// Normalize a name into a URL key: trim, lowercase, collapse every run of
/// non-alphanumerics into a single dash, then trim leading/trailing dashes.
pub fn normalize_agent_url_key(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let delim_re = Regex::new(r"[^a-z0-9]+").unwrap();
    let lowercase = trimmed.to_lowercase();
    let normalized = delim_re.replace_all(&lowercase, "-");

    let trim_re = Regex::new(r"^-+|-+$").unwrap();
    let result = trim_re.replace_all(&normalized, "").to_string();

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Derive an agent URL key from its name, falling back to the agent UUID and
/// finally to the literal `agent`.
///
/// Unlike `derive_project_url_key`, the fallback is not suffixed onto the name:
/// it is used only when the name yields nothing, which matches paperclip's
/// `deriveAgentUrlKey(name, fallback)` and keeps `urlKey` stable for the
/// ordinary case of a plain ASCII name.
pub fn derive_agent_url_key(name: Option<&str>, fallback: Option<Uuid>) -> String {
    if let Some(key) = name.and_then(normalize_agent_url_key) {
        return key;
    }
    if let Some(uuid) = fallback {
        return uuid.to_string();
    }
    "agent".to_string()
}

/// SQL fragment producing the same value as [`derive_agent_url_key`] for a row
/// aliased `alias` (which must expose `name` and `id`). Needed because two SQL
/// projections must filter/emit the key without loading the row first: the
/// `GET /agents/:reference` shortname resolver and the company-skill
/// `usedByAgents` projection.
///
/// Keep the pipeline in lockstep with the Rust derivation above: trim →
/// lowercase → collapse non-alphanumeric runs to `-` → trim dashes → fall back to
/// the raw UUID when nothing survives.
pub fn agent_url_key_sql(alias: &str) -> String {
    format!(
        "COALESCE(NULLIF(btrim(regexp_replace(lower(btrim({alias}.name)), '[^a-z0-9]+', '-', 'g'), '-'), ''), {alias}.id::text)",
        alias = alias
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_matches_paperclip() {
        assert_eq!(normalize_agent_url_key("My Cool Agent!"), Some("my-cool-agent".to_string()));
        assert_eq!(normalize_agent_url_key("  spaces  "), Some("spaces".to_string()));
        assert_eq!(normalize_agent_url_key("a__b"), Some("a-b".to_string()));
        assert_eq!(normalize_agent_url_key("---trim---"), Some("trim".to_string()));
        assert_eq!(normalize_agent_url_key(""), None);
        assert_eq!(normalize_agent_url_key("   "), None);
        assert_eq!(normalize_agent_url_key("你好"), None, "non-ASCII names strip to nothing");
    }

    #[test]
    fn derive_prefers_name_then_uuid() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();

        // The ordinary case: a stable name slug with no id suffix.
        assert_eq!(derive_agent_url_key(Some("Summarizer"), Some(id)), "summarizer");
        assert_eq!(derive_agent_url_key(Some("Chief of staff"), Some(id)), "chief-of-staff");

        // A name with no URL-safe characters falls back to the raw UUID, so the
        // existing UUID lookup path still resolves the agent.
        assert_eq!(derive_agent_url_key(Some("你好"), Some(id)), id.to_string());
        assert_eq!(derive_agent_url_key(Some("---"), Some(id)), id.to_string());

        assert_eq!(derive_agent_url_key(None, Some(id)), id.to_string());
        assert_eq!(derive_agent_url_key(None, None), "agent");
    }
}
