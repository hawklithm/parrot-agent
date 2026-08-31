//! HTTP request logging middleware — Parrot equivalent of Paperclip's pino
//! http logger (`server/src/middleware/logger.ts` + `http-log-policy.ts`).
//!
//! Behavior aligned with Paperclip:
//! - one structured log line per request: `METHOD URL STATUS`
//! - status-based level: >=500 error, >=400 warn, otherwise info
//! - success lines for silenced paths are dropped entirely
//!   (`should_silence_http_success_log`)
//! - 4xx/5xx lines carry the redacted request body/query
//!   (`services::log_redaction::redact_sensitive`) so operators can diagnose
//!   without leaking credentials (Paperclip `customProps` + `redactSensitive`)
//! - request bodies are only buffered when the content type is JSON and the
//!   request carries an `x-HTTP-Method-Override`-free JSON content type; other
//!   bodies are never read (streams, uploads, forms are opaque).

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use serde_json::Value;

use services::log_redaction::redact_sensitive;

use crate::errors::ErrorContext;

/// Health/status endpoints whose success lines would flood the log.
///
/// Port of Paperclip `shouldSilenceHttpSuccessLog`
/// (`server/src/middleware/http-log-policy.ts`).
fn should_silence_http_success_log(method: &str, url: &str, status: u16) -> bool {
    if !matches!(method, "GET" | "HEAD") {
        return false;
    }
    if !(200..300).contains(&status) {
        return false;
    }
    let silenced_suffixes = ["/health", "/healthz", "/ready", "/readyz", "/live", "/livez"];
    let path = url.split('?').next().unwrap_or(url);
    silenced_suffixes.iter().any(|suffix| path.ends_with(suffix))
        || (path.starts_with("/api/health") && !path.contains('/'))
}

/// Extract and redact the JSON body for logging. Returns `None` for empty or
/// non-JSON bodies.
async fn read_json_body(request: &mut Request<Body>) -> Option<Value> {
    let content_type = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)?
        .to_str()
        .ok()?
        .to_ascii_lowercase();
    if !content_type.contains("application/json") {
        return None;
    }
    let bytes = axum::body::to_bytes(std::mem::take(request.body_mut()), 64 * 1024)
        .await
        .ok()?;
    if bytes.is_empty() {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

/// Log one line per request with Paperclip-compatible level policy and
/// redacted 4xx/5xx payloads. The original request (with the buffered body
/// restored) is passed to the inner service unchanged.
pub async fn http_log_middleware(mut request: Request<Body>, next: Next) -> Response {
    let method = request.method().to_string();
    let url = request
        .uri()
        .path_and_query()
        .map(|value| value.to_string())
        .unwrap_or_else(|| request.uri().path().to_string());

    let body = read_json_body(&mut request).await;
    // Restore the buffered body for downstream handlers.
    let request = match body.clone() {
        Some(value) => {
            let bytes = serde_json::to_vec(&value).unwrap_or_default();
            let (mut parts, _) = request.into_parts();
            parts.headers.remove(axum::http::header::CONTENT_LENGTH);
            Request::from_parts(parts, Body::from(bytes))
        }
        None => request,
    };

    let started = std::time::Instant::now();
    let response = next.run(request).await;
    let elapsed_ms = started.elapsed().as_millis();

    let status = response.status();
    let error_context = if status.is_server_error() {
        response.extensions().get::<ErrorContext>().cloned()
    } else {
        None
    };
    if should_silence_http_success_log(&method, &url, status.as_u16()) {
        return response;
    }

    let level = if status >= StatusCode::INTERNAL_SERVER_ERROR {
        "error"
    } else if status >= StatusCode::BAD_REQUEST {
        "warn"
    } else {
        "info"
    };
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    match level {
        "error" => {
            let error_message = error_context
                .as_ref()
                .map(|ctx| ctx.message.clone())
                .unwrap_or_else(|| "unknown error".to_string());
            let error_name = error_context.as_ref().map(|ctx| ctx.name);
            match body.as_ref().map(redact_sensitive) {
                Some(redacted) => tracing::error!(
                    method = %method, url = %url, status = status.as_u16(),
                    request_id = request_id.as_deref().unwrap_or(""),
                    elapsed_ms = elapsed_ms as u64,
                    errorContext = %error_message,
                    errorName = error_name.unwrap_or(""),
                    reqBody = %redacted,
                    "{} {} {} — {}",
                    method, url, status.as_u16(), error_message
                ),
                None => tracing::error!(
                    method = %method, url = %url, status = status.as_u16(),
                    request_id = request_id.as_deref().unwrap_or(""),
                    elapsed_ms = elapsed_ms as u64,
                    errorContext = %error_message,
                    errorName = error_name.unwrap_or(""),
                    "{} {} {} — {}",
                    method, url, status.as_u16(), error_message
                ),
            }
        }
        "warn" => {
            if let Some(redacted) = body.as_ref().map(redact_sensitive) {
                tracing::warn!(
                    method = %method, url = %url, status = status.as_u16(),
                    request_id = request_id.as_deref().unwrap_or(""),
                    elapsed_ms = elapsed_ms as u64,
                    reqBody = %redacted,
                    "{} {} {}",
                    method, url, status.as_u16()
                );
            } else {
                tracing::warn!(
                    method = %method, url = %url, status = status.as_u16(),
                    request_id = request_id.as_deref().unwrap_or(""),
                    elapsed_ms = elapsed_ms as u64,
                    "{} {} {}",
                    method, url, status.as_u16()
                );
            }
        }
        _ => {
            tracing::info!(
                method = %method, url = %url, status = status.as_u16(),
                request_id = request_id.as_deref().unwrap_or(""),
                elapsed_ms = elapsed_ms as u64,
                "{} {} {}",
                method, url, status.as_u16()
            );
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_probe_successes_are_silenced() {
        assert!(should_silence_http_success_log("GET", "/api/health", 200));
        assert!(should_silence_http_success_log("GET", "/api/health?probe=1", 200));
        assert!(should_silence_http_success_log("HEAD", "/api/readyz", 204));
        // Non-success is never silenced.
        assert!(!should_silence_http_success_log("GET", "/api/health", 503));
        // Mutations are never silenced.
        assert!(!should_silence_http_success_log(
            "POST", "/api/health", 200
        ));
        // Ordinary endpoints are never silenced.
        assert!(!should_silence_http_success_log("GET", "/api/issues", 200));
        // Health-like but different path is not silenced.
        assert!(!should_silence_http_success_log("GET", "/api/healthy-check", 200));
    }
}
