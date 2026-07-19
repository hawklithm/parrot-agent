use async_trait::async_trait;
use axum::{
    extract::State,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use uuid::Uuid;

use sqlx::PgPool;

use repositories::auth_repositories::{
    AuthSessionRepository, AuthUserRepository, CompanyMembershipRepository, CompanyRepository,
    PgAuthSessionRepository, PgAuthUserRepository, PgCompanyRepository,
    PgCompanyMembershipRepository,
};
use repositories::board_api_key_repository::{
    BoardApiKeyRepository, PgBoardApiKeyRepository, hash_api_key,
};
use repositories::agent_api_key_repository::{
    AgentApiKeyRepository as _, PgAgentApiKeyRepository as PgAgentKeyRepo,
};
use repositories::pg_agent_repository::PgAgentRepository;
use repositories::agent_repository::AgentRepository;
use models::agent::AgentStatus;
use repositories::activity_log_repository::{
    Activity, ActivityAction, ActivityLogRepository, ActorType, PgActivityLogRepository,
    ResourceType,
};

use crate::auth::{
    ActorSource, AgentApiKeyScope, AuthError, AuthResult, AuthorizationActor, JwtConfig,
    load_responsible_user_memberships, resolve_board_access, verify_local_agent_jwt,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    LocalTrusted,
    Authenticated,
}

/// Derive an instance-isolated BetterAuth-compatible cookie prefix.
pub fn auth_cookie_prefix(instance_id: &str) -> String {
    let safe: String = instance_id.trim().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' }).collect();
    format!("parrot-{}", if safe.trim_matches('-').is_empty() { "default" } else { safe.trim_matches('-') })
}

/// Build trusted origins from configured hostnames and listen port.
pub fn auth_trusted_origins(hostnames: &[String], port: u16, explicit_base_url: Option<&str>) -> Vec<String> {
    let mut origins = std::collections::BTreeSet::new();
    if let Some(base) = explicit_base_url.map(str::trim).filter(|v| !v.is_empty()) {
        let origin = base.split_once("//").map(|(_, rest)| rest.split('/').next().unwrap_or(rest)).unwrap_or(base);
        origins.insert(format!("{}://{}", base.split("://").next().unwrap_or("https"), origin));
    }
    for hostname in hostnames.iter().map(|h| h.trim()).filter(|h| !h.is_empty()) {
        for scheme in ["http", "https"] {
            origins.insert(format!("{scheme}://{hostname}:{port}"));
            if port == 80 || port == 443 { origins.insert(format!("{scheme}://{hostname}")); }
        }
    }
    origins.into_iter().collect()
}

impl AuthMode {
    /// Development defaults to local_trusted; release builds fail closed.
    pub fn from_env() -> Self {
        match std::env::var("DEPLOYMENT_MODE").ok().as_deref() {
            Some("authenticated") => Self::Authenticated,
            Some("local_trusted") => Self::LocalTrusted,
            _ if cfg!(debug_assertions) => Self::LocalTrusted,
            _ => Self::Authenticated,
        }
    }
}

#[async_trait]
pub trait ActorResolver: Send + Sync {
    async fn resolve(&self, headers: &HeaderMap) -> AuthResult<Option<AuthorizationActor>>;
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
    async fn resolve(&self, _headers: &HeaderMap) -> AuthResult<Option<AuthorizationActor>> {
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

    fn extract_bearer_token(&self, headers: &HeaderMap) -> Option<String> {
        headers
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

        // 检查 API Key 是否过期
        if let Some(expires_at) = key.expires_at {
            if chrono::Utc::now() > expires_at {
                // 记录过期拒绝审计事件
                crate::auth::audit::audit_api_key_rejected(
                    &self.pool,
                    key.id,
                    key.user_id,
                    key.user_id,
                    "user",
                    "API key has expired",
                )
                .await;
                return Ok(None);
            }
        }

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
            Some(k) if k.is_active() => k,
            _ => return Ok(None),
        };

        let _ = repo.update_last_used(key.id).await;

        let scope = if key.scope.is_null() || key.scope == serde_json::json!({}) {
            AgentApiKeyScope::new(key.agent_id, key.company_id)
        } else {
            AgentApiKeyScope::from_json(key.scope.clone())
                .ok_or_else(|| AuthError::InvalidApiKey { reason: "Invalid agent API key scope".to_string() })?
        };

        // 查询关联的 Agent 记录，确认其存在且处于活跃状态。
        let agent_repo = PgAgentRepository::new((*self.pool).clone());
        let agent = agent_repo.get_by_id(key.agent_id).await.map_err(|e| {
            AuthError::Internal { message: format!("Agent lookup failed: {}", e) }
        })?;

        let responsible_user_id = match &agent {
            a if a.status == AgentStatus::Running => a.reports_to,
            _ => {
                crate::auth::audit::audit_missing_responsible_user(&self.pool, key.agent_id, key.company_id).await;
                return Err(AuthError::Forbidden {
                    reason: "Agent is not active or does not exist".to_string(),
                    code: Some("AGENT_INACTIVE".to_string()),
                });
            }
        };

        // 加载 responsible user 在指定公司内的活跃成员关系（供 Agent 权限检查）。
        let on_behalf_of_memberships = match responsible_user_id {
            Some(uid) => load_responsible_user_memberships(&self.pool, uid, key.company_id)
                .await
                .unwrap_or_default(),
            None => Vec::new(),
        };

        let actor = AuthorizationActor::Agent {
            agent_id: key.agent_id,
            company_id: key.company_id,
            run_id: None,
            source: ActorSource::AgentKey,
            key_id: Some(key.id),
            key_scope: Some(scope),
            responsible_user_id,
            on_behalf_of_user_id: responsible_user_id,
            on_behalf_of_memberships,
        };

        Ok(Some(actor))
    }

    async fn resolve_jwt(&self, token: &str, headers: &HeaderMap) -> AuthResult<Option<AuthorizationActor>> {
        let claims = match verify_local_agent_jwt(&self.jwt_config, token) {
            Some(c) => c,
            None => return Ok(None),
        };

        let agent_id = Uuid::parse_str(&claims.sub)
            .map_err(|_| AuthError::InvalidToken { reason: "invalid agent id in JWT".to_string() })?;
        let company_id = Uuid::parse_str(&claims.company_id)
            .map_err(|_| AuthError::InvalidToken { reason: "invalid company id in JWT".to_string() })?;
        let run_id = match claims.run_id {
            Some(s) => Some(
                Uuid::parse_str(&s)
                    .map_err(|_| AuthError::InvalidToken { reason: "invalid run id in JWT".to_string() })?,
            ),
            None => None,
        };

        // 查询 Agent 是否存在且 active。
        let agent_repo = PgAgentRepository::new((*self.pool).clone());
        let agent = agent_repo.get_by_id(agent_id).await.map_err(|e| {
            AuthError::Internal { message: format!("Agent lookup failed: {}", e) }
        })?;

        let agent = match agent {
            a if a.status == AgentStatus::Running => a,
            _ => {
                // Agent 不存在或未处于运行态：记审计日志后拒绝。
                self.audit_jwt_rejected(&agent_id, &company_id, run_id, "agent inactive or not found")
                    .await;
                return Err(AuthError::Forbidden {
                    reason: "Agent is not active or does not exist".to_string(),
                    code: Some("AGENT_INACTIVE".to_string()),
                });
            }
        };

        if let Some(claim_run_id) = run_id {
            if let Some(header_run_id) = headers.get("x-paperclip-run-id")
                .or_else(|| headers.get("x-parrot-run-id"))
                .and_then(|v| v.to_str().ok()).map(str::trim)
            {
                if header_run_id != claim_run_id.to_string() {
                    self.audit_jwt_rejected(&agent_id, &company_id, Some(claim_run_id), "run_id mismatch").await;
                    return Err(AuthError::unprocessable(
                        "Run ID header does not match signed agent JWT run_id",
                        "agent_jwt_run_id_mismatch",
                    ));
                }
            }
        }

        let responsible_user_id = if let Some(value) = claims.responsible_user_id {
            Uuid::parse_str(&value).ok()
        } else if std::env::var("PAPERCLIP_AGENT_JWT_DISABLE_LEGACY_FALLBACK").ok().as_deref() == Some("true") {
            None
        } else if let Some(run) = run_id {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT responsible_user_id FROM heartbeat_runs WHERE id = $1 AND company_id = $2 AND agent_id = $3"
            ).bind(run).bind(company_id).bind(agent_id).fetch_optional(&*self.pool).await.ok().flatten()
                .flatten().and_then(|v| Uuid::parse_str(&v).ok())
        } else { None };
        let memberships = match responsible_user_id {
            Some(uid) => load_responsible_user_memberships(&self.pool, uid, company_id).await.unwrap_or_default(),
            None => Vec::new(),
        };

        Ok(Some(AuthorizationActor::Agent {
            agent_id, company_id, run_id, source: ActorSource::AgentJwt,
            key_id: None,
            key_scope: claims.key_scope.and_then(AgentApiKeyScope::from_json),
            responsible_user_id,
            on_behalf_of_user_id: responsible_user_id,
            on_behalf_of_memberships: memberships,
        }))
    }

    /// 记录 JWT 认证拒绝事件（最佳努力，不阻塞主流程）。
    async fn audit_jwt_rejected(
        &self,
        agent_id: &Uuid,
        company_id: &Uuid,
        run_id: Option<Uuid>,
        reason: &str,
    ) {
        let repo = PgActivityLogRepository::new((*self.pool).clone());
        let activity = Activity {
            id: Uuid::new_v4(),
            company_id: *company_id,
            actor_type: ActorType::Agent,
            actor_id: *agent_id,
            action: ActivityAction::View,
            resource_type: ResourceType::Agent,
            resource_id: *agent_id,
            metadata: Some(serde_json::json!({
                "event": "agent_jwt_rejected",
                "reason": reason,
                "run_id": run_id,
            })),
            created_at: chrono::Utc::now(),
        };
        let _ = repo.log_activity(&activity).await;
    }
}

#[async_trait]
impl ActorResolver for BearerTokenResolver {
    async fn resolve(&self, headers: &HeaderMap) -> AuthResult<Option<AuthorizationActor>> {
        let token = match self.extract_bearer_token(headers) {
            Some(t) => t,
            None => return Ok(None),
        };

        if token.starts_with("bak_") {
            self.resolve_board_key(&token).await
        } else if token.starts_with("aak_") {
            self.resolve_agent_key(&token).await
        } else {
            self.resolve_jwt(&token, headers).await
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
    async fn resolve(&self, headers: &HeaderMap) -> AuthResult<Option<AuthorizationActor>> {
        let session_token = match headers
            .get("cookie")
            .and_then(|h| h.to_str().ok())
            .and_then(|c| extract_cookie(c, &format!("{}-session", auth_cookie_prefix(&std::env::var("INSTANCE_ID").unwrap_or_else(|_| "default".to_string())))))
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

        // Sliding session expiry, matching BetterAuth's active-session behavior.
        let _ = session_repo.extend(session.id, 30 * 24 * 60 * 60).await;

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
    async fn resolve(&self, headers: &HeaderMap) -> AuthResult<Option<AuthorizationActor>> {
        let expected = match std::env::var("PAPERCLIP_CLOUD_TENANT_SERVER_TOKEN") {
            Ok(value) if !value.is_empty() => value,
            _ => return Ok(None),
        };
        let supplied = headers.get("x-paperclip-cloud-tenant-token").and_then(|v| v.to_str().ok()).unwrap_or("");
        if !constant_time_eq(expected.as_bytes(), supplied.as_bytes()) {
            return Ok(None);
        }
        let stack_id = headers
            .get("x-paperclip-cloud-stack-id")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());
        let stack_role = headers
            .get("x-paperclip-cloud-stack-role")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        let (stack_id, stack_role) = match (stack_id, stack_role) {
            (Some(id), Some(role)) => (id, role),
            _ => return Ok(None),
        };

        // Prefer the cloud identity headers; stack-derived identities retain
        // compatibility with older cloud callers.
        let company_id = derive_uuid_from(&stack_id);
        let user_id = headers.get("x-paperclip-cloud-user-id").and_then(|v| v.to_str().ok())
            .and_then(|v| Uuid::parse_str(v).ok()).unwrap_or_else(|| derive_uuid_from(&format!("{}-user", stack_id)));
        let user_email = headers.get("x-paperclip-cloud-user-email").and_then(|v| v.to_str().ok())
            .map(str::to_owned).unwrap_or_else(|| format!("{}@cloud.paperclip.local", stack_id));
        let user_name = headers.get("x-paperclip-cloud-user-name").and_then(|v| v.to_str().ok())
            .map(str::to_owned).or_else(|| Some(stack_id.clone()));

        let user_repo = PgAuthUserRepository::new((*self.pool).clone());
        if user_repo.find_by_id(user_id).await.map_err(|e| {
            AuthError::Internal { message: format!("Cloud user lookup failed: {}", e) }
        })?.is_none()
        {
            let user = repositories::models::auth::AuthUser {
                id: user_id,
                email: user_email,
                name: user_name,
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

        // Cloud identities must never inherit the historical instance-admin
        // grant from an older deployment.
        let _ = sqlx::query("DELETE FROM instance_user_roles WHERE user_id = $1 AND role = 'instance_admin'")
            .bind(user_id).execute(&*self.pool).await;

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
        if let Some(existing) = membership_repo
            .find_by_principal(company_id, "user", user_id)
            .await
            .map_err(|e| AuthError::Internal { message: format!("Cloud membership lookup failed: {}", e) })?
            .is_none() {
            let _ = membership_repo
                .create(repositories::models::authorization::CompanyMembershipRow::new(
                    company_id,
                    "user".to_string(),
                    user_id,
                    format!("{:?}", role).to_lowercase(),
                ))
                .await;
        } else {
            let _ = membership_repo.update_role(existing.id, format!("{:?}", role).to_lowercase()).await;
        }

        let membership = crate::auth::membership::CompanyMembership::new(
            company_id,
            crate::auth::PrincipalType::User,
            user_id,
            role,
        );

        ensure_human_role_default_grants(&self.pool, company_id, user_id, role).await;

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

/// Seed the company-scoped defaults used by CloudTenant users. This mirrors
/// Paperclip's `ensureHumanRoleDefaultGrants` while remaining idempotent.
async fn ensure_human_role_default_grants(
    pool: &PgPool,
    company_id: Uuid,
    user_id: Uuid,
    role: crate::auth::MembershipRole,
) {
    let permissions: &[&str] = match role {
        crate::auth::MembershipRole::Owner | crate::auth::MembershipRole::Admin => &[
            "companies:read", "companies:update", "projects:read", "projects:create",
            "issues:read", "issues:write", "agents:read", "tasks:assign",
        ],
        crate::auth::MembershipRole::Operator => &["companies:read", "projects:read", "issues:read", "issues:write"],
        crate::auth::MembershipRole::Viewer => &["companies:read", "projects:read", "issues:read"],
    };
    for permission in permissions {
        let _ = sqlx::query(
            "INSERT INTO principal_permission_grants (id, company_id, principal_type, principal_id, permission_key, scope, granted_by_user_id, created_at, updated_at) VALUES ($1,$2,'user',$3,$4,'{}'::jsonb,$3,NOW(),NOW()) ON CONFLICT DO NOTHING"
        ).bind(Uuid::new_v4()).bind(company_id).bind(user_id).bind(permission).execute(pool).await;
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

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut diff = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        diff |= usize::from(*left.get(index).unwrap_or(&0) ^ *right.get(index).unwrap_or(&0));
    }
    diff == 0
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

    pub async fn resolve_actor(&self, headers: &HeaderMap) -> AuthResult<AuthorizationActor> {
        match self.mode {
            AuthMode::LocalTrusted => {
                for resolver in &self.resolvers {
                    if let Some(actor) = resolver.resolve(headers).await? {
                        return Ok(actor);
                    }
                }
                Ok(AuthorizationActor::board_with_source(
                    Uuid::nil(), Uuid::nil(), ActorSource::LocalImplicit, vec![], false,
                ))
            }
            AuthMode::Authenticated => {
                for resolver in &self.resolvers {
                    if let Some(actor) = resolver.resolve(headers).await? {
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
    mut request: axum::extract::Request,
    next: Next,
) -> Result<Response, AuthError> {
    let headers = request.headers().clone();
    let actor = middleware.resolve_actor(&headers).await?;
    request.extensions_mut().insert(actor);
    Ok(next.run(request).await)
}

/// 从 request extensions 提取已解析的 actor。
pub fn extract_actor(request: &axum::extract::Request) -> AuthResult<&AuthorizationActor> {
    request
        .extensions()
        .get::<AuthorizationActor>()
        .ok_or_else(|| AuthError::Internal {
            message: "Actor not found in request extensions".to_string(),
        })
}

/// Require a Board actor in handlers that operate on human-owned resources.
pub fn require_board(actor: &AuthorizationActor) -> AuthResult<Uuid> {
    match actor {
        AuthorizationActor::Board { user_id, .. } => Ok(*user_id),
        AuthorizationActor::None => Err(AuthError::unauthenticated("Board authentication required")),
        AuthorizationActor::Agent { .. } => Err(AuthError::forbidden_with_code("Board actor required", "BOARD_ACTOR_REQUIRED")),
    }
}

/// Require an Agent actor in agent-runtime handlers.
pub fn require_agent(actor: &AuthorizationActor) -> AuthResult<Uuid> {
    match actor {
        AuthorizationActor::Agent { agent_id, .. } => Ok(*agent_id),
        AuthorizationActor::None => Err(AuthError::unauthenticated("Agent authentication required")),
        AuthorizationActor::Board { .. } => Err(AuthError::forbidden_with_code("Agent actor required", "AGENT_ACTOR_REQUIRED")),
    }
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

/// Builds the resolver chain used by every API route, in Paperclip order.
pub fn middleware_from_env(pool: Arc<PgPool>) -> AuthMiddleware {
    let mode = AuthMode::from_env();
    let jwt = JwtConfig::from_env().unwrap_or_else(|| JwtConfig::new(
        String::new(), 3600, "parrot-agent".to_string(), "agent-runtime".to_string(), "local".to_string(),
    ));
    let mut middleware = AuthMiddleware::new(mode)
        .with_resolver(Arc::new(BearerTokenResolver::new(pool.clone(), Arc::new(jwt))))
        .with_resolver(Arc::new(SessionCookieResolver::new(pool.clone())))
        .with_resolver(Arc::new(CloudTenantHeaderResolver::new(pool)));
    if mode == AuthMode::LocalTrusted {
        let user_id = std::env::var("LOCAL_TRUSTED_USER_ID").ok().and_then(|v| Uuid::parse_str(&v).ok()).unwrap_or_else(Uuid::nil);
        let company_id = std::env::var("LOCAL_TRUSTED_COMPANY_ID").ok().and_then(|v| Uuid::parse_str(&v).ok()).unwrap_or_else(Uuid::nil);
        middleware = middleware.with_resolver(Arc::new(LocalTrustedResolver::new(user_id, company_id)));
    }
    middleware
}
