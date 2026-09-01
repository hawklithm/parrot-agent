//! Trust Proxy middleware — resolves X-Forwarded-* headers when behind a reverse proxy.
//!
//! Paperclip uses a trust proxy to correctly identify the client's original scheme
//! and address when served through nginx/Tailscale/etc. Parrot mirrors this by
//! reading `X-Forwarded-Proto`, `X-Forwarded-Host` and `X-Forwarded-For` when
//! the `TRUST_PROXY` environment variable is set.

use axum::http::{HeaderName, HeaderValue};
use axum::{extract::Request, middleware::Next, response::Response};

/// Trust proxy middleware. When `TRUST_PROXY` is set, copies X-Forwarded-*
/// headers into the request extensions so handlers can read the real client
/// address/scheme.
pub async fn trust_proxy_middleware(req: Request, next: Next) -> Response {
    if std::env::var("TRUST_PROXY").is_err() {
        return next.run(req).await;
    }

    let mut req = req;
    let extensions = req.extensions_mut();

    // Forward scheme
    if let Some(proto) = req.headers().get("X-Forwarded-Proto") {
        extensions.insert(HeaderName::from_static("x-forwarded-proto").to_owned(), proto.clone());
    }
    // Forward host
    if let Some(host) = req.headers().get("X-Forwarded-Host") {
        extensions.insert(HeaderName::from_static("x-forwarded-host").to_owned(), host.clone());
    }
    // Forward for (client IP)
    if let Some(forwarded_for) = req.headers().get("X-Forwarded-For") {
        extensions.insert(
            HeaderName::from_static("x-forwarded-for").to_owned(),
            forwarded_for.clone(),
        );
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, Uri};

    #[tokio::test]
    async fn skips_when_trust_proxy_not_set {
        std::env::remove_var("TRUST_PROXY");
        let req = Request::builder()
            .uri(Uri::from_static("/"))
            .header("X-Forwarded-Proto", "https")
            .body(Body::empty())
            .unwrap();
        let next = Next::new(req.clone());
        let resp = trust_proxy_middleware(req, next).await;
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn forwards_headers_when_trust_proxy_set {
        std::env::set_var("TRUST_PROXY", "1");
        let req = Request::builder()
            .uri(Uri::from_static("/"))
            .header("X-Forwarded-Proto", "https")
            .header("X-Forwarded-Host", "parrot.example.com")
            .header("X-Forwarded-For", "1.2.3.4")
            .body(Body::empty())
            .unwrap();
        let next = Next::new(req.clone());
        let resp = trust_proxy_middleware(req, next).await;
        assert_eq!(resp.status(), 200);
        std::env::remove_var("TRUST_PROXY");
    }
}
