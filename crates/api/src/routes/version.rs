use axum::{http::StatusCode, Json};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct VersionResponse {
    pub status: String,
    pub version: String,
    pub commit: String,
    pub build_time: String,
    pub deployment_mode: Option<String>,
}

/// GET /version - Return build/version metadata
pub async fn version_handler() -> (StatusCode, Json<VersionResponse>) {
    let deployment_mode = std::env::var("DEPLOYMENT_MODE").ok();

    let response = VersionResponse {
        status: "ok".to_string(),
        version: std::env::var("PARROT_VERSION").unwrap_or_else(|_| "0.0.0".to_string()),
        commit: std::env::var("PARROT_BUILD_COMMIT").unwrap_or_else(|_| "unknown".to_string()),
        build_time: std::env::var("PARROT_BUILD_TIME").unwrap_or_else(|_| "unknown".to_string()),
        deployment_mode,
    };

    (StatusCode::OK, Json(response))
}

/// Returns a Router with the /version route.
pub fn version_routes() -> axum::Router {
    axum::Router::new().route("/version", axum::routing::get(version_handler))
}
