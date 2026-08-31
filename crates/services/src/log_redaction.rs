//! HTTP log payload redaction — faithful Rust port of Paperclip
//! `server/src/middleware/redact-sensitive.ts`.
//!
//! `customProps` in Paperclip's pino logger copies `req.body` / `req.params` /
//! `req.query` verbatim into 4xx/5xx log lines so operators can diagnose. That
//! means auth sign-in bodies (plaintext passwords) and similar payloads (API
//! keys, OAuth codes, setup-token login fields) would end up on disk. This
//! walker returns a copy of the input with values for sensitive keys replaced
//! with the literal string `[REDACTED]`, recursing into nested objects and
//! arrays, with a depth cap so hostile or accidental cycles cannot pin the
//! logger.
//!
//! Sensitive keys and URL-ish keys match Paperclip exactly (lowercased). URLs
//! classified as URL-ish keep origin and path only: username, password, query,
//! and fragment are stripped (Paperclip SR-5 backstop).

/// Literal marker written for redacted values.
pub const REDACTED: &str = "[REDACTED]";

/// Maximum recursion depth; beyond this values are dropped entirely.
const MAX_DEPTH: usize = 6;

/// Keys whose values are replaced with `[REDACTED]` (lowercased).
const SENSITIVE_KEYS: &[&str] = &[
    "password",
    "currentpassword",
    "newpassword",
    "passwordconfirmation",
    "password_confirmation",
    "passwordconfirm",
    "password_confirm",
    "confirmpassword",
    "confirm_password",
    "secret",
    "client_secret",
    "clientsecret",
    "access_token",
    "accesstoken",
    "refresh_token",
    "refreshtoken",
    "id_token",
    "idtoken",
    "api_key",
    "apikey",
    "authorization",
    "auth_token",
    "authtoken",
    "session_token",
    "sessiontoken",
    "private_key",
    "privatekey",
    // The Claude setup-token login fields. `browserCode` carries the one-time
    // sign-in code and `authorization_code` carries the OAuth code; neither
    // may reach a log line.
    "browsercode",
    "authorization_code",
    "authorizationcode",
];

/// String-bearing keys whose URL credentials/query/fragment are stripped.
const URLISH_KEYS: &[&str] = &[
    "href",
    "locator",
    "source",
    "source_locator",
    "sourcelocator",
    "source_url",
    "sourceurl",
    "uri",
    "url",
    // The Claude setup-token login URL: a structured `loginUrl` reaching a log
    // sink keeps origin and path only (SR-5 backstop).
    "loginurl",
    "login_url",
];

fn is_sensitive_key(key: &str) -> bool {
    SENSITIVE_KEYS.contains(&key.to_lowercase().as_str())
}

fn is_urlish_key(key: &str) -> bool {
    URLISH_KEYS.contains(&key.to_lowercase().as_str())
}

/// Strip username, password, query, and fragment from a URL string, keeping
/// origin and path. Non-parsing strings are returned unchanged.
fn strip_secret_bearing_url_parts(value: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(value) else {
        return value.to_string();
    };
    if url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
    {
        return value.to_string();
    }
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

/// Redact a JSON value for logging. Objects: sensitive keys become
/// `[REDACTED]`, URL-ish string values get credentials/query/fragment
/// stripped, everything else recurses. Arrays recurse element-wise. Depth is
/// capped: values deeper than [`MAX_DEPTH`] are dropped (`Null`).
pub fn redact_sensitive(value: &serde_json::Value) -> serde_json::Value {
    redact_inner(value, 0)
}

fn redact_inner(value: &serde_json::Value, depth: usize) -> serde_json::Value {
    if depth > MAX_DEPTH {
        return serde_json::Value::Null;
    }
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, entry) in map {
                if is_sensitive_key(key) {
                    out.insert(key.clone(), serde_json::Value::String(REDACTED.into()));
                    continue;
                }
                if let Some(text) = entry.as_str() {
                    if is_urlish_key(key) {
                        out.insert(
                            key.clone(),
                            serde_json::Value::String(strip_secret_bearing_url_parts(text)),
                        );
                        continue;
                    }
                }
                out.insert(key.clone(), redact_inner(entry, depth + 1));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items.iter().map(|item| redact_inner(item, depth + 1)).collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_sensitive_keys_at_every_depth() {
        let body = json!({
            "password": "hunter2",
            "nested": {
                "api_key": "sk-123",
                "deeper": [{"secret": "s3cret"}]
            },
            "user": "john",
            "keep": "visible"
        });
        let out = redact_sensitive(&body);
        assert_eq!(out["password"], REDACTED);
        assert_eq!(out["nested"]["api_key"], REDACTED);
        assert_eq!(out["nested"]["deeper"][0]["secret"], REDACTED);
        assert_eq!(out["user"], "john");
        assert_eq!(out["keep"], "visible");
    }

    #[test]
    fn key_matching_is_case_insensitive() {
        let body = json!({"Password": "x", "API_KEY": "y", "Authorization": "Bearer z"});
        let out = redact_sensitive(&body);
        assert_eq!(out["Password"], REDACTED);
        assert_eq!(out["API_KEY"], REDACTED);
        assert_eq!(out["Authorization"], REDACTED);
    }

    #[test]
    fn claude_setup_token_fields_are_redacted() {
        let body = json!({
            "browserCode": "one-time-code",
            "authorization_code": "oauth-code",
            "loginUrl": "https://claude.ai/oauth/authorize?code=abc&state=xyz#frag",
        });
        let out = redact_sensitive(&body);
        assert_eq!(out["browserCode"], REDACTED);
        assert_eq!(out["authorization_code"], REDACTED);
        // loginUrl is URL-ish: origin+path kept, query+fragment stripped.
        assert_eq!(out["loginUrl"], "https://claude.ai/oauth/authorize");
    }

    #[test]
    fn urlish_keys_strip_credentials_query_and_fragment() {
        let body = json!({
            "url": "https://user:pass@example.com/path?token=abc#frag",
            "source_url": "postgres://admin:pw@db.example.test:5432/app",
            "href": "https://example.com/plain",
        });
        let out = redact_sensitive(&body);
        assert_eq!(out["url"], "https://example.com/path");
        assert_eq!(out["source_url"], "postgres://db.example.test:5432/app");
        // Nothing to strip: unchanged.
        assert_eq!(out["href"], "https://example.com/plain");
    }

    #[test]
    fn depth_cap_drops_deep_values() {
        // Build a value nested deeper than MAX_DEPTH.
        let mut value = json!("bottom");
        for _ in 0..(MAX_DEPTH + 2) {
            value = json!({ "child": value });
        }
        let out = redact_sensitive(&value);
        // Walk the redacted output; at some depth the value must be Null.
        let mut current = &out;
        let mut dropped = false;
        for _ in 0..(MAX_DEPTH + 3) {
            match current.get("child") {
                Some(next) => current = next,
                None => {
                    dropped = true;
                    break;
                }
            }
            if current.is_null() {
                dropped = true;
                break;
            }
        }
        assert!(dropped, "deeply nested value must be dropped");
    }

    #[test]
    fn scalars_pass_through_untouched() {
        assert_eq!(redact_sensitive(&json!(42)), json!(42));
        assert_eq!(redact_sensitive(&json!("text")), json!("text"));
        assert_eq!(redact_sensitive(&json!(null)), json!(null));
        assert_eq!(redact_sensitive(&json!(true)), json!(true));
    }
}
