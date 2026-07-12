use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use uuid::Uuid;

use sqlx::PgPool;

use repositories::auth_repositories::{
    AuthSessionRepository, AuthUserRepository, CompanyRepository, PgAuthSessionRepository,
    PgAuthUserRepository, PgCompanyRepository, PgCompanyMembershipRepository,
};
use repositories::board_api_key_repository::{
    BoardApiKeyRepository, PgBoardApiKeyRepository, hash_api_key,
};
use repositories::agent_api_key_repository::{
    AgentApiKeyRepository as _, PgAgentApiKeyRepository as PgAgentKeyRepo,
};

use crate::auth::{
    ActorSource, AuthError, AuthResult, AuthorizationActor, JwtConfig, resolve_board_access,
};

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

/// 本地隐式认证（单用户/开发模式）：始终返回默认 Board 用户。
pub struct LocalTrustedResolver {
    default_user_id: Uuid,
    default_company_id: Uuid,
}

impl LocalTrustedResolver {
    pub fn new(default_user_id: Uuid, default_company_id: Uuid) -> Self {
        Self {
            default_user_id,
            default_company_id,
        }
    }
}

#[async_trait]
impl ActorResolver for LocalTrustedResolver {
    async fn resolve(&self, _request: &Request) -> AuthResult<Option<AuthorizationActor>> {
        Ok(Some(AuthorizationActor::board_with_source(
            self.default_user_id,
            self.default_company_id,
            ActorSource::LocalImplicit,
            vec![],
            false,
        )))
    }

    fn priority(&self) -> u8 {
        0
    }
}

/// Bearer Token 分派：Board API Key (bak_) / Agent API Key (aak_) / Agent JWT。
pub struct BearerTokenResolver {
    pool: Arc<PgPool>,
    jwt_config: Arc<JwtConfig>,
}

impl BearerTokenResolver {
    pub fn new(pool: Arc<PgPool>, jwt_config: Arc<JwtConfig>) -> Self {
        Self { pool, jwt_config }
    }

    fn extract_bearer_token(&self, request: &Request) -> Option<String> {
        request
            .headers()
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string())
    }

    async fn resolve_board_key(
        &self,
        token: &str,
    ) -> AuthResult<Option<AuthorizationActor>> {
        let key_hash = hash_api_key(token);
        let repo = PgBoardApiKeyRepository::new((*self.pool).clone());
        let key = repo.find_by_key_hash(&key_hash).await.map_err(|e| {
            AuthError::Internal { message: format!("Board key lookup failed: {}", e) }
        })?;

        let key = match key {
            Some(k) if !k.is_revoked => k,
            _ => return Ok(None),
        };

        // 记录使用（不阻塞主流程）
        let _ = repo.record_usage(key.id).await;

        let (_, memberships, is_instance_admin) =
            resolve_board_access(&self.pool, key.user_id).await?;

        // Board API Key 本身无 company_id，从用户首个活跃成员关系派生
        let company_id = memberships
            .iter()
            .find(|m| m.status.is_active())
            .map(|m| m.company_id)
            .unwrap_or_else(Uuid::nil);

        Ok(Some(AuthorizationActor::board_with_source(
            key.user_id,
            company_id,
            ActorSource::BoardKey,
            memberships,
            is_instance_admin,
        )))
    }

    async fn resolve_agent_key(
        &self,
        token: &str,
    ) -> AuthResult<Option<AuthorizationActor>> {
        let key_hash = hash_api_key(token);
        let repo = PgAgentKeyRepo::new((*self.pool).clone());
        let key = repo.find_by_key_hash(&key_hash).await.map_err(|e| {
            AuthError::Internal { message: format!("Agent key lookup failed: {}", e) }
        })?;

        let key = match key {
            Some(k) if !k.is_revoked => k,
            _ => return Ok(None),
        };

        let _ = repo.update_last_used(key.id).await;

        Ok(Some(AuthorizationActor::agent_with_source(
            key.agent_id,
            key.company_id,
            None,
            ActorSource::AgentKey,
        )))
    }

    fn resolve_jwt(&self, token: &str) -> AuthResult<Option<AuthorizationActor>> {
        let claims = match verify_local_agent_jwt(&self.jwt_config, token) {
            Some(c) => c,
            None => return Ok(None),
        };

        let agent_id = Uuid::parse_str(&claims.sub)
            .map_err(|_| AuthError::InvalidToken { reason: "invalid agent id in JWT".to_string() })?;
        let company_id = Uuid::parse_str(&claims.company_id)
            .map_err(|_| AuthError::InvalidToken { reason: "invalid company id in JWT".to_string() })?;
        let run_id = match claims.run_id {
            Some(ref s) => Some(
                Uuid::parse_str(s)
                    .map_err(|_| AuthError::InvalidToken { reason: "invalid run id in JWT".to_string() })?,
            ),
            None => None,
        };

        Ok(Some(AuthorizationActor::agent_with_source(
            agent_id,
            company_id,
            run_id,
            ActorSource::AgentJwt,
        )))
    }
}

#[async_trait]
impl ActorResolver for BearerTokenResolver {
    async fn resolve(&self, request: &Request) -> AuthResult<Option<AuthorizationActor>> {
        let token = match self.extract_bearer_token(request) {
            Some(t) => t,
            None => return Ok(None),
        };

        if token.starts_with("bak_") {
            self.resolve_board_key(&token).await
        } else if token.starts_with("aak_") {
            self.resolve_agent_key(&token).await
        } else {
            self.resolve_jwt(&token)
        }
    }

    fn priority(&self) -> u8 {
        10
    }
}

/// Session Cookie 认证：解析 BetterAuth 会话 token 并加载用户身份。
pub struct SessionCookieResolver {
    pool: Arc<PgPool>,
}

impl SessionCookieResolver {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ActorResolver for SessionCookieResolver {
    async fn resolve(&self, request: &Request) -> AuthResult<Option<AuthorizationActor>> {
        let session_token = match request
            .headers()
            .get("cookie")
            .and_then(|h| h.to_str().ok())
            .and_then(|c| extract_cookie(c, "session"))
        {
            Some(t) => t,
            None => return Ok(None),
        };

        let session_repo = PgAuthSessionRepository::new((*self.pool).clone());
        let session = session_repo.find_by_token(&session_token).await.map_err(|e| {
            AuthError::Internal { message: format!("Session lookup failed: {}", e) }
        })?;
        let session = match session {
            Some(s) => s,
            None => return Ok(None),
        };

        let (_, memberships, is_instance_admin) =
            resolve_board_access(&self.pool, session.user_id).await?;

        // 会话来源的 company_id 取用户首个活跃成员关系所属公司
        let company_id = memberships
            .iter()
            .find(|m| m.status.is_active())
            .map(|m| m.company_id)
            .unwrap_or_else(Uuid::nil);

        Ok(Some(AuthorizationActor::board_with_source(
            session.user_id,
            company_id,
            ActorSource::Session,
            memberships,
            is_instance_admin,
        )))
    }

    fn priority(&self) -> u8 {
        5
    }
}

/// Cloud Tenant Header 认证：根据 X-Paperclip-Cloud-* 头派生/upsert 用户与公司。
pub struct CloudTenantHeaderResolver {
    pool: Arc<PgPool>,
}

impl CloudTenantHeaderResolver {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ActorResolver for CloudTenantHeaderResolver {
    async fn resolve(&self, request: &Request) -> AuthResult<Option<AuthorizationActor>> {
        let stack_id = request
            .headers()
            .get("x-paperclip-cloud-stack-id")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());
        let stack_role = request
            .headers()
            .get("x-paperclip-cloud-stack-role")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        let (stack_id, stack_role) = match (stack_id, stack_role) {
            (Some(id), Some(role)) => (id, role),
            _ => return Ok(None),
        };

        // 基于 stack id 派生稳定的公司 ID 与用户 ID
        let company_id = derive_uuid_from(&stack_id);
        let user_id = derive_uuid_from(&format!("{}-user", stack_id));

        let user_repo = PgAuthUserRepository::new((*self.pool).clone());
        if user_repo.find_by_id(user_id).await.map_err(|e| {
            AuthError::Internal { message: format!("Cloud user lookup failed: {}", e) }
        })?.is_none()
        {
            let user = repositories::models::auth::AuthUser {
                id: user_id,
                email: format!("{}@cloud.paperclip.local", stack_id),
                name: Some(stack_id.clone()),
                password_hash: None,
                email_verified: true,
                email_verified_at: Some(chrono::Utc::now()),
                avatar_url: None,
                oauth_provider: Some("cloud".to_string()),
                oauth_provider_id: Some(stack_id.clone()),
                cloud_tenant_id: Some(stack_id.clone()),
                is_active: true,
                last_login_at: Some(chrono::Utc::now()),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            let _ = user_repo.create(user).await;
        }

        let company_repo = PgCompanyRepository::new((*self.pool).clone());
        if company_repo.find_by_id(company_id).await.map_err(|e| {
            AuthError::Internal { message: format!("Cloud company lookup failed: {}", e) }
        })?.is_none()
        {
            let _ = company_repo
                .create(repositories::models::auth::Company {
                    id: company_id,
                    name: stack_id.clone(),
                    slug: stack_id.clone().to_lowercase(),
                    description: None,
                    logo_url: None,
                    website: None,
                    industry: None,
                    size: None,
                    cloud_stack_id: Some(stack_id.clone()),
                    settings: serde_json::Value::Null,
                    is_active: true,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                })
                .await;
        }

        // 角色映射：owner/admin -> Owner，其余 -> Operator
        let role = match stack_role.to_ascii_lowercase().as_str() {
            "owner" | "admin" => crate::auth::MembershipRole::Owner,
            _ => crate::auth::MembershipRole::Operator,
        };

        let membership_repo = PgCompanyMembershipRepository::new((*self.pool).clone());
        if membership_repo
            .find_by_principal(company_id, "user", user_id)
            .await
            .map_err(|e| AuthError::Internal { message: format!("Cloud membership lookup failed: {}", e) })?
            .is_none()
        {
            let _ = membership_repo
                .create(repositories::models::authorization::CompanyMembershipRow::new(
                    company_id,
                    "user".to_string(),
                    user_id,
                    format!("{:?}", role).to_lowercase(),
                ))
                .await;
        }

        let membership = crate::auth::membership::CompanyMembership::new(
            company_id,
            crate::auth::PrincipalType::User,
            user_id,
            role,
        );

        Ok(Some(AuthorizationActor::board_with_source(
            user_id,
            company_id,
            ActorSource::CloudTenant,
            vec![membership],
            false,
        )))
    }

    fn priority(&self) -> u8 {
        3
    }
}

/// 从稳定字符串派生确定性 UUID（用于 cloud tenant 的 id 映射）。
fn derive_uuid_from(seed: &str) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    let digest = hasher.finalize();
    Uuid::from_slice(&digest[..16]).unwrap_or_else(|_| Uuid::nil())
}

/// 从 Cookie 头中解析指定名称的 cookie 值。
fn extract_cookie(cookie_header: &str, name: &str) -> Option<String> {
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// 认证中间件：聚合多个 ActorResolver，按优先级尝试解析请求主体。
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
                Err(AuthError::unauthenticated("No actor resolver succeeded"))
            }
            AuthMode::Authenticated => {
                for resolver in &self.resolvers {
                    if let Some(actor) = resolver.resolve(request).await? {
                        return Ok(actor);
                    }
                }
                Ok(AuthorizationActor::none())
            }
        }
    }
}

/// axum 中间件函数：解析 actor 并注入 request extensions。
pub async fn auth_middleware_fn(
    State(middleware): State<Arc<AuthMiddleware>>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let actor = middleware.resolve_actor(&request).await?;
    request.extensions_mut().insert(actor);
    Ok(next.run(request).await)
}

/// 从 request extensions 提取已解析的 actor。
pub fn extract_actor(request: &Request) -> AuthResult<&AuthorizationActor> {
    request
        .extensions()
        .get::<AuthorizationActor>()
        .ok_or_else(|| AuthError::Internal {
            message: "Actor not found in request extensions".to_string(),
        })
}

/// 便捷构造：用 Bearer + Session + CloudTenant 解析器组装 Authenticated 模式中间件。
pub fn authenticated_middleware(pool: Arc<PgPool>, jwt_config: Arc<JwtConfig>) -> AuthMiddleware {
    AuthMiddleware::new(AuthMode::Authenticated)
        .with_resolver(Arc::new(BearerTokenResolver::new(pool.clone(), jwt_config)))
        .with_resolver(Arc::new(SessionCookieResolver::new(pool.clone())))
        .with_resolver(Arc::new(CloudTenantHeaderResolver::new(pool)))
}

/// 便捷构造：LocalTrusted 模式中间件（默认身份）。
pub fn local_trusted_middleware(default_user_id: Uuid, default_company_id: Uuid) -> AuthMiddleware {
    AuthMiddleware::new(AuthMode::LocalTrusted)
        .with_resolver(Arc::new(LocalTrustedResolver::new(default_user_id, default_company_id)))
}
