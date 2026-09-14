use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::membership::{CompanyMembership, MembershipRole};

/// Actor 类型系统 - 授权主体抽象
///
/// 核心概念：
/// - AuthorizationActor: 执行操作的主体（Board用户、Agent、None匿名）
/// - ActorSource: 主体身份来源（会话token、API密钥、JWT等）
/// - AgentApiKeyScope: Agent API密钥的权限范围限定

/// 授权主体类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthorizationActor {
    /// Board 用户主体
    Board {
        user_id: Uuid,
        company_id: Uuid,
        /// 身份来源（会话/API Key/本地隐式等）
        source: ActorSource,
        /// 该用户在当前解析上下文中的公司成员关系（用于角色级权限检查）
        memberships: Vec<CompanyMembership>,
        /// 是否为实例管理员（跨公司全局权限）
        is_instance_admin: bool,
        /// 关联的 Board API Key ID（仅 board_key 来源；Paperclip `actor.keyId`）
        key_id: Option<Uuid>,
    },
    /// Agent 主体
    Agent {
        agent_id: Uuid,
        company_id: Uuid,
        /// Agent所属的运行时上下文（可选）
        run_id: Option<Uuid>,
        /// 身份来源（API Key/JWT等）
        source: ActorSource,
        /// 关联的 API Key ID（仅 agent_key 来源）
        key_id: Option<Uuid>,
        /// API Key 的权限范围（仅 agent_key 来源）
        key_scope: Option<AgentApiKeyScope>,
        /// 负责的 Board 用户（responsible user），用于权限委托
        responsible_user_id: Option<Uuid>,
        /// 以 on_behalf_of 方式委托的用户 ID
        on_behalf_of_user_id: Option<Uuid>,
        /// on_behalf_of 用户的成员关系（用于权限检查）
        on_behalf_of_memberships: Vec<CompanyMembership>,
    },
    /// 匿名/未认证主体
    None,
}

impl AuthorizationActor {
    /// 创建 Board 用户主体（无成员关系/非实例管理员）
    pub fn board(user_id: Uuid, company_id: Uuid) -> Self {
        Self::Board {
            user_id,
            company_id,
            source: ActorSource::LocalImplicit,
            memberships: Vec::new(),
            is_instance_admin: false,
            key_id: None,
        }
    }

    /// 创建带成员关系与实例管理员标记的 Board 主体
    pub fn board_with_memberships(
        user_id: Uuid,
        company_id: Uuid,
        memberships: Vec<CompanyMembership>,
        is_instance_admin: bool,
    ) -> Self {
        Self::Board {
            user_id,
            company_id,
            source: ActorSource::LocalImplicit,
            memberships,
            is_instance_admin,
            key_id: None,
        }
    }

    /// 创建带来源的 Board 用户主体
    pub fn board_with_source(
        user_id: Uuid,
        company_id: Uuid,
        source: ActorSource,
        memberships: Vec<CompanyMembership>,
        is_instance_admin: bool,
    ) -> Self {
        Self::Board {
            user_id,
            company_id,
            source,
            memberships,
            is_instance_admin,
            key_id: None,
        }
    }

    /// 附加当前 Board API Key ID（仅对 board_key 来源有意义；其余主体无操作）
    ///
    /// Paperclip 的 `req.actor.keyId` 由此对应：`/cli-auth/me` 需据此返回
    /// `keyId`，`/cli-auth/revoke-current` 需据此只撤销当前这把 Key。
    pub fn with_key_id(mut self, key_id: Uuid) -> Self {
        if let Self::Board { key_id: slot, .. } = &mut self {
            *slot = Some(key_id);
        }
        self
    }

    /// 关联的 Board API Key ID（非 API Key 认证时为 None）
    pub fn key_id(&self) -> Option<Uuid> {
        match self {
            Self::Board { key_id, .. } => *key_id,
            _ => None,
        }
    }

    /// 创建 Agent 主体
    pub fn agent(agent_id: Uuid, company_id: Uuid, run_id: Option<Uuid>) -> Self {
        Self::Agent {
            agent_id,
            company_id,
            run_id,
            source: ActorSource::AgentJwt,
            key_id: None,
            key_scope: None,
            responsible_user_id: None,
            on_behalf_of_user_id: None,
            on_behalf_of_memberships: Vec::new(),
        }
    }

    /// 创建带来源的 Agent 主体
    pub fn agent_with_source(
        agent_id: Uuid,
        company_id: Uuid,
        run_id: Option<Uuid>,
        source: ActorSource,
    ) -> Self {
        Self::Agent {
            agent_id,
            company_id,
            run_id,
            source,
            key_id: None,
            key_scope: None,
            responsible_user_id: None,
            on_behalf_of_user_id: None,
            on_behalf_of_memberships: Vec::new(),
        }
    }

    /// 创建带 API Key 上下文的 Agent 主体（agent_key 来源）
    pub fn agent_with_key(
        agent_id: Uuid,
        company_id: Uuid,
        key_id: Uuid,
        key_scope: AgentApiKeyScope,
        responsible_user_id: Option<Uuid>,
    ) -> Self {
        Self::Agent {
            agent_id,
            company_id,
            run_id: None,
            source: ActorSource::AgentKey,
            key_id: Some(key_id),
            key_scope: Some(key_scope),
            responsible_user_id,
            on_behalf_of_user_id: responsible_user_id,
            on_behalf_of_memberships: Vec::new(),
        }
    }

    /// 创建匿名主体
    pub fn none() -> Self {
        Self::None
    }

    /// 获取公司ID（如果存在）
    pub fn company_id(&self) -> Option<Uuid> {
        match self {
            Self::Board { company_id, .. } | Self::Agent { company_id, .. } => Some(*company_id),
            Self::None => None,
        }
    }

    /// 是否为实例管理员（仅 Board 用户可持有）
    pub fn is_instance_admin(&self) -> bool {
        matches!(self, Self::Board { is_instance_admin, .. } if *is_instance_admin)
    }

    /// 查找该 Actor 在指定公司的活跃成员角色
    pub fn role_in(&self, company_id: Uuid) -> Option<MembershipRole> {
        match self {
            Self::Board { memberships, .. } => memberships
                .iter()
                .find(|m| m.company_id == company_id && m.status.is_active())
                .map(|m| m.role),
            _ => None,
        }
    }

    /// 获取主体ID（Board返回user_id，Agent返回agent_id）
    pub fn principal_id(&self) -> Option<Uuid> {
        match self {
            Self::Board { user_id, .. } => Some(*user_id),
            Self::Agent { agent_id, .. } => Some(*agent_id),
            Self::None => None,
        }
    }

    /// 是否为Board用户
    pub fn is_board(&self) -> bool {
        matches!(self, Self::Board { .. })
    }

    /// 是否为Agent
    pub fn is_agent(&self) -> bool {
        matches!(self, Self::Agent { .. })
    }

    /// 是否为匿名主体
    pub fn is_anonymous(&self) -> bool {
        matches!(self, Self::None)
    }

    /// 获取 Actor 类型字符串（用于日志和审计）
    pub fn actor_type(&self) -> &'static str {
        match self {
            Self::Board { .. } => "user",
            Self::Agent { .. } => "agent",
            Self::None => "system",
        }
    }
}

/// 主体身份来源 - 标识Actor的认证方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorSource {
    /// 本地隐式认证（单用户模式下的默认身份）
    LocalImplicit,
    /// 会话token认证（Board用户登录后的session）
    Session,
    /// Board API密钥认证
    BoardKey,
    /// Agent API密钥认证
    AgentKey,
    /// Agent JWT认证（短期临时token）
    AgentJwt,
    /// 云租户认证（多租户SaaS模式）
    CloudTenant,
    /// 无认证（匿名访问）
    None,
}

impl ActorSource {
    /// 是否为API密钥类认证
    pub fn is_api_key(&self) -> bool {
        matches!(self, Self::BoardKey | Self::AgentKey)
    }

    /// 是否为会话类认证（需要CSRF保护）
    pub fn is_session_based(&self) -> bool {
        matches!(self, Self::Session | Self::LocalImplicit)
    }

    /// 是否为临时令牌认证
    pub fn is_ephemeral(&self) -> bool {
        matches!(self, Self::AgentJwt)
    }
}

/// Agent API 密钥的权限范围。
///
/// 形状对齐 Paperclip `agentApiKeyScopeSchema`
/// （`packages/shared/src/validators/agent.ts:144-176`）：`kind` 判别联合，
/// camelCase 字段。历史上的 `scope_type` 形状通过 serde alias 继续可读，
/// 因此旧行不需要数据迁移。
///
/// 注意：Paperclip 在 `normalizeAgentApiKeyScope` 里对无法解析的 scope
/// **降级为 `standard`**，而不是拒绝。这一语义由 [`Self::from_json`] 保留，
/// 调用方不再需要为「scope 不合法」构造 403。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentApiKeyScope {
    /// 判别符：`standard` / `task_bridge` / `skill_test`。
    ///
    /// Paperclip 拼写为 `kind`；`scope_type` 作为 alias 保留，用于读取
    /// 迁移前的持久化行。
    #[serde(default = "default_scope_type", alias = "scope_type")]
    pub kind: String,
    /// 密钥所属 Agent ID（Parrot 额外记录，用于审计）。
    #[serde(default, alias = "agent_id")]
    pub agent_id: Option<Uuid>,
    /// 密钥所属公司 ID（Parrot 额外记录，用于审计）。
    #[serde(default, alias = "company_id")]
    pub company_id: Option<Uuid>,
    /// `task_bridge` 的单个项目边界。
    #[serde(default, alias = "project_id")]
    pub project_id: Option<Uuid>,
    /// `task_bridge` 的项目边界集合。
    #[serde(default, alias = "project_ids")]
    pub project_ids: Vec<Uuid>,
    /// `task_bridge` 的单个父 issue 边界。
    #[serde(default, alias = "parent_issue_id")]
    pub parent_issue_id: Option<Uuid>,
    /// `task_bridge` 的父 issue 边界集合。
    #[serde(default, alias = "parent_issue_ids")]
    pub parent_issue_ids: Vec<Uuid>,
    /// `task_bridge` 允许指派到的 agent 集合。
    #[serde(default, alias = "allowed_assignee_agent_ids")]
    pub allowed_assignee_agent_ids: Vec<Uuid>,
    /// `skill_test` 唯一允许访问的 issue。
    #[serde(default, alias = "issue_id")]
    pub issue_id: Option<Uuid>,
}

/// Paperclip `agentApiKeyScopeSchema` 的三条分支以 `kind` 字面量区分。
pub const AGENT_KEY_SCOPE_KINDS: [&str; 3] = ["standard", "task_bridge", "skill_test"];

fn default_scope_type() -> String {
    "standard".to_string()
}

impl Default for AgentApiKeyScope {
    fn default() -> Self {
        Self {
            kind: default_scope_type(),
            agent_id: None,
            company_id: None,
            project_id: None,
            project_ids: Vec::new(),
            parent_issue_id: None,
            parent_issue_ids: Vec::new(),
            allowed_assignee_agent_ids: Vec::new(),
            issue_id: None,
        }
    }
}

impl AgentApiKeyScope {
    /// 标准（无边界）scope。
    pub fn new(agent_id: Uuid, company_id: Uuid) -> Self {
        Self {
            kind: default_scope_type(),
            agent_id: Some(agent_id),
            company_id: Some(company_id),
            ..Default::default()
        }
    }

    /// `skill_test` scope：只允许访问 `issue_id`。
    pub fn skill_test(agent_id: Option<Uuid>, company_id: Option<Uuid>, issue_id: Uuid) -> Self {
        Self {
            kind: "skill_test".to_string(),
            agent_id,
            company_id,
            issue_id: Some(issue_id),
            ..Default::default()
        }
    }

    /// `task_bridge` scope。
    pub fn task_bridge(
        agent_id: Option<Uuid>,
        company_id: Option<Uuid>,
        project_ids: Vec<Uuid>,
        parent_issue_ids: Vec<Uuid>,
        allowed_assignee_agent_ids: Vec<Uuid>,
    ) -> Self {
        Self {
            kind: "task_bridge".to_string(),
            agent_id,
            company_id,
            project_ids,
            parent_issue_ids,
            allowed_assignee_agent_ids,
            ..Default::default()
        }
    }

    /// 是否是标准 scope（无任何边界约束）。
    pub fn is_standard(&self) -> bool {
        self.kind == "standard"
    }

    /// `task_bridge` / `skill_test` 是否声明了任一项目或父 issue 边界。
    ///
    /// Paperclip `taskBridgeAgentKeyScopeSchema.superRefine` 要求二者**至少其一**。
    pub fn has_task_bridge_boundary(&self) -> bool {
        self.project_id.is_some()
            || !self.project_ids.is_empty()
            || self.parent_issue_id.is_some()
            || !self.parent_issue_ids.is_empty()
    }

    /// 解析持久化/JWT 中的 scope JSON。
    ///
    /// 与 Paperclip `normalizeAgentApiKeyScope`（`validators/agent.ts:182-185`）
    /// 一致：**任何**无法通过校验的值都降级为 `{kind:"standard"}`，而不是报错。
    /// `None` 仅在输入为空（`null` / `{}` / 全空对象）时返回，表示「该行没有
    /// 记录 scope」，此时调用方应构造 [`Self::new`]。
    pub fn from_json(value: serde_json::Value) -> Option<Self> {
        if value.is_null() || value == serde_json::json!({}) {
            return None;
        }
        let parsed: Self = match serde_json::from_value(value) {
            Ok(parsed) => parsed,
            // 形状不认识 —— 按 Paperclip 的规范化语义降级为 standard。
            Err(_) => return Some(Self::default()),
        };
        if !AGENT_KEY_SCOPE_KINDS.contains(&parsed.kind.as_str()) {
            return Some(Self::default());
        }
        // `task_bridge` 缺边界、`skill_test` 缺 issue 都是 Paperclip 中
        // `superRefine` 会判失败、从而被 `normalizeAgentApiKeyScope` 折叠成
        // standard 的情况。
        if parsed.kind == "task_bridge" && !parsed.has_task_bridge_boundary() {
            return Some(Self::default());
        }
        if parsed.kind == "skill_test" && parsed.issue_id.is_none() {
            return Some(Self::default());
        }
        Some(parsed)
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authorization_actor_board() {
        let user_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let actor = AuthorizationActor::board(user_id, company_id);

        assert!(actor.is_board());
        assert!(!actor.is_agent());
        assert!(!actor.is_anonymous());
        assert_eq!(actor.company_id(), Some(company_id));
        assert_eq!(actor.principal_id(), Some(user_id));
    }

    #[test]
    fn test_authorization_actor_agent() {
        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let run_id = Some(Uuid::new_v4());
        let actor = AuthorizationActor::agent(agent_id, company_id, run_id);

        assert!(!actor.is_board());
        assert!(actor.is_agent());
        assert!(!actor.is_anonymous());
        assert_eq!(actor.company_id(), Some(company_id));
        assert_eq!(actor.principal_id(), Some(agent_id));
    }

    #[test]
    fn test_authorization_actor_none() {
        let actor = AuthorizationActor::none();

        assert!(!actor.is_board());
        assert!(!actor.is_agent());
        assert!(actor.is_anonymous());
        assert_eq!(actor.company_id(), None);
        assert_eq!(actor.principal_id(), None);
    }

    #[test]
    fn test_actor_source_api_key() {
        assert!(ActorSource::BoardKey.is_api_key());
        assert!(ActorSource::AgentKey.is_api_key());
        assert!(!ActorSource::Session.is_api_key());
    }

    #[test]
    fn test_actor_source_session_based() {
        assert!(ActorSource::Session.is_session_based());
        assert!(ActorSource::LocalImplicit.is_session_based());
        assert!(!ActorSource::AgentJwt.is_session_based());
    }

    #[test]
    fn test_scope_parses_paperclip_kind_union() {
        let issue_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();

        let standard = AgentApiKeyScope::from_json(serde_json::json!({"kind": "standard"}))
            .expect("standard scope should parse");
        assert!(standard.is_standard());

        let skill_test = AgentApiKeyScope::from_json(serde_json::json!({
            "kind": "skill_test",
            "issueId": issue_id,
        }))
        .expect("skill_test scope should parse");
        assert_eq!(skill_test.kind, "skill_test");
        assert_eq!(skill_test.issue_id, Some(issue_id));

        // Paperclip 的边界是「项目 **或** 父 issue」（`superRefine`），
        // 不是两者都要。只有 projectId 也必须成立。
        let bridge = AgentApiKeyScope::from_json(serde_json::json!({
            "kind": "task_bridge",
            "projectId": project_id,
        }))
        .expect("task_bridge scope should parse");
        assert_eq!(bridge.kind, "task_bridge");
        assert_eq!(bridge.project_id, Some(project_id));
    }

    #[test]
    fn test_scope_accepts_legacy_snake_case_rows() {
        // 迁移前写入的行使用 `scope_type` 与 snake_case 字段，必须继续可读，
        // 否则所有既有 key 会在鉴权时被判为无 scope。
        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let scope = AgentApiKeyScope::from_json(serde_json::json!({
            "scope_type": "standard",
            "agent_id": agent_id,
            "company_id": company_id,
        }))
        .expect("legacy scope should parse");
        assert!(scope.is_standard());
        assert_eq!(scope.agent_id, Some(agent_id));
        assert_eq!(scope.company_id, Some(company_id));
    }

    #[test]
    fn test_scope_normalizes_invalid_shapes_to_standard() {
        // Paperclip `normalizeAgentApiKeyScope`（`validators/agent.ts:182-185`）
        // 对任何解析失败都返回 `{kind:"standard"}`，从不报错。
        let cases = vec![
            serde_json::json!({"kind": "unknown_kind"}),
            serde_json::json!({"kind": "task_bridge"}),
            serde_json::json!({"kind": "skill_test"}),
            serde_json::json!({"kind": "standard", "unexpected": 1}),
            serde_json::json!("not-an-object"),
            serde_json::json!([1, 2, 3]),
        ];
        for case in cases {
            let scope = AgentApiKeyScope::from_json(case.clone())
                .unwrap_or_else(|| panic!("{case} should normalize, not vanish"));
            assert!(
                scope.is_standard(),
                "{case} should normalize to standard, got {}",
                scope.kind
            );
        }
    }

    #[test]
    fn test_scope_empty_values_mean_no_scope_recorded() {
        assert!(AgentApiKeyScope::from_json(serde_json::Value::Null).is_none());
        assert!(AgentApiKeyScope::from_json(serde_json::json!({})).is_none());
    }

    #[test]
    fn test_scope_serializes_back_to_paperclip_shape() {
        let issue_id = Uuid::new_v4();
        let scope = AgentApiKeyScope::skill_test(None, None, issue_id);
        let value = serde_json::to_value(&scope).expect("scope should serialize");
        assert_eq!(value["kind"], "skill_test");
        assert_eq!(value["issueId"], issue_id.to_string());
        // camelCase 契约：不得出现 snake_case 键。
        assert!(value.get("issue_id").is_none());
    }
}
