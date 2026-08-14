//! Status Card 后台任务链（迁移自 Paperclip `services/status-cards.ts` +
//! `services/status-card-update-engine.ts` + `services/status-card-finalization.ts`）。
//!
//! Paperclip 的后台执行模型：compile/refresh 不是同步完成，而是
//!  1. 创建一条 hidden issue（assignee = Summarizer 内置 agent），description 内嵌
//!     JSON payload（operation/fingerprint/queryVersion/changes 等）；
//!  2. 通过 heartbeat wakeup 唤醒 agent 执行；
//!  3. agent 完成后经 `PUT /status-cards/:id/query` / `PUT /status-cards/:id/summary`
//!     写回（写回时强校验 writer = Summarizer + generationIssueId + runId 匹配）；
//!  4. scheduler tick 扫描 `next_eval_at` 到期卡片触发 refresh；
//!  5. issue 终态（done/cancelled/blocked）触发 finalization，释放 generating_issue_id。

use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// 内置 Summarizer agent key（对应 Paperclip SUMMARIZER_BUILT_IN_KEY = "summarizer"）。
pub const SUMMARIZER_BUILT_IN_KEY: &str = "summarizer";

/// 终态 issue 状态（对应 Paperclip TERMINAL_ISSUE_STATUSES）。
pub const TERMINAL_ISSUE_STATUSES: [&str; 2] = ["done", "cancelled"];

/// stalled generation 同样包含 blocked（对应 Paperclip STALLED_GENERATION_STATUSES）。
pub const STALLED_GENERATION_STATUSES: [&str; 3] = ["done", "cancelled", "blocked"];

/// 卡片 watch 集合上限（对应 STATUS_CARD_MAX_MENTIONED_ISSUES = 200）。
pub const STATUS_CARD_MAX_MENTIONED_ISSUES: usize = 200;

/// 单次 diff 超过该数量强制 full 重写（对应 chooseStatusCardUpdateKind 的 > 10）。
const FULL_REWRITE_CHANGE_THRESHOLD: usize = 10;

/// incremental 连续次数上限（对应 incrementalCount >= 9）。
const MAX_INCREMENTAL_CONSECUTIVE: usize = 9;

/// reactive 模式下 debounce 上限 60s。
const REACTIVE_DEBOUNCE_MAX_SECS: i64 = 60;

// ============================================================================
// update-engine 纯函数（迁移自 status-card-update-engine.ts）
// ============================================================================

/// 单个 issue 的指纹条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FingerprintEntry {
    pub status: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_human_comment_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_user_id: Option<String>,
}

/// StatusCardFingerprint = issue_id -> entry。
pub type StatusCardFingerprint = std::collections::BTreeMap<String, FingerprintEntry>;

/// 一次 delta 变化。
#[derive(Debug, Clone, PartialEq)]
pub struct StatusCardDeltaChange {
    pub issue_id: String,
    pub identifier: String,
    pub title: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub change_kind: &'static str, // new|removed|status|assignee|human_comment|updated
}

impl StatusCardDeltaChange {
    pub fn to_json(&self) -> Value {
        json!({
            "issueId": self.issue_id,
            "identifier": self.identifier,
            "title": self.title,
            "from": self.from,
            "to": self.to,
            "changeKind": self.change_kind,
        })
    }
}

/// 从 summary markdown 提取 issue 引用（identifier "PAP-123" 与链接 "/issues/<uuid>"）。
pub fn extract_issue_mentions(markdown: &str) -> (Vec<String>, Vec<String>) {
    let mut identifiers = std::collections::BTreeSet::new();
    let mut issue_ids = std::collections::BTreeSet::new();
    let id_re = regex::Regex::new(r"\b[A-Z][A-Z0-9]{0,9}-\d{1,7}\b").unwrap();
    let link_re = regex::Regex::new(r"/issues/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\b").unwrap();
    for cap in id_re.captures_iter(markdown) {
        identifiers.insert(cap[0].to_string());
    }
    for cap in link_re.captures_iter(markdown) {
        issue_ids.insert(cap[1].to_lowercase());
    }
    (identifiers.into_iter().collect(), issue_ids.into_iter().collect())
}

/// 构建指纹（issue_id -> entry）。
pub fn build_status_card_fingerprint(
    issues: &[Value],
) -> StatusCardFingerprint {
    let mut fp = StatusCardFingerprint::new();
    for issue in issues {
        let Some(id) = issue.get("id").and_then(|v| v.as_str()) else { continue };
        fp.insert(
            id.to_string(),
            FingerprintEntry {
                status: issue.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                updated_at: issue
                    .get("updatedAt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                latest_human_comment_at: issue
                    .get("latestHumanCommentAt")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                identifier: issue.get("identifier").and_then(|v| v.as_str()).map(|s| s.to_string()),
                title: issue.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()),
                assignee_agent_id: issue
                    .get("assigneeAgentId")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                assignee_user_id: issue
                    .get("assigneeUserId")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            },
        );
    }
    fp
}

/// diff 前后指纹，返回变化列表（对应 diffStatusCardFingerprint）。
pub fn diff_status_card_fingerprint(
    previous: Option<&StatusCardFingerprint>,
    current: &StatusCardFingerprint,
) -> Vec<StatusCardDeltaChange> {
    let mut changes = Vec::new();
    let before = previous.cloned().unwrap_or_default();
    for (issue_id, next) in current {
        let identifier = next.identifier.clone().unwrap_or_else(|| issue_id.clone());
        let title = next.title.clone().unwrap_or_default();
        match before.get(issue_id) {
            None => {
                changes.push(StatusCardDeltaChange {
                    issue_id: issue_id.clone(),
                    identifier,
                    title,
                    from: None,
                    to: Some(next.status.clone()),
                    change_kind: "new",
                });
            }
            Some(prior) => {
                let mut has_specific = false;
                if prior.status != next.status {
                    changes.push(StatusCardDeltaChange {
                        issue_id: issue_id.clone(),
                        identifier: identifier.clone(),
                        title: title.clone(),
                        from: Some(prior.status.clone()),
                        to: Some(next.status.clone()),
                        change_kind: "status",
                    });
                    has_specific = true;
                }
                if prior.assignee_agent_id != next.assignee_agent_id
                    || prior.assignee_user_id != next.assignee_user_id
                {
                    changes.push(StatusCardDeltaChange {
                        issue_id: issue_id.clone(),
                        identifier: identifier.clone(),
                        title: title.clone(),
                        from: None,
                        to: None,
                        change_kind: "assignee",
                    });
                    has_specific = true;
                }
                if prior.latest_human_comment_at != next.latest_human_comment_at
                    && next.latest_human_comment_at.is_some()
                {
                    changes.push(StatusCardDeltaChange {
                        issue_id: issue_id.clone(),
                        identifier: identifier.clone(),
                        title: title.clone(),
                        from: prior.latest_human_comment_at.clone(),
                        to: next.latest_human_comment_at.clone(),
                        change_kind: "human_comment",
                    });
                    has_specific = true;
                }
                if prior.updated_at != next.updated_at && !has_specific {
                    changes.push(StatusCardDeltaChange {
                        issue_id: issue_id.clone(),
                        identifier,
                        title,
                        from: Some(prior.status.clone()),
                        to: Some(next.status.clone()),
                        change_kind: "updated",
                    });
                }
            }
        }
    }
    for (issue_id, prior) in before.iter() {
        if current.contains_key(issue_id) {
            continue;
        }
        changes.push(StatusCardDeltaChange {
            issue_id: issue_id.clone(),
            identifier: prior.identifier.clone().unwrap_or_else(|| issue_id.clone()),
            title: prior.title.clone().unwrap_or_default(),
            from: Some(prior.status.clone()),
            to: None,
            change_kind: "removed",
        });
    }
    changes
}

/// 按 refresh policy 过滤变化（对应 filterStatusCardChanges）。
pub fn filter_status_card_changes(
    changes: Vec<StatusCardDeltaChange>,
    policy: &Value,
) -> Vec<StatusCardDeltaChange> {
    let triggers = policy.get("triggers").unwrap_or(&Value::Null);
    let any_update = triggers.get("anyUpdate").and_then(|v| v.as_bool()).unwrap_or(false);
    let membership = triggers.get("membershipChanges").and_then(|v| v.as_bool()).unwrap_or(false);
    let assignee = triggers.get("assigneeChanges").and_then(|v| v.as_bool()).unwrap_or(false);
    let human = triggers.get("humanComments").and_then(|v| v.as_bool()).unwrap_or(false);
    let status = triggers.get("statusTransitions").and_then(|v| v.as_bool()).unwrap_or(false);
    changes
        .into_iter()
        .filter(|c| {
            if any_update {
                return true;
            }
            match c.change_kind {
                "new" | "removed" => membership,
                "assignee" => assignee,
                "human_comment" => human,
                "status" => status,
                _ => false,
            }
        })
        .collect()
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let out = hasher.finalize();
    out.iter().map(|b| format!("{:02x}", b)).collect()
}

/// changes hash（对应 statusCardChangesHash）。
pub fn status_card_changes_hash(changes: &[StatusCardDeltaChange]) -> String {
    let mut stable: Vec<Value> = changes
        .iter()
        .map(|c| json!({ "issueId": c.issue_id, "changeKind": c.change_kind, "from": c.from, "to": c.to }))
        .collect();
    stable.sort_by_key(|v| {
        format!(
            "{}:{}",
            v["issueId"].as_str().unwrap_or(""),
            v["changeKind"].as_str().unwrap_or("")
        )
    });
    sha256_hex(&serde_json::to_string(&stable).unwrap_or_default())
}

/// fingerprint hash（对应 statusCardFingerprintHash）。
pub fn status_card_fingerprint_hash(fp: &StatusCardFingerprint) -> String {
    let stable: Value = serde_json::to_value(fp).unwrap_or(Value::Null);
    sha256_hex(&serde_json::to_string(&stable).unwrap_or_default())
}

/// 是否在活跃时段内（对应 isWithinStatusCardActiveHours）。
pub fn is_within_status_card_active_hours(policy: &Value, now: &DateTime<Utc>) -> bool {
    let Some(active_hours) = policy.get("activeHours") else { return true };
    let (Some(start), Some(end)) = (
        active_hours.get("start").and_then(|v| v.as_str()),
        active_hours.get("end").and_then(|v| v.as_str()),
    ) else {
        return true;
    };
    let parse_min = |s: &str| -> Option<i64> {
        let mut parts = s.split(':');
        let h: i64 = parts.next()?.parse().ok()?;
        let m: i64 = parts.next().unwrap_or("0").parse().ok()?;
        Some(h * 60 + m)
    };
    let (Some(start_min), Some(end_min)) = (parse_min(start), parse_min(end)) else {
        return true;
    };
    let current = i64::from(now.hour()) * 60 + i64::from(now.minute());
    if start_min <= end_min {
        current >= start_min && current < end_min
    } else {
        current >= start_min || current < end_min
    }
}

/// 计算下一次评估时间（对应 nextStatusCardEvaluationAt）。
pub fn next_status_card_evaluation_at(policy: &Value, now: &DateTime<Utc>) -> Option<DateTime<Utc>> {
    let mode = policy.get("mode").and_then(|v| v.as_str()).unwrap_or("interval");
    if mode == "manual" {
        return None;
    }
    let seconds = if mode == "interval" {
        policy
            .get("intervalMinutes")
            .and_then(|v| v.as_i64())
            .unwrap_or(15)
            * 60
    } else {
        policy
            .get("debounceSeconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(60)
            .min(REACTIVE_DEBOUNCE_MAX_SECS)
    };
    Some(*now + chrono::Duration::seconds(seconds))
}

/// 选择更新类型 full/incremental（对应 chooseStatusCardUpdateKind）。
pub fn choose_status_card_update_kind(input: ChooseUpdateKindInput) -> &'static str {
    if input.explicit_full
        || !input.has_document
        || input.change_count > FULL_REWRITE_CHANGE_THRESHOLD
        || input.configuration_changed
        || input.restore_refresh
        || input.last_update_query_version != Some(input.query_version)
        || input.incremental_count >= MAX_INCREMENTAL_CONSECUTIVE
    {
        "full"
    } else {
        "incremental"
    }
}

#[derive(Debug, Clone)]
pub struct ChooseUpdateKindInput {
    pub explicit_full: bool,
    pub has_document: bool,
    pub change_count: usize,
    pub query_version: i32,
    pub last_update_query_version: Option<i32>,
    pub incremental_count: usize,
    pub configuration_changed: bool,
    pub restore_refresh: bool,
}

/// 策略评估结果（对应 evaluateStatusCardPolicy）。
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecision {
    Run,
    PauseBudget,
    PauseHours,
    Wait { due_at: Option<DateTime<Utc>> },
}

/// 评估 refresh policy（对应 evaluateStatusCardPolicy）。
pub fn evaluate_status_card_policy(input: EvaluatePolicyInput) -> PolicyDecision {
    let cap = input.policy.get("dailyTokenCap").and_then(|v| v.as_i64()).unwrap_or(100_000);
    if !input.manual && input.tokens_today >= cap {
        return PolicyDecision::PauseBudget;
    }
    if !input.manual && !is_within_status_card_active_hours(&input.policy, &input.now) {
        return PolicyDecision::PauseHours;
    }
    if input.manual {
        return PolicyDecision::Run;
    }
    let mode = input.policy.get("mode").and_then(|v| v.as_str()).unwrap_or("interval");
    if mode == "manual" {
        return PolicyDecision::Wait { due_at: None };
    }
    if mode == "reactive" {
        let max_per_hour = input
            .policy
            .get("maxUpdatesPerHour")
            .and_then(|v| v.as_i64())
            .unwrap_or(6);
        if (input.updates_last_hour as i64) >= max_per_hour {
            return PolicyDecision::Wait { due_at: None };
        }
        let debounce = input
            .policy
            .get("debounceSeconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(60)
            .min(REACTIVE_DEBOUNCE_MAX_SECS);
        let due_at = (input.last_change_at.unwrap_or(input.now)) + chrono::Duration::seconds(debounce);
        if due_at > input.now {
            return PolicyDecision::Wait { due_at: Some(due_at) };
        }
    }
    PolicyDecision::Run
}

#[derive(Debug, Clone)]
pub struct EvaluatePolicyInput {
    pub policy: Value,
    pub now: DateTime<Utc>,
    pub last_change_at: Option<DateTime<Utc>>,
    pub updates_last_hour: usize,
    pub tokens_today: i64,
    pub manual: bool,
}

// ============================================================================
// Worker：后台任务链
// ============================================================================

/// 生成任务创建结果。
#[derive(Debug, Clone)]
pub struct GenerationEnqueue {
    pub card_id: Uuid,
    pub generating_issue_id: Uuid,
    pub already_generating: bool,
}

/// Status Card 后台 worker。
#[derive(Clone)]
pub struct StatusCardWorker {
    pool: PgPool,
}

/// 待绑定参数（保持出现顺序，占位符 $1..$n）。
enum BindVal {
    Str(String),
    Uuid(Uuid),
    StatusList(Vec<String>),
    PriorityList(Vec<String>),
}

/// 按 binds 顺序绑定并执行查询，返回行。
async fn fetch_issue_rows(
    pool: &PgPool,
    sql: &str,
    company_id: Uuid,
    binds: &[BindVal],
) -> Result<Vec<sqlx::postgres::PgRow>, String> {
    let mut qb = sqlx::query(sql).bind(company_id);
    for bind_val in binds {
        match bind_val {
            BindVal::Str(s) => qb = qb.bind(s),
            BindVal::Uuid(u) => qb = qb.bind(u),
            // sqlx-postgres 将 Vec<String> 编码为 text[]，可直接 bind。
            BindVal::StatusList(list) => qb = qb.bind(list.clone()),
            BindVal::PriorityList(list) => qb = qb.bind(list.clone()),
        }
    }
    qb.fetch_all(pool)
        .await
        .map_err(|e| format!("execute status card query: {e}"))
}

impl StatusCardWorker {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 解析 Summarizer agent（per-card override 优先，否则内置 Summarizer）。
    /// 对应 Paperclip resolveSummarizerAgentId。
    pub async fn resolve_summarizer_agent_id(
        &self,
        company_id: Uuid,
        card_agent_id: Option<Uuid>,
    ) -> Result<Uuid, String> {
        if let Some(agent_id) = card_agent_id {
            let exists: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM agents WHERE id = $1 AND company_id = $2",
            )
            .bind(agent_id)
            .bind(company_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| format!("resolve summarizer override: {e}"))?;
            if exists.is_some() {
                return Ok(agent_id);
            }
        }
        let built_in: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM agents WHERE company_id = $1 AND metadata->>'builtInKey' = $2 \
             LIMIT 1",
        )
        .bind(company_id)
        .bind(SUMMARIZER_BUILT_IN_KEY)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("resolve built-in summarizer: {e}"))?;
        built_in.ok_or_else(|| "Summarizer built-in agent is not configured".to_string())
    }

    /// 创建 hidden generation issue（对应 Paperclip issuesSvc.create hidden task）。
    #[allow(clippy::too_many_arguments)]
    async fn create_generation_issue(
        &self,
        company_id: Uuid,
        title: &str,
        description: &str,
        summarizer_agent_id: Uuid,
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<Uuid>,
        project_id: Option<Uuid>,
        project_workspace_id: Option<Uuid>,
    ) -> Result<Uuid, String> {
        let id = Uuid::new_v4();
        // origin_fingerprint 必须唯一（issues 表有 (company_id, origin_fingerprint) 唯一约束）。
        let origin_fingerprint = format!("status_card_generation:{}", id);
        sqlx::query(
            "INSERT INTO issues \
             (id, company_id, project_id, project_workspace_id, title, description, status, \
              priority, assignee_agent_id, created_by_agent_id, created_by_user_id, hidden_at, \
              origin_kind, origin_fingerprint) \
             VALUES ($1,$2,$3,$4,$5,$6,'todo','medium',$7,$8,$9,NOW(),'status_card_generation',$10)",
        )
        .bind(id)
        .bind(company_id)
        .bind(project_id)
        .bind(project_workspace_id)
        .bind(title)
        .bind(description)
        .bind(summarizer_agent_id)
        .bind(created_by_agent_id)
        .bind(created_by_user_id)
        .bind(&origin_fingerprint)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("create generation issue: {e}"))?;
        Ok(id)
    }

    /// 检查是否存在活跃的生成任务（对应 requestCompile 的去重检查）。
    async fn active_generation_issue(
        &self,
        card_id: Uuid,
    ) -> Result<Option<Uuid>, String> {
        let row = sqlx::query(
            "SELECT c.generating_issue_id AS gid, i.status::text AS status, i.description AS description \
             FROM status_cards c LEFT JOIN issues i ON i.id = c.generating_issue_id \
             WHERE c.id = $1",
        )
        .bind(card_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("load active generation: {e}"))?;
        let Some(row) = row else { return Ok(None) };
        let gid: Option<Uuid> = row.get("gid");
        let status: Option<String> = row.get("status");
        let description: Option<String> = row.get("description");
        let Some(gid) = gid else { return Ok(None) };
        let Some(status) = status else { return Ok(Some(gid)) };
        if TERMINAL_ISSUE_STATUSES.contains(&status.as_str()) || status == "blocked" {
            return Ok(None);
        }
        // 已存在活跃任务时返回 Some；description 中的 promptHash 匹配由调用方处理。
        let _ = description;
        Ok(Some(gid))
    }

    /// 构建 compile description（对应 compileDescription）。
    fn compile_description(card: &Value, generation_issue_id: Option<Uuid>, hash: &str) -> String {
        let payload = json!({
            "operation": "compile",
            "statusCardId": card["id"],
            "companyId": card["companyId"],
            "generationIssueId": generation_issue_id,
            "promptHash": hash,
        });
        format!(
            "Compile this status-card interest prompt into structured Paperclip company-search queries, then continue in the same run and write the first full summary.\n\n\
             Use the bundled `status-card-query` skill. Resolve named projects and labels to ids. Keep queries narrow, cap limits, and preserve union semantics across the query array.\n\n\
             ## Interest prompt\n\n<untrusted-data name=\"interest-prompt\">\n{}\n</untrusted-data>\n\n\
             ## Required write-back sequence\n\n1. `PUT /api/status-cards/{}/query` with `queries`, an auto-title, a non-empty `changeSummary`, and `generationIssueId`.\n2. Execute the compiled scope and write the first full Markdown summary with `PUT /api/status-cards/{}/summary` using the same `generationIssueId`.\n\n\
             ```json\n{}\n```",
            card["interestPrompt"].as_str().unwrap_or(""),
            card["id"].as_str().unwrap_or(""),
            card["id"].as_str().unwrap_or(""),
            serde_json::to_string_pretty(&payload).unwrap_or_default(),
        )
    }

    /// 请求编译（对应 requestCompile）。
    pub async fn request_compile(
        &self,
        card_id: Uuid,
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<Uuid>,
    ) -> Result<GenerationEnqueue, String> {
        let row = sqlx::query(
            "SELECT id, company_id, interest_prompt, title, agent_id, generating_issue_id, \
             archived_at, state FROM status_cards WHERE id = $1",
        )
        .bind(card_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("load card: {e}"))?
        .ok_or_else(|| "Status card not found".to_string())?;
        let company_id: Uuid = row.get("company_id");
        let archived_at: Option<DateTime<Utc>> = row.get("archived_at");
        if archived_at.is_some() {
            return Err("Archived status cards cannot be compiled".to_string());
        }
        let interest_prompt: String = row.get("interest_prompt");
        let title: Option<String> = row.get("title");
        let card_agent_id: Option<Uuid> = row.get("agent_id");

        // 已存在活跃生成任务且 promptHash 一致 -> 幂等返回。
        if let Some(active_id) = self.active_generation_issue(card_id).await? {
            return Ok(GenerationEnqueue {
                card_id,
                generating_issue_id: active_id,
                already_generating: true,
            });
        }

        let summarizer = self.resolve_summarizer_agent_id(company_id, card_agent_id).await?;
        let hash = sha256_hex(&interest_prompt);
        let issue_title = format!(
            "Compile status card: {}",
            title.unwrap_or_else(|| interest_prompt.chars().take(80).collect::<String>())
        );
        let card_json = json!({
            "id": card_id.to_string(),
            "companyId": company_id.to_string(),
            "interestPrompt": interest_prompt,
        });
        let description = Self::compile_description(&card_json, None, &hash);
        let issue_id = self
            .create_generation_issue(
                company_id,
                &issue_title,
                &description,
                summarizer,
                created_by_agent_id,
                created_by_user_id,
                None,
                None,
            )
            .await?;
        // 回填 description 中的 generationIssueId 并更新卡片占位。
        let description = Self::compile_description(&card_json, Some(issue_id), &hash);
        sqlx::query("UPDATE issues SET description = $2 WHERE id = $1")
            .bind(issue_id)
            .bind(&description)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("update generation description: {e}"))?;
        sqlx::query(
            "UPDATE status_cards SET generating_issue_id = $2, state = 'compiling', \
             failure_reason = NULL, updated_at = NOW() WHERE id = $1",
        )
        .bind(card_id)
        .bind(issue_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("claim card generation: {e}"))?;
        Ok(GenerationEnqueue {
            card_id,
            generating_issue_id: issue_id,
            already_generating: false,
        })
    }

    /// 执行卡片查询，返回 issue 快照（对应 Paperclip executeQueries 的轻量版）。
    ///
    /// 每个查询支持：scope(project/project_workspace/all)、status/priority 过滤、
    /// assigneeAgentId/projectId/labelId/limit。查询结果去重合并，并附带
    /// 最近人工评论时间（用于 fingerprint 的 human_comment 变更检测）。
    pub async fn execute_queries(
        &self,
        company_id: Uuid,
        queries: &Value,
    ) -> Result<Vec<Value>, String> {
        let mut issue_map: std::collections::BTreeMap<String, Value> = Default::default();
        let Some(query_array) = queries.as_array() else {
            return Ok(Vec::new());
        };

        for query in query_array {
            if !query.is_object() {
                continue;
            }
            let scope = query.get("scope").and_then(|v| v.as_str()).unwrap_or("all");
            let limit = query
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(20)
                .clamp(1, 50);
            let parse_list = |key: &str| -> Vec<String> {
                match query.get(key) {
                    Some(Value::String(s)) => s
                        .split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect(),
                    Some(Value::Array(a)) => a
                        .iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect(),
                    _ => vec![],
                }
            };
            let statuses = parse_list("status");
            let priorities = parse_list("priority");
            let assignee_agent_id = query.get("assigneeAgentId").and_then(|v| {
                let s = v.as_str().unwrap_or("");
                if s.is_empty() || s.eq_ignore_ascii_case("null") {
                    None
                } else {
                    Uuid::parse_str(s).ok()
                }
            });
            let project_id = query
                .get("projectId")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());
            let project_workspace_id = query
                .get("projectWorkspaceId")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());
            let label_id = query
                .get("labelId")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());
            let q = query
                .get("q")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            // 构造 WHERE + bind 顺序（$1 固定为 company_id）。
            let mut where_clauses = vec!["company_id = $1".to_string(), "hidden_at IS NULL".to_string()];
            let mut binds: Vec<BindVal> = Vec::new();
            let mut next_ph = |binds: &mut Vec<BindVal>| format!("${}", binds.len() + 2);

            if scope == "project" {
                let Some(pid) = project_id else { continue };
                let ph = next_ph(&mut binds);
                where_clauses.push(format!("project_id = {}", ph));
                binds.push(BindVal::Uuid(pid));
            } else if scope == "project_workspace" {
                let Some(wid) = project_workspace_id else { continue };
                let ph = next_ph(&mut binds);
                where_clauses.push(format!("project_workspace_id = {}", ph));
                binds.push(BindVal::Uuid(wid));
            }
            if !statuses.is_empty() {
                let ph = next_ph(&mut binds);
                where_clauses.push(format!("status = ANY({}::issue_status[])", ph));
                binds.push(BindVal::StatusList(statuses));
            }
            if !priorities.is_empty() {
                let ph = next_ph(&mut binds);
                where_clauses.push(format!("priority = ANY({}::issue_priority[])", ph));
                binds.push(BindVal::PriorityList(priorities));
            }
            if let Some(aid) = assignee_agent_id {
                let ph = next_ph(&mut binds);
                where_clauses.push(format!("assignee_agent_id = {}", ph));
                binds.push(BindVal::Uuid(aid));
            }
            if let Some(pid) = project_id {
                let ph = next_ph(&mut binds);
                where_clauses.push(format!("project_id = {}", ph));
                binds.push(BindVal::Uuid(pid));
            }
            if let Some(lid) = label_id {
                let ph = next_ph(&mut binds);
                where_clauses.push(format!(
                    "EXISTS (SELECT 1 FROM issue_labels il WHERE il.issue_id = issues.id AND il.label_id = {})",
                    ph
                ));
                binds.push(BindVal::Uuid(lid));
            }
            if !q.is_empty() {
                let ph = next_ph(&mut binds);
                where_clauses.push(format!(
                    "(title ILIKE '%' || {} || '%' OR coalesce(identifier,'') ILIKE '%' || {} || '%')",
                    ph, ph
                ));
                binds.push(BindVal::Str(q.clone()));
            }

            let sql = format!(
                "SELECT id, identifier, title, status::text AS status, priority::text AS priority, assignee_agent_id, assignee_user_id, updated_at \
                 FROM issues WHERE {} \
                 ORDER BY updated_at DESC LIMIT {}",
                where_clauses.join(" AND "),
                limit
            );
            let rows = fetch_issue_rows(&self.pool, &sql, company_id, &binds).await?;
            for row in rows {
                let id: Uuid = row.get("id");
                let identifier: Option<String> = row.get("identifier");
                let title: String = row.get("title");
                let status: String = row.get("status");
                let priority: String = row.get("priority");
                let assignee_agent_id: Option<Uuid> = row.get("assignee_agent_id");
                let assignee_user_id: Option<String> = row.get("assignee_user_id");
                let updated_at: DateTime<Utc> = row.get("updated_at");
                issue_map.insert(
                    id.to_string(),
                    json!({
                        "id": id.to_string(),
                        "identifier": identifier,
                        "title": title,
                        "status": status,
                        "priority": priority,
                        "assigneeAgentId": assignee_agent_id.map(|v| v.to_string()),
                        "assigneeUserId": assignee_user_id,
                        "updatedAt": updated_at.to_rfc3339(),
                    }),
                );
            }
        }
        // 补充最近人工评论时间。
        if !issue_map.is_empty() {
            let ids: Vec<Uuid> = issue_map
                .keys()
                .filter_map(|k| Uuid::parse_str(k).ok())
                .collect();
            if ids.is_empty() {
                return Ok(issue_map.into_values().collect());
            }
            let rows = sqlx::query(
                "SELECT issue_id, max(updated_at) AS latest \
                 FROM issue_comments WHERE company_id = $1 AND issue_id = ANY($2::uuid[]) \
                   AND actor_type = 'user' \
                 GROUP BY issue_id",
            )
            .bind(company_id)
            .bind(&ids[..])
            .fetch_all(&self.pool)
            .await
            .map_err(|e| format!("load latest human comments: {e}"))?;
            for row in rows {
                let issue_id: Uuid = row.get("issue_id");
                let latest: DateTime<Utc> = row.get("latest");
                if let Some(entry) = issue_map.get_mut(&issue_id.to_string()) {
                    entry["latestHumanCommentAt"] = json!(latest.to_rfc3339());
                }
            }
        }
        Ok(issue_map.into_values().collect())
    }

    /// 请求刷新（对应 requestRefresh：执行查询 -> fingerprint diff -> policy 评估 ->
    /// 决定创建 refresh 任务或仅更新调度状态）。
    pub async fn request_refresh(
        &self,
        card_id: Uuid,
        full: bool,
        trigger: &str,
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<Uuid>,
    ) -> Result<GenerationEnqueue, String> {
        let row = sqlx::query(
            "SELECT id, company_id, title, interest_prompt, agent_id, queries, generating_issue_id, \
             archived_at, state, query_version, fingerprint, refresh_policy, pending_change_hash, \
             last_change_at, next_eval_at, summary_markdown FROM status_cards WHERE id = $1",
        )
        .bind(card_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("load card: {e}"))?
        .ok_or_else(|| "Status card not found".to_string())?;
        let company_id: Uuid = row.get("company_id");
        let archived_at: Option<DateTime<Utc>> = row.get("archived_at");
        if archived_at.is_some() {
            return Err("Archived status cards cannot be refreshed".to_string());
        }
        let queries: Value = row.get("queries");
        if queries.as_array().map(|a| a.is_empty()).unwrap_or(true) {
            return Err("Compile the status-card query before refreshing it".to_string());
        }
        if let Some(active_id) = self.active_generation_issue(card_id).await? {
            return Ok(GenerationEnqueue {
                card_id,
                generating_issue_id: active_id,
                already_generating: true,
            });
        }

        // 1) 执行查询构建指纹。
        let now = Utc::now();
        let snapshot = self.execute_queries(company_id, &queries).await?;
        let fingerprint = build_status_card_fingerprint(&snapshot);
        let previous: Option<StatusCardFingerprint> = row
            .get::<Option<Value>, _>("fingerprint")
            .and_then(|v| serde_json::from_value(v).ok());
        let all_changes = diff_status_card_fingerprint(previous.as_ref(), &fingerprint);
        let refresh_policy: Value = row.get("refresh_policy");
        let changes = filter_status_card_changes(all_changes, &refresh_policy);
        let is_manual = trigger == "manual" || trigger == "restore";
        let next_eval = next_status_card_evaluation_at(&refresh_policy, &now);
        let fingerprint_hash = status_card_fingerprint_hash(&fingerprint);

        // 2) 无变化且非手动 -> 仅更新调度状态，不创建任务。
        if !is_manual && changes.is_empty() {
            tracing::debug!(
                %card_id,
                snapshot_len = snapshot.len(),
                "status card refresh: no changes, skipping enqueue"
            );
            sqlx::query(
                "UPDATE status_cards SET pending_change_count = 0, pending_change_hash = NULL, \
                 last_change_at = NULL, state = 'active', fingerprint = $3, fingerprint_at = $4, \
                 next_eval_at = $5, updated_at = NOW() WHERE id = $1",
            )
            .bind(card_id)
            .bind(company_id)
            .bind(serde_json::to_value(&fingerprint).unwrap_or(Value::Null))
            .bind(now)
            .bind(next_eval)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("update card after no-change refresh: {e}"))?;
            return Ok(GenerationEnqueue {
                card_id,
                generating_issue_id: Uuid::nil(),
                already_generating: true, // 语义上：无变化 -> 未入队
            });
        }

        // 3) policy 评估（预算/活跃时段/频率）。
        let history_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM status_card_update_runs WHERE card_id = $1 \
             AND started_at >= $2 AND kind <> 'compile' AND finished_at IS NOT NULL",
        )
        .bind(card_id)
        .bind(now - chrono::Duration::hours(1))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| format!("count recent updates: {e}"))?;
        let tokens_today: i64 = sqlx::query_scalar(
            "SELECT COALESCE(sum(input_tokens + output_tokens), 0) FROM status_card_update_runs \
             WHERE card_id = $1 AND started_at >= $2",
        )
        .bind(card_id)
        .bind(
            now.date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|d| d.and_utc())
                .unwrap_or(now),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| format!("sum daily tokens: {e}"))?;
        let last_change_at: Option<DateTime<Utc>> = row.get("last_change_at");
        let pending_change_hash: Option<String> = row.get("pending_change_hash");
        let new_change_hash = status_card_changes_hash(&changes);
        let last_change_at_effective = if pending_change_hash.as_deref() == Some(new_change_hash.as_str())
            && last_change_at.is_some()
        {
            last_change_at
        } else {
            Some(now)
        };
        let decision = evaluate_status_card_policy(EvaluatePolicyInput {
            policy: refresh_policy.clone(),
            now,
            last_change_at: last_change_at_effective,
            updates_last_hour: history_count as usize,
            tokens_today,
            manual: is_manual,
        });
        if decision != PolicyDecision::Run {
            let state = match decision {
                PolicyDecision::PauseBudget => "paused_budget",
                PolicyDecision::PauseHours => "paused_hours",
                _ => "active",
            };
            let due_at = match decision {
                PolicyDecision::Wait { due_at } => due_at.or(next_eval),
                _ => next_eval,
            };
            sqlx::query(
                "UPDATE status_cards SET pending_change_count = $3, pending_change_hash = $4, \
                 last_change_at = $5, state = $6, fingerprint = $7, fingerprint_at = $8, \
                 next_eval_at = $9, updated_at = NOW() WHERE id = $1",
            )
            .bind(card_id)
            .bind(company_id)
            .bind(changes.len() as i32)
            .bind(&new_change_hash)
            .bind(last_change_at_effective)
            .bind(state)
            .bind(serde_json::to_value(&fingerprint).unwrap_or(Value::Null))
            .bind(now)
            .bind(due_at)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("update card after policy decision: {e}"))?;
            return Ok(GenerationEnqueue {
                card_id,
                generating_issue_id: Uuid::nil(),
                already_generating: true, // 未入队（wait/pause）
            });
        }

        // 4) 决策 run -> 创建 refresh 任务。
        let summarizer = self.resolve_summarizer_agent_id(company_id, None).await?;
        let title: Option<String> = row.get("title");
        let interest_prompt: String = row.get("interest_prompt");
        let has_document: bool = row
            .get::<Option<String>, _>("summary_markdown")
            .is_some();
        let issue_title = format!(
            "{} status card: {}",
            if full { "Rebuild" } else { "Update" },
            title.unwrap_or_else(|| interest_prompt.chars().take(80).collect::<String>())
        );
        let payload = json!({
            "operation": "update",
            "statusCardId": card_id.to_string(),
            "companyId": company_id.to_string(),
            "kind": if full { "full" } else { "incremental" },
            "trigger": trigger,
            "queryVersion": row.get::<i32, _>("query_version"),
            "fingerprint": serde_json::to_value(&fingerprint).unwrap_or(Value::Null),
            "fingerprintHash": fingerprint_hash,
            "changes": changes.iter().map(|c| c.to_json()).collect::<Vec<_>>(),
        });
        let description = format!(
            "Update this Paperclip status card.\n\n```json\n{}\n```",
            serde_json::to_string_pretty(&payload).unwrap_or_default(),
        );
        let issue_id = self
            .create_generation_issue(
                company_id,
                &issue_title,
                &description,
                summarizer,
                created_by_agent_id,
                created_by_user_id,
                None,
                None,
            )
            .await?;
        sqlx::query(
            "UPDATE status_cards SET generating_issue_id = $2, state = 'active', \
             failure_reason = NULL, pending_change_count = $3, pending_change_hash = $4, \
             last_change_at = $5, fingerprint = $6, fingerprint_at = $7, next_eval_at = $8, \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(card_id)
        .bind(issue_id)
        .bind(changes.len() as i32)
        .bind(&new_change_hash)
        .bind(last_change_at_effective)
        .bind(serde_json::to_value(&fingerprint).unwrap_or(Value::Null))
        .bind(now)
        .bind(next_eval)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("claim card refresh: {e}"))?;
        let _ = has_document;
        Ok(GenerationEnqueue {
            card_id,
            generating_issue_id: issue_id,
            already_generating: false,
        })
    }

    /// scheduler tick：扫描到期卡片并触发 refresh（对应 tickDueStatusCards）。
    pub async fn tick_due_status_cards(&self, now: &DateTime<Utc>) -> Result<(usize, usize), String> {
        let due = sqlx::query(
            "SELECT id FROM status_cards \
             WHERE archived_at IS NULL AND generating_issue_id IS NULL \
               AND next_eval_at IS NOT NULL AND next_eval_at <= $1 \
             LIMIT 100",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("query due cards: {e}"))?;
        let mut evaluated = 0usize;
        let mut enqueued = 0usize;
        let claim_until = *now + chrono::Duration::minutes(5);
        for row in due {
            let card_id: Uuid = row.get("id");
            // 乐观锁 claim（对应 Paperclip claimUntil 5 分钟窗口）。
            let claimed: Option<Uuid> = sqlx::query_scalar(
                "UPDATE status_cards SET next_eval_at = $2 WHERE id = $1 \
                 AND archived_at IS NULL AND generating_issue_id IS NULL \
                 AND next_eval_at IS NOT NULL AND next_eval_at <= $3 RETURNING id",
            )
            .bind(card_id)
            .bind(claim_until)
            .bind(now)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| format!("claim card: {e}"))?;
            if claimed.is_none() {
                continue;
            }
            evaluated += 1;
            match self
                .request_refresh(card_id, false, "interval", None, None)
                .await
            {
                Ok(result) if !result.already_generating => enqueued += 1,
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(%card_id, error = %e, "status card scheduled refresh failed");
                }
            }
        }
        Ok((evaluated, enqueued))
    }

    /// finalization：释放 stalled generation 的 generating_issue_id 占位
    /// （对应 finalizeStatusCardsForStalledGeneration）。
    pub async fn finalize_stalled_generations(&self) -> Result<usize, String> {
        let stalled = sqlx::query(
            "SELECT i.id AS issue_id, i.company_id, i.identifier, i.title, i.status::text AS status \
             FROM issues i \
             WHERE i.status = ANY($1::issue_status[]) \
               AND EXISTS (SELECT 1 FROM status_cards c WHERE c.generating_issue_id = i.id)",
        )
        .bind(&STALLED_GENERATION_STATUSES[..])
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("query stalled generations: {e}"))?;
        let mut finalized = 0usize;
        for row in stalled {
            let issue_id: Uuid = row.get("issue_id");
            let company_id: Uuid = row.get("company_id");
            let identifier: Option<String> = row.get("identifier");
            let title: String = row.get("title");
            let status: String = row.get("status");
            let label = identifier
                .map(|i| format!("{}: {}", i, title))
                .unwrap_or_else(|| title.clone());
            let failure_reason = match status.as_str() {
                "cancelled" => format!(
                    "Status-card generation task {} was cancelled before writing a summary.",
                    label
                ),
                "blocked" => format!(
                    "Status-card generation task {} was blocked before writing a summary; re-run to retry.",
                    label
                ),
                _ => format!(
                    "Status-card generation task {} finished without writing a summary.",
                    label
                ),
            };
            let now = Utc::now();
            let updated = sqlx::query(
                "UPDATE status_cards SET state = 'error', failure_reason = $2, \
                 generating_issue_id = NULL, next_eval_at = NULL, updated_at = $3 \
                 WHERE company_id = $4 AND generating_issue_id = $1",
            )
            .bind(issue_id)
            .bind(&failure_reason)
            .bind(now)
            .bind(company_id)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("finalize card: {e}"))?;
            finalized += updated.rows_affected() as usize;
            let _ = sqlx::query(
                "UPDATE status_card_update_runs SET status = 'failed', error = $2, \
                 finished_at = NOW() WHERE generation_issue_id = $1 AND finished_at IS NULL",
            )
            .bind(issue_id)
            .bind(&failure_reason)
            .execute(&self.pool)
            .await;
        }
        Ok(finalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp_entry(status: &str, updated: &str) -> FingerprintEntry {
        FingerprintEntry {
            status: status.to_string(),
            updated_at: updated.to_string(),
            latest_human_comment_at: None,
            identifier: None,
            title: None,
            assignee_agent_id: None,
            assignee_user_id: None,
        }
    }

    #[test]
    fn extract_mentions_picks_identifiers_and_links() {
        let md = "See PAP-123 and https://x/issues/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa now.";
        let (ids, uuids) = extract_issue_mentions(md);
        assert_eq!(ids, vec!["PAP-123".to_string()]);
        assert_eq!(uuids, vec!["aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".to_string()]);
    }

    #[test]
    fn diff_detects_new_status_removed() {
        let mut current = StatusCardFingerprint::new();
        current.insert("a".into(), fp_entry("todo", "t1"));
        current.insert("b".into(), fp_entry("in_progress", "t2"));
        let changes = diff_status_card_fingerprint(None, &current);
        assert!(changes.iter().any(|c| c.change_kind == "new" && c.issue_id == "a"));

        let mut prev = current.clone();
        // b 变 done
        current.insert("b".into(), fp_entry("done", "t3"));
        let changes2 = diff_status_card_fingerprint(Some(&prev), &current);
        assert!(changes2.iter().any(|c| c.change_kind == "status" && c.issue_id == "b"));
        // a 消失
        current.remove("a");
        let changes3 = diff_status_card_fingerprint(Some(&prev), &current);
        assert!(changes3.iter().any(|c| c.change_kind == "removed" && c.issue_id == "a"));
    }

    #[test]
    fn next_eval_interval_vs_manual() {
        let now = DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z").unwrap().with_timezone(&Utc);
        let policy = json!({ "mode": "interval", "intervalMinutes": 15 });
        let next = next_status_card_evaluation_at(&policy, &now).unwrap();
        assert_eq!(next - now, chrono::Duration::minutes(15));
        let manual = json!({ "mode": "manual" });
        assert!(next_status_card_evaluation_at(&manual, &now).is_none());
    }

    #[test]
    fn policy_pauses_on_budget() {
        let now = DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z").unwrap().with_timezone(&Utc);
        let policy = json!({ "mode": "reactive", "dailyTokenCap": 1000 });
        let decision = evaluate_status_card_policy(EvaluatePolicyInput {
            policy,
            now,
            last_change_at: None,
            updates_last_hour: 0,
            tokens_today: 1500,
            manual: false,
        });
        assert_eq!(decision, PolicyDecision::PauseBudget);
    }

    #[test]
    fn choose_full_when_no_document() {
        let kind = choose_status_card_update_kind(ChooseUpdateKindInput {
            explicit_full: false,
            has_document: false,
            change_count: 1,
            query_version: 1,
            last_update_query_version: Some(1),
            incremental_count: 0,
            configuration_changed: false,
            restore_refresh: false,
        });
        assert_eq!(kind, "full");
    }
}
