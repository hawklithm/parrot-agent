use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use crate::auth::{AuthorizationActor, ActorSource, AuthError, AuthResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    LocalTrusted,
    Authenticated,
}

#[async_trait]
pub trait ActorResolver: Send + Sync {
    async fn resolve(&self, request: &Request) -> AuthResult<Option<AuthorizationActor>>;
    fn priority(&self) -> u8;
}

pub struct LocalTrustedResolver {
    default_user_id: uuid::Uuid,
    default_company_id: uuid::Uuid,
}

impl LocalTrustedResolver {
    pub fn new(default_user_id: uuid::Uuid, default_company_id: uuid::Uuid) -> Self {
        Self {
            default_user_id,
            default_company_id,
        }
    }
}

#[async_trait]
impl ActorResolver for LocalTrustedResolver {
    async fn resolve(&self, _request: &Request) -> AuthResult<Option<AuthorizationActor>> {
        Ok(Some(AuthorizationActor::Board {
            user_id: self.default_user_id,
            company_id: self.default_company_id,
            source: ActorSource::LocalImplicit,
            memberships: vec![],
            is_instance_admin: false,
        }))
    }

    fn priority(&self) -> u8 {
        0
    }
}

pub struct BearerTokenResolver<R> {
    board_key_repo: Arc<R>,
    agent_key_repo: Arc<R>,
    jwt_config: Arc<crate::auth::jwt::JwtConfig>,
}

impl<R> BearerTokenResolver<R> {
    pub fn new(
        board_key_repo: Arc<R>,
        agent_key_repo: Arc<R>,
        jwt_config: Arc<crate::auth::jwt::JwtConfig>,
    ) -> Self {
        Self {
            board_key_repo,
            agent_key_repo,
            jwt_config,
        }
    }

    fn extract_bearer_token(&self, request: &Request) -> Option<String> {
        request
            .headers()
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string())
    }
}

#[async_trait]
impl<R> ActorResolver for BearerTokenResolver<R>
where
    R: Send + Sync,
{
    async fn resolve(&self, request: &Request) -> AuthResult<Option<AuthorizationActor>> {
        let token = match self.extract_bearer_token(request) {
            Some(t) => t,
            None => return Ok(None),
        };

        if token.starts_with("bak_") {
            // Board API Key
            return Ok(None); // TODO: implement BoardAuthResolver
        } else if token.starts_with("aak_") {
            // Agent API Key
            return Ok(None); // TODO: implement AgentKeyResolver
        } else {
            // JWT token
            if let Some(claims) = crate::auth::jwt::verify_local_agent_jwt(&self.jwtfig, &token) {
                return Ok(Some(AuthorizationActor::Agent {
                    agent_id: claims.sub,
                    company_id: claims.company_id,
                    run_id: claims.run_id,
                    source: ActorSource::AgentJwt,
                }));
            }
        }

        Ok(None)
    }

    fn priority(&self) -> u8 {
        10
    }
}

pub struct SessionCookieResolver {
    // TODO: integrate with BetterAuth session
}

impl SessionCookieResolver {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for SessionCookieResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ActorResolver for SessionCookieResolver {
    async fn resolve(&self, _request: &Request) -> AuthResult<Option<AuthorizationActor>> {
        // TODO: implement session parsing
        Ok(None)
    }

    fn priority(&self) -> u8 {
        5
    }
}

pub struct CloudTenantHeaderResolver {
    // TODO: integrate with cloud tenant upsert logic
}

impl CloudTenantHeaderResolver {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for CloudTenantHeaderResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ActorResolver for CloudTenantHeaderResolver {
    async fn resolve(&self, request: &Request) -> AuthResult<Option<AuthorizationActor>> {
        let stack_id = request.headers().get("x-paperclip-cloud-stack-id");
        let stack_role = request.headers().get("x-paperclip-cloud-stack-role");

        if stack_id.is_some() && stack_role.is_some() {
            // TODO: implement tenant upsert and actor construction
            return Ok(None);
        }

        Ok(None)
    }

    fn priority(&self) -> u8 {
        3
    }
}

pub struct AuthMiddleware {
    mode: AuthMode,
    resolvers: Vec<Arc<dyn ActorResolver>>,
}

impl AuthMiddleware {
    pub fn new(mode: AuthMode) -> Self {
        Self {
            mode,
            resolvers: vec![],
        }
    }

    pub fn with_resolver(mut self, resolver: Arc<dyn ActorResolver>) -> Self {
        self.resolvers.push(resolver);
        self.resolvers.sort_by_key(|r| std::cmp::Reverse(r.priority()));
        self
    }

    pub async fn resolve_actor(&self, request: &Request) -> AuthResult<AuthorizationActor> {
        match self.mode {
            AuthMode::LocalTrusted => {
                for resolver in &self.resolvers {
                    if let Some(actor) = resolver.resolve(request).await? {
                        return Ok(actor);
                    }
                }
                Err(AuthError::Unauthorized("No actor resolver succeeded".to_string()))
            }
            AuthMode::Authenticated => {
                for resolver in &self.resolvers {
                    if let Some(actor) = resolver.resolve(request).await? {
                        return Ok(actor);
                    }
                }
                Ok(AuthorizationActor::None)
            }
        }
    }
}

pub async fn auth_middleware_fn(
    State(middleware): State<Arc<AuthMiddleware>>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let actor = middleware.resolve_actor(&request).await?;
    request.extensions_mut().insert(actor);
    Ok(next.run(request).await)
}

pub fn extract_actor(request: &Request) -> AuthResult<&AuthorizationActor> {
    request
        .extensions()
        .get::<AuthorizationActor>()
        .ok_or_else(|| AuthError::Internal("Actor not found in request extensions".to_string()))
}
