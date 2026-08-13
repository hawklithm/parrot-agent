//! Request correlation id middleware（P2.3 可观测性）。
//!
//! - 从 `X-Request-Id` 请求头透传；缺省时生成 UUID v4。
//! - 写入 request extensions（`RequestId`），供 handler 与日志消费。
//! - 回写响应头 `X-Request-Id`。
//! - 以 tracing span（`http_request.request_id`）包裹请求，
//!   handler 内所有结构化日志自动继承该字段，实现跨请求关联。

use axum::{
    body::Body,
    http::{HeaderName, HeaderValue, Request},
    middleware::Next,
    response::Response,
};
use tracing::Instrument;
use uuid::Uuid;

/// 请求头/响应头名称。
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// 注入到 request extensions 的请求 ID。
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// 生成请求 ID：优先透传外部 `X-Request-Id`，空值则生成 UUID v4。
pub fn generate_request_id(existing: Option<&str>) -> String {
    match existing {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => Uuid::new_v4().to_string(),
    }
}

/// Axum 中间件：注入/透传请求 ID 并关联结构化日志。
///
/// 用法（挂在 router 最外层）：
/// ```rust,ignore
/// router.layer(axum::middleware::from_fn(request_id_middleware));
/// ```
pub async fn request_id_middleware(mut request: Request<Body>, next: Next) -> Response {
    let incoming = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let request_id = generate_request_id(incoming.as_deref());
    request.extensions_mut().insert(RequestId(request_id.clone()));

    let span = tracing::info_span!("http_request", request_id = %request_id);
    let response = next.run(request).instrument(span).await;

    let mut response = response;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static(REQUEST_ID_HEADER), value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_uuid_when_header_missing() {
        let id = generate_request_id(None);
        assert!(!id.is_empty());
        assert!(Uuid::parse_str(&id).is_ok());
    }

    #[test]
    fn passthroughs_valid_header() {
        let id = generate_request_id(Some("req-123"));
        assert_eq!(id, "req-123");
    }

    #[test]
    fn ignores_blank_header() {
        let id = generate_request_id(Some("   "));
        assert!(Uuid::parse_str(&id).is_ok());
    }
}
