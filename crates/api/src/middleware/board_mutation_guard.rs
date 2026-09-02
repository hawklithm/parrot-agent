//! Board Mutation Guard — rejects board-sourced mutations from untrusted origins.
//!
//! Paperclip's `board-mutation-guard` middleware (express) blocks POST/PUT/PATCH/DELETE
//! from `actor.type === "board"` unless the request carries a trusted Origin or
//! Referer header matching the server's own host(s). This prevents CSRF-style
//! mutation from malicious third-party pages when board session cookies are
//! present.
//!
//! The guard is skipped (passes through) for:
//! - Safe HTTP methods (GET / HEAD / OPTIONS).
//! - Non-board actors (agent keys, system actors, etc.).
//! - Board sources that are not browser-session-based — these are machine-to-machine
//!   requests where Origin is absent by design:
//!   * `local_implicit` — single-user local mode
//!   * `board_key` — board bearer key authentication
//!   * `cloud_tenant` — multi-tenant SaaS caller
//!
//! When enabled (TRUST_PROXY must also be set so X-Forwarded-* are resolved),
//! trusted origins are derived from Host / X-Forwarded-Host and the explicit
//! PARROT_PUBLIC_URL env var (mirrors PAPERCLIP_PUBLIC_URL in Paperclip).
//!
//! Parrot-specific notes:
//! - Uses `ActorSource` from `services::auth` for source discrimination.
//! - Reads `AuthorizationActor` from `req.extensions()`.
//! - `TRUST_PROXY` env var gates Origin/Referer resolution (same as `trust_proxy`).
//!
//! See paperclip reference: `server/src/middleware/board-mutation-guard.ts`.

use axum::http::{HeaderName, HeaderValue};
use axum::{extract::Request, middleware::Next, response::Response};

use services::auth::{ActorSource, AuthorizationActor};

const SAFE_METHODS: &[&str] = &["GET", "HEAD", "OPTIONS"];

fn parse_origin(value: Option<&HeaderValue>) -> Option<String> {
    let raw = value?.to_str().ok()?;
    let url = url::Url::parse(raw).ok()?;
    Some(format!("{}://{}", url.scheme(), url.host()?))
}

fn trusted_origins(req: &Request) -> Vec<String> {
    let mut origins: Vec<String> = Vec::new();

    // Always trust direct-host requests
    if let Some(host) = req.headers().get(axum::http::header::HOST) {
        if let Ok(host_str) = host.to_str() {
            origins.push(format!("http://{host_str}"));
            origins.push(format!("https://{host_str}"));
        }
    }

    // When behind a reverse proxy, also trust the forwarded host.
    // Only applies when TRUST_PROXY is set (handled by trust_proxy middleware).
    if std::env::var("TRUST_PROXY").is_ok() {
        if let Some(forwarded_host) = req.headers().get("x-forwarded-host") {
            if let Ok(fh) = forwarded_host.to_str() {
                // X-Forwarded-Host may contain a comma-separated list; take the first.
                let first = fh.split(',').next().map(|s| s.trim()).filter(|s| !s.is_empty());
                if let Some(host) = first {
                    origins.push(format!("http://{host}"));
                    origins.push(format!("https://{host}"));
                }
            }
        }
    }

    // Explicit public URL overrides / additions (e.g. behind TLS-terminating proxy).
    if let Ok(public_url) = std::env::var("PARROT_PUBLIC_URL") {
        if let Some(url) = url::Url::parse(public_url.trim()).ok() {
            origins.push(format!("{}://{}", url.scheme(), url.host().unwrap_or(&String::new())));
        }
    }

    origins
}

fn is_trusted_origin(req: &Request, allowed: &Vec<String>) -> bool {
    let origin = parse_origin(req.headers().get(axum::http::header::ORIGIN));
    if let Some(o) = origin {
        if allowed.contains(&o) {
            return true;
        }
    }
    let referer = parse_origin(req.headers().get(axum::http::header::REFERER));
    if let Some(r) = referer {
        if allowed.contains(&r) {
            return true;
        }
    }
    false
}

pub async fn board_mutation_guard(req: Request, next: Next) -> Response {
    let method = req.method().to_string().to_uppercase();
    if SAFE_METHODS.iter().any(|m| m == &method) {
        return next.run(req).await;
    }

    // Must have an AuthorizationActor attached (auth middleware runs first).
    let actor = match req.extensions().get::<AuthorizationActor>() {
        Some(a) => a,
        None => return next.run(req).await,
    };

    // Only board-sourced actors are subject to this guard.
    let source = match actor {
        AuthorizationActor::Board { source, .. } => *source,
        _ => return next.run(req).await,
    };

    // Machine-to-machine sources don't carry Origin/Referer — always allow.
    if matches!(
        source,
        ActorSource::LocalImplicit | ActorSource::BoardKey | ActorSource::CloudTenant
    ) {
        return next.run(req).await;
    }

    // Session-based board actors: enforce trusted origin.
    let allowed = trusted_origins(&req);
    if is_trusted_origin(&req, &allowed) {
        return next.run(req).await;
    }

    tracing::warn!(
        actor_type = "board",
        actor_source = ?source,
        "board mutation blocked: untrusted origin",
    );
    axum::http::Response::builder()
        .status(axum::http::StatusCode::FORBIDDEN)
        .body(axum::body::Body::from(
            serde_json::json!({ "error": "Board mutation requires trusted browser origin" }).to_string(),
        ))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, Uri};
    use uuid::Uuid;

    fn make_request(headers: &[(&str, &str)]) -> Request {
        let mut req = Request::builder().uri(Uri::from_static("/api/issues")).method("POST");
        for (k, v) in headers {
            req = req.header(k, v);
        }
        req.body(Body::empty()).unwrap()
    }

    fn board_actor(source: ActorSource) -> AuthorizationActor {
        AuthorizationActor::Board {
            user_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            source,
            memberships: vec![],
            is_instance_admin: false,
        }
    }

    #[tokio::test]
    async fn safe_methods_pass() {
        let req = make_request(&[]);
        let mut req = req
            .extensions_mut()
            .insert(board_actor(ActorSource::Session));
        let next = Next::new(req.clone());
        let resp = board_mutation_guard(req, next).await;
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn non_board_actor_passes() {
        let req = make_request(&[]);
        let agent_actor = AuthorizationActor::Agent {
            agent_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            run_id: None,
            source: ActorSource::AgentKey,
            key_id: None,
            key_scope: None,
            responsible_user_id: None,
            on_behalf_of_user_id: None,
            on_behalf_of_memberships: vec![],
        };
        let mut req = req.extensions_mut().insert(agent_actor);
        let next = Next::new(req.clone());
        let resp = board_mutation_guard(req, next).await;
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn missing_actor_passes() {
        let req = make_request(&[]);
        let next = Next::new(req.clone());
        let resp = board_mutation_guard(req, next).await;
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn local_implicit_passes_without_origin() {
        let req = make_request(&[]);
        let mut req = req
            .extensions_mut()
            .insert(board_actor(ActorSource::LocalImplicit));
        let next = Next::new(req.clone());
        let resp = board_mutation_guard(req, next).await;
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn board_key_passes_without_origin() {
        let req = make_request(&[]);
        let mut req = req
            .extensions_mut()
            .insert(board_actor(ActorSource::BoardKey));
        let next = Next::new(req.clone());
        let resp = board_mutation_guard(req, next).await;
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn cloud_tenant_passes_without_origin() {
        let req = make_request(&[]);
        let mut req = req
            .extensions_mut()
            .insert(board_actor(ActorSource::CloudTenant));
        let next = Next::new(req.clone());
        let resp = board_mutation_guard(req, next).await;
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn session_board_blocked_without_origin() {
        let req = make_request(&[]);
        let mut req = req
            .extensions_mut()
            .insert(board_actor(ActorSource::Session));
        let next = Next::new(req.clone());
        let resp = board_mutation_guard(req, next).await;
        assert_eq!(resp.status(), 403);
    }

    #[tokio::test]
    async fn session_board_allowed_with_matching_origin() {
        std::env::set_var("PARROT_PUBLIC_URL", "http://localhost:3100");
        let req = make_request(&[
            ("Host", "localhost:3100"),
            ("Origin", "http://localhost:3100"),
        ]);
        let mut req = req
            .extensions_mut()
            .insert(board_actor(ActorSource::Session));
        let next = Next::new(req.clone());
        let resp = board_mutation_guard(req, next).await;
        assert_eq!(resp.status(), 200);
        std::env::remove_var("PARROT_PUBLIC_URL");
    }

    #[tokio::test]
    async fn session_board_allowed_with_trusted_referer() {
        let req = make_request(&[
            ("Host", "parrot.example.com"),
            ("Referer", "https://parrot.example.com/issues"),
        ]);
        let mut req = req
            .extensions_mut()
            .insert(board_actor(ActorSource::Session));
        let next = Next::new(req.clone());
        let resp = board_mutation_guard(req, next).await;
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn session_board_blocked_with_untrusted_origin() {
        let req = make_request(&[
            ("Host", "parrot.example.com"),
            ("Origin", "https://evil.example.com"),
        ]);
        let mut req = req
            .extensions_mut()
            .insert(board_actor(ActorSource::Session));
        let next = Next::new(req.clone());
        let resp = board_mutation_guard(req, next).await;
        assert_eq!(resp.status(), 403);
    }
}
