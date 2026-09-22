//! Paperclip parity: normalize axum's request-body rejections to `400`.
//!
//! Paperclip parses bodies with `express.json()` and validates them with a Zod
//! `validate()` middleware (`server/src/middleware/validate.ts`). A missing,
//! malformed, or schema-invalid body therefore always surfaces as `400`
//! (`ZodError` handled by `server/src/middleware/error-handler.ts`), which
//! responds `{ error: "Validation error", details: [...] }`. Express never
//! emits `415` or `422` for a body problem.
//!
//! Axum's `Json<T>` extractor instead rejects with two statuses express would
//! never produce:
//! - `415 Unsupported Media Type` (`MissingJsonContentType`) for a missing or
//!   wrong content type.
//! - `422 Unprocessable Entity` (`JsonDataError`) for a well-formed body that
//!   does not match the target type, including unknown fields.
//!
//! Both rejections happen *before* the handler body runs — including before the
//! handler's own authorization check — so the divergence is observable to
//! clients. A `422` is especially misleading: it reads as "semantically invalid
//! content" when the real cause is a shape mismatch.
//!
//! The rewrite is scoped by content type so it cannot mask a deliberate status.
//! Axum's extractor rejections are `text/plain`; every `AppError` in this crate
//! responds `application/json` (`errors.rs::IntoResponse`). So a `422` that is
//! genuinely JSON — the shape handlers use for their own semantic validation —
//! passes through untouched.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};

/// Axum middleware: rewrite extractor-level `415`/`422` rejections to `400`.
///
/// ```rust,ignore
/// router.layer(axum::middleware::from_fn(normalize_body_rejection_status));
/// ```
pub async fn normalize_body_rejection_status(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;

    let status = response.status();
    if status != StatusCode::UNSUPPORTED_MEDIA_TYPE && status != StatusCode::UNPROCESSABLE_ENTITY {
        return response;
    }

    // A JSON body means the handler ran and chose this status deliberately.
    let is_json = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.starts_with("application/json"))
        .unwrap_or(false);

    if !is_json {
        *response.status_mut() = StatusCode::BAD_REQUEST;
    }

    response
}
