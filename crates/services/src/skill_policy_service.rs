//! Company Skill Policy service.
//!
//! 逐行对齐 paperclip `server/src/services/company-skill-policy.ts` 与
//! `packages/shared/src/validators/skill-policy.ts`：文档模型
//! （`schemaVersion` / `defaultEffect` / `rules`）取代了 Parrot 早期的
//! `mode` / `allowRules` / `denyRules` / `allowedSkills` 模型。
//!
//! DB 映射：`company_skill_policies.policy`(jsonb) 存文档
//! `{schemaVersion, defaultEffect, rules}`，`version` 即 API 的 `revision`。
//! `replace` / `reset` 与 `activity_logs` 写入同一事务，任一步失败整体回滚。
//!
//! 校验器逐字段复刻 zod schema（含 `.strict()` 的未知键拒绝、trim 与
//! locator 规范化），产出 zod 形状的 `issues`，由路由层映射为
//! 422 `skill_policy_validation_failed`。

use async_trait::async_trait;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::HashSet;
use std::sync::LazyLock;
use uuid::Uuid;

pub const SKILL_POLICY_SCHEMA_VERSION: i32 = 1;
pub const MAX_RULES: usize = 1_000;
pub const MAX_SELECTOR_ENTRIES: usize = 500;
pub const MAX_SKILL_KEY_LEN: usize = 512;
pub const MAX_LOCATOR_LEN: usize = 2_048;
pub const MAX_RULE_ID_LEN: usize = 128;
pub const MIN_PRIORITY: i64 = -1_000_000;
pub const MAX_PRIORITY: i64 = 1_000_000;

/// Paperclip `SKILL_POLICY_ACTIONS`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum SkillPolicyAction {
    #[serde(rename = "skills.create")]
    Create,
    #[serde(rename = "skills.import")]
    Import,
    #[serde(rename = "skills.install")]
    Install,
    #[serde(rename = "skills.edit")]
    Edit,
    #[serde(rename = "skills.update")]
    Update,
    #[serde(rename = "skills.test")]
    Test,
    #[serde(rename = "skills.reset")]
    Reset,
    #[serde(rename = "skills.remove")]
    Remove,
}

pub const SKILL_POLICY_ACTIONS: [SkillPolicyAction; 8] = [
    SkillPolicyAction::Create,
    SkillPolicyAction::Import,
    SkillPolicyAction::Install,
    SkillPolicyAction::Edit,
    SkillPolicyAction::Update,
    SkillPolicyAction::Test,
    SkillPolicyAction::Reset,
    SkillPolicyAction::Remove,
];

impl SkillPolicyAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Create => "skills.create",
            Self::Import => "skills.import",
            Self::Install => "skills.install",
            Self::Edit => "skills.edit",
            Self::Update => "skills.update",
            Self::Test => "skills.test",
            Self::Reset => "skills.reset",
            Self::Remove => "skills.remove",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        SKILL_POLICY_ACTIONS
            .into_iter()
            .find(|action| action.as_str() == value)
    }
}

/// Paperclip `SKILL_POLICY_SOURCE_TYPES`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillPolicySourceType {
    Workspace,
    Catalog,
    Git,
    ExternalPackage,
    Generated,
    Unknown,
}

pub const SKILL_POLICY_SOURCE_TYPES: [SkillPolicySourceType; 6] = [
    SkillPolicySourceType::Workspace,
    SkillPolicySourceType::Catalog,
    SkillPolicySourceType::Git,
    SkillPolicySourceType::ExternalPackage,
    SkillPolicySourceType::Generated,
    SkillPolicySourceType::Unknown,
];

impl SkillPolicySourceType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Catalog => "catalog",
            Self::Git => "git",
            Self::ExternalPackage => "external_package",
            Self::Generated => "generated",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        SKILL_POLICY_SOURCE_TYPES
            .into_iter()
            .find(|source_type| source_type.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillPolicyEffect {
    Allow,
    Deny,
}

impl SkillPolicyEffect {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

/// Paperclip `SkillPolicyDecisionReason`。`evaluate` 只会产出
/// `no_policy_default` / `explicit_rule` / `policy_default` / `legacy_compatibility`；
/// `platform_invariant` 由技能路由的平台层拒绝使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillPolicyDecisionReason {
    PlatformInvariant,
    NoPolicyDefault,
    ExplicitRule,
    PolicyDefault,
    LegacyCompatibility,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillPolicySubject {
    AllAgents,
    Agents {
        #[serde(rename = "agentIds")]
        agent_ids: Vec<Uuid>,
    },
    Roles {
        roles: Vec<String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPolicyResourceSelector {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_ids: Option<Vec<Uuid>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_keys: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_types: Option<Vec<SkillPolicySourceType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_locators: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPolicyRule {
    pub id: String,
    pub priority: i64,
    pub effect: SkillPolicyEffect,
    pub subject: SkillPolicySubject,
    pub actions: Vec<SkillPolicyAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<SkillPolicyResourceSelector>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPolicyDocument {
    pub schema_version: i32,
    pub default_effect: SkillPolicyEffect,
    pub rules: Vec<SkillPolicyRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveSkillPolicy {
    pub schema_version: i32,
    pub revision: i32,
    pub default_effect: SkillPolicyEffect,
    pub rules: Vec<SkillPolicyRule>,
    pub materialized: bool,
}

/// 无任何策略行时的开放默认（Paperclip `OPEN_DEFAULT_POLICY`）。
pub fn open_default_policy() -> EffectiveSkillPolicy {
    EffectiveSkillPolicy {
        schema_version: SKILL_POLICY_SCHEMA_VERSION,
        revision: 0,
        default_effect: SkillPolicyEffect::Allow,
        rules: Vec::new(),
        materialized: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillPolicyPrincipalType {
    Agent,
    Board,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillPolicyPrincipal {
    pub principal_type: SkillPolicyPrincipalType,
    /// Agent id 或 board user id 的字面量（Paperclip 里 board 可回落为 `"board"`）。
    pub id: String,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SkillPolicyEvaluationResource {
    pub skill_id: Option<Uuid>,
    pub skill_key: Option<String>,
    pub source_type: Option<SkillPolicySourceType>,
    pub source_locator: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPolicyDecision {
    pub allowed: bool,
    pub action: SkillPolicyAction,
    pub reason: SkillPolicyDecisionReason,
    pub policy_revision: i32,
    pub matched_rule_id: Option<String>,
    pub remediation: Option<String>,
}

/// `activity_logs` 写入所需的 actor 上下文（Paperclip `getActorInfo(req)` 子集）。
#[derive(Debug, Clone)]
pub struct SkillPolicyActivity {
    /// `"agent" | "user" | "system"`。
    pub actor_type: String,
    pub actor_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct SkillPolicyEvaluateInput {
    pub company_id: Uuid,
    pub principal: SkillPolicyPrincipal,
    pub action: SkillPolicyAction,
    pub resource: SkillPolicyEvaluationResource,
}

#[derive(Debug, Clone)]
pub struct SkillPolicyReplaceInput {
    pub company_id: Uuid,
    pub expected_revision: i32,
    pub policy: SkillPolicyDocument,
    pub activity: SkillPolicyActivity,
}

#[derive(Debug, Clone)]
pub struct SkillPolicyResetInput {
    pub company_id: Uuid,
    pub activity: SkillPolicyActivity,
}

#[derive(Debug, thiserror::Error)]
pub enum SkillPolicyError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("skill policy revision is stale")]
    RevisionConflict {
        expected_revision: i32,
        current_revision: i32,
    },
    #[error("agent not found")]
    AgentNotFound,
    #[error("agent cannot be evaluated for another company")]
    CompanyBoundaryDenied,
    /// 库里已有的行不是合法文档。Paperclip 侧 `skillPolicyDocumentSchema.parse`
    /// 在 `effectivePolicyFromRow` 内抛 ZodError，被错误中间件渲染为 400
    /// `{ error: "Validation error", details: issues }`，这里保留同样的形状。
    #[error("stored skill policy is not a valid document")]
    CorruptPolicy(Vec<Value>),
}

pub type SkillPolicyResult<T> = Result<T, SkillPolicyError>;

#[async_trait]
pub trait SkillPolicyService: Send + Sync {
    /// 读取 company 当前生效策略（无行时返回开放默认）。
    async fn get(&self, company_id: Uuid) -> SkillPolicyResult<EffectiveSkillPolicy>;

    /// 解析 agent 主体：不存在 → 404；属于别的公司 → 403 boundary。
    async fn resolve_agent_principal(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
    ) -> SkillPolicyResult<SkillPolicyPrincipal>;

    /// 评估一次 skill 操作。
    async fn evaluate(
        &self,
        input: SkillPolicyEvaluateInput,
    ) -> SkillPolicyResult<SkillPolicyDecision>;

    /// 以 `expected_revision` 做 CAS 替换，并同事务写审计。
    async fn replace(
        &self,
        input: SkillPolicyReplaceInput,
    ) -> SkillPolicyResult<EffectiveSkillPolicy>;

    /// 删除策略行（恢复开放默认），并同事务写审计。
    async fn reset(&self, input: SkillPolicyResetInput) -> SkillPolicyResult<EffectiveSkillPolicy>;
}

pub struct DefaultSkillPolicyService {
    pool: PgPool,
}

impl DefaultSkillPolicyService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// ---------------------------------------------------------------------------
// 规范化
// ---------------------------------------------------------------------------

/// Paperclip `normalizeSkillPolicySourceType`。
pub fn normalize_skill_policy_source_type(source_type: Option<&str>) -> SkillPolicySourceType {
    match source_type.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) => match value.as_str() {
            "local_path" | "workspace" | "project_scan" => SkillPolicySourceType::Workspace,
            "catalog" | "bundled" | "optional" => SkillPolicySourceType::Catalog,
            "git" | "github" => SkillPolicySourceType::Git,
            "skills_sh" | "external_package" | "npm" => SkillPolicySourceType::ExternalPackage,
            "generated" => SkillPolicySourceType::Generated,
            _ => SkillPolicySourceType::Unknown,
        },
        None => SkillPolicySourceType::Unknown,
    }
}

/// Paperclip `normalizeSkillPolicySourceLocator`：repo 形态的 https URL 会被
/// 规整为 `https://{host}/{owner}/{repo}{/suffix}`（host/owner/repo 小写、
/// 去掉 `.git`），其余原样返回。
pub fn normalize_skill_policy_source_locator(value: &str) -> String {
    let trimmed = value.trim();
    let Ok(url) = reqwest::Url::parse(trimmed) else {
        return trimmed.to_string();
    };
    if url.scheme() != "https" && url.scheme() != "http" {
        return trimmed.to_string();
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let hostname = if host == "www.github.com" {
        "github.com".to_string()
    } else {
        host
    };
    let segments: Vec<&str> = url
        .path()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let path = url.path();
    let is_repo_style = url.scheme() == "https"
        && !hostname.ends_with(".githubusercontent.com")
        && hostname != "gist.github.com"
        && segments.len() >= 2
        && !path.ends_with(".md")
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().unwrap_or("").is_empty()
        && url.fragment().unwrap_or("").is_empty();
    if !is_repo_style {
        return url.to_string();
    }
    let owner = segments[0].to_ascii_lowercase();
    let repo = segments[1]
        .trim_end_matches(".git")
        .trim_end_matches(".GIT")
        .to_ascii_lowercase();
    let suffix = segments[2..].join("/");
    if suffix.is_empty() {
        format!("https://{hostname}/{owner}/{repo}")
    } else {
        format!("https://{hostname}/{owner}/{repo}/{suffix}")
    }
}

/// Paperclip `credentialParameter = /token|secret|password|api[-_]?key|authorization/i`。
static CREDENTIAL_PARAMETER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)token|secret|password|api[-_]?key|authorization")
        .expect("credential parameter regex")
});

/// Paperclip `isSafeSourceLocator` 的首个正则 `:\/\/[^/@\s]+:[^/@\s]+@`。
static URL_USERINFO_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"://[^/@\s]+:[^/@\s]+@").expect("url userinfo regex"));

/// fragment 内的凭据参数：`(?:^|[?&;])(?:token|secret|password|api[-_]?key|authorization)=`
static FRAGMENT_CREDENTIAL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[?&;])(?:token|secret|password|api[-_]?key|authorization)=")
        .expect("fragment credential regex")
});

/// Paperclip 规则 id 正则 `^[a-zA-Z0-9][a-zA-Z0-9._:-]*$`。
static RULE_ID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9._:-]*$").expect("rule id regex"));

/// Paperclip `isSafeSourceLocator`：拒绝带凭据的 URL 与带密钥类 query/fragment
/// 参数；无法解析为 URL 的字符串一律放行。
pub fn is_safe_source_locator(value: &str) -> bool {
    if URL_USERINFO_REGEX.is_match(value) {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(value) else {
        return true;
    };
    if !url.username().is_empty() || url.password().is_some_and(|p| !p.is_empty()) {
        return false;
    }
    if url
        .query_pairs()
        .any(|(key, _)| CREDENTIAL_PARAMETER_REGEX.is_match(&key))
    {
        return false;
    }
    let fragment = url.fragment().unwrap_or_default();
    !FRAGMENT_CREDENTIAL_REGEX.is_match(fragment)
}

// ---------------------------------------------------------------------------
// zod 形状的 issues
// ---------------------------------------------------------------------------

fn issue(code: &str, path: &[Value], message: impl Into<String>) -> Value {
    json!({
        "code": code,
        "path": path,
        "message": message.into(),
    })
}

fn child(path: &[Value], key: &str) -> Vec<Value> {
    let mut next = path.to_vec();
    next.push(Value::from(key));
    next
}

fn indexed(path: &[Value], index: usize) -> Vec<Value> {
    let mut next = path.to_vec();
    next.push(Value::from(index as u64));
    next
}

fn check_unknown_keys(value: &Value, allowed: &[&str], path: &[Value], issues: &mut Vec<Value>) {
    let Some(object) = value.as_object() else {
        return;
    };
    let unknown: Vec<String> = object
        .keys()
        .filter(|key| !allowed.contains(&key.as_str()))
        .cloned()
        .collect();
    if unknown.is_empty() {
        return;
    }
    let mut entry = issue(
        "unrecognized_keys",
        path,
        format!("Unrecognized key(s) in object: {}", unknown.join(", ")),
    );
    entry["keys"] = json!(unknown);
    issues.push(entry);
}

/// `nonEmptyUniqueStrings`：trim 后 1..=max_len，数组 1..=max_entries，去重。
fn parse_string_list(
    value: &Value,
    path: &[Value],
    max_entries: usize,
    max_len: usize,
    issues: &mut Vec<Value>,
) -> Vec<String> {
    let Some(entries) = value.as_array() else {
        issues.push(issue("invalid_type", path, "Expected array"));
        return Vec::new();
    };
    if entries.is_empty() {
        issues.push(issue(
            "too_small",
            path,
            "Array must contain at least 1 element(s)",
        ));
    }
    if entries.len() > max_entries {
        issues.push(issue(
            "too_big",
            path,
            format!("Array must contain at most {max_entries} element(s)"),
        ));
    }
    let mut parsed = Vec::with_capacity(entries.len());
    let mut seen: HashSet<String> = HashSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let entry_path = indexed(path, index);
        let Some(raw) = entry.as_str() else {
            issues.push(issue("invalid_type", &entry_path, "Expected string"));
            continue;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            issues.push(issue(
                "too_small",
                &entry_path,
                "String must contain at least 1 character(s)",
            ));
            continue;
        }
        if trimmed.chars().count() > max_len {
            issues.push(issue(
                "too_big",
                &entry_path,
                format!("String must contain at most {max_len} character(s)"),
            ));
            continue;
        }
        if !seen.insert(trimmed.to_string()) {
            issues.push(issue("custom", &entry_path, "Values must be unique"));
            continue;
        }
        parsed.push(trimmed.to_string());
    }
    parsed
}

fn parse_uuid_list(
    value: &Value,
    path: &[Value],
    max_entries: usize,
    issues: &mut Vec<Value>,
) -> Vec<Uuid> {
    let Some(entries) = value.as_array() else {
        issues.push(issue("invalid_type", path, "Expected array"));
        return Vec::new();
    };
    if entries.is_empty() {
        issues.push(issue(
            "too_small",
            path,
            "Array must contain at least 1 element(s)",
        ));
    }
    if entries.len() > max_entries {
        issues.push(issue(
            "too_big",
            path,
            format!("Array must contain at most {max_entries} element(s)"),
        ));
    }
    let mut parsed = Vec::with_capacity(entries.len());
    let mut seen: HashSet<Uuid> = HashSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let entry_path = indexed(path, index);
        let parsed_id = entry.as_str().and_then(|raw| Uuid::parse_str(raw).ok());
        match parsed_id {
            Some(id) => {
                if !seen.insert(id) {
                    issues.push(issue("custom", &entry_path, "Values must be unique"));
                    continue;
                }
                parsed.push(id);
            }
            None => issues.push(issue("invalid_string", &entry_path, "Invalid uuid")),
        }
    }
    parsed
}

fn parse_source_locator(raw: &str, path: &[Value], issues: &mut Vec<Value>) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        issues.push(issue(
            "too_small",
            path,
            "String must contain at least 1 character(s)",
        ));
        return None;
    }
    if trimmed.chars().count() > MAX_LOCATOR_LEN {
        issues.push(issue(
            "too_big",
            path,
            format!("String must contain at most {MAX_LOCATOR_LEN} character(s)"),
        ));
        return None;
    }
    if !is_safe_source_locator(trimmed) {
        issues.push(issue(
            "custom",
            path,
            "Source locators must not contain credentials or secret query or fragment parameters",
        ));
        return None;
    }
    Some(normalize_skill_policy_source_locator(trimmed))
}

fn parse_subject(value: &Value, path: &[Value], issues: &mut Vec<Value>) -> SkillPolicySubject {
    let Some(object) = value.as_object() else {
        issues.push(issue("invalid_type", path, "Expected object"));
        return SkillPolicySubject::AllAgents;
    };
    match object.get("type").and_then(Value::as_str) {
        Some("all_agents") => {
            check_unknown_keys(value, &["type"], path, issues);
            SkillPolicySubject::AllAgents
        }
        Some("agents") => {
            check_unknown_keys(value, &["type", "agentIds"], path, issues);
            match object.get("agentIds") {
                Some(agent_ids) => SkillPolicySubject::Agents {
                    agent_ids: parse_uuid_list(
                        agent_ids,
                        &child(path, "agentIds"),
                        MAX_SELECTOR_ENTRIES,
                        issues,
                    ),
                },
                None => {
                    issues.push(issue(
                        "invalid_type",
                        &child(path, "agentIds"),
                        "Required",
                    ));
                    SkillPolicySubject::Agents {
                        agent_ids: Vec::new(),
                    }
                }
            }
        }
        Some("roles") => {
            check_unknown_keys(value, &["type", "roles"], path, issues);
            match object.get("roles") {
                Some(roles) => SkillPolicySubject::Roles {
                    roles: parse_string_list(
                        roles,
                        &child(path, "roles"),
                        MAX_SELECTOR_ENTRIES,
                        MAX_SKILL_KEY_LEN,
                        issues,
                    ),
                },
                None => {
                    issues.push(issue("invalid_type", &child(path, "roles"), "Required"));
                    SkillPolicySubject::Roles { roles: Vec::new() }
                }
            }
        }
        Some(other) => {
            issues.push(issue(
                "invalid_union_discriminator",
                &child(path, "type"),
                format!("Invalid discriminator value. Expected 'all_agents' | 'agents' | 'roles' (received '{other}')"),
            ));
            SkillPolicySubject::AllAgents
        }
        None => {
            issues.push(issue("invalid_type", &child(path, "type"), "Required"));
            SkillPolicySubject::AllAgents
        }
    }
}

fn parse_resource_selector(
    value: &Value,
    path: &[Value],
    issues: &mut Vec<Value>,
) -> SkillPolicyResourceSelector {
    let Some(object) = value.as_object() else {
        issues.push(issue("invalid_type", path, "Expected object"));
        return SkillPolicyResourceSelector::default();
    };
    check_unknown_keys(
        value,
        &["skillIds", "skillKeys", "sourceTypes", "sourceLocators"],
        path,
        issues,
    );
    if object.is_empty() {
        issues.push(issue(
            "custom",
            path,
            "At least one resource selector is required",
        ));
    }
    let mut selector = SkillPolicyResourceSelector::default();
    if let Some(skill_ids) = object.get("skillIds") {
        selector.skill_ids = Some(parse_uuid_list(
            skill_ids,
            &child(path, "skillIds"),
            MAX_SELECTOR_ENTRIES,
            issues,
        ));
    }
    if let Some(skill_keys) = object.get("skillKeys") {
        selector.skill_keys = Some(parse_string_list(
            skill_keys,
            &child(path, "skillKeys"),
            MAX_SELECTOR_ENTRIES,
            MAX_SKILL_KEY_LEN,
            issues,
        ));
    }
    if let Some(source_types) = object.get("sourceTypes") {
        let types_path = child(path, "sourceTypes");
        let mut parsed = Vec::new();
        let mut seen: HashSet<SkillPolicySourceType> = HashSet::new();
        match source_types.as_array() {
            None => issues.push(issue("invalid_type", &types_path, "Expected array")),
            Some(entries) => {
                if entries.is_empty() {
                    issues.push(issue(
                        "too_small",
                        &types_path,
                        "Array must contain at least 1 element(s)",
                    ));
                }
                for (index, entry) in entries.iter().enumerate() {
                    let entry_path = indexed(&types_path, index);
                    match entry.as_str().and_then(SkillPolicySourceType::parse) {
                        Some(source_type) => {
                            if !seen.insert(source_type) {
                                issues.push(issue("custom", &entry_path, "Source types must be unique"));
                                continue;
                            }
                            parsed.push(source_type);
                        }
                        None => issues.push(issue(
                            "invalid_enum_value",
                            &entry_path,
                            "Invalid enum value",
                        )),
                    }
                }
            }
        }
        selector.source_types = Some(parsed);
    }
    if let Some(source_locators) = object.get("sourceLocators") {
        let locators_path = child(path, "sourceLocators");
        let mut parsed = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        match source_locators.as_array() {
            None => issues.push(issue("invalid_type", &locators_path, "Expected array")),
            Some(entries) => {
                if entries.is_empty() {
                    issues.push(issue(
                        "too_small",
                        &locators_path,
                        "Array must contain at least 1 element(s)",
                    ));
                }
                if entries.len() > MAX_SELECTOR_ENTRIES {
                    issues.push(issue(
                        "too_big",
                        &locators_path,
                        format!("Array must contain at most {MAX_SELECTOR_ENTRIES} element(s)"),
                    ));
                }
                for (index, entry) in entries.iter().enumerate() {
                    let entry_path = indexed(&locators_path, index);
                    let Some(raw) = entry.as_str() else {
                        issues.push(issue("invalid_type", &entry_path, "Expected string"));
                        continue;
                    };
                    let Some(normalized) = parse_source_locator(raw, &entry_path, issues) else {
                        continue;
                    };
                    if !seen.insert(normalized.clone()) {
                        issues.push(issue(
                            "custom",
                            &entry_path,
                            "Source locators must be unique",
                        ));
                        continue;
                    }
                    parsed.push(normalized);
                }
            }
        }
        selector.source_locators = Some(parsed);
    }
    selector
}

fn parse_rule(value: &Value, index: usize, issues: &mut Vec<Value>) -> Option<SkillPolicyRule> {
    let base: Vec<Value> = vec![Value::from("rules"), Value::from(index as u64)];
    let Some(object) = value.as_object() else {
        issues.push(issue("invalid_type", &base, "Expected object"));
        return None;
    };
    let before = issues.len();
    check_unknown_keys(
        value,
        &["id", "priority", "effect", "subject", "actions", "resources"],
        &base,
        issues,
    );

    let id = match object.get("id") {
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            let id_path = child(&base, "id");
            if trimmed.is_empty() {
                issues.push(issue(
                    "too_small",
                    &id_path,
                    "String must contain at least 1 character(s)",
                ));
            } else if trimmed.chars().count() > MAX_RULE_ID_LEN {
                issues.push(issue(
                    "too_big",
                    &id_path,
                    format!("String must contain at most {MAX_RULE_ID_LEN} character(s)"),
                ));
            } else if !RULE_ID_REGEX.is_match(trimmed) {
                issues.push(issue("invalid_string", &id_path, "Invalid"));
            }
            trimmed.to_string()
        }
        Some(_) => {
            issues.push(issue("invalid_type", &child(&base, "id"), "Expected string"));
            return None;
        }
        None => {
            issues.push(issue("invalid_type", &child(&base, "id"), "Required"));
            return None;
        }
    };

    let priority = match object.get("priority") {
        Some(value) => {
            let priority_path = child(&base, "priority");
            match value.as_i64() {
                Some(priority) if (MIN_PRIORITY..=MAX_PRIORITY).contains(&priority) => priority,
                Some(priority) if priority < MIN_PRIORITY => {
                    issues.push(issue(
                        "too_small",
                        &priority_path,
                        format!("Number must be greater than or equal to {MIN_PRIORITY}"),
                    ));
                    return None;
                }
                Some(_) => {
                    issues.push(issue(
                        "too_big",
                        &priority_path,
                        format!("Number must be less than or equal to {MAX_PRIORITY}"),
                    ));
                    return None;
                }
                None => {
                    issues.push(issue("invalid_type", &priority_path, "Expected integer"));
                    return None;
                }
            }
        }
        None => {
            issues.push(issue("invalid_type", &child(&base, "priority"), "Required"));
            return None;
        }
    };

    let effect = match object.get("effect").and_then(Value::as_str) {
        Some("allow") => SkillPolicyEffect::Allow,
        Some("deny") => SkillPolicyEffect::Deny,
        Some(_) => {
            issues.push(issue(
                "invalid_enum_value",
                &child(&base, "effect"),
                "Invalid enum value. Expected 'allow' | 'deny'",
            ));
            return None;
        }
        None => {
            issues.push(issue("invalid_type", &child(&base, "effect"), "Required"));
            return None;
        }
    };

    let subject = match object.get("subject") {
        Some(subject) => parse_subject(subject, &child(&base, "subject"), issues),
        None => {
            issues.push(issue("invalid_type", &child(&base, "subject"), "Required"));
            return None;
        }
    };

    let actions = match object.get("actions") {
        Some(value) => {
            let actions_path = child(&base, "actions");
            let mut parsed = Vec::new();
            let mut seen: HashSet<SkillPolicyAction> = HashSet::new();
            match value.as_array() {
                None => {
                    issues.push(issue("invalid_type", &actions_path, "Expected array"));
                    Vec::new()
                }
                Some(entries) => {
                    if entries.is_empty() {
                        issues.push(issue(
                            "too_small",
                            &actions_path,
                            "Array must contain at least 1 element(s)",
                        ));
                    }
                    for (action_index, entry) in entries.iter().enumerate() {
                        let entry_path = indexed(&actions_path, action_index);
                        match entry.as_str().and_then(SkillPolicyAction::parse) {
                            Some(action) => {
                                if !seen.insert(action) {
                                    issues.push(issue(
                                        "custom",
                                        &entry_path,
                                        "Actions must be unique",
                                    ));
                                    continue;
                                }
                                parsed.push(action);
                            }
                            None => issues.push(issue(
                                "invalid_enum_value",
                                &entry_path,
                                "Invalid enum value",
                            )),
                        }
                    }
                    parsed
                }
            }
        }
        None => {
            issues.push(issue("invalid_type", &child(&base, "actions"), "Required"));
            return None;
        }
    };

    let resources = object
        .get("resources")
        .map(|value| parse_resource_selector(value, &child(&base, "resources"), issues));

    if issues.len() > before {
        return None;
    }

    Some(SkillPolicyRule {
        id,
        priority,
        effect,
        subject,
        actions,
        resources,
    })
}

/// 解析文档（允许 `extra_keys` 出现但不纳入文档，用于 PUT 的 `expectedRevision`）。
fn parse_document_into(
    value: &Value,
    extra_keys: &[&str],
    issues: &mut Vec<Value>,
) -> SkillPolicyDocument {
    let Some(object) = value.as_object() else {
        issues.push(issue("invalid_type", &[], "Expected object"));
        return SkillPolicyDocument {
            schema_version: SKILL_POLICY_SCHEMA_VERSION,
            default_effect: SkillPolicyEffect::Allow,
            rules: Vec::new(),
        };
    };
    let mut allowed = vec!["schemaVersion", "defaultEffect", "rules"];
    allowed.extend_from_slice(extra_keys);
    check_unknown_keys(value, &allowed, &[], issues);

    let schema_version = match object.get("schemaVersion") {
        Some(value) if value.as_i64() == Some(SKILL_POLICY_SCHEMA_VERSION as i64) => {
            SKILL_POLICY_SCHEMA_VERSION
        }
        Some(_) => {
            issues.push(issue(
                "invalid_literal",
                &[Value::from("schemaVersion")],
                "Invalid literal value, expected 1",
            ));
            SKILL_POLICY_SCHEMA_VERSION
        }
        None => {
            issues.push(issue(
                "invalid_type",
                &[Value::from("schemaVersion")],
                "Required",
            ));
            SKILL_POLICY_SCHEMA_VERSION
        }
    };

    let default_effect = match object.get("defaultEffect").and_then(Value::as_str) {
        Some("allow") => SkillPolicyEffect::Allow,
        Some("deny") => SkillPolicyEffect::Deny,
        Some(_) => {
            issues.push(issue(
                "invalid_enum_value",
                &[Value::from("defaultEffect")],
                "Invalid enum value. Expected 'allow' | 'deny'",
            ));
            SkillPolicyEffect::Allow
        }
        None => {
            issues.push(issue(
                "invalid_type",
                &[Value::from("defaultEffect")],
                "Required",
            ));
            SkillPolicyEffect::Allow
        }
    };

    let rules = match object.get("rules") {
        None => {
            issues.push(issue("invalid_type", &[Value::from("rules")], "Required"));
            Vec::new()
        }
        Some(Value::Array(entries)) => {
            if entries.len() > MAX_RULES {
                issues.push(issue(
                    "too_big",
                    &[Value::from("rules")],
                    format!("Array must contain at most {MAX_RULES} element(s)"),
                ));
            }
            let mut parsed = Vec::with_capacity(entries.len());
            let mut seen_ids: HashSet<String> = HashSet::new();
            for (index, entry) in entries.iter().enumerate() {
                let Some(rule) = parse_rule(entry, index, issues) else {
                    continue;
                };
                if !seen_ids.insert(rule.id.clone()) {
                    issues.push(issue(
                        "custom",
                        &indexed(&[Value::from("rules")], index),
                        "Rule IDs must be unique",
                    ));
                    continue;
                }
                parsed.push(rule);
            }
            parsed
        }
        Some(_) => {
            issues.push(issue(
                "invalid_type",
                &[Value::from("rules")],
                "Expected array",
            ));
            Vec::new()
        }
    };

    SkillPolicyDocument {
        schema_version,
        default_effect,
        rules,
    }
}

/// `skillPolicyDocumentSchema`（`.strict()`）。
pub fn parse_skill_policy_document(value: &Value) -> Result<SkillPolicyDocument, Vec<Value>> {
    let mut issues = Vec::new();
    let document = parse_document_into(value, &[], &mut issues);
    if issues.is_empty() {
        Ok(document)
    } else {
        Err(issues)
    }
}

/// `replaceSkillPolicySchema`：文档 + `expectedRevision`。
pub fn parse_replace_skill_policy_request(
    value: &Value,
) -> Result<(SkillPolicyDocument, i32), Vec<Value>> {
    let mut issues = Vec::new();
    let document = parse_document_into(value, &["expectedRevision"], &mut issues);
    let expected_path = [Value::from("expectedRevision")];
    let expected_revision = match value.get("expectedRevision") {
        Some(raw) => match raw.as_i64() {
            Some(revision) if (0..=i32::MAX as i64).contains(&revision) => revision as i32,
            Some(_) => {
                issues.push(issue(
                    "too_small",
                    &expected_path,
                    "Number must be greater than or equal to 0",
                ));
                0
            }
            None => {
                issues.push(issue("invalid_type", &expected_path, "Expected integer"));
                0
            }
        },
        None => {
            issues.push(issue("invalid_type", &expected_path, "Required"));
            0
        }
    };
    if issues.is_empty() {
        Ok((document, expected_revision))
    } else {
        Err(issues)
    }
}

/// `skillPolicyEvaluationResourceSchema`（`.strict()`）。
pub fn parse_skill_policy_evaluation_resource(
    value: &Value,
    path: &[Value],
    issues: &mut Vec<Value>,
) -> SkillPolicyEvaluationResource {
    let Some(object) = value.as_object() else {
        issues.push(issue("invalid_type", path, "Expected object"));
        return SkillPolicyEvaluationResource::default();
    };
    check_unknown_keys(
        value,
        &["skillId", "skillKey", "sourceType", "sourceLocator"],
        path,
        issues,
    );
    let mut resource = SkillPolicyEvaluationResource::default();
    if let Some(raw) = object.get("skillId") {
        match raw.as_str().and_then(|value| Uuid::parse_str(value).ok()) {
            Some(id) => resource.skill_id = Some(id),
            None => issues.push(issue("invalid_string", &child(path, "skillId"), "Invalid uuid")),
        }
    }
    if let Some(raw) = object.get("skillKey") {
        let key_path = child(path, "skillKey");
        match raw.as_str() {
            None => issues.push(issue("invalid_type", &key_path, "Expected string")),
            Some(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    issues.push(issue(
                        "too_small",
                        &key_path,
                        "String must contain at least 1 character(s)",
                    ));
                } else if trimmed.chars().count() > MAX_SKILL_KEY_LEN {
                    issues.push(issue(
                        "too_big",
                        &key_path,
                        format!("String must contain at most {MAX_SKILL_KEY_LEN} character(s)"),
                    ));
                } else {
                    resource.skill_key = Some(trimmed.to_string());
                }
            }
        }
    }
    if let Some(raw) = object.get("sourceType") {
        let source_path = child(path, "sourceType");
        match raw.as_str().and_then(SkillPolicySourceType::parse) {
            Some(source_type) => resource.source_type = Some(source_type),
            None => issues.push(issue("invalid_enum_value", &source_path, "Invalid enum value")),
        }
    }
    if let Some(raw) = object.get("sourceLocator") {
        let locator_path = child(path, "sourceLocator");
        match raw.as_str() {
            None => issues.push(issue("invalid_type", &locator_path, "Expected string")),
            Some(value) => {
                resource.source_locator = parse_source_locator(value, &locator_path, issues);
            }
        }
    }
    resource
}

#[derive(Debug, Clone)]
pub struct EvaluateSkillPolicyRequest {
    pub action: SkillPolicyAction,
    pub resource: SkillPolicyEvaluationResource,
    /// `principal.agentId`。
    pub principal_agent_id: Option<Uuid>,
}

/// `evaluateSkillPolicySchema`（`.strict()`）。
pub fn parse_evaluate_skill_policy_request(
    value: &Value,
) -> Result<EvaluateSkillPolicyRequest, Vec<Value>> {
    let mut issues = Vec::new();
    let Some(object) = value.as_object() else {
        return Err(vec![issue("invalid_type", &[], "Expected object")]);
    };
    check_unknown_keys(value, &["action", "resource", "principal"], &[], &mut issues);

    let action = match object.get("action").and_then(Value::as_str).and_then(SkillPolicyAction::parse)
    {
        Some(action) => action,
        None => {
            issues.push(issue(
                "invalid_enum_value",
                &[Value::from("action")],
                "Invalid enum value",
            ));
            SkillPolicyAction::Create
        }
    };

    let resource = match object.get("resource") {
        None => SkillPolicyEvaluationResource::default(),
        Some(raw) => {
            parse_skill_policy_evaluation_resource(raw, &[Value::from("resource")], &mut issues)
        }
    };

    let principal_agent_id = match object.get("principal") {
        None => None,
        Some(raw) => {
            let path = [Value::from("principal")];
            match raw.as_object() {
                None => {
                    issues.push(issue("invalid_type", &path, "Expected object"));
                    None
                }
                Some(principal) => {
                    check_unknown_keys(raw, &["agentId"], &path, &mut issues);
                    match principal
                        .get("agentId")
                        .and_then(Value::as_str)
                        .and_then(|value| Uuid::parse_str(value).ok())
                    {
                        Some(agent_id) => Some(agent_id),
                        None => {
                            issues.push(issue(
                                "invalid_string",
                                &child(&path, "agentId"),
                                "Invalid uuid",
                            ));
                            None
                        }
                    }
                }
            }
        }
    };

    if issues.is_empty() {
        Ok(EvaluateSkillPolicyRequest {
            action,
            resource,
            principal_agent_id,
        })
    } else {
        Err(issues)
    }
}

// ---------------------------------------------------------------------------
// 匹配
// ---------------------------------------------------------------------------

fn normalize_role(role: Option<&str>) -> Option<String> {
    role.map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn subject_matches(rule: &SkillPolicyRule, principal: &SkillPolicyPrincipal) -> bool {
    match &rule.subject {
        SkillPolicySubject::AllAgents => {
            principal.principal_type == SkillPolicyPrincipalType::Agent
        }
        SkillPolicySubject::Agents { agent_ids } => {
            principal.principal_type == SkillPolicyPrincipalType::Agent
                && Uuid::parse_str(&principal.id)
                    .map(|id| agent_ids.contains(&id))
                    .unwrap_or(false)
        }
        SkillPolicySubject::Roles { roles } => {
            let Some(role) = normalize_role(principal.role.as_deref()) else {
                return false;
            };
            roles
                .iter()
                .any(|candidate| normalize_role(Some(candidate.as_str())).as_deref() == Some(&role))
        }
    }
}

fn resource_matches(rule: &SkillPolicyRule, resource: &SkillPolicyEvaluationResource) -> bool {
    let Some(selector) = rule.resources.as_ref() else {
        return true;
    };
    if let Some(skill_ids) = selector.skill_ids.as_ref() {
        match resource.skill_id {
            Some(id) if skill_ids.contains(&id) => {}
            _ => return false,
        }
    }
    if let Some(skill_keys) = selector.skill_keys.as_ref() {
        match resource.skill_key.as_ref() {
            Some(key) if skill_keys.contains(key) => {}
            _ => return false,
        }
    }
    if let Some(source_types) = selector.source_types.as_ref() {
        match resource.source_type {
            Some(source_type) if source_types.contains(&source_type) => {}
            _ => return false,
        }
    }
    if let Some(source_locators) = selector.source_locators.as_ref() {
        // 两侧都做规范化：规范化之前写入的规则仍是原始形态，调用方也可能传未
        // 规范化的资源；混合形态下严格相等会让 deny 规则静默失配。
        let Some(locator) = resource
            .source_locator
            .as_deref()
            .map(normalize_skill_policy_source_locator)
        else {
            return false;
        };
        if !source_locators
            .iter()
            .any(|candidate| normalize_skill_policy_source_locator(candidate) == locator)
        {
            return false;
        }
    }
    true
}

fn decision(
    allowed: bool,
    action: SkillPolicyAction,
    reason: SkillPolicyDecisionReason,
    revision: i32,
    matched_rule_id: Option<String>,
) -> SkillPolicyDecision {
    SkillPolicyDecision {
        allowed,
        action,
        reason,
        policy_revision: revision,
        matched_rule_id,
        remediation: if allowed {
            None
        } else {
            Some("Contact a company administrator to change the skill policy.".to_string())
        },
    }
}

async fn insert_activity(
    tx: &mut Transaction<'_, Postgres>,
    activity: &SkillPolicyActivity,
    company_id: Uuid,
    event_type: &str,
    details: Value,
) -> Result<(), sqlx::Error> {
    // activity_logs.resource_id 为 UUID NOT NULL：company 级实体用 company_id，
    // 与 `teams_catalog_service` 的 company 级审计一致。
    sqlx::query(
        "INSERT INTO activity_logs (\
             id, company_id, event_type, actor_type, actor_id, \
             resource_type, resource_id, metadata, created_at, run_id, agent_id\
         ) VALUES ($1, $2, $3, $4, $5, 'company_skill_policy', $6, $7, NOW(), $8, $9)",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(event_type)
    .bind(&activity.actor_type)
    .bind(activity.actor_id)
    .bind(company_id)
    .bind(&details)
    .bind(activity.run_id)
    .bind(activity.agent_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[async_trait]
impl SkillPolicyService for DefaultSkillPolicyService {
    async fn get(&self, company_id: Uuid) -> SkillPolicyResult<EffectiveSkillPolicy> {
        let row = sqlx::query(
            "SELECT policy, version FROM company_skill_policies WHERE company_id = $1",
        )
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(open_default_policy());
        };
        let revision: i32 = row.get("version");
        let stored: Value = row.get("policy");
        let document = parse_skill_policy_document(&stored)
            .map_err(SkillPolicyError::CorruptPolicy)?;
        Ok(EffectiveSkillPolicy {
            schema_version: document.schema_version,
            revision,
            default_effect: document.default_effect,
            rules: document.rules,
            materialized: true,
        })
    }

    async fn resolve_agent_principal(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
    ) -> SkillPolicyResult<SkillPolicyPrincipal> {
        let row = sqlx::query("SELECT company_id, role FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Err(SkillPolicyError::AgentNotFound);
        };
        let agent_company: Uuid = row.get("company_id");
        if agent_company != company_id {
            return Err(SkillPolicyError::CompanyBoundaryDenied);
        }
        Ok(SkillPolicyPrincipal {
            principal_type: SkillPolicyPrincipalType::Agent,
            id: agent_id.to_string(),
            role: row.get::<Option<String>, _>("role"),
        })
    }

    async fn evaluate(
        &self,
        input: SkillPolicyEvaluateInput,
    ) -> SkillPolicyResult<SkillPolicyDecision> {
        let policy = self.get(input.company_id).await?;
        if !policy.materialized {
            return Ok(decision(
                true,
                input.action,
                SkillPolicyDecisionReason::NoPolicyDefault,
                policy.revision,
                None,
            ));
        }
        let mut rules = policy.rules.clone();
        rules.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| left.id.cmp(&right.id))
        });
        let matched = rules.iter().find(|rule| {
            rule.actions.contains(&input.action)
                && subject_matches(rule, &input.principal)
                && resource_matches(rule, &input.resource)
        });
        if let Some(rule) = matched {
            return Ok(decision(
                rule.effect == SkillPolicyEffect::Allow,
                input.action,
                SkillPolicyDecisionReason::ExplicitRule,
                policy.revision,
                Some(rule.id.clone()),
            ));
        }
        // 这两条历史 grant 曾授权整个 company-skill 变更面。仅在 default-deny
        // 下作为兼容回落保留；显式规则与平台不变量依旧优先。
        if policy.default_effect == SkillPolicyEffect::Deny
            && self
                .has_legacy_broad_mutation_grant(input.company_id, &input.principal)
                .await?
        {
            return Ok(decision(
                true,
                input.action,
                SkillPolicyDecisionReason::LegacyCompatibility,
                policy.revision,
                None,
            ));
        }
        Ok(decision(
            policy.default_effect == SkillPolicyEffect::Allow,
            input.action,
            SkillPolicyDecisionReason::PolicyDefault,
            policy.revision,
            None,
        ))
    }

    async fn replace(
        &self,
        input: SkillPolicyReplaceInput,
    ) -> SkillPolicyResult<EffectiveSkillPolicy> {
        let mut tx = self.pool.begin().await?;
        let current_revision: Option<i32> =
            sqlx::query_scalar("SELECT version FROM company_skill_policies WHERE company_id = $1")
                .bind(input.company_id)
                .fetch_optional(&mut *tx)
                .await?;
        let current_revision = current_revision.unwrap_or(0);
        if current_revision != input.expected_revision {
            return Err(SkillPolicyError::RevisionConflict {
                expected_revision: input.expected_revision,
                current_revision,
            });
        }
        let next_revision = current_revision + 1;
        let document = json!({
            "schemaVersion": input.policy.schema_version,
            "defaultEffect": input.policy.default_effect,
            "rules": input.policy.rules,
        });
        let stored_revision: Option<i32> = if current_revision == 0 {
            sqlx::query_scalar(
                "INSERT INTO company_skill_policies (company_id, policy, version, updated_at) \
                 VALUES ($1, $2, $3, NOW()) \
                 ON CONFLICT (company_id) DO NOTHING \
                 RETURNING version",
            )
            .bind(input.company_id)
            .bind(&document)
            .bind(next_revision)
            .fetch_optional(&mut *tx)
            .await?
        } else {
            sqlx::query_scalar(
                "UPDATE company_skill_policies \
                 SET policy = $2, version = $3, updated_at = NOW() \
                 WHERE company_id = $1 AND version = $4 \
                 RETURNING version",
            )
            .bind(input.company_id)
            .bind(&document)
            .bind(next_revision)
            .bind(current_revision)
            .fetch_optional(&mut *tx)
            .await?
        };
        if stored_revision.is_none() {
            return Err(SkillPolicyError::RevisionConflict {
                expected_revision: input.expected_revision,
                current_revision,
            });
        }
        insert_activity(
            &mut tx,
            &input.activity,
            input.company_id,
            "company.skill_policy_replaced",
            json!({
                "previousRevision": current_revision,
                "newRevision": next_revision,
                "defaultEffect": input.policy.default_effect,
                "ruleCount": input.policy.rules.len(),
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(EffectiveSkillPolicy {
            schema_version: input.policy.schema_version,
            revision: next_revision,
            default_effect: input.policy.default_effect,
            rules: input.policy.rules,
            materialized: true,
        })
    }

    async fn reset(&self, input: SkillPolicyResetInput) -> SkillPolicyResult<EffectiveSkillPolicy> {
        let mut tx = self.pool.begin().await?;
        let previous_revision: Option<i32> =
            sqlx::query_scalar("DELETE FROM company_skill_policies WHERE company_id = $1 RETURNING version")
                .bind(input.company_id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(previous_revision) = previous_revision {
            insert_activity(
                &mut tx,
                &input.activity,
                input.company_id,
                "company.skill_policy_reset",
                json!({ "previousRevision": previous_revision, "newRevision": 0 }),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(open_default_policy())
    }
}

impl DefaultSkillPolicyService {
    async fn has_legacy_broad_mutation_grant(
        &self,
        company_id: Uuid,
        principal: &SkillPolicyPrincipal,
    ) -> SkillPolicyResult<bool> {
        let principal_type = match principal.principal_type {
            SkillPolicyPrincipalType::Agent => "agent",
            SkillPolicyPrincipalType::Board => "user",
        };
        let Ok(principal_id) = Uuid::parse_str(&principal.id) else {
            return Ok(false);
        };
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(\
                 SELECT 1 FROM principal_permission_grants \
                 WHERE company_id = $1 \
                   AND principal_type = $2::principal_type \
                   AND principal_id = $3 \
                   AND permission_key IN ('skills:create', 'skills:suggest-changes')\
             )",
        )
        .bind(company_id)
        .bind(principal_type)
        .bind(principal_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }
}
