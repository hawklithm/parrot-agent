//! Trust Proxy middleware — resolves X-Forwarded-* headers when behind a reverse proxy.
//!
//! Paperclip uses a trust proxy to correctly identify the client's original scheme
//! and address when served through nginx/Tailscale/etc. Parrot mirrors this by
//! reading `X-Forwarded-Proto`, `X-Forwarded-Host` and `X-Forwarded-For` when
//! the `TRUST_PROXY` environment variable is set.

use axum::{extract::Request, middleware::Next, response::Response};

/// Represents the X-Forwarded-Proto value stored in request extensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardedProto(pub String);

/// Represents the X-Forwarded-Host value stored in request extensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardedHost(pub String);

/// Represents the X-Forwarded-For value stored in request extensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardedFor(pub String);

/// Trust proxy middleware. When `TRUST_PROXY` is set, copies X-Forwarded-*
/// headers into the request extensions so handlers can read the real client
/// address/scheme.
pub async fn trust_proxy_middleware(req: Request, next: Next) -> Response {
    if std::env::var("TRUST_PROXY").is_err() {
        return next.run(req).await;
    }

    // Read headers first before taking mutable borrow
    let proto = req.headers().get("X-Forwarded-Proto").and_then(|h| h.to_str().ok()).map(|s| s.to_string());
    let host = req.headers().get("X-Forwarded-Host").and_then(|h| h.to_str().ok()).map(|s| s.to_string());
    let forwarded_for = req.headers().get("X-Forwarded-For").and_then(|h| h.to_str().ok()).map(|s| s.to_string());

    let mut req = req;
    let extensions = req.extensions_mut();

    if let Some(s) = proto {
        extensions.insert(ForwardedProto(s));
    }
    if let Some(s) = host {
        extensions.insert(ForwardedHost(s));
    }
    if let Some(s) = forwarded_for {
        extensions.insert(ForwardedFor(s));
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body};
    use axum::http::{Request, Uri};
    use tower::ServiceExt;

    #[tokio::test]
    async fn skips_when_trust_proxy_not_set() {
        std::env::remove_var("TRUST_PROXY");
        let router = Router::new()
            .route("/test", axum::routing::get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(trust_proxy_middleware));
        let req = Request::builder()
            .uri(Uri::from_static("/test"))
            .header("X-Forwarded-Proto", "https")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn forwards_headers_when_trust_proxy_set() {
        std::env::set_var("TRUST_PROXY", "1");
        let router = Router::new()
            .route("/test", axum::routing::get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(trust_proxy_middleware));
        let req = Request::builder()
            .uri(Uri::from_static("/test"))
            .header("X-Forwarded-Proto", "https")
            .header("X-Forwarded-Host", "parrot.example.com")
            .header("X-Forwarded-For", "1.2.3.4")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        std::env::remove_var("TRUST_PROXY");
    }
}
