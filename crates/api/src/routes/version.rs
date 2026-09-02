use axum::{http::StatusCode, Json};
use serde::Serialize;

/// API version follows Semantic Versioning (MAJOR.MINOR.PATCH).
/// Increment MAJOR on breaking changes, MINOR on new features, PATCH on fixes.
pub const API_VERSION: &str = "1.0.0";

/// Database schema version corresponds to the max migration file number.
/// Changes require a new migration and this value must be bumped.
pub const SCHEMA_VERSION: &str = option_env!("SCHEMA_VERSION").unwrap_or("79");

/// Git commit hash from build time.
pub const BUILD_COMMIT: &str = option_env!("BUILD_GIT_COMMIT").unwrap_or("unknown");

/// ISO 8601 build timestamp.
pub const BUILD_TIME: &str = option_env!("BUILD_TIME").unwrap_or("unknown");

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
        commit: BUILD_COMMIT.to_string(),
        build_time: BUILD_TIME.to_string(),
        api_version: API_VERSION.to_string(),
        schema_version: SCHEMA_VERSION.to_string(),
        deployment_mode,
    };

    (StatusCode::OK, Json(response))
}

/// Returns a Router with the /version route.
pub fn version_routes() -> axum::Router {
    axum::Router::new().route("/version", axum::routing::get(version_handler))
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
        assert!(SCHEMA_VERSION.parse::<u32>().is_ok(), "Schema version must be numeric");
    }

    #[tokio::test]
    async fn version_endpoint_returns_ok() {
        let router = super::version_routes();
        let app = axum::Router::new().merge(router);

        let client = reqwest::Client::new();
        // Use test client instead
        use axum::body::Body;
        use axum::http::Request;

        let req = Request::builder()
            .method("GET")
            .uri("/version")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["api_version"], API_VERSION);
        assert_eq!(json["schema_version"], SCHEMA_VERSION);
        assert!(!json["commit"].is_null());
        assert!(!json["build_time"].is_null());
    }
}
