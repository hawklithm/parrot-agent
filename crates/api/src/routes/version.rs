use axum::{http::StatusCode, Json};
use serde::Serialize;

/// API version follows Semantic Versioning (MAJOR.MINOR.PATCH).
/// Increment MAJOR on breaking changes, MINOR on new features, PATCH on fixes.
pub const API_VERSION: &str = "1.0.0";

/// Database schema version corresponds to the max migration file number.
/// Changes require a new migration and this value must be bumped.
pub fn schema_version() -> &'static str {
    option_env!("SCHEMA_VERSION").unwrap_or("79")
}

/// Git commit hash from build time.
pub fn build_commit() -> &'static str {
    option_env!("BUILD_GIT_COMMIT").unwrap_or("unknown")
}

/// ISO 8601 build timestamp.
pub fn build_time() -> &'static str {
    option_env!("BUILD_TIME").unwrap_or("unknown")
}

#[derive(Debug, Serialize)]
pub struct VersionResponse {
    pub status: String,
    pub version: String,
    pub commit: String,
    pub build_time: String,
    pub api_version: String,
    pub schema_version: String,
    pub deployment_mode: Option<String>,
}

/// GET /version - Return build/version metadata
pub async fn version_handler() -> (StatusCode, Json<VersionResponse>) {
    let deployment_mode = std::env::var("DEPLOYMENT_MODE").ok();
    let version =
        std::env::var("PARROT_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());

    let response = VersionResponse {
        status: "ok".to_string(),
        version,
        commit: build_commit().to_string(),
        build_time: build_time().to_string(),
        schema_version: schema_version().to_string(),
        api_version: API_VERSION.to_string(),
        deployment_mode,
    };

    (StatusCode::OK, Json(response))
}

/// Returns a Router with the /version route.
pub fn version_routes() -> crate::app_state::Router<crate::app_state::AppState> {
    crate::app_state::Router::new().route("/version", axum::routing::get(version_handler))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_version_follows_semver() {
        let parts: Vec<&str> = API_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "API version must be MAJOR.MINOR.PATCH");
        parts.iter().for_each(|p| {
            assert!(p.parse::<u32>().is_ok(), "Version parts must be numeric");
        });
    }

    #[test]
    fn schema_version_is_numeric() {
        assert!(schema_version().parse::<u32>().is_ok(), "Schema version must be numeric");
    }

    #[tokio::test]
    async fn version_handler_returns_ok() {
        let (status, json) = version_handler().await;
        assert_eq!(status, StatusCode::OK);
        let body = json.0;
        assert_eq!(body.status, "ok");
        assert!(!body.api_version.is_empty());
        assert!(!body.schema_version.is_empty());
        assert!(!body.commit.is_empty());
        assert!(!body.build_time.is_empty());
    }
}
