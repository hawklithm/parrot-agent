//! Company Search Service
//!
//! 对齐 Paperclip `services/company-search.ts` 的 `CompanySearchResponse`
//! 形状（§4C.3）。本阶段落地 issue 作用域（标题/标识符/描述）的全文/分词
//! 匹配、scope/limit/offset/sort 与租户隔离；评论/文档/artifact/agent/project
//! 作用域、模糊匹配、snippet/高亮、facet 计数与 zero-result 建议、以及
//! `/search/extract` 端点仍待后续阶段。

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// 搜索作用域。Paperclip 全量还包括 comments/documents/artifacts/agents/projects；
/// 本阶段仅 issue 文本匹配可见，其余作用域按 scope 语义保留但仅返回 issue 命中。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanySearchScope {
    All,
    Issues,
    Comments,
    Documents,
    Artifacts,
    Agents,
    Projects,
}

impl CompanySearchScope {
    pub fn from_str(value: &str) -> Self {
        match value {
            "issues" => CompanySearchScope::Issues,
            "comments" => CompanySearchScope::Comments,
            "documents" => CompanySearchScope::Documents,
            "artifacts" => CompanySearchScope::Artifacts,
            "agents" => CompanySearchScope::Agents,
            "projects" => CompanySearchScope::Projects,
            _ => CompanySearchScope::All,
        }
    }

    /// 是否允许 issue 文本命中进入结果（Paperclip: all/issues/comments/documents 命中 issue）。
    pub fn includes_issues(&self) -> bool {
        matches!(
            self,
            CompanySearchScope::All
                | CompanySearchScope::Issues
                | CompanySearchScope::Comments
                | CompanySearchScope::Documents
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanySearchSort {
    Relevance,
    Updated,
    Created,
    Priority,
}

/// 对齐 Paperclip `CompanySearchIssueSummary`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchIssueSummary {
    pub id: Uuid,
    pub identifier: Option<String>,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub updated_at: String,
}

/// 对齐 Paperclip `CompanySearchResult`（issue 作用域）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchResult {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub result_type: String,
    pub score: f64,
    pub title: String,
    pub href: String,
    pub matched_fields: Vec<String>,
    pub source_label: Option<String>,
    pub snippet: Option<String>,
    pub snippets: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<CompanySearchIssueSummary>,
    pub updated_at: Option<String>,
    pub preview_image_url: Option<String>,
}

/// 对齐 Paperclip `CompanySearchResponse`（countsByType/filterOptionCounts/
/// zeroResults 在本阶段为最小实现，随作用域扩展补全）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchResponse {
    pub query: String,
    pub normalized_query: String,
    pub scope: String,
    pub sort: String,
    pub limit: i64,
    pub offset: i64,
    pub results: Vec<CompanySearchResult>,
    pub counts_by_type: std::collections::HashMap<String, i64>,
    pub filter_option_counts: serde_json::Value,
    pub zero_results: Option<serde_json::Value>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CompanySearchQuery {
    pub q: String,
    pub scope: CompanySearchScope,
    pub sort: CompanySearchSort,
    pub limit: i64,
    pub offset: i64,
}

impl Default for CompanySearchScope {
    fn default() -> Self {
        CompanySearchScope::All
    }
}

impl Default for CompanySearchSort {
    fn default() -> Self {
        CompanySearchSort::Relevance
    }
}
impl CompanySearchSort {
    pub fn from_str(value: &str) -> Self {
        match value {
            "updated" => CompanySearchSort::Updated,
            "created" => CompanySearchSort::Created,
            "priority" => CompanySearchSort::Priority,
            _ => CompanySearchSort::Relevance,
        }
    }
}

pub struct CompanySearchService {
    pool: PgPool,
}

impl CompanySearchService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 对齐 Paperclip `companySearchService.search`：issue 作用域全文/分词匹配。
    ///
    /// 命中规则（title/identifier/description 的短语 ILIKE + 分词 ILIKE ANY），
    /// 经 company_id 租户隔离；排序按 relevance（命中字段加权）/ updated /
    /// created / priority；分页 limit/offset；has_more 由 fetch+1 探测。
    pub async fn search(
        &self,
        company_id: Uuid,
        query: CompanySearchQuery,
    ) -> Result<CompanySearchResponse, sqlx::Error> {
        let normalized_query = query.q.trim().to_lowercase();
        let has_text = !normalized_query.is_empty();
        let tokens: Vec<String> = if has_text {
            normalized_query
                .split_whitespace()
                .filter(|t| t.len() >= 2)
                .map(|t| t.to_string())
                .collect()
        } else {
            Vec::new()
        };
        // 无查询文本且非 issue-only 过滤（本阶段无额外过滤）→ 空结果。
        if !has_text {
            return Ok(empty_response(&query.q, &normalized_query));
        }

        let escaped_query = escape_like(&normalized_query);
        let contains_pattern = format!("%{escaped_query}%");
        let token_patterns: Vec<String> = tokens
            .iter()
            .map(|t| {
                let e = escape_like(t);
                format!("%{e}%")
            })
            .collect();
/// 转义 LIKE 模式中的 `\ % _` 特殊字符（ILIKE 默认以反斜杠为转义符）。
fn escape_like(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 4);
    for c in value.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

        let scope_includes_issues = query.scope.includes_issues();

        // 作用域不含 issue 命中时（如纯 agents/projects/artifacts），本阶段无数据返回。
        if !scope_includes_issues {
            return Ok(empty_response(&query.q, &normalized_query));
        }

        let fetch_limit = query.limit + 1;
        let title_phrase = contains_pattern.clone();
        let ident_phrase = contains_pattern.clone();
        let desc_phrase = contains_pattern.clone();
        let token_any: String = if token_patterns.is_empty() {
            "%__paperclip_no_match__%".to_string()
        } else {
            token_patterns
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(",")
        };

        // 命中字段权重（relevance 排序用）：title 短语 > identifier 短语 >
        // title 分词 > description 短语 > identifier 分词 > description 分词。
        let order_sql = match query.sort {
            CompanySearchSort::Updated => "issues.updated_at DESC",
            CompanySearchSort::Created => "issues.created_at DESC",
            CompanySearchSort::Priority => "issues.priority DESC, issues.updated_at DESC",
            CompanySearchSort::Relevance => {
                "(CASE \
                   WHEN lower(issues.title) = $1 THEN 6 \
                   WHEN issues.title ILIKE $2 THEN 5 \
                   WHEN coalesce(issues.identifier,'') ILIKE $2 THEN 4 \
                   WHEN issues.title ILIKE ANY(string_to_array($3, ',')::text[]) THEN 3 \
                   WHEN coalesce(issues.description,'') ILIKE $2 THEN 2 \
                   WHEN coalesce(issues.identifier,'') ILIKE ANY(string_to_array($3, ',')::text[]) THEN 1 \
                   ELSE 0 END) DESC, issues.updated_at DESC"
            }
        };

        let sql = format!(
            "SELECT \
                issues.id, issues.title, issues.identifier, \
                issues.status::text, issues.priority::text, \
                issues.assignee_agent_id, issues.assignee_user_id, issues.project_id, \
                issues.updated_at, issues.created_at, issues.description \
             FROM issues \
             WHERE issues.company_id = $4 \
               AND (\
                 issues.title ILIKE $2 \
                 OR coalesce(issues.identifier,'') ILIKE $2 \
                 OR coalesce(issues.description,'') ILIKE $2 \
                 OR issues.title ILIKE ANY(string_to_array($3, ',')::text[]) \
                 OR coalesce(issues.identifier,'') ILIKE ANY(string_to_array($3, ',')::text[]) \
                 OR coalesce(issues.description,'') ILIKE ANY(string_to_array($3, ',')::text[]) \
               ) \
             ORDER BY {order_sql} \
             LIMIT $5 OFFSET $6",
        );

        let rows = sqlx::query(&sql)
            .bind(&normalized_query)
            .bind(&title_phrase)
            .bind(&token_any)
            .bind(company_id)
            .bind(fetch_limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await?;

        let has_more = rows.len() as i64 > query.limit;
        let page = if has_more { &rows[..rows.len() - 1] } else { &rows[..] };

        let mut results = Vec::with_capacity(page.len());
        let mut counts: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for row in page {
            let id: Uuid = row.get("id");
            let title: String = row.get("title");
            let identifier: Option<String> = row.get("identifier");
            let status: String = row.get("status");
            let priority: String = row.get("priority");
            let assignee_agent_id: Option<Uuid> = row.get("assignee_agent_id");
            let assignee_user_id: Option<Uuid> = row.get("assignee_user_id");
            let project_id: Option<Uuid> = row.get("project_id");
            let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
            let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");

            // 命中字段探测（relevance 计分 + matchedFields）。
            let mut matched_fields = Vec::new();
            let mut score = 0.0f64;
            if title.to_lowercase() == normalized_query {
                score += 6.0;
                matched_fields.push("title".to_string());
            } else if title.to_lowercase().contains(&normalized_query) {
                score += 5.0;
                matched_fields.push("title".to_string());
            }
            if let Some(ident) = &identifier {
                if ident.to_lowercase().contains(&normalized_query) {
                    score += 4.0;
                    matched_fields.push("identifier".to_string());
                }
            }
            if let Some(desc) = row.try_get::<String, _>("description").ok() {
                if desc.to_lowercase().contains(&normalized_query) {
                    score += 2.0;
                    matched_fields.push("description".to_string());
                }
            }

            let href = match &identifier {
                Some(ident) => format!("/company/issues/{ident}"),
                None => format!("/company/issues/{id}"),
            };

            let updated_at_str = updated_at.to_rfc3339();

            let issue_summary = CompanySearchIssueSummary {
                id,
                identifier: identifier.clone(),
                title: title.clone(),
                status,
                priority,
                assignee_agent_id,
                assignee_user_id,
                project_id,
                updated_at: updated_at_str.clone(),
            };

            results.push(CompanySearchResult {
                id,
                result_type: "issue".to_string(),
                score,
                title,
                href,
                matched_fields,
                source_label: identifier,
                snippet: None,
                snippets: Vec::new(),
                issue: Some(issue_summary),
                updated_at: Some(updated_at_str),
                preview_image_url: None,
            });
            *counts.entry("issue".to_string()).or_insert(0) += 1;
        }

        Ok(CompanySearchResponse {
            query: query.q.clone(),
            normalized_query,
            scope: scope_name(&query.scope).to_string(),
            sort: sort_name(&query.sort).to_string(),
            limit: query.limit,
            offset: query.offset,
            results,
            counts_by_type: counts,
            filter_option_counts: serde_json::json!({}),
            zero_results: None,
            has_more,
        })
    }
}

fn scope_name(scope: &CompanySearchScope) -> &'static str {
    match scope {
        CompanySearchScope::All => "all",
        CompanySearchScope::Issues => "issues",
        CompanySearchScope::Comments => "comments",
        CompanySearchScope::Documents => "documents",
        CompanySearchScope::Artifacts => "artifacts",
        CompanySearchScope::Agents => "agents",
        CompanySearchScope::Projects => "projects",
    }
}

fn sort_name(sort: &CompanySearchSort) -> &'static str {
    match sort {
        CompanySearchSort::Relevance => "relevance",
        CompanySearchSort::Updated => "updated",
        CompanySearchSort::Created => "created",
        CompanySearchSort::Priority => "priority",
    }
}

fn empty_response(query: &str, normalized_query: &str) -> CompanySearchResponse {
    CompanySearchResponse {
        query: query.to_string(),
        normalized_query: normalized_query.to_string(),
        scope: "all".to_string(),
        sort: "relevance".to_string(),
        limit: 0,
        offset: 0,
        results: Vec::new(),
        counts_by_type: std::collections::HashMap::new(),
        filter_option_counts: serde_json::json!({}),
        zero_results: None,
        has_more: false,
    }
}
