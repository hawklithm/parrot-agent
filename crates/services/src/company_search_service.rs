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

    /// 是否把评论/文档命中纳入候选（Paperclip: all/comments/documents 触发
    /// commentMatches/documentMatches CTE），并据此把 issue 纳入结果。
    pub fn includes_comments_or_documents(&self) -> bool {
        matches!(
            self,
            CompanySearchScope::All | CompanySearchScope::Comments | CompanySearchScope::Documents
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
    pub snippets: Vec<CompanySearchSnippet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<CompanySearchIssueSummary>,
    pub updated_at: Option<String>,
    pub preview_image_url: Option<String>,
}

/// 对齐 Paperclip `CompanySearchHighlight`：原文中的命中区间（字符偏移）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchHighlight {
    pub start: usize,
    pub end: usize,
}

/// 对齐 Paperclip `CompanySearchSnippet`：单字段截断摘录 + 命中高亮区间。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchSnippet {
    pub field: String,
    pub label: String,
    pub text: String,
    pub highlights: Vec<CompanySearchHighlight>,
}

/// 对齐 Paperclip `CompanySearchFilterOptionCounts`：候选命中集上的 facet 计数。
/// status/priority/updatedWithin 为离散枚举计数，assignee/project/label 为 id→计数。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchFilterOptionCounts {
    pub status: std::collections::HashMap<String, i64>,
    pub priority: std::collections::HashMap<String, i64>,
    pub assignee_agent_id: std::collections::HashMap<String, i64>,
    pub assignee_user_id: std::collections::HashMap<String, i64>,
    pub project_id: std::collections::HashMap<String, i64>,
    pub label_id: std::collections::HashMap<String, i64>,
    pub updated_within: std::collections::HashMap<String, i64>,
}

impl Default for CompanySearchFilterOptionCounts {
    fn default() -> Self {
        Self {
            status: std::collections::HashMap::new(),
            priority: std::collections::HashMap::new(),
            assignee_agent_id: std::collections::HashMap::new(),
            assignee_user_id: std::collections::HashMap::new(),
            project_id: std::collections::HashMap::new(),
            label_id: std::collections::HashMap::new(),
            updated_within: std::collections::HashMap::new(),
        }
    }
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
    pub filter_option_counts: CompanySearchFilterOptionCounts,
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

// ============ Company Search Extract（§4C.3 `/search/extract`） ============
//
// 对齐 Paperclip `services/company-search-extract.ts` 的
// `CompanySearchExtractResponse`。给定 `contains` 文本（或 URL 模式），
// 在 issue 标题/描述、评论、文档中找出命中片段并截断摘录。

/// 提取匹配类型：字面子串（literal）或 URL 模式（url）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanySearchExtractKind {
    Literal,
    Url,
}

impl Default for CompanySearchExtractKind {
    fn default() -> Self {
        CompanySearchExtractKind::Literal
    }
}

/// 提取作用域（对齐 Paperclip `CompanySearchExtractQuery.scope`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanySearchExtractScope {
    All,
    Issues,
    Comments,
    Documents,
}

impl CompanySearchExtractScope {
    pub fn from_str(value: &str) -> Self {
        match value {
            "issues" => CompanySearchExtractScope::Issues,
            "comments" => CompanySearchExtractScope::Comments,
            "documents" => CompanySearchExtractScope::Documents,
            _ => CompanySearchExtractScope::All,
        }
    }
    pub fn includes(&self, source: CompanySearchExtractScope) -> bool {
        *self == CompanySearchExtractScope::All || *self == source
    }
}

impl Default for CompanySearchExtractScope {
    fn default() -> Self {
        CompanySearchExtractScope::All
    }
}

/// 单条命中片段来源（issue/comment/document）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchExtractMatchSource {
    #[serde(rename = "type")]
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_key: Option<String>,
}

/// 单条命中片段（对齐 Paperclip `CompanySearchExtractMatch`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchExtractMatch {
    pub value: String,
    pub field: String,
    pub label: String,
    pub excerpt: String,
    pub excerpt_truncated: bool,
    pub source: CompanySearchExtractMatchSource,
}

/// 单个 issue 的提取结果（对齐 Paperclip `CompanySearchExtractIssueResult`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchExtractIssueResult {
    pub issue_id: Uuid,
    pub identifier: Option<String>,
    pub title: String,
    pub status: String,
    pub assignee_agent_id: Option<Uuid>,
    pub updated_at: String,
    pub matches: Vec<CompanySearchExtractMatch>,
    pub matches_truncated: bool,
}

/// 提取响应（对齐 Paperclip `CompanySearchExtractResponse`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchExtractResponse {
    pub contains: String,
    pub kind: String,
    pub scope: String,
    pub limit: i64,
    pub offset: i64,
    pub matches_per_issue: i64,
    pub results: Vec<CompanySearchExtractIssueResult>,
    pub has_more: bool,
    pub truncated: bool,
}

/// `extract` 查询参数（已解析 + 校验）。
#[derive(Debug, Clone, Default)]
pub struct CompanySearchExtractQuery {
    pub contains: String,
    pub kind: CompanySearchExtractKind,
    pub scope: CompanySearchExtractScope,
    pub limit: i64,
    pub offset: i64,
    pub matches_per_issue: i64,
    pub statuses: Vec<String>,
    pub updated_within: Option<String>,
    pub updated_after: Option<String>,
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

        // 模糊匹配开关：对齐 Paperclip MIN_FUZZY_QUERY_LENGTH=4 且无 LIKE 通配符。
        // 仅对 identifier 做 pg_trgm similarity（精确对齐 Paperclip fuzzyIdentifierMatch）；
        // 标题模糊为 Levenshtein 分词方案，本阶段以 trigram 近似，避免过度噪声故仅 identifier。
        let fuzzy_enabled = normalized_query.len() >= 4 && !normalized_query.contains(['\\', '%', '_']);
        const FUZZY_IDENTIFIER_SIMILARITY_THRESHOLD: f64 = 0.45;


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

        // 高亮术语集（对齐 Paperclip matchTerms）：normalized_query + 各 token，去重。
        let terms: Vec<String> = {
            let mut t = vec![normalized_query.clone()];
            t.extend(token_patterns.iter().cloned());
            t.retain(|s| !s.is_empty());
            t.dedup();
            t
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

        // 候选 WHERE 的 OR 命中条件：issue 文本匹配 +（作用域含 comments/documents 时）
        // 评论/文档匹配的 EXISTS。Paperclip 的 anySearchMatch 不随 scope 收窄候选，
        // 仅影响结果呈现，故 comment/document EXISTS 在 all/issues/comments/documents 作用域加入。
        let mut match_conditions: Vec<String> = vec![
            "issues.title ILIKE $2".to_string(),
            "coalesce(issues.identifier,'') ILIKE $2".to_string(),
            "coalesce(issues.description,'') ILIKE $2".to_string(),
            "issues.title ILIKE ANY(string_to_array($3, ',')::text[])".to_string(),
            "coalesce(issues.identifier,'') ILIKE ANY(string_to_array($3, ',')::text[])".to_string(),
            "coalesce(issues.description,'') ILIKE ANY(string_to_array($3, ',')::text[])".to_string(),
        ];
        if fuzzy_enabled {
            // 对齐 Paperclip fuzzyIdentifierMatch：identifier 与查询的 pg_trgm 相似度阈值。
            match_conditions.push(
                "similarity(lower(coalesce(issues.identifier,'')), $8) >= 0.45".to_string(),
            );
        }
        if query.scope.includes_comments_or_documents() {
            match_conditions.push(
                "EXISTS (SELECT 1 FROM issue_comments sc WHERE sc.company_id = $4 \
                 AND sc.issue_id = issues.id AND sc.body ILIKE $7)".to_string(),
            );
            match_conditions.push(
                "EXISTS (SELECT 1 FROM issue_documents idoc \
                 INNER JOIN documents d ON d.id = idoc.document_id AND d.company_id = idoc.company_id \
                 WHERE idoc.company_id = $4 AND idoc.issue_id = issues.id \
                 AND (d.title ILIKE $7 OR d.content ILIKE $7))".to_string(),
            );
        }
        let where_sql = match_conditions.join(" OR ");

        let sql = format!(
            "SELECT \
                issues.id, issues.title, issues.identifier, \
                issues.status::text, issues.priority::text, \
                issues.assignee_agent_id, issues.assignee_user_id, issues.project_id, \
                issues.updated_at, issues.created_at, issues.description, \
                similarity(lower(coalesce(issues.identifier,'')), $8)::double precision AS ident_sim \
             FROM issues \
             WHERE issues.company_id = $4 \
               AND ({where_sql}) \
             ORDER BY {order_sql} \
             LIMIT $5 OFFSET $6",
        );
        // 评论/文档命中探测：对分页内的 issue 批量查询匹配的评论/文档原文，
        // 用于补充 matchedFields / countsByType / snippets（对齐 Paperclip comment/document CTE）。
        let mut comment_doc_hits: std::collections::HashMap<
            Uuid,
            (Option<String>, Option<(String, String)>),
        > = std::collections::HashMap::new();
        let rows = sqlx::query(&sql)
            .bind(&normalized_query)
            .bind(&title_phrase)
            .bind(&token_any)
            .bind(company_id)
            .bind(fetch_limit)
            .bind(query.offset)
            .bind(&contains_pattern)
            .bind(&normalized_query)
            .fetch_all(&self.pool)
            .await?;

        let has_more = rows.len() as i64 > query.limit;
        let page = if has_more { &rows[..rows.len() - 1] } else { &rows[..] };

        if query.scope.includes_comments_or_documents() && !page.is_empty() {
            let page_ids: Vec<Uuid> = page.iter().map(|r| r.get::<Uuid, _>("id")).collect();
            let crows = sqlx::query(
                "SELECT issue_id, body FROM issue_comments \
                 WHERE company_id = $1 AND issue_id = ANY($2) AND body ILIKE $3",
            )
            .bind(company_id)
            .bind(&page_ids)
            .bind(&contains_pattern)
            .fetch_all(&self.pool)
            .await?;
            for r in crows {
                let iid: Uuid = r.get("issue_id");
                let body: String = r.get("body");
                let entry = comment_doc_hits.entry(iid).or_insert((None, None));
                entry.0 = Some(body);
            }
        let drows = sqlx::query(
                "SELECT DISTINCT idoc.issue_id, d.title, d.content FROM issue_documents idoc \
                 INNER JOIN documents d ON d.id = idoc.document_id AND d.company_id = idoc.company_id \
                 WHERE idoc.company_id = $1 AND idoc.issue_id = ANY($2) \
                 AND (d.title ILIKE $3 OR d.content ILIKE $3)",
            )
            .bind(company_id)
            .bind(&page_ids)
            .bind(&contains_pattern)
            .fetch_all(&self.pool)
            .await?;
            for r in drows {
                let iid: Uuid = r.get("issue_id");
                let title: String = r.get("title");
                let content: String = r.get("content");
                let entry = comment_doc_hits.entry(iid).or_insert((None, None));
                entry.1 = Some((title, content));
            }
        }


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
            if fuzzy_enabled {
                let ident_sim: f64 = row.try_get("ident_sim").unwrap_or(0.0);
                if ident_sim >= FUZZY_IDENTIFIER_SIMILARITY_THRESHOLD && !matched_fields.contains(&"identifier".to_string()) {
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

            // 评论/文档命中补充 matchedFields + 收集 snippet 源文本。
            let mut snippet_sources: Vec<(String, String, String)> = Vec::new();
            if let Some((comment_body, doc)) = comment_doc_hits.get(&id) {
                if let Some(body) = comment_body {
                    matched_fields.push("comment".to_string());
                    snippet_sources.push(("comment".to_string(), "Comment".to_string(), body.clone()));
                }
                if let Some((doc_title, doc_content)) = doc {
                    matched_fields.push("document".to_string());
                    snippet_sources.push((
                        "document".to_string(),
                        doc_title.clone(),
                        doc_content.clone(),
                    ));
                }
            }

            // 标题/描述命中也生成 snippet（对齐 Paperclip selectPrimarySnippets 优先级）。
            if title.to_lowercase().contains(&normalized_query) {
                snippet_sources.push(("title".to_string(), "Title".to_string(), title.clone()));
            }
            if let Some(desc) = row.try_get::<String, _>("description").ok() {
                if desc.to_lowercase().contains(&normalized_query) {
                    snippet_sources.push(("description".to_string(), "Description".to_string(), desc));
                }
            }
            let snippets = build_issue_snippets(&snippet_sources, &terms);
            let snippet = snippets.first().map(|s| s.text.clone());

            // previewImageUrl：描述 → 评论 → 文档 的 markdown 首图（对齐 Paperclip
            // extractFirstImageUrl 优先级；文档用 content 近似 latest_body）。
            let desc_raw = row.try_get::<String, _>("description").ok();
            let preview_image_url = desc_raw
                .as_deref()
                .and_then(extract_first_image_url)
                .or_else(|| {
                    comment_doc_hits.get(&id).and_then(|(cb, _)| {
                        cb.as_deref().and_then(extract_first_image_url)
                    })
                })
                .or_else(|| {
                    comment_doc_hits.get(&id).and_then(|(_, doc)| {
                        doc.as_ref().and_then(|(_, content)| extract_first_image_url(content))
                    })
                });

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
                snippet,
                snippets,
                issue: Some(issue_summary),
                updated_at: Some(updated_at_str),
                preview_image_url,
            });
            *counts.entry("issue".to_string()).or_insert(0) += 1;
            if let Some((comment_body, doc)) = comment_doc_hits.get(&id) {
                if comment_body.is_some() {
                    *counts.entry("comment".to_string()).or_insert(0) += 1;
                }
                if doc.is_some() {
                    *counts.entry("document".to_string()).or_insert(0) += 1;
                }
            }
        }

        // facet 计数：对全候选命中集（非仅分页）聚合，对齐 Paperclip filterOptionCounts。
        // 参数重编号：where_sql 用 $2/$3/$4/$7/$8 → $1/$2/$3/$4/$5（见 fetch_filter_option_counts）。
        let filter_option_counts = self
            .fetch_filter_option_counts(
                company_id,
                &where_sql,
                &title_phrase,
                &token_any,
                &contains_pattern,
                &normalized_query,
            )
            .await?;

        Ok(CompanySearchResponse {
            query: query.q.clone(),
            normalized_query,
            scope: scope_name(&query.scope).to_string(),
            sort: sort_name(&query.sort).to_string(),
            limit: query.limit,
            offset: query.offset,
            results,
            counts_by_type: counts,
            filter_option_counts,
            zero_results: None,
            has_more,
        })
    }

    /// 对齐 Paperclip `filterOptionCounts`：在候选命中集（同一 `match_conditions`）
    /// 上聚合 status/priority/assigneeAgentId/assigneeUserId/projectId/labelId/
    /// updatedWithin(24h/7d/30d/90d) 的计数。Parrot 尚无活动过滤器，
    /// 故每个 facet 均不带 `optionCond`/`omit`（对齐 Paperclip 无过滤时的语义）。
    async fn fetch_filter_option_counts(
        &self,
        company_id: Uuid,
        where_sql: &str,
        title_phrase: &str,
        token_any: &str,
        contains_pattern: &str,
        normalized_query: &str,
    ) -> Result<CompanySearchFilterOptionCounts, sqlx::Error> {
        // 重编号 where_sql 参数占位符（从高到低避免二次替换）：
        // $8->$5, $7->$4, $4->$3, $3->$2, $2->$1
        // 用哨兵占位符防止级联二次替换（$4->$3 后再被 $3->$2 命中）。
        let mut ws = where_sql.to_string();
        for (from, to) in [("$8", "$5"), ("$7", "$4"), ("$4", "$3"), ("$3", "$2"), ("$2", "$1")] {
            ws = ws.replace(from, &format!("\u{0}{}", to.trim_start_matches('$')));
        }
        for n in 1..=5 {
            ws = ws.replace(&format!("\u{0}{}", n), &format!("${}", n));
        }
        // 候选条件引用 `issues.` 表名；facet 查询以 `i` 为别名，需整体替换。
        ws = ws.replace("issues.", "i.");
        let sql = format!(
            "SELECT 'status' AS kind, i.status::text AS value, count(*)::int8 AS count \
             FROM issues i WHERE i.company_id = $3 AND ({ws}) GROUP BY 2 \
             UNION ALL \
             SELECT 'priority' AS kind, i.priority::text AS value, count(*)::int8 AS count \
             FROM issues i WHERE i.company_id = $3 AND ({ws}) GROUP BY 2 \
             UNION ALL \
             SELECT 'assigneeAgentId' AS kind, i.assignee_agent_id::text AS value, count(*)::int8 AS count \
             FROM issues i WHERE i.company_id = $3 AND ({ws}) AND i.assignee_agent_id IS NOT NULL GROUP BY 2 \
             UNION ALL \
             SELECT 'assigneeUserId' AS kind, i.assignee_user_id::text AS value, count(*)::int8 AS count \
             FROM issues i WHERE i.company_id = $3 AND ({ws}) AND i.assignee_user_id IS NOT NULL GROUP BY 2 \
             UNION ALL \
             SELECT 'projectId' AS kind, i.project_id::text AS value, count(*)::int8 AS count \
             FROM issues i WHERE i.company_id = $3 AND ({ws}) AND i.project_id IS NOT NULL GROUP BY 2 \
             UNION ALL \
             SELECT 'labelId' AS kind, il.label_id::text AS value, count(*)::int8 AS count \
             FROM issues i INNER JOIN issue_labels il ON il.issue_id = i.id \
             WHERE i.company_id = $3 AND ({ws}) GROUP BY 2 \
             UNION ALL \
             SELECT 'updatedWithin' AS kind, '24h' AS value, count(*)::int8 AS count \
             FROM issues i WHERE i.company_id = $3 AND ({ws}) AND i.updated_at >= now() - interval '24 hours' \
             UNION ALL \
             SELECT 'updatedWithin' AS kind, '7d' AS value, count(*)::int8 AS count \
             FROM issues i WHERE i.company_id = $3 AND ({ws}) AND i.updated_at >= now() - interval '7 days' \
             UNION ALL \
             SELECT 'updatedWithin' AS kind, '30d' AS value, count(*)::int8 AS count \
             FROM issues i WHERE i.company_id = $3 AND ({ws}) AND i.updated_at >= now() - interval '30 days' \
             UNION ALL \
             SELECT 'updatedWithin' AS kind, '90d' AS value, count(*)::int8 AS count \
             FROM issues i WHERE i.company_id = $3 AND ({ws}) AND i.updated_at >= now() - interval '90 days'"
        );
        let mut q = sqlx::query(&sql)
            .bind(title_phrase)      // $1（原 $2）
            .bind(token_any)         // $2（原 $3）
            .bind(company_id);       // $3
        // Postgres 参数编号必须连续到最高引用位：where_sql 可能引用 $5 而不引用 $4
        // （模糊开、评论/文档 EXISTS 关），此时 $4 位置仍需占位绑定。
        let max_param = if ws.contains("$5") {
            5
        } else if ws.contains("$4") {
            4
        } else {
            3
        };
        let mut q = sqlx::query(&sql)
            .bind(title_phrase)      // $1（原 $2）
            .bind(token_any)         // $2（原 $3）
            .bind(company_id);       // $3
        if max_param >= 4 {
            q = q.bind(contains_pattern); // $4（原 $7；未引用时占位）
        }
        if max_param >= 5 {
            q = q.bind(normalized_query); // $5（原 $8）
        }
        let rows = q.fetch_all(&self.pool).await?;
        let mut out = CompanySearchFilterOptionCounts::default();
        for row in rows {
            let kind: String = row.get("kind");
            let value: String = row.get("value");
            let count: i64 = row.get("count");
            match kind.as_str() {
                "status" => { out.status.insert(value, count); }
                "priority" => { out.priority.insert(value, count); }
                "assigneeAgentId" => { out.assignee_agent_id.insert(value, count); }
                "assigneeUserId" => { out.assignee_user_id.insert(value, count); }
                "projectId" => { out.project_id.insert(value, count); }
                "labelId" => { out.label_id.insert(value, count); }
                "updatedWithin" => { out.updated_within.insert(value, count); }
                _ => {}
            }
        }
        Ok(out)
    }

    /// 对齐 Paperclip `companySearchExtractService.extract`：在 issue 标题/描述、
    /// 评论、文档中查找 `contains` 命中片段并生成截断摘录。
    ///
    /// 作用域由 `query.scope` 决定（issues/comments/documents/all）；`kind` 控制
    /// literal（子串 ILIKE）或 url（URL 正则）；命中数受 `matches_per_issue` 限制，
    /// 分页 `limit`/`offset` 作用于 issue 候选；`has_more`/`truncated` 由 fetch+1 探测。
    pub async fn extract(
        &self,
        company_id: Uuid,
        query: CompanySearchExtractQuery,
    ) -> Result<CompanySearchExtractResponse, sqlx::Error> {
        let contains = query.contains.trim();
        let contains_pattern = format!("%{}%", escape_like(contains));

        let is_url = query.kind == CompanySearchExtractKind::Url;

        // issue 候选：命中任一作用域的 contentMatch。
        let mut scope_conditions = Vec::new();
        if query.scope.includes(CompanySearchExtractScope::Issues) {
            scope_conditions.push(format!(
                "(issues.title ILIKE $2 OR issues.description ILIKE $2)"
            ));
        }
        if query.scope.includes(CompanySearchExtractScope::Comments) {
            scope_conditions.push(format!(
                "EXISTS (SELECT 1 FROM issue_comments ec WHERE ec.company_id = $1 \
                 AND ec.issue_id = issues.id AND ec.body ILIKE $2)"
            ));
        }
        if query.scope.includes(CompanySearchExtractScope::Documents) {
            scope_conditions.push(format!(
                "EXISTS (SELECT 1 FROM issue_documents idoc \
                 INNER JOIN documents d ON d.id = idoc.document_id AND d.company_id = idoc.company_id \
                 WHERE idoc.company_id = $1 AND idoc.issue_id = issues.id \
                 AND (d.title ILIKE $2 OR d.content ILIKE $2))"
            ));
        }
        let scope_sql = if scope_conditions.is_empty() {
            "false".to_string()
        } else {
            scope_conditions.join(" OR ")
        };

        let mut conditions = vec![
            "issues.company_id = $1".to_string(),
            "issues.hidden_at IS NULL".to_string(),
            format!("({scope_sql})"),
        ];
        if !query.statuses.is_empty() {
            let placeholders: Vec<String> = query
                .statuses
                .iter()
                .enumerate()
                .map(|(i, _)| format!("${}", 7 + i))
                .collect();
            conditions.push(format!("issues.status::text = ANY(ARRAY[{}])", placeholders.join(",")));
        }
        if let Some(within) = &query.updated_within {
            if let Some(dt) = parse_updated_within(within) {
                conditions.push(format!("issues.updated_at >= '{dt}'"));
            }
        }
        if let Some(after) = &query.updated_after {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(after) {
                let dt = dt.with_timezone(&chrono::Utc);
                conditions.push(format!("issues.updated_at >= '{dt}'"));
            }
        }

        let fetch_limit = query.limit + 1;
        let sql = format!(
            "SELECT issues.id, issues.identifier, issues.title, issues.status::text, \
             issues.assignee_agent_id, issues.updated_at, issues.description \
             FROM issues \
             WHERE {} \
             ORDER BY issues.updated_at DESC, issues.id DESC \
             LIMIT {} OFFSET {}",
            conditions.join(" AND "),
            fetch_limit,
            query.offset
        );

        let mut q = sqlx::query(&sql).bind(company_id).bind(&contains_pattern);
        for st in &query.statuses {
            q = q.bind(st);
        }
        let rows = q.fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > query.limit;
        let page = if has_more { &rows[..rows.len() - 1] } else { &rows[..] };

        // 收集每个 issue 的命中来源文本（使用模块级 SourceForMatch）。
        let mut sources_by_issue: std::collections::HashMap<Uuid, Vec<SourceForMatch>> =
            std::collections::HashMap::new();
        let add_source = |map: &mut std::collections::HashMap<Uuid, Vec<SourceForMatch>>, s: SourceForMatch| {
            map.entry(s.issue_id).or_default().push(s);
        };

        if query.scope.includes(CompanySearchExtractScope::Issues) {
            for row in page {
                let id: Uuid = row.get("id");
                let title: String = row.get("title");
                let description: Option<String> = row.try_get("description").ok().flatten();
                if source_occurrence_count(&title, contains, is_url) > 0 {
                    add_source(
                        &mut sources_by_issue,
                        SourceForMatch {
                            issue_id: id,
                            field: "title",
                            label: "Issue title".to_string(),
                            text: title,
                            source: CompanySearchExtractMatchSource {
                                source_type: "issue".to_string(),
                                issue_id: Some(id),
                                comment_id: None,
                                document_id: None,
                                document_key: None,
                            },
                        },
                    );
                }
                if let Some(desc) = description {
                    if source_occurrence_count(&desc, contains, is_url) > 0 {
                        add_source(
                            &mut sources_by_issue,
                            SourceForMatch {
                                issue_id: id,
                                field: "description",
                                label: "Issue description".to_string(),
                                text: desc,
                                source: CompanySearchExtractMatchSource {
                                    source_type: "issue".to_string(),
                                    issue_id: Some(id),
                                    comment_id: None,
                                    document_id: None,
                                    document_key: None,
                                },
                            },
                        );
                    }
                }
            }
        }

        let issue_ids: Vec<Uuid> = page.iter().map(|r| r.get::<Uuid, _>("id")).collect();
        if !issue_ids.is_empty() {
            if query.scope.includes(CompanySearchExtractScope::Comments) {
                let csql = format!(
                    "SELECT id, issue_id, body FROM issue_comments \
                     WHERE company_id = $1 AND issue_id = ANY($2) AND body ILIKE $3 \
                     ORDER BY created_at ASC, id ASC"
                );
                let crows = sqlx::query(&csql)
                    .bind(company_id)
                    .bind(&issue_ids)
                    .bind(&contains_pattern)
                    .fetch_all(&self.pool)
                    .await?;
                for row in crows {
                    let id: Uuid = row.get("id");
                    let issue_id: Uuid = row.get("issue_id");
                    let body: String = row.get("body");
                    add_source(
                        &mut sources_by_issue,
                        SourceForMatch {
                            issue_id,
                            field: "comment",
                            label: "Comment".to_string(),
                            text: body,
                            source: CompanySearchExtractMatchSource {
                                source_type: "comment".to_string(),
                                issue_id: Some(issue_id),
                                comment_id: Some(id),
                                document_id: None,
                                document_key: None,
                            },
                        },
                    );
                }
            }

            if query.scope.includes(CompanySearchExtractScope::Documents) {
                let dsql = format!(
                    "SELECT d.id AS doc_id, idoc.issue_id, idoc.key, d.title, d.content \
                     FROM issue_documents idoc \
                     INNER JOIN documents d ON d.id = idoc.document_id AND d.company_id = idoc.company_id \
                     WHERE idoc.company_id = $1 AND idoc.issue_id = ANY($2) AND (d.title ILIKE $3 OR d.content ILIKE $3) \
                     ORDER BY idoc.key ASC, d.id ASC"
                );
                let drows = sqlx::query(&dsql)
                    .bind(company_id)
                    .bind(&issue_ids)
                    .bind(&contains_pattern)
                    .fetch_all(&self.pool)
                    .await?;
                for row in drows {
                    let doc_id: Uuid = row.get("doc_id");
                    let issue_id: Uuid = row.get("issue_id");
                    let key: String = row.get("key");
                    let doc_title: String = row.get("title");
                    let content: String = row.get("content");
                    let source = CompanySearchExtractMatchSource {
                        source_type: "document".to_string(),
                        issue_id: Some(issue_id),
                        comment_id: None,
                        document_id: Some(doc_id),
                        document_key: Some(key.clone()),
                    };
                    if source_occurrence_count(&doc_title, contains, is_url) > 0 {
                        add_source(
                            &mut sources_by_issue,
                            SourceForMatch {
                                issue_id,
                                field: "document_title",
                                label: format!("Document title ({key})"),
                                text: doc_title,
                                source: source.clone(),
                            },
                        );
                    }
                    if source_occurrence_count(&content, contains, is_url) > 0 {
                        add_source(
                            &mut sources_by_issue,
                            SourceForMatch {
                                issue_id,
                                field: "document_body",
                                label: format!("Document ({key})"),
                                text: content,
                                source,
                            },
                        );
                    }
                }
            }
        }

        let mut results = Vec::with_capacity(page.len());
        let mut any_truncated = false;
        for row in page {
            let id: Uuid = row.get("id");
            let identifier: Option<String> = row.get("identifier");
            let title: String = row.get("title");
            let status: String = row.get("status");
            let assignee_agent_id: Option<Uuid> = row.get("assignee_agent_id");
            let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

            let sources = sources_by_issue.get(&id);
            let (matches, matches_truncated) = extract_matches(
                sources.map(|s| s.as_slice()).unwrap_or(&[]),
                contains,
                is_url,
                query.matches_per_issue,
            );
            if matches_truncated {
                any_truncated = true;
            }

            results.push(CompanySearchExtractIssueResult {
                issue_id: id,
                identifier,
                title,
                status,
                assignee_agent_id,
                updated_at: updated_at.to_rfc3339(),
                matches,
                matches_truncated,
            });
        }

        let truncated = has_more || any_truncated;
        Ok(CompanySearchExtractResponse {
            contains: query.contains,
            kind: match query.kind {
                CompanySearchExtractKind::Literal => "literal",
                CompanySearchExtractKind::Url => "url",
            }
            .to_string(),
            scope: match query.scope {
                CompanySearchExtractScope::All => "all",
                CompanySearchExtractScope::Issues => "issues",
                CompanySearchExtractScope::Comments => "comments",
                CompanySearchExtractScope::Documents => "documents",
            }
            .to_string(),
            limit: query.limit,
            offset: query.offset,
            matches_per_issue: query.matches_per_issue,
            results,
            has_more,
            truncated,
        })
    }
}

// ============ Extract 匹配/摘录辅助函数（对齐 Paperclip company-search-extract.ts） ============

const EXCERPT_MAX_CHARS: usize = 180;

/// URL 匹配锚点：本实现以「按空白/引号切分的 token 中包含 contains 子串」近似
/// Paperclip 的 URL 正则；返回 contains 本身供 occurrence 计数复用（对齐语义）。
fn url_contains_pattern(contains: &str) -> String {
    contains.to_string()
}

/// 计算文本中 `contains` 的命中次数（literal 子串 / url token 子串），对齐
/// Paperclip `literalOccurrences` / `urlOccurrences`。
fn source_occurrence_count(text: &str, contains: &str, is_url: bool) -> usize {
    if is_url {
        let contains_lower = contains.to_lowercase();
        text.split(|c: char| c.is_whitespace() || c == '"' || c == '<' || c == '>' || c == '`')
            .filter(|tok| !tok.is_empty() && tok.to_lowercase().contains(&contains_lower))
            .count()
    } else {
        let lower = text.to_lowercase();
        let contains_lower = contains.to_lowercase();
        if contains_lower.is_empty() {
            return 0;
        }
        let mut count = 0;
        let mut start = 0;
        while start + contains_lower.len() <= lower.len() {
            match lower[start..].find(&contains_lower) {
                Some(idx) => {
                    count += 1;
                    start += idx + contains_lower.len().max(1);
                }
                None => break,
            }
        }
        count
    }
}

/// 对齐 Paperclip `excerpt`：超过 180 字符则截断并加 `…`，折叠空白。
fn make_excerpt(text: &str, start: usize, length: usize) -> (String, bool) {
    if text.len() <= EXCERPT_MAX_CHARS {
        return (collapse_whitespace(text), false);
    }
    let context = (EXCERPT_MAX_CHARS.saturating_sub(length)) / 2;
    let mut excerpt_start = start.saturating_sub(context);
    let excerpt_end = (excerpt_start + EXCERPT_MAX_CHARS).min(text.len());
    excerpt_start = excerpt_end.saturating_sub(EXCERPT_MAX_CHARS);
    let prefix = if excerpt_start > 0 { "…" } else { "" };
    let suffix = if excerpt_end < text.len() { "…" } else { "" };
    let body = collapse_whitespace(&text[excerpt_start..excerpt_end]);
    (format!("{prefix}{body}{suffix}"), true)
}

fn collapse_whitespace(s: &str) -> String {
    let trimmed: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    trimmed
}

/// 对齐 Paperclip `extractMatches`：遍历 source 文本找命中，按 `matches_per_issue`
/// 截断；value 小写去重；excerpt 由命中位置生成。
fn extract_matches(
    sources: &[SourceForMatch],
    contains: &str,
    is_url: bool,
    matches_per_issue: i64,
) -> (Vec<CompanySearchExtractMatch>, bool) {
    let mut matches = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut matches_truncated = false;
    let cap = matches_per_issue.max(1) as usize;
    for src in sources {
        let occurrences = literal_or_url_occurrences(&src.text, contains, is_url);
        for occ in occurrences {
            if seen.contains(&occ.value.to_lowercase()) {
                continue;
            }
            seen.insert(occ.value.to_lowercase());
            if matches.len() >= cap {
                matches_truncated = true;
                continue;
            }
            let (excerpt, excerpt_truncated) = make_excerpt(&src.text, occ.start, occ.value.len());
            matches.push(CompanySearchExtractMatch {
                value: occ.value,
                field: src.field.to_string(),
                label: src.label.clone(),
                excerpt,
                excerpt_truncated,
                source: src.source.clone(),
            });
        }
    }
    (matches, matches_truncated)
}

/// 单条 source 在 `extract_matches` 中使用；对齐 Paperclip `ExtractSource`。
struct SourceForMatch {
    issue_id: Uuid,
    field: &'static str,
    label: String,
    text: String,
    source: CompanySearchExtractMatchSource,
}

/// 对齐 Paperclip `literalOccurrences` / `urlOccurrences` 返回的 {value,start}。
fn literal_or_url_occurrences(text: &str, contains: &str, is_url: bool) -> Vec<Occurrence> {
    if is_url {
        let contains_lower = contains.to_lowercase();
        text.split(|c: char| c.is_whitespace() || c == '"' || c == '<' || c == '>' || c == '`')
            .enumerate()
            .filter_map(|(i, tok)| {
                if tok.is_empty() || !tok.to_lowercase().contains(&contains_lower) {
                    return None;
                }
                // 近似 start：用累计长度（仅用于 excerpt 上下文，足够）。
                Some(Occurrence {
                    value: tok.to_string(),
                    start: text.match_indices(tok).nth(i).map(|(s, _)| s).unwrap_or(0),
                })
            })
            .collect()
    } else {
        let lower = text.to_lowercase();
        let contains_lower = contains.to_lowercase();
        let mut out = Vec::new();
        if contains_lower.is_empty() {
            return out;
        }
        let mut start = 0;
        while start + contains_lower.len() <= lower.len() {
            match lower[start..].find(&contains_lower) {
                Some(idx) => {
                    let abs = start + idx;
                    out.push(Occurrence {
                        value: text[abs..abs + contains_lower.len()].to_string(),
                        start: abs,
                    });
                    start = abs + contains_lower.len().max(1);
                }
                None => break,
            }
        }
        out
    }
}

struct Occurrence {
    value: String,
    start: usize,
}

/// 对齐 Paperclip `updatedWithinStart`：解析 `1d`/`7d`/`30d` 等相对时间。
fn parse_updated_within(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let value = value.trim();
    let (num, unit) = value.split_at(value.find(|c: char| c.is_alphabetic())?);
    let num: i64 = num.parse().ok()?;
    let secs = match unit {
        "m" => num * 60,
        "h" => num * 3600,
        "d" => num * 86_400,
        "w" => num * 604_800,
        _ => return None,
    };
    let now = chrono::Utc::now();
    Some(now - chrono::Duration::seconds(secs))
}

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
        filter_option_counts: CompanySearchFilterOptionCounts::default(),
        zero_results: None,
        has_more: false,
    }
}

/// 对齐 Paperclip `createSnippet` + `selectPrimarySnippets`：
/// 将 `(field, label, source_text)` 候选转为截断摘录 + 命中高亮区间。
/// 窗口 = 首个命中前 80 字符起、最长 240 字符；前缀/后缀 `...`；最多 2 条，
/// 优先级由调用方传入顺序决定（title > comment > document > description）。
fn build_issue_snippets(
    sources: &[(String, String, String)],
    terms: &[String],
    ) -> Vec<CompanySearchSnippet> {
    let mut out = Vec::new();
    for (field, label, raw) in sources {
        if let Some(snippet) = make_snippet(field, label, raw, terms) {
            out.push(snippet);
            if out.len() >= 2 {
                break;
            }
        }
    }
    out
}

/// 单字段摘录生成（对齐 Paperclip createSnippet）。基于字符索引；
/// 高亮区间为原文中的命中字符区间（经窗口偏移映射到摘录文本）。
fn make_snippet(
    field: &str,
    label: &str,
    raw: &str,
    terms: &[String],
    ) -> Option<CompanySearchSnippet> {
    let text: String = plain_text(raw);
    if text.is_empty() {
        return None;
    }
    let first = find_first_match_index(&text, terms);
    let window_start: usize = if first < 0 { 0 } else { (first as usize).saturating_sub(80) };
    let text_len = text.chars().count();
    let window_end: usize = (window_start + 240).min(text_len);
    let prefix = if window_start > 0 { "..." } else { "" };
    let suffix = if window_end < text_len { "..." } else { "" };
    let slice: String = text
        .chars()
        .skip(window_start)
        .take(window_end - window_start)
        .collect();
    let snippet_text = format!("{prefix}{slice}{suffix}");
    let offset: isize = prefix.chars().count() as isize - window_start as isize;
    let snippet_len = snippet_text.chars().count() as isize;
    let highlights: Vec<CompanySearchHighlight> = highlight_ranges(&text, terms)
        .into_iter()
        .filter(|(s, e)| *e > window_start && *s < window_end)
        .map(|(s, e)| CompanySearchHighlight {
            start: ((s as isize + offset).max(0)) as usize,
            end: ((e as isize + offset).min(snippet_len)) as usize,
        })
        .collect();
    Some(CompanySearchSnippet {
        field: field.to_string(),
        label: label.to_string(),
        text: snippet_text,
        highlights,
    })
}

/// 提取 markdown 首图 URL（对齐 Paperclip MARKDOWN_IMAGE_PATTERN：
/// `![alt](url)` 或带 title 引号形式）。
fn extract_first_image_url(value: &str) -> Option<String> {
    let pattern = regex::Regex::new(r#"!\[[^\]]*\]\(\s*([^)\s]+)(?:\s+"[^"]*")?\s*\)"#).ok()?;
    pattern.captures(value).map(|c| c.get(1).unwrap().as_str().to_string())
}

/// 折叠 markdown/空白为纯文本（对齐 Paperclip plainText）。
fn plain_text(value: &str) -> String {
    value
        .replace("```", " ")
        .replace('`', " ")
        .replace('[', " ")
        .replace(']', " ")
        .replace('(', " ")
        .replace(')', " ")
        .replace(['#', '*', '>', '_', '~', '|'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// 返回首个命中术语的字符索引（对齐 Paperclip findFirstMatchIndex）。
fn find_first_match_index(value: &str, terms: &[String]) -> isize {
    let lower = value.to_lowercase();
    let mut best: isize = -1;
    for term in terms {
        let normalized = term.to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if let Some(idx) = lower.find(&normalized) {
            let idx = idx as isize;
            if best < 0 || idx < best {
                best = idx;
            }
        }
    }
    best
}

/// 返回原文中所有命中术语的字符区间（对齐 Paperclip highlightRanges）。
fn highlight_ranges(value: &str, terms: &[String]) -> Vec<(usize, usize)> {
    let lower = value.to_lowercase();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for term in terms {
        let normalized = term.to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(idx) = lower[from..].find(&normalized) {
            let start = from + idx;
            let end = start + normalized.chars().count();
            let next = (start, end);
            let overlaps = ranges
                .iter()
                .any(|(s, e)| next.0 < *e && next.1 > *s);
            if !overlaps {
                ranges.push(next);
            }
            from = start + normalized.chars().count();
        }
    }
    ranges.sort_by_key(|(s, _)| *s);
    ranges
}
