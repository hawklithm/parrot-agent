//! Port of Paperclip `server/src/middleware/private-json-etag.ts`.
//!
//! Adds `ETag` + `Cache-Control: private, must-revalidate` to successful
//! `application/json` GET responses, and short-circuits `304 Not Modified`
//! when the client's `If-None-Match` matches the computed tag.
use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, Response, StatusCode};
use axum::middleware::Next;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

fn compute_etag(serialized: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    let digest = hasher.finalize();
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    format!("\"{encoded}\"")
}

fn matches_etag(header: Option<&HeaderValue>, etag: &str) -> bool {
    let Some(header) = header else {
        return false;
    };
    let Ok(value) = header.to_str() else {
        return false;
    };
    value.split(',').any(|candidate| {
        let normalized = candidate.trim();
        normalized == "*" || normalized == etag || normalized == format!("W/{etag}")
    })
}

fn is_json_content_type(value: Option<&HeaderValue>) -> bool {
    value
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("application/json"))
        .unwrap_or(false)
}

/// Tower/axum middleware equivalent of Paperclip's `privateJsonEtag()`.
pub async fn private_json_etag_middleware(request: Request, next: Next) -> Response<Body> {
    // Only GET responses are eligible for ETag caching.
    if request.method() != axum::http::Method::GET {
        return next.run(request).await;
    }

    let if_none_match = request
        .headers()
        .get(header::IF_NONE_MATCH)
        .map(|v| v.clone());

    let response = next.run(request).await;

    let (parts, body) = response.into_parts();

    // Only transform successful JSON responses.
    if parts.status.is_success() && is_json_content_type(parts.headers.get(header::CONTENT_TYPE)) {
        // Collect the body bytes so we can hash and (optionally) short-circuit.
        let bytes = match axum::body::to_bytes(body, usize::MAX).await {
            Ok(b) => b,
            Err(_) => {
                // If we cannot read the body, fall back to passing through unchanged.
                return Response::from_parts(parts, Body::empty());
            }
        };

        let serialized = String::from_utf8_lossy(&bytes);
        let etag = compute_etag(&serialized);

        let mut builder = Response::builder()
            .status(parts.status)
            .header(header::ETAG, etag.clone())
            .header(header::CACHE_CONTROL, "private, must-revalidate");
        for (name, value) in parts.headers.iter() {
            builder = builder.header(name, value);
        }

        if matches_etag(if_none_match.as_ref(), &etag) {
            return builder
                .status(StatusCode::NOT_MODIFIED)
                .body(Body::empty())
                .unwrap();
        }

        return builder.body(Body::from(bytes)).unwrap();
    }

    Response::from_parts(parts, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    async fn json_handler() -> Response<Body> {
        let mut resp = Response::new(Body::from(r#"{"id":"abc","value":42}"#));
        resp.headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
        resp
    }

    fn app() -> Router {
        Router::new()
            .route("/data", get(json_handler))
            .layer(axum::middleware::from_fn(private_json_etag_middleware))
    }

    #[tokio::test]
    async fn sets_etag_and_cache_control_on_json_get() {
        let resp = app()
            .oneshot(Request::get("/data").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().contains_key(header::ETAG));
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, must-revalidate"
        );
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.as_ref(), br#"{"id":"abc","value":42}"#);
    }

    #[tokio::test]
    async fn returns_304_when_if_none_match_matches() {
        // First request to learn the ETag.
        let first = app()
            .oneshot(Request::get("/data").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let etag = first.headers().get(header::ETAG).unwrap().clone();

        // Second request with matching If-None-Match.
        let req = Request::get("/data")
            .header(header::IF_NONE_MATCH, etag)
            .body(Body::empty())
            .unwrap();
        let resp = app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn non_json_responses_are_untouched() {
        async fn text_handler() -> Response<Body> {
            let mut resp = Response::new(Body::from("plain"));
            resp.headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
            resp
        }
        let router = Router::new()
            .route("/text", get(text_handler))
            .layer(axum::middleware::from_fn(private_json_etag_middleware));
        let resp = router
            .oneshot(Request::get("/text").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(!resp.headers().contains_key(header::ETAG));
    }
}
