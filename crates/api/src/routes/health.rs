use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub message: String,
    pub deployment_mode: Option<String>,
    pub bootstrap_status: Option<String>,
}

/// GET /health - System health check with deployment info
pub async fn health_check() -> (StatusCode, Json<HealthResponse>) {
    let deployment_mode = std::env::var("DEPLOYMENT_MODE")
        .ok()
        .or_else(|| if cfg!(debug_assertions) { Some("local_trusted".to_string()) } else { None });
    
    // For local_trusted mode, bootstrap is not required
    // For authenticated mode, we'll check via a separate endpoint
    let bootstrap_status = if deployment_mode.as_deref() == Some("local_trusted") {
        None // local_trusted doesn't need bootstrap
    } else {
        Some("unknown".to_string()) // authenticated mode would need a DB query
    };
    
    let response = HealthResponse {
        status: "ok".to_string(),
        message: "Service is healthy".to_string(),
        deployment_mode,
        bootstrap_status,
    };
    
    (StatusCode::OK, Json(response))
}

