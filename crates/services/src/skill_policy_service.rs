use async_trait::async_trait;
use repositories::company_skill_policy_repository::{
    CompanySkillPolicyRepository, CompanySkillPolicyRow,
};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SkillPolicyError {
    #[error("repository error: {0}")]
    Repository(#[from] repositories::company_skill_policy_repository::SkillPolicyRepositoryError),
    #[error("invalid policy: {0}")]
    InvalidPolicy(String),
}

pub type SkillPolicyResult<T> = Result<T, SkillPolicyError>;

/// Policy 判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub reason: String,
    /// 区分「平台安全拒绝」与「策略拒绝」，调用方据此返回不同语义。
    pub denial_type: DenialType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialType {
    /// 由 company skill policy 规则拒绝。
    Policy,
    /// 由平台安全层（如受保护 skill / 系统保留）拒绝，与策略无关。
    Platform,
}

/// 拒绝来源，用于契约返回。
impl DenialType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DenialType::Policy => "policy",
            DenialType::Platform => "platform",
        }
    }
}

/// 受平台保护的 skill（无论 policy 如何均禁止 mutation/执行）。
pub const PROTECTED_SKILLS: &[&str] = &["system", "internal", "core", "platform"];

#[async_trait]
pub trait SkillPolicyService: Send + Sync {
    /// 评估某次 skill 操作是否被允许。
    async fn evaluate(
        &self,
        company_id: Uuid,
        agent_id: Option<Uuid>,
        role: &str,
        action: &str,
        source: &str,
        skill_key: &str,
    ) -> SkillPolicyResult<PolicyDecision>;

    /// 读取 company 当前 policy（None 表示默认开放）。
    async fn get_policy(&self, company_id: Uuid) -> SkillPolicyResult<Option<Value>>;

    /// 写入/更新 company policy（做结构校验与 version 自增）。
    async fn set_policy(&self, company_id: Uuid, policy: Value) -> SkillPolicyResult<Value>;

    /// 删除 company policy（恢复默认开放）。
    async fn delete_policy(&self, company_id: Uuid) -> SkillPolicyResult<()>;

    /// 模拟一次评估（不落库），返回结构化结果，便于管理端预览。
    async fn simulate(
        &self,
        company_id: Uuid,
        agent_id: Option<Uuid>,
        role: &str,
        action: &str,
        source: &str,
        skill_key: &str,
    ) -> SkillPolicyResult<Value>;
}

/// policy 模式：defaultOpen / defaultDeny / allowList。
fn policy_mode(policy: &Value) -> &str {
    policy
        .get("mode")
        .and_then(|m| m.as_str())
        .unwrap_or("defaultOpen")
}

/// 判断 skill_key 是否在受保护集合中。
pub fn is_protected_skill(skill_key: &str) -> bool {
    let key = skill_key.to_ascii_lowercase();
    PROTECTED_SKILLS.iter().any(|p| key == *p || key.starts_with(&format!("{p}.")) || key.starts_with(&format!("{p}_")))
}

/// 纯函数：基于 policy JSON 评估是否允许（不含公司读取，便于测试）。
pub fn evaluate_against_policy(
    policy: &Value,
    role: &str,
    action: &str,
    source: &str,
    skill_key: &str,
) -> bool {
    // 受保护 skill 永远禁止（平台安全），由调用方在 evaluate 中优先判定。
    match policy_mode(policy) {
        "defaultDeny" => {
            // 仅 allowRules 命中才允许
            allow_rules_match(policy, role, action, source, skill_key)
        }
        "allowList" => {
            // 白名单：skill 必须在 allowedSkills 中，且 allowRules（若有）不拒绝
            let in_allow = policy
                .get("allowedSkills")
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().any(|s| s.as_str() == Some(skill_key)))
                .unwrap_or(false);
            in_allow && allow_rules_match(policy, role, action, source, skill_key)
        }
        _ => {
            // defaultOpen：默认允许，但若存在 denyRules 命中则拒绝
            !deny_rules_match(policy, role, action, source, skill_key)
        }
    }
}

fn allow_rules_match(policy: &Value, role: &str, action: &str, source: &str, skill_key: &str) -> bool {
    policy
        .get("allowRules")
        .and_then(|r| r.as_array())
        .map(|rules| {
            rules.iter().any(|rule| rule_matches(rule, role, action, source, skill_key))
        })
        .unwrap_or(false)
}

fn deny_rules_match(policy: &Value, role: &str, action: &str, source: &str, skill_key: &str) -> bool {
    policy
        .get("denyRules")
        .and_then(|r| r.as_array())
        .map(|rules| {
            rules.iter().any(|rule| rule_matches(rule, role, action, source, skill_key))
        })
        .unwrap_or(false)
}

/// 单条规则匹配：支持 role/action/source/skill 字段（任一缺失视为通配）。
fn rule_matches(rule: &Value, role: &str, action: &str, source: &str, skill_key: &str) -> bool {
    let role_ok = rule
        .get("role")
        .and_then(|v| v.as_str())
        .map(|r| r == role)
        .unwrap_or(true);
    let action_ok = rule
        .get("action")
        .and_then(|v| v.as_str())
        .map(|a| a == action)
        .unwrap_or(true);
    let source_ok = rule
        .get("source")
        .and_then(|v| v.as_str())
        .map(|s| s == source)
        .unwrap_or(true);
    let skill_ok = rule
        .get("skill")
        .and_then(|v| v.as_str())
        .map(|s| s == skill_key)
        .unwrap_or(true);
    role_ok && action_ok && source_ok && skill_ok
}

/// 校验 policy 结构合法性（mode 取值、rules/skills 为数组）。
pub fn validate_policy(policy: &Value) -> SkillPolicyResult<()> {
    if !policy.is_object() {
        return Err(SkillPolicyError::InvalidPolicy("policy must be an object".into()));
    }
    match policy.get("mode").and_then(|m| m.as_str()) {
        None | Some("defaultOpen") | Some("defaultDeny") | Some("allowList") => {}
        Some(other) => {
            return Err(SkillPolicyError::InvalidPolicy(format!(
                "unknown policy mode '{}' (expected defaultOpen|defaultDeny|allowList)",
                other
            )));
        }
    }
    for key in ["allowRules", "denyRules", "allowedSkills"] {
        if let Some(v) = policy.get(key) {
            if !v.is_array() {
                return Err(SkillPolicyError::InvalidPolicy(format!(
                    "'{key}' must be an array"
                )));
            }
        }
    }
    Ok(())
}

pub struct DefaultSkillPolicyService {
    repo: std::sync::Arc<dyn CompanySkillPolicyRepository>,
}

impl DefaultSkillPolicyService {
    pub fn new(repo: std::sync::Arc<dyn CompanySkillPolicyRepository>) -> Self {
        Self { repo }
    }

    fn to_value(row: &CompanySkillPolicyRow) -> Value {
        json!({
            "companyId": row.company_id,
            "policy": row.policy,
            "version": row.version,
            "createdAt": row.created_at,
            "updatedAt": row.updated_at,
        })
    }
}

#[async_trait]
impl SkillPolicyService for DefaultSkillPolicyService {
    async fn evaluate(
        &self,
        company_id: Uuid,
        _agent_id: Option<Uuid>,
        role: &str,
        action: &str,
        source: &str,
        skill_key: &str,
    ) -> SkillPolicyResult<PolicyDecision> {
        // 1) 平台安全层：受保护 skill 永远拒绝，与 policy 无关。
        if is_protected_skill(skill_key) {
            return Ok(PolicyDecision {
                allowed: false,
                reason: format!("skill '{}' is platform-protected", skill_key),
                denial_type: DenialType::Platform,
            });
        }

        // 2) company policy 层。
        let row = self.repo.get_by_company(company_id).await?;
        let policy = match row {
            Some(r) => r.policy,
            None => Value::Null, // 无 policy → 默认开放
        };

        let allowed = if policy.is_null() {
            true
        } else {
            evaluate_against_policy(&policy, role, action, source, skill_key)
        };

        Ok(PolicyDecision {
            allowed,
            reason: if allowed {
                "allowed by policy".to_string()
            } else {
                format!(
                    "denied by company skill policy (mode={})",
                    policy_mode(&policy)
                )
            },
            denial_type: DenialType::Policy,
        })
    }

    async fn get_policy(&self, company_id: Uuid) -> SkillPolicyResult<Option<Value>> {
        let row = self.repo.get_by_company(company_id).await?;
        Ok(row.map(|r| Self::to_value(&r)))
    }

    async fn set_policy(&self, company_id: Uuid, policy: Value) -> SkillPolicyResult<Value> {
        validate_policy(&policy)?;
        let existing = self.repo.get_by_company(company_id).await?;
        let next_version = existing.map(|r| r.version + 1).unwrap_or(1);
        let row = self
            .repo
            .upsert(company_id, policy, next_version)
            .await?;
        Ok(Self::to_value(&row))
    }

    async fn delete_policy(&self, company_id: Uuid) -> SkillPolicyResult<()> {
        self.repo.delete_by_company(company_id).await?;
        Ok(())
    }

    async fn simulate(
        &self,
        company_id: Uuid,
        agent_id: Option<Uuid>,
        role: &str,
        action: &str,
        source: &str,
        skill_key: &str,
    ) -> SkillPolicyResult<Value> {
        let decision = self
            .evaluate(company_id, agent_id, role, action, source, skill_key)
            .await?;
        Ok(json!({
            "companyId": company_id,
            "skill": skill_key,
            "role": role,
            "action": action,
            "source": source,
            "allowed": decision.allowed,
            "reason": decision.reason,
            "denialType": decision.denial_type.as_str(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_open_allows_when_no_deny_rules() {
        let policy = json!({ "mode": "defaultOpen" });
        assert!(evaluate_against_policy(&policy, "engineer", "invoke", "catalog", "web.search"));
    }

    #[test]
    fn default_open_denies_on_matching_deny_rule() {
        let policy = json!({
            "mode": "defaultOpen",
            "denyRules": [{ "skill": "web.search" }]
        });
        assert!(!evaluate_against_policy(&policy, "engineer", "invoke", "catalog", "web.search"));
        // 未命中的 skill 仍允许
        assert!(evaluate_against_policy(&policy, "engineer", "invoke", "catalog", "docs.read"));
    }

    #[test]
    fn default_deny_requires_allow_rule() {
        let policy = json!({
            "mode": "defaultDeny",
            "allowRules": [{ "role": "admin", "action": "invoke" }]
        });
        assert!(evaluate_against_policy(&policy, "admin", "invoke", "catalog", "web.search"));
        assert!(!evaluate_against_policy(&policy, "engineer", "invoke", "catalog", "web.search"));
    }

    #[test]
    fn allow_list_requires_skill_in_list() {
        let policy = json!({
            "mode": "allowList",
            "allowedSkills": ["docs.read"],
            "allowRules": [{}]
        });
        assert!(evaluate_against_policy(&policy, "engineer", "invoke", "catalog", "docs.read"));
        assert!(!evaluate_against_policy(&policy, "engineer", "invoke", "catalog", "web.search"));
    }

    #[test]
    fn rule_fields_are_wildcards_when_absent() {
        let rule = json!({ "action": "install" });
        assert!(rule_matches(&rule, "any-role", "install", "any-source", "any.skill"));
        assert!(!rule_matches(&rule, "any-role", "invoke", "any-source", "any.skill"));
    }

    #[test]
    fn protected_skills_detected_with_prefix() {
        assert!(is_protected_skill(PROTECTED_SKILLS[0]));
        assert!(!is_protected_skill("docs.read"));
    }

    #[test]
    fn validate_policy_rejects_unknown_mode_and_non_array() {
        assert!(validate_policy(&json!({ "mode": "defaultOpen" })).is_ok());
        assert!(validate_policy(&json!({ "mode": "nope" })).is_err());
        assert!(validate_policy(&json!({ "allowRules": "x" })).is_err());
        assert!(validate_policy(&json!("not-an-object")).is_err());
    }
}
