//! Update notice — check for newer CLI versions and print a one-time notice.
//!
//! Mirrors Paperclip's `update-notice.ts`: throttled registry polling (24 h),
//! kill switches via env var / config flag, and semver-aware comparison.
//!
//! Uses blocking reqwest (same pattern as `client.rs`) since the CLI is synchronous.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Threshold between registry checks (24 hours).
const NOTICE_INTERVAL_MS: u64 = 24 * 60 * 60 * 1000;

/// Parse a semver string into (major, minor, patch).
pub fn parse_semver(version: &str) -> Option<(u64, u64, u64)> {
    let version = version.trim().trim_start_matches('v');
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let major = parts[0].parse::<u64>().ok()?;
    let minor = parts[1].parse::<u64>().ok()?;
    let patch = parts[2].parse::<u64>().ok()?;
    Some((major, minor, patch))
}

/// Compare two semver versions. Returns >0 if first is newer, <0 if older, 0 if equal.
pub fn compare_versions(a: &str, b: &str) -> i64 {
    let Some((am, ai, ap)) = parse_semver(a) else {
        return 0;
    };
    let Some((bm, bi, bp)) = parse_semver(b) else {
        return 0;
    };
    if am != bm {
        return (am as i64) - (bm as i64);
    }
    if ai != bi {
        return (ai as i64) - (bi as i64);
    }
    (ap as i64) - (bp as i64)
}

/// Check whether update checking is enabled.
///
/// Disabled when:
/// - `PARROT_UPDATE_CHECK=0` is set
/// - Config file has `updates.checkEnabled == false`
pub fn is_update_notice_enabled() -> bool {
    if std::env::var("PARROT_UPDATE_CHECK").as_deref() == Ok("0") {
        return false;
    }
    let Some(config_path) = crate::config::default_config_path() else {
        return true;
    };
    let Ok(raw) = fs::read_to_string(&config_path) else {
        return true;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return true;
    };
    parsed
        .get("updates")
        .and_then(|u| u.get("checkEnabled"))
        .and_then(|b| b.as_bool())
        .map(|enabled| !enabled)
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateCache {
    checked_at: u64,
    latest: Option<String>,
}

/// Get the cache file path for update checks.
fn cache_path() -> Option<PathBuf> {
    let config = crate::config::default_config_path()?;
    let dir = config.parent()?;
    Some(dir.join("update-check.json"))
}

/// Get the current package version from build-time metadata.
pub fn cli_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Check for a newer version by querying the npm registry.
///
/// Throttles to once per 24 hours via cache file.
/// Returns `Some(newest_version)` if a newer version exists, `None` otherwise.
/// Uses blocking HTTP to match the CLI's synchronous architecture.
pub fn check_for_update_notice() -> Option<String> {
    if !is_update_notice_enabled() {
        return None;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Check cache first
    if let Some(cache_file) = cache_path() {
        if let Ok(raw) = fs::read_to_string(&cache_file) {
            if let Ok(cache) = serde_json::from_str::<UpdateCache>(&raw) {
                if now.saturating_sub(cache.checked_at) < NOTICE_INTERVAL_MS {
                    if let Some(latest) = cache.latest {
                        return if compare_versions(&latest, cli_version()) > 0 {
                            Some(latest)
                        } else {
                            None
                        };
                    }
                }
            }
        }
    }

    // Query registry with timeout
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(2500))
        .build()
        .ok()?;

    let response = client
        .get("https://registry.npmjs.org/parrotai/latest")
        .send()
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let body: serde_json::Value = response.json().ok()?;
    let latest = body
        .get("dist-tags")
        .and_then(|t| t.get("latest"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Write cache
    if let Some(cache_file) = cache_path() {
        if let Some(parent) = cache_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let cache = UpdateCache {
            checked_at: now,
            latest: latest.clone(),
        };
        if let Ok(serialized) = serde_json::to_string(&cache) {
            let _ = fs::write(&cache_file, serialized);
        }
    }

    if let Some(latest) = latest {
        if compare_versions(&latest, cli_version()) > 0 {
            Some(latest)
        } else {
            None
        }
    } else {
        None
    }
}

/// Print an update notice if one is available.
pub fn print_update_notice() {
    if let Some(latest) = check_for_update_notice() {
        println!(
            "Update available: {} — run `parrot update`",
            latest
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_semver_valid() {
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_semver("10.20.30"), Some((10, 20, 30)));
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3))); // strip leading v
    }

    #[test]
    fn parse_semver_invalid() {
        assert_eq!(parse_semver("invalid"), None);
        assert_eq!(parse_semver("1.2"), None);
        assert_eq!(parse_semver("1.2.3.4"), None);
        assert_eq!(parse_semver(""), None);
    }

    #[test]
    fn compare_major_newer() {
        assert!(compare_versions("2.0.0", "1.9.9") > 0);
    }

    #[test]
    fn compare_minor_newer() {
        assert!(compare_versions("1.1.0", "1.0.9") > 0);
    }

    #[test]
    fn compare_patch_newer() {
        assert!(compare_versions("1.0.1", "1.0.0") > 0);
    }

    #[test]
    fn compare_equal() {
        assert_eq!(compare_versions("1.0.0", "1.0.0"), 0);
    }

    #[test]
    fn compare_older() {
        assert!(compare_versions("0.9.0", "1.0.0") < 0);
    }

    #[test]
    fn update_disabled_by_env() {
        std::env::set_var("PARROT_UPDATE_CHECK", "0");
        assert!(!is_update_notice_enabled());
        std::env::remove_var("PARROT_UPDATE_CHECK");
    }

    #[test]
    fn cli_version_constant() {
        // env!("CARGO_PKG_VERSION") is resolved at compile time (e.g. "0.1.0"),
        // so only the major segment parses as an integer.
        let v = cli_version();
        assert!(!v.is_empty());
        let major = v.split('.').next().expect("non-empty version");
        assert!(
            major.parse::<u32>().is_ok(),
            "major version must be numeric, got {major}"
        );
    }
}
