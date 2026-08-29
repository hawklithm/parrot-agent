//! Company Search Service
//!
//! 对齐 Paperclip `services/company-search.ts` 的 `CompanySearchResponse`
//! 形状（§4C.3）。搜索候选在 company 租户内按 issue、artifact、agent、project
//! 合并，再按统一排序键分页；issue 过滤器、facet 与 zero-result 建议共用同一
//! SQL 条件，避免分页前后语义漂移。

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

/// 搜索作用域。Paperclip 全量还包括 comments/documents/artifacts/agents/projects。
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

    /// 是否包含 agent 命中（Paperclip: all/agents）。
    pub fn includes_agents(&self) -> bool {
        matches!(self, CompanySearchScope::All | CompanySearchScope::Agents)
    }

    /// 是否包含 project 命中（Paperclip: all/projects）。
    pub fn includes_projects(&self) -> bool {
        matches!(self, CompanySearchScope::All | CompanySearchScope::Projects)
    }

    pub fn includes_artifacts(&self) -> bool {
        matches!(
            self,
            CompanySearchScope::All | CompanySearchScope::Artifacts
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
    pub id: String,
    pub identifier: Option<String>,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assignee_agent_id: Option<String>,
    pub assignee_user_id: Option<String>,
    pub project_id: Option<String>,
    pub updated_at: String,
}

/// 对齐 Paperclip `CompanySearchResult`（issue 作用域）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchResult {
    pub id: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact: Option<CompanySearchArtifactSummary>,
    pub updated_at: Option<String>,
    pub preview_image_url: Option<String>,
}

/// 对齐 Paperclip `CompanySearchArtifactSummary`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchArtifactSummary {
    pub id: String,
    pub source: String,
    pub media_kind: String,
    pub issue_id: String,
    pub issue_identifier: String,
    pub issue_title: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub updated_at: String,
}

/// 对齐 Paperclip `CompanySearchZeroResultsLoosenSuggestion`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchZeroResultsLoosenSuggestion {
    pub filter: String,
    pub values: Vec<String>,
    pub result_count: i64,
    pub additional_count: i64,
}

/// 对齐 Paperclip `CompanySearchZeroResults`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySearchZeroResults {
    pub unfiltered_total: i64,
    pub loosen_suggestions: Vec<CompanySearchZeroResultsLoosenSuggestion>,
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

/// agent 搜索命中行（对齐 Paperclip fetchAgentRows 的 SimpleSearchRow 字段子集）。
struct AgentSearchRow {
    id: Uuid,
    name: String,
    role: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

/// project 搜索命中行（对齐 Paperclip fetchProjectRows）。
struct ProjectSearchRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchFacet {
    All,
    Status,
    Priority,
    AssigneeAgent,
    AssigneeUser,
    Project,
    Label,
    UpdatedWithin,
    UpdatedAfter,
}

#[derive(Debug, Clone)]
struct IssueSearchRow {
    id: Uuid,
    title: String,
    identifier: Option<String>,
    status: String,
    priority: String,
    assignee_agent_id: Option<Uuid>,
    assignee_user_id: Option<Uuid>,
    project_id: Option<Uuid>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_at: chrono::DateTime<chrono::Utc>,
    description: Option<String>,
    identifier_similarity: f64,
    fuzzy_title: bool,
}

#[derive(Debug, Clone, Default)]
struct IssueSearchSources {
    comments: Vec<String>,
    documents: Vec<DocumentSearchSource>,
}

#[derive(Debug, Clone)]
struct DocumentSearchSource {
    key: String,
    title: String,
    content: String,
}

#[derive(Debug, Clone)]
struct ArtifactSearchRow {
    id: Uuid,
    source: String,
    media_hint: Option<String>,
    title: String,
    body: String,
    issue_id: Uuid,
    issue_identifier: String,
    issue_title: String,
    project_id: Option<Uuid>,
    project_name: Option<String>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_at: chrono::DateTime<chrono::Utc>,
    key: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Clone)]
struct SearchResultItem {
    result: CompanySearchResult,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    priority_rank: i32,
}

/// 对齐 Paperclip `CompanySearchResponse`。
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
    pub zero_results: Option<CompanySearchZeroResults>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CompanySearchQuery {
    pub q: String,
    pub scope: CompanySearchScope,
    pub sort: CompanySearchSort,
    pub limit: i64,
    pub offset: i64,
    pub statuses: Vec<String>,
    pub priorities: Vec<String>,
    /// `None` means the filter is absent; `Some(None)` means unassigned.
    pub assignee_agent_id: Option<Option<Uuid>>,
    pub assignee_user_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub label_id: Option<Uuid>,
    pub updated_within: Option<String>,
    pub updated_after: Option<chrono::DateTime<chrono::Utc>>,
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

impl CompanySearchQuery {
    pub fn has_active_issue_filters(&self) -> bool {
        !self.statuses.is_empty()
            || !self.priorities.is_empty()
            || self.assignee_agent_id.is_some()
            || self.assignee_user_id.is_some()
            || self.project_id.is_some()
            || self.label_id.is_some()
            || self.updated_within.is_some()
            || self.updated_after.is_some()
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

    /// 对齐 Paperclip `companySearchService.search`。
    ///
    /// 各类候选先在租户内完整合并，再按统一排序键切片。这样 `scope=all` 的
    /// offset 不会只作用于 issue，而 facet/zero-result 也能复用同一组过滤条件。
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
        let has_issue_filters = query.has_active_issue_filters();
        if !has_text && !has_issue_filters {
            return Ok(empty_response(&query, &normalized_query));
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
        // identifier 用 pg_trgm similarity（阈值 0.45）；标题用 Levenshtein 分词。
        let fuzzy_enabled =
            normalized_query.len() >= 4 && !normalized_query.contains(['\\', '%', '_']);
        const FUZZY_IDENTIFIER_SIMILARITY_THRESHOLD: f64 = 0.45;
        let fuzzy_tokens: Vec<String> = tokens.iter().filter(|t| t.len() >= 4).cloned().collect();
        let fuzzy_tokens_enabled = fuzzy_enabled && !fuzzy_tokens.is_empty();
        let scope_includes_issues = query.scope.includes_issues();
        let issue_results_enabled = scope_includes_issues
            && (has_text
                || !matches!(
                    query.scope,
                    CompanySearchScope::Comments | CompanySearchScope::Documents
                ));
        let mut items: Vec<SearchResultItem> = Vec::new();
        let mut source_counts = (0_i64, 0_i64);

        if issue_results_enabled {
            let issue_rows = self
                .fetch_issue_rows(
                    company_id,
                    &query,
                    &normalized_query,
                    &contains_pattern,
                    &token_patterns,
                    &fuzzy_tokens,
                    fuzzy_enabled,
                    fuzzy_tokens_enabled,
                    None,
                )
                .await?;
            let issue_ids: Vec<Uuid> = issue_rows.iter().map(|row| row.id).collect();
            let sources = if has_text && query.scope.includes_comments_or_documents() {
                self.fetch_issue_sources(company_id, &issue_ids, &contains_pattern, &token_patterns)
                    .await?
        } else {
                std::collections::HashMap::new()
        };
            for row in issue_rows {
                if let Some(source) = sources.get(&row.id) {
                    if !source.comments.is_empty() {
                        source_counts.0 += 1;
            }
                    if !source.documents.is_empty() {
                        source_counts.1 += 1;
        }
        }
                items.push(issue_search_item(
                    &row,
                    sources.get(&row.id),
                    &normalized_query,
                    &token_patterns,
                    fuzzy_enabled,
                    fuzzy_tokens_enabled,
                    FUZZY_IDENTIFIER_SIMILARITY_THRESHOLD,
                ));
            }
        }

        if query.scope.includes_artifacts() && has_text {
            let artifacts = self
                .fetch_artifact_rows(
                    company_id,
                    &query,
                    &normalized_query,
                    &contains_pattern,
                    &token_patterns,
            )
            .await?;
            for row in artifacts {
                items.push(artifact_search_item(&row, &normalized_query, &tokens));
            }
        }

        // Paperclip does not mix simple entities with issue-only filters. Their
        // results are otherwise part of the same global result set for `all`.
        if has_text && !has_issue_filters {
            if query.scope.includes_agents() {
                for row in self
                    .fetch_all_agent_rows(company_id, &contains_pattern, &token_patterns)
                    .await?
                {
                    let created_at = row.created_at;
                    let updated_at = row.updated_at;
                    items.push(SearchResultItem {
                        result: agent_search_result(&row, &normalized_query, &tokens),
                        created_at,
                        updated_at,
                        priority_rank: 0,
                    });
            }
                }
            if query.scope.includes_projects() {
                for row in self
                    .fetch_all_project_rows(company_id, &contains_pattern, &token_patterns)
                    .await?
                {
                    let created_at = row.created_at;
                    let updated_at = row.updated_at;
                    items.push(SearchResultItem {
                        result: project_search_result(&row, &normalized_query, &tokens),
                        created_at,
                        updated_at,
                        priority_rank: 0,
                    });
            }
                }
            }

        let filter_option_counts = if scope_includes_issues {
            self.fetch_filter_option_counts(
                company_id,
                &query,
                &normalized_query,
                &contains_pattern,
                &token_patterns,
                &fuzzy_tokens,
                fuzzy_enabled,
                fuzzy_tokens_enabled,
            )
            .await?
        } else {
            CompanySearchFilterOptionCounts::default()
        };

        let mut counts = empty_counts_by_type();
        for item in &items {
            *counts.entry(item.result.result_type.clone()).or_insert(0) += 1;
                }
        counts.insert("comment".to_string(), source_counts.0);
        counts.insert("document".to_string(), source_counts.1);

        let total = items.len() as i64;
        let zero_results = if total == 0 && has_issue_filters && scope_includes_issues {
            Some(
                self.build_zero_results(
                    company_id,
                    &query,
                    &normalized_query,
                    &contains_pattern,
                    &token_patterns,
                    &fuzzy_tokens,
                    fuzzy_enabled,
                    fuzzy_tokens_enabled,
                )
                .await?,
            )
        } else {
            None
        };

        items.sort_by(|a, b| compare_search_items(a, b, query.sort));
        let start = query.offset.max(0) as usize;
        let end = start.saturating_add(query.limit.max(0) as usize);
        let page_start = start.min(items.len());
        let page_end = end.min(items.len());
        let results = items
            .into_iter()
            .skip(page_start)
            .take(page_end.saturating_sub(page_start))
            .map(|item| item.result)
            .collect();

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
            zero_results,
            has_more: page_end < total as usize,
        })
            }

    async fn fetch_issue_rows(
        &self,
        company_id: Uuid,
        query: &CompanySearchQuery,
        normalized_query: &str,
        contains_pattern: &str,
        token_patterns: &[String],
        fuzzy_tokens: &[String],
        fuzzy_enabled: bool,
        fuzzy_tokens_enabled: bool,
        omit_filter: Option<SearchFacet>,
    ) -> Result<Vec<IssueSearchRow>, sqlx::Error> {
        let mut qb = QueryBuilder::<Postgres>::new(
            "SELECT i.id, i.title, i.identifier, i.status::text AS status, \
                    i.priority::text AS priority, i.assignee_agent_id, i.assignee_user_id, \
                    i.project_id, i.updated_at, i.created_at, i.description",
        );
        if fuzzy_enabled {
            qb.push(", similarity(lower(coalesce(i.identifier,'')), ")
                .push_bind(normalized_query.to_string())
                .push(")::double precision AS ident_sim");
        } else {
            qb.push(", 0.0::double precision AS ident_sim");
                }
        if fuzzy_tokens_enabled {
            qb.push(", ");
            push_fuzzy_title_expression(&mut qb, "i", fuzzy_tokens);
            qb.push(" AS fuzzy_title");
        } else {
            qb.push(", false AS fuzzy_title");
            }
        qb.push(" FROM issues i WHERE i.company_id = ")
            .push_bind(company_id)
            .push(" AND i.hidden_at IS NULL");

        if !normalized_query.is_empty() {
            qb.push(" AND ");
            push_issue_search_match(
                &mut qb,
                query,
                normalized_query,
                contains_pattern,
                token_patterns,
                fuzzy_tokens,
                fuzzy_enabled,
                fuzzy_tokens_enabled,
            );
                }
        push_issue_filters(&mut qb, "i", query, omit_filter);
        qb.push(" ORDER BY i.updated_at DESC, i.id DESC");

        let rows = qb.build().fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| IssueSearchRow {
                id: row.get("id"),
                title: row.get("title"),
                identifier: row.get("identifier"),
                status: row.get("status"),
                priority: row.get("priority"),
                assignee_agent_id: row.get("assignee_agent_id"),
                assignee_user_id: row.get("assignee_user_id"),
                project_id: row.get("project_id"),
                updated_at: row.get("updated_at"),
                created_at: row.get("created_at"),
                description: row.get("description"),
                identifier_similarity: row.try_get("ident_sim").unwrap_or(0.0),
                fuzzy_title: row.try_get("fuzzy_title").unwrap_or(false),
            })
            .collect())
                }

    async fn fetch_issue_sources(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
        contains_pattern: &str,
        token_patterns: &[String],
    ) -> Result<std::collections::HashMap<Uuid, IssueSearchSources>, sqlx::Error> {
        let mut sources = std::collections::HashMap::new();
        if issue_ids.is_empty() {
            return Ok(sources);
            }

        let mut comments = QueryBuilder::<Postgres>::new(
            "SELECT issue_id, body FROM issue_comments WHERE company_id = ",
        );
        comments
            .push_bind(company_id)
            .push(" AND issue_id = ANY(")
            .push_bind(issue_ids.to_vec())
            .push(") AND (");
        push_or_ilike_expression(
            &mut comments,
            "body",
            contains_pattern,
            token_patterns,
            true,
        );
        comments.push(") AND deleted_at IS NULL ORDER BY created_at ASC, id ASC");
        for row in comments.build().fetch_all(&self.pool).await? {
            sources
                .entry(row.get::<Uuid, _>("issue_id"))
                .or_insert_with(IssueSearchSources::default)
                .comments
                .push(row.get("body"));
        }

        let mut documents = QueryBuilder::<Postgres>::new(
            "SELECT idoc.issue_id, d.id AS document_id, idoc.key, d.title, d.content \
             FROM issue_documents idoc INNER JOIN documents d ON d.id = idoc.document_id \
             AND d.company_id = idoc.company_id WHERE idoc.company_id = ",
        );
        documents
            .push_bind(company_id)
            .push(" AND idoc.issue_id = ANY(")
            .push_bind(issue_ids.to_vec())
            .push(") AND (");
        push_or_ilike_expression(
            &mut documents,
            "d.title",
            contains_pattern,
            token_patterns,
            true,
        );
        documents.push(" OR ");
        push_or_ilike_expression(
            &mut documents,
            "d.content",
            contains_pattern,
            token_patterns,
            false,
        );
        documents.push(") ORDER BY idoc.key ASC, d.id ASC");
        for row in documents.build().fetch_all(&self.pool).await? {
            sources
                .entry(row.get::<Uuid, _>("issue_id"))
                .or_insert_with(IssueSearchSources::default)
                .documents
                .push(DocumentSearchSource {
                    key: row.get("key"),
                    title: row.get("title"),
                    content: row.get("content"),
                });
                }
        Ok(sources)
    }

    async fn fetch_artifact_rows(
        &self,
        company_id: Uuid,
        query: &CompanySearchQuery,
        normalized_query: &str,
        contains_pattern: &str,
        token_patterns: &[String],
    ) -> Result<Vec<ArtifactSearchRow>, sqlx::Error> {
        let has_text = !normalized_query.is_empty();
        if !has_text {
            return Ok(Vec::new());
        }
        let mut qb = QueryBuilder::<Postgres>::new(
            "SELECT d.id AS artifact_id, 'document'::text AS source, d.content_type::text AS media_hint, \
                    d.title AS artifact_title, d.content AS artifact_body, i.id AS issue_id, \
                    coalesce(i.identifier, i.id::text) AS issue_identifier, i.title AS issue_title, \
                    i.project_id, p.name AS project_name, d.updated_at AS artifact_updated_at, \
                    d.created_at AS artifact_created_at, idoc.key AS artifact_key, NULL::text AS artifact_url \
             FROM issue_documents idoc \
             INNER JOIN documents d ON d.id = idoc.document_id AND d.company_id = idoc.company_id \
             INNER JOIN issues i ON i.id = idoc.issue_id AND i.company_id = idoc.company_id \
             LEFT JOIN projects p ON p.id = i.project_id AND p.company_id = i.company_id \
             WHERE idoc.company_id = ",
        );
        qb.push_bind(company_id).push(
            " AND i.hidden_at IS NULL AND idoc.key NOT IN ('description', 'continuation-summary') \
                    AND (d.created_by_agent_id IS NOT NULL OR d.updated_by_agent_id IS NOT NULL)",
        );
        if has_text {
            qb.push(" AND (");
            push_or_ilike_expression(&mut qb, "d.title", contains_pattern, token_patterns, true);
            qb.push(" OR ");
            push_or_ilike_expression(
                &mut qb,
                "d.content",
                contains_pattern,
                token_patterns,
                false,
            );
            qb.push(" OR ");
            push_or_ilike_expression(
                &mut qb,
                "coalesce(i.identifier, '')",
                contains_pattern,
                token_patterns,
                false,
            );
            qb.push(" OR ");
            push_or_ilike_expression(&mut qb, "i.title", contains_pattern, token_patterns, false);
            qb.push(")");
        }
        push_issue_filters(&mut qb, "i", query, None);

        qb.push(" UNION ALL SELECT wp.id AS artifact_id, 'work_product'::text AS source, \
                    coalesce(wp.metadata->>'mediaKind', CASE WHEN wp.url IS NULL THEN 'text' ELSE 'file' END) AS media_hint, \
                    coalesce(nullif(wp.title, ''), wp.name) AS artifact_title, \
                    coalesce(wp.summary, wp.description, wp.artifact::text, '') AS artifact_body, \
                    i.id AS issue_id, coalesce(i.identifier, i.id::text) AS issue_identifier, \
                    i.title AS issue_title, i.project_id, p.name AS project_name, wp.updated_at AS artifact_updated_at, \
                    wp.created_at AS artifact_created_at, NULL::text AS artifact_key, wp.url AS artifact_url \
             FROM issue_work_products wp \
             INNER JOIN issues i ON i.id = wp.issue_id AND i.company_id = wp.company_id \
             LEFT JOIN projects p ON p.id = i.project_id AND p.company_id = i.company_id \
             WHERE wp.company_id = ");
        qb.push_bind(company_id)
            .push(" AND wp.type = 'artifact' AND i.hidden_at IS NULL");
        if has_text {
            qb.push(" AND (");
            for (index, expression) in [
                "coalesce(nullif(wp.title, ''), wp.name)",
                "coalesce(wp.summary, '')",
                "coalesce(wp.description, '')",
                "coalesce(wp.url, '')",
                "coalesce(wp.artifact::text, '')",
                "coalesce(i.identifier, '')",
                "i.title",
            ]
            .into_iter()
            .enumerate()
            {
                if index > 0 {
                    qb.push(" OR ");
                }
                push_or_ilike_expression(
                    &mut qb,
                    expression,
                    contains_pattern,
                    token_patterns,
                    true,
                );
            }
            qb.push(")");
        }
        push_issue_filters(&mut qb, "i", query, None);

        qb.push(" UNION ALL SELECT a.id AS artifact_id, 'attachment'::text AS source, \
                    a.content_type::text AS media_hint, a.filename AS artifact_title, \
                    (a.filename || ' ' || coalesce(asset.object_key, '')) AS artifact_body, \
                    i.id AS issue_id, coalesce(i.identifier, i.id::text) AS issue_identifier, \
                    i.title AS issue_title, i.project_id, p.name AS project_name, a.updated_at AS artifact_updated_at, \
                    a.created_at AS artifact_created_at, NULL::text AS artifact_key, NULL::text AS artifact_url \
             FROM attachments a INNER JOIN assets asset ON asset.id = a.asset_id AND asset.company_id = a.company_id \
             INNER JOIN issues i ON i.id = a.parent_id AND i.company_id = a.company_id \
             LEFT JOIN projects p ON p.id = i.project_id AND p.company_id = i.company_id \
             WHERE a.company_id = ");
        qb.push_bind(company_id).push(
            " AND a.parent_type = 'issue' AND i.hidden_at IS NULL \
                    AND asset.created_by_agent_id IS NOT NULL",
        );
        if has_text {
            qb.push(" AND (");
            for (index, expression) in [
                "a.filename",
                "coalesce(asset.original_filename, '')",
                "asset.object_key",
                "coalesce(i.identifier, '')",
                "i.title",
            ]
            .into_iter()
            .enumerate()
            {
                if index > 0 {
                    qb.push(" OR ");
                }
                push_or_ilike_expression(
                    &mut qb,
                    expression,
                    contains_pattern,
                    token_patterns,
                    true,
                );
                }
            qb.push(")");
            }
        push_issue_filters(&mut qb, "i", query, None);
        qb.push(" ORDER BY artifact_updated_at DESC, artifact_id DESC");

        let rows = qb.build().fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| ArtifactSearchRow {
                id: row.get("artifact_id"),
                source: row.get("source"),
                media_hint: row.try_get("media_hint").ok(),
                title: row.get("artifact_title"),
                body: row.get("artifact_body"),
                issue_id: row.get("issue_id"),
                issue_identifier: row.get("issue_identifier"),
                issue_title: row.get("issue_title"),
                project_id: row.get("project_id"),
                project_name: row.get("project_name"),
                updated_at: row.get("artifact_updated_at"),
                created_at: row.get("artifact_created_at"),
                key: row.get("artifact_key"),
                url: row.get("artifact_url"),
                })
            .collect())
    }

    async fn fetch_all_agent_rows(
        &self,
        company_id: Uuid,
        contains_pattern: &str,
        token_patterns: &[String],
    ) -> Result<Vec<AgentSearchRow>, sqlx::Error> {
        let mut qb = QueryBuilder::<Postgres>::new(
            "SELECT id, name, role, created_at, updated_at FROM agents WHERE company_id = ",
        );
        qb.push_bind(company_id).push(" AND (");
        push_or_ilike_expression(&mut qb, "name", contains_pattern, token_patterns, true);
        qb.push(" OR ");
        push_or_ilike_expression(&mut qb, "role", contains_pattern, token_patterns, false);
        qb.push(" OR ");
        push_or_ilike_expression(
            &mut qb,
            "metadata::text",
            contains_pattern,
            token_patterns,
            false,
        );
        qb.push(") ORDER BY updated_at DESC, id DESC");
        Ok(qb
            .build()
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| AgentSearchRow {
                id: row.get("id"),
                name: row.get("name"),
                role: row.get("role"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                    })
            .collect())
    }

    async fn fetch_all_project_rows(
        &self,
        company_id: Uuid,
        contains_pattern: &str,
        token_patterns: &[String],
    ) -> Result<Vec<ProjectSearchRow>, sqlx::Error> {
        let mut qb = QueryBuilder::<Postgres>::new(
            "SELECT id, name, description, created_at, updated_at FROM projects \
             WHERE company_id = ",
        );
        qb.push_bind(company_id)
            .push(" AND archived_at IS NULL AND (");
        push_or_ilike_expression(&mut qb, "name", contains_pattern, token_patterns, true);
        qb.push(" OR ");
        push_or_ilike_expression(
            &mut qb,
            "coalesce(description, '')",
            contains_pattern,
            token_patterns,
            false,
        );
        qb.push(") ORDER BY updated_at DESC, id DESC");
        Ok(qb
            .build()
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| ProjectSearchRow {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect())
    }

    async fn fetch_filter_option_counts(
        &self,
        company_id: Uuid,
        query: &CompanySearchQuery,
        normalized_query: &str,
        contains_pattern: &str,
        token_patterns: &[String],
        fuzzy_tokens: &[String],
        fuzzy_enabled: bool,
        fuzzy_tokens_enabled: bool,
    ) -> Result<CompanySearchFilterOptionCounts, sqlx::Error> {
        let mut out = CompanySearchFilterOptionCounts::default();
        if normalized_query.is_empty()
            && matches!(
                query.scope,
                CompanySearchScope::Comments | CompanySearchScope::Documents
            )
        {
            return Ok(out);
        }
        let fetch = |facet| async move {
            self.fetch_issue_rows(
                company_id,
                query,
                normalized_query,
                contains_pattern,
                token_patterns,
                fuzzy_tokens,
                fuzzy_enabled,
                fuzzy_tokens_enabled,
                Some(facet),
            )
            .await
            };

        for row in fetch(SearchFacet::Status).await? {
            *out.status.entry(row.status).or_insert(0) += 1;
                }
        for row in fetch(SearchFacet::Priority).await? {
            *out.priority.entry(row.priority).or_insert(0) += 1;
                }
        for row in fetch(SearchFacet::AssigneeAgent).await? {
            if let Some(id) = row.assignee_agent_id {
                *out.assignee_agent_id.entry(id.to_string()).or_insert(0) += 1;
            }
        }
        for row in fetch(SearchFacet::AssigneeUser).await? {
            if let Some(id) = row.assignee_user_id {
                *out.assignee_user_id.entry(id.to_string()).or_insert(0) += 1;
            }
        }
        for row in fetch(SearchFacet::Project).await? {
            if let Some(id) = row.project_id {
                *out.project_id.entry(id.to_string()).or_insert(0) += 1;
            }
        }
        let label_rows = fetch(SearchFacet::Label).await?;
        let label_issue_ids: Vec<Uuid> = label_rows.iter().map(|row| row.id).collect();
        if !label_issue_ids.is_empty() {
            for row in sqlx::query(
                "SELECT label_id FROM issue_labels WHERE company_id = $1 AND issue_id = ANY($2)",
            )
            .bind(company_id)
            .bind(&label_issue_ids)
            .fetch_all(&self.pool)
            .await?
            {
                let id: Uuid = row.get("label_id");
                *out.label_id.entry(id.to_string()).or_insert(0) += 1;
            }
        }
        // Paperclip computes the time buckets from the non-time-filtered match
        // set, so an active updatedAfter must not skew updatedWithin counts.
        let mut updated_query = query.clone();
        updated_query.updated_within = None;
        updated_query.updated_after = None;
        for row in self
            .fetch_issue_rows(
                company_id,
                &updated_query,
                normalized_query,
                contains_pattern,
                token_patterns,
                fuzzy_tokens,
                fuzzy_enabled,
                fuzzy_tokens_enabled,
                None,
            )
            .await?
        {
            let age = chrono::Utc::now() - row.updated_at;
            for (key, duration) in [
                ("24h", chrono::Duration::hours(24)),
                ("7d", chrono::Duration::days(7)),
                ("30d", chrono::Duration::days(30)),
                ("90d", chrono::Duration::days(90)),
            ] {
                if age <= duration {
                    *out.updated_within.entry(key.to_string()).or_insert(0) += 1;
                }
            }
        }
        Ok(out)
    }

    async fn fetch_non_issue_count(
        &self,
        company_id: Uuid,
        query: &CompanySearchQuery,
        normalized_query: &str,
        contains_pattern: &str,
        token_patterns: &[String],
    ) -> Result<i64, sqlx::Error> {
        if normalized_query.is_empty() || query.has_active_issue_filters() {
            return Ok(0);
        }
        let mut total = 0_i64;
        if query.scope.includes_artifacts() {
            total += self
                .fetch_artifact_rows(
                    company_id,
                    query,
            normalized_query,
                    contains_pattern,
                    token_patterns,
                )
                .await?
                .len() as i64;
        }
        if query.scope.includes_agents() {
            total += self
                .fetch_all_agent_rows(company_id, contains_pattern, token_patterns)
                .await?
                .len() as i64;
        }
        if query.scope.includes_projects() {
            total += self
                .fetch_all_project_rows(company_id, contains_pattern, token_patterns)
                .await?
                .len() as i64;
        }
        Ok(total)
    }

    async fn fetch_zero_result_count(
        &self,
        company_id: Uuid,
        query: &CompanySearchQuery,
        normalized_query: &str,
        contains_pattern: &str,
        token_patterns: &[String],
        fuzzy_tokens: &[String],
        fuzzy_enabled: bool,
        fuzzy_tokens_enabled: bool,
        omit_filter: SearchFacet,
    ) -> Result<i64, sqlx::Error> {
        let issue_results_enabled = query.scope.includes_issues()
            && (!normalized_query.is_empty()
                || !matches!(
                    query.scope,
                    CompanySearchScope::Comments | CompanySearchScope::Documents
                ));
        let issue_count = if issue_results_enabled {
            self.fetch_issue_rows(
                company_id,
                query,
                normalized_query,
                contains_pattern,
                token_patterns,
                fuzzy_tokens,
                fuzzy_enabled,
                fuzzy_tokens_enabled,
                Some(omit_filter),
            )
            .await?
            .len() as i64
        } else {
            0
        };
        let candidate_query = query_without_filter(query, omit_filter);
        Ok(issue_count
            + self
                .fetch_non_issue_count(
                    company_id,
                    &candidate_query,
                    normalized_query,
                    contains_pattern,
                    token_patterns,
                )
                .await?)
    }

    async fn build_zero_results(
        &self,
        company_id: Uuid,
        query: &CompanySearchQuery,
        normalized_query: &str,
        contains_pattern: &str,
        token_patterns: &[String],
        fuzzy_tokens: &[String],
        fuzzy_enabled: bool,
        fuzzy_tokens_enabled: bool,
    ) -> Result<CompanySearchZeroResults, sqlx::Error> {
        let issue_results_enabled = query.scope.includes_issues()
            && (!normalized_query.is_empty()
                || !matches!(
                    query.scope,
                    CompanySearchScope::Comments | CompanySearchScope::Documents
                ));
        let unfiltered_query = query_without_issue_filters(query);
        let unfiltered_issue_total = if issue_results_enabled {
            self.fetch_issue_rows(
                company_id,
                &unfiltered_query,
                normalized_query,
                contains_pattern,
                token_patterns,
                fuzzy_tokens,
                fuzzy_enabled,
                fuzzy_tokens_enabled,
                Some(SearchFacet::All),
            )
            .await?
            .len() as i64
        } else {
            0
        };
        let unfiltered_total = unfiltered_issue_total
            + self
                .fetch_non_issue_count(
                    company_id,
                    &unfiltered_query,
                    normalized_query,
                    contains_pattern,
                    token_patterns,
                )
                .await?;
        let current_total = 0_i64;
        let mut loosen_suggestions = Vec::new();
        let mut add = |filter: &str, values: Vec<String>, count: i64| {
            if !values.is_empty() {
                loosen_suggestions.push(CompanySearchZeroResultsLoosenSuggestion {
                    filter: filter.to_string(),
                    values,
                    result_count: count,
                    additional_count: (count - current_total).max(0),
                });
            }
        };

        if !query.statuses.is_empty() {
            let count = self
                .fetch_zero_result_count(
                    company_id,
                    query,
                    normalized_query,
                    contains_pattern,
                    token_patterns,
                    fuzzy_tokens,
                    fuzzy_enabled,
                    fuzzy_tokens_enabled,
                    SearchFacet::Status,
                )
                .await?;
            add("status", query.statuses.clone(), count);
            }
        if !query.priorities.is_empty() {
            let count = self
                .fetch_zero_result_count(
                    company_id,
                    query,
                    normalized_query,
                    contains_pattern,
                    token_patterns,
                    fuzzy_tokens,
                    fuzzy_enabled,
                    fuzzy_tokens_enabled,
                    SearchFacet::Priority,
                )
                .await?;
            add("priority", query.priorities.clone(), count);
        }
        if let Some(value) = &query.assignee_agent_id {
            let count = self
                .fetch_zero_result_count(
                    company_id,
                    query,
                    normalized_query,
                    contains_pattern,
                    token_patterns,
                    fuzzy_tokens,
                    fuzzy_enabled,
                    fuzzy_tokens_enabled,
                    SearchFacet::AssigneeAgent,
                )
                .await?;
            add(
                "assigneeAgentId",
                vec![value
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "null".to_string())],
                count,
            );
            }
        if let Some(value) = query.assignee_user_id {
            let count = self
                .fetch_zero_result_count(
                    company_id,
                    query,
                    normalized_query,
                    contains_pattern,
                    token_patterns,
                    fuzzy_tokens,
                    fuzzy_enabled,
                    fuzzy_tokens_enabled,
                    SearchFacet::AssigneeUser,
                )
                .await?;
            add("assigneeUserId", vec![value.to_string()], count);
        }
        if let Some(value) = query.project_id {
            let count = self
                .fetch_zero_result_count(
                    company_id,
                    query,
                    normalized_query,
                    contains_pattern,
                    token_patterns,
                    fuzzy_tokens,
                    fuzzy_enabled,
                    fuzzy_tokens_enabled,
                    SearchFacet::Project,
                )
                .await?;
            add("projectId", vec![value.to_string()], count);
    }
        if let Some(value) = query.label_id {
            let count = self
                .fetch_zero_result_count(
                    company_id,
                    query,
                    normalized_query,
                    contains_pattern,
                    token_patterns,
                    fuzzy_tokens,
                    fuzzy_enabled,
                    fuzzy_tokens_enabled,
                    SearchFacet::Label,
        )
        .await?;
            add("labelId", vec![value.to_string()], count);
    }
        if let Some(value) = &query.updated_within {
            let count = self
                .fetch_zero_result_count(
                    company_id,
                    query,
                    normalized_query,
                    contains_pattern,
                    token_patterns,
                    fuzzy_tokens,
                    fuzzy_enabled,
                    fuzzy_tokens_enabled,
                    SearchFacet::UpdatedWithin,
        )
        .await?;
            add("updatedWithin", vec![value.clone()], count);
        }
        if let Some(value) = &query.updated_after {
            let count = self
                .fetch_zero_result_count(
                    company_id,
                    query,
                    normalized_query,
                    contains_pattern,
                    token_patterns,
                    fuzzy_tokens,
                    fuzzy_enabled,
                    fuzzy_tokens_enabled,
                    SearchFacet::UpdatedAfter,
                )
                .await?;
            add("updatedAfter", vec![value.to_rfc3339()], count);
        }

        Ok(CompanySearchZeroResults {
            unfiltered_total,
            loosen_suggestions,
            })
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
                 AND ec.issue_id = issues.id AND ec.deleted_at IS NULL AND ec.body ILIKE $2)"
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
            conditions.push(format!(
                "issues.status::text = ANY(ARRAY[{}])",
                placeholders.join(",")
            ));
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
        let page = if has_more {
            &rows[..rows.len() - 1]
        } else {
            &rows[..]
        };

        // 收集每个 issue 的命中来源文本（使用模块级 SourceForMatch）。
        let mut sources_by_issue: std::collections::HashMap<Uuid, Vec<SourceForMatch>> =
            std::collections::HashMap::new();
        let add_source = |map: &mut std::collections::HashMap<Uuid, Vec<SourceForMatch>>,
                          s: SourceForMatch| {
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
                     WHERE company_id = $1 AND issue_id = ANY($2) AND deleted_at IS NULL AND body ILIKE $3 \
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

fn push_or_ilike_expression(
    qb: &mut QueryBuilder<'_, Postgres>,
    expression: &str,
    contains_pattern: &str,
    token_patterns: &[String],
    _first_expression: bool,
) {
    qb.push(expression)
        .push(" ILIKE ")
        .push_bind(contains_pattern.to_string());
    for token in token_patterns {
        qb.push(" OR ")
            .push(expression)
            .push(" ILIKE ")
            .push_bind(token.clone());
    }
}

fn push_fuzzy_title_expression(
    qb: &mut QueryBuilder<'_, Postgres>,
    alias: &str,
    fuzzy_tokens: &[String],
) {
    qb.push("coalesce((SELECT bool_and(EXISTS (SELECT 1 FROM regexp_split_to_table(lower(")
        .push(alias)
        .push(
            ".title), '[^a-z0-9]+') AS title_word(value) \
             WHERE length(title_word.value) >= 4 \
               AND levenshtein_less_equal(qt.value, title_word.value, \
                 CASE \
                   WHEN least(length(qt.value), length(title_word.value)) >= 6 THEN 2 \
                   WHEN least(length(qt.value), length(title_word.value)) >= 5 THEN 1 \
                   ELSE 0 END) <= CASE \
                 WHEN least(length(qt.value), length(title_word.value)) >= 6 THEN 2 \
                 WHEN least(length(qt.value), length(title_word.value)) >= 5 THEN 1 \
                 ELSE 0 END)) FROM unnest(",
        )
        .push_bind(fuzzy_tokens.to_vec())
        .push(") AS qt(value)), false)");
}

fn push_issue_search_match(
    qb: &mut QueryBuilder<'_, Postgres>,
    query: &CompanySearchQuery,
    normalized_query: &str,
    contains_pattern: &str,
    token_patterns: &[String],
    fuzzy_tokens: &[String],
    fuzzy_enabled: bool,
    fuzzy_tokens_enabled: bool,
) {
    let mut first = true;
    qb.push("(");
    if query.scope.includes_issues() {
        for expression in [
            "i.title",
            "coalesce(i.identifier, '')",
            "coalesce(i.description, '')",
        ] {
            if !first {
                qb.push(" OR ");
            }
            push_or_ilike_expression(qb, expression, contains_pattern, token_patterns, true);
            first = false;
        }
        if fuzzy_enabled {
            qb.push(" OR similarity(lower(coalesce(i.identifier, '')), ")
                .push_bind(normalized_query.to_string())
                .push(") >= 0.45");
            first = false;
        }
        if fuzzy_tokens_enabled {
            qb.push(" OR ");
            push_fuzzy_title_expression(qb, "i", fuzzy_tokens);
            first = false;
        }
    }
    if query.scope.includes_comments_or_documents() {
        if !first {
            qb.push(" OR ");
        }
        qb.push(
            "EXISTS (SELECT 1 FROM issue_comments sc WHERE sc.company_id = i.company_id \
                 AND sc.issue_id = i.id AND sc.deleted_at IS NULL AND (",
        );
        push_or_ilike_expression(qb, "sc.body", contains_pattern, token_patterns, true);
        qb.push("))");

        qb.push(" OR ");
        qb.push("EXISTS (SELECT 1 FROM issue_documents idoc \
                 INNER JOIN documents d ON d.id = idoc.document_id AND d.company_id = idoc.company_id \
                 WHERE idoc.company_id = i.company_id AND idoc.issue_id = i.id AND (");
        push_or_ilike_expression(qb, "d.title", contains_pattern, token_patterns, true);
        qb.push(" OR ");
        push_or_ilike_expression(qb, "d.content", contains_pattern, token_patterns, false);
        qb.push("))");
    }
    qb.push(")");
}

fn query_without_filter(query: &CompanySearchQuery, filter: SearchFacet) -> CompanySearchQuery {
    let mut candidate = query.clone();
    match filter {
        SearchFacet::All => return query_without_issue_filters(query),
        SearchFacet::Status => candidate.statuses.clear(),
        SearchFacet::Priority => candidate.priorities.clear(),
        SearchFacet::AssigneeAgent => candidate.assignee_agent_id = None,
        SearchFacet::AssigneeUser => candidate.assignee_user_id = None,
        SearchFacet::Project => candidate.project_id = None,
        SearchFacet::Label => candidate.label_id = None,
        SearchFacet::UpdatedWithin => candidate.updated_within = None,
        SearchFacet::UpdatedAfter => candidate.updated_after = None,
    }
    candidate
}

fn query_without_issue_filters(query: &CompanySearchQuery) -> CompanySearchQuery {
    let mut candidate = query.clone();
    candidate.statuses.clear();
    candidate.priorities.clear();
    candidate.assignee_agent_id = None;
    candidate.assignee_user_id = None;
    candidate.project_id = None;
    candidate.label_id = None;
    candidate.updated_within = None;
    candidate.updated_after = None;
    candidate
}

fn push_issue_filters(
    qb: &mut QueryBuilder<'_, Postgres>,
    alias: &str,
    query: &CompanySearchQuery,
    omit_filter: Option<SearchFacet>,
) {
    let omit_all = omit_filter == Some(SearchFacet::All);
    let omitted = |facet| omit_all || omit_filter == Some(facet);

    if !omitted(SearchFacet::Status) && !query.statuses.is_empty() {
        qb.push(" AND ")
            .push(alias)
            .push(".status::text = ANY(")
            .push_bind(query.statuses.clone())
            .push(")");
    }
    if !omitted(SearchFacet::Priority) && !query.priorities.is_empty() {
        qb.push(" AND ")
            .push(alias)
            .push(".priority::text = ANY(")
            .push_bind(query.priorities.clone())
            .push(")");
    }
    if !omitted(SearchFacet::AssigneeAgent) {
        if let Some(value) = &query.assignee_agent_id {
            match value {
                Some(id) => {
                    qb.push(" AND ")
                        .push(alias)
                        .push(".assignee_agent_id = ")
                        .push_bind(*id);
                }
                None => {
                    qb.push(" AND ")
                        .push(alias)
                        .push(".assignee_agent_id IS NULL");
                }
            };
        }
    }
    if !omitted(SearchFacet::AssigneeUser) {
        if let Some(id) = query.assignee_user_id {
            qb.push(" AND ")
                .push(alias)
                .push(".assignee_user_id = ")
                .push_bind(id);
        }
    }
    if !omitted(SearchFacet::Project) {
        if let Some(id) = query.project_id {
            qb.push(" AND ")
                .push(alias)
                .push(".project_id = ")
                .push_bind(id);
        }
    }
    if !omitted(SearchFacet::Label) {
        if let Some(id) = query.label_id {
            qb.push(" AND EXISTS (SELECT 1 FROM issue_labels lf WHERE lf.company_id = ")
                .push(alias)
                .push(".company_id AND lf.issue_id = ")
                .push(alias)
                .push(".id AND lf.label_id = ")
                .push_bind(id)
                .push(")");
        }
    }
    if !omitted(SearchFacet::UpdatedWithin) {
        if let Some(value) = &query.updated_within {
            if let Some(start) = parse_updated_within(value) {
                qb.push(" AND ")
                    .push(alias)
                    .push(".updated_at >= ")
                    .push_bind(start);
            }
        }
    }
    if !omitted(SearchFacet::UpdatedAfter) {
        if let Some(value) = query.updated_after {
            qb.push(" AND ")
                .push(alias)
                .push(".updated_at >= ")
                .push_bind(value);
        }
    }
}

fn issue_search_item(
    row: &IssueSearchRow,
    sources: Option<&IssueSearchSources>,
    normalized_query: &str,
    tokens: &[String],
    fuzzy_enabled: bool,
    fuzzy_tokens_enabled: bool,
    fuzzy_identifier_threshold: f64,
) -> SearchResultItem {
    let title_lower = row.title.to_lowercase();
    let description_lower = row
        .description
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    let identifier_lower = row.identifier.as_deref().unwrap_or_default().to_lowercase();
    let mut matched_fields = Vec::new();
    let mut score = 0.0;
    let mut title_literal_match = false;

    if !normalized_query.is_empty() {
        if title_lower == normalized_query {
            score += 6.0;
            title_literal_match = true;
            push_unique(&mut matched_fields, "title");
        } else if title_lower.contains(normalized_query) {
            score += 5.0;
            title_literal_match = true;
            push_unique(&mut matched_fields, "title");
        }
        if identifier_lower.contains(normalized_query) {
            score += 4.0;
            push_unique(&mut matched_fields, "identifier");
        }
        if title_lower.contains_any(tokens) {
            score += 3.0;
            push_unique(&mut matched_fields, "title");
        }
        if description_lower.contains(normalized_query) {
            score += 2.0;
            push_unique(&mut matched_fields, "description");
        }
        if identifier_lower.contains_any(tokens) {
            score += 1.0;
            push_unique(&mut matched_fields, "identifier");
        }
        if fuzzy_enabled && row.identifier_similarity >= fuzzy_identifier_threshold {
            score += 4.0;
            push_unique(&mut matched_fields, "identifier");
        }
        if fuzzy_tokens_enabled && row.fuzzy_title {
            score += 5.0;
            push_unique(&mut matched_fields, "title");
        }
    }

    let mut snippet_sources = Vec::new();
    if title_literal_match || row.fuzzy_title {
        snippet_sources.push(("title".to_string(), "Title".to_string(), row.title.clone()));
    }
    if let Some(sources) = sources {
        if let Some(comment) = sources.comments.first() {
            push_unique(&mut matched_fields, "comment");
            snippet_sources.push((
                "comment".to_string(),
                "Comment".to_string(),
                comment.clone(),
            ));
        }
        if let Some(document) = sources.documents.first() {
            push_unique(&mut matched_fields, "document");
            let text = if document.title.to_lowercase().contains(normalized_query) {
                document.title.clone()
            } else {
                document.content.clone()
            };
            snippet_sources.push((
                "document".to_string(),
                format!("Document ({})", document.key),
                text,
            ));
        }
    }
    if !normalized_query.is_empty() && description_lower.contains(normalized_query) {
        if let Some(description) = &row.description {
            snippet_sources.push((
                "description".to_string(),
                "Description".to_string(),
                description.clone(),
            ));
        }
    }

    let mut terms = vec![normalized_query.to_string()];
    terms.extend(tokens.iter().cloned());
    terms.retain(|term| !term.is_empty());
    terms.dedup();
    let snippets = build_issue_snippets(&snippet_sources, &terms);
    let snippet = snippets.first().map(|value| value.text.clone());
    let preview_image_url = row
        .description
        .as_deref()
        .and_then(extract_first_image_url)
        .or_else(|| {
            sources.and_then(|value| {
                value
                    .comments
                    .first()
                    .and_then(|comment| extract_first_image_url(comment))
            })
        })
        .or_else(|| {
            sources.and_then(|value| {
                value
                    .documents
                    .first()
                    .and_then(|document| extract_first_image_url(&document.content))
            })
        });
    let updated_at = row.updated_at.to_rfc3339();
    let identifier = row.identifier.clone();
    let href = identifier
        .as_deref()
        .map(|value| format!("/company/issues/{}", urlencoding::encode(value)))
        .unwrap_or_else(|| format!("/company/issues/{}", row.id));
    let issue = CompanySearchIssueSummary {
        id: row.id.to_string(),
        identifier: identifier.clone(),
        title: row.title.clone(),
        status: row.status.clone(),
        priority: row.priority.clone(),
        assignee_agent_id: row.assignee_agent_id.map(|id| id.to_string()),
        assignee_user_id: row.assignee_user_id.map(|id| id.to_string()),
        project_id: row.project_id.map(|id| id.to_string()),
        updated_at: updated_at.clone(),
    };
    SearchResultItem {
        result: CompanySearchResult {
            id: row.id.to_string(),
            result_type: "issue".to_string(),
            score,
            title: identifier
                .as_deref()
                .map(|value| format!("{value} {}", row.title))
                .unwrap_or_else(|| row.title.clone()),
            href,
            matched_fields,
            source_label: identifier,
            snippet,
            snippets,
            issue: Some(issue),
            artifact: None,
            updated_at: Some(updated_at),
            preview_image_url,
        },
        created_at: row.created_at,
        updated_at: row.updated_at,
        priority_rank: priority_rank(&row.priority),
    }
}

fn artifact_search_item(
    row: &ArtifactSearchRow,
    normalized_query: &str,
    tokens: &[String],
) -> SearchResultItem {
    let mut terms = vec![normalized_query.to_string()];
    terms.extend(tokens.iter().cloned());
    terms.retain(|term| !term.is_empty());
    terms.dedup();
    let snippets = build_issue_snippets(
        &[
            (
                "title".to_string(),
                "Artifact".to_string(),
                row.title.clone(),
            ),
            (
                "content".to_string(),
                "Artifact".to_string(),
                row.body.clone(),
            ),
        ],
        &terms,
    );
    let snippet = snippets.first().map(|value| value.text.clone());
    let href = match row.source.as_str() {
        "document" => format!(
            "/company/issues/{}#document-{}",
            urlencoding::encode(&row.issue_identifier),
            urlencoding::encode(row.key.as_deref().unwrap_or("document"))
        ),
        "work_product" => format!(
            "/company/issues/{}#work-product-{}",
            urlencoding::encode(&row.issue_identifier),
            row.id
        ),
        _ => format!(
            "/company/issues/{}#attachment-{}",
            urlencoding::encode(&row.issue_identifier),
            row.id
        ),
    };
    let updated_at = row.updated_at.to_rfc3339();
    let mut score_description = row.body.clone();
    score_description.push(' ');
    score_description.push_str(&row.issue_identifier);
    score_description.push(' ');
    score_description.push_str(&row.issue_title);
    if let Some(project_name) = &row.project_name {
        score_description.push(' ');
        score_description.push_str(project_name);
    }
    let public_id = artifact_public_id(&row.source, row.id);
    let artifact = CompanySearchArtifactSummary {
        id: public_id.clone(),
        source: row.source.clone(),
        media_kind: media_kind(row.source.as_str(), row.media_hint.as_deref()),
        issue_id: row.issue_id.to_string(),
        issue_identifier: row.issue_identifier.clone(),
        issue_title: row.issue_title.clone(),
        project_id: row.project_id.map(|id| id.to_string()),
        project_name: row.project_name.clone(),
        updated_at: updated_at.clone(),
    };
    SearchResultItem {
        result: CompanySearchResult {
            id: public_id,
            result_type: "artifact".to_string(),
            score: score_simple_row(
                &row.title,
                Some(&score_description),
                None,
                normalized_query,
                tokens,
            ),
            title: row.title.clone(),
            href,
            matched_fields: vec!["artifact".to_string()],
            source_label: row.key.clone().or_else(|| Some(row.source.clone())),
            snippet,
            snippets,
            issue: None,
            artifact: Some(artifact),
            updated_at: Some(updated_at),
            preview_image_url: row
                .url
                .clone()
                .or_else(|| extract_first_image_url(&row.body)),
        },
        created_at: row.created_at,
        updated_at: row.updated_at,
        priority_rank: 0,
    }
}

fn media_kind(source: &str, hint: Option<&str>) -> String {
    if source == "document" {
        return "document".to_string();
    }
    let hint = hint.unwrap_or_default().to_lowercase();
    if hint.starts_with("image/") || hint == "image" {
        "image".to_string()
    } else if hint.starts_with("video/") || hint == "video" {
        "video".to_string()
    } else if hint.starts_with("text/") || hint == "text" {
        "text".to_string()
    } else if hint.is_empty() {
        "empty".to_string()
    } else {
        "file".to_string()
    }
}

fn artifact_public_id(source: &str, id: Uuid) -> String {
    format!("{source}:{id}")
}

fn priority_rank(value: &str) -> i32 {
    match value {
        "critical" => 4,
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn search_type_rank(value: &str) -> i32 {
    match value {
        "issue" => 0,
        "artifact" => 1,
        "agent" => 2,
        "project" => 3,
        _ => 4,
    }
}

fn compare_search_items(
    left: &SearchResultItem,
    right: &SearchResultItem,
    sort: CompanySearchSort,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let primary = match sort {
        CompanySearchSort::Relevance => right
            .result
            .score
            .partial_cmp(&left.result.score)
            .unwrap_or(Ordering::Equal),
        CompanySearchSort::Updated => right.updated_at.cmp(&left.updated_at),
        CompanySearchSort::Created => right.created_at.cmp(&left.created_at),
        CompanySearchSort::Priority => right.priority_rank.cmp(&left.priority_rank),
    };
    primary
        .then_with(|| right.updated_at.cmp(&left.updated_at))
        .then_with(|| {
            search_type_rank(&left.result.result_type)
                .cmp(&search_type_rank(&right.result.result_type))
        })
        .then_with(|| left.result.id.cmp(&right.result.id))
}

fn empty_counts_by_type() -> std::collections::HashMap<String, i64> {
    [
        "issue", "artifact", "agent", "project", "comment", "document",
    ]
    .into_iter()
    .map(|key| (key.to_string(), 0))
    .collect()
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|entry| entry == value) {
        values.push(value.to_string());
    }
}

trait ContainsAny {
    fn contains_any(&self, values: &[String]) -> bool;
}

impl ContainsAny for str {
    fn contains_any(&self, values: &[String]) -> bool {
        values
            .iter()
            .any(|value| !value.is_empty() && self.contains(value))
    }
}

// ============ Extract 匹配/摘录辅助函数（对齐 Paperclip company-search-extract.ts） ============

const EXCERPT_MAX_CHARS: usize = 180;

/// URL 匹配锚点：本实现以「按空白/引号切分的 token 中包含 contains 子串」近似
/// Paperclip 的 URL 正则；返回 contains 本身供 occurrence 计数复用（对齐语义）。
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

fn empty_response(query: &CompanySearchQuery, normalized_query: &str) -> CompanySearchResponse {
    CompanySearchResponse {
        query: query.q.clone(),
        normalized_query: normalized_query.to_string(),
        scope: scope_name(&query.scope).to_string(),
        sort: sort_name(&query.sort).to_string(),
        limit: query.limit,
        offset: query.offset,
        results: Vec::new(),
        counts_by_type: empty_counts_by_type(),
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
    let window_start: usize = if first < 0 {
        0
    } else {
        (first as usize).saturating_sub(80)
    };
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
    pattern
        .captures(value)
        .map(|c| c.get(1).unwrap().as_str().to_string())
}

/// 对齐 Paperclip scoreSimpleRow：短语命中 90 + 每 token 20 + 标题前缀 80。
fn score_simple_row(
    title: &str,
    description: Option<&str>,
    role: Option<&str>,
    normalized_query: &str,
    tokens: &[String],
) -> f64 {
    let mut haystack = title.to_string();
    if let Some(d) = description {
        haystack.push(' ');
        haystack.push_str(d);
    }
    if let Some(r) = role {
        haystack.push(' ');
        haystack.push_str(r);
    }
    let haystack = haystack.to_lowercase();
    let mut score = if haystack.contains(normalized_query) {
        90.0
    } else {
        0.0
    };
    for t in tokens {
        if haystack.contains(t) {
            score += 20.0;
        }
    }
    if title.to_lowercase().starts_with(normalized_query) {
        score += 80.0;
    }
    score
}

/// agent 搜索结果（对齐 Paperclip agent 分支：type=agent、matchedFields=["agent"]、
/// snippet 源为 role??name，href /company/agents/{id}）。
fn agent_search_result(
    row: &AgentSearchRow,
    normalized_query: &str,
    tokens: &[String],
) -> CompanySearchResult {
    let terms: Vec<String> = std::iter::once(normalized_query.to_string())
        .chain(tokens.iter().cloned())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let src = if row.role.is_empty() {
        row.name.clone()
    } else {
        row.role.clone()
    };
    let snippets = build_issue_snippets(
        &[("capabilities".to_string(), "Agent".to_string(), src)],
        &terms,
    );
    let snippet = snippets.first().map(|s| s.text.clone());
    CompanySearchResult {
        id: row.id.to_string(),
        result_type: "agent".to_string(),
        score: score_simple_row(&row.name, None, Some(&row.role), normalized_query, tokens),
        title: row.name.clone(),
        href: format!("/company/agents/{}", row.id),
        matched_fields: vec!["agent".to_string()],
        source_label: snippets.first().map(|s| s.label.clone()),
        snippet,
        snippets,
        issue: None,
        artifact: None,
        updated_at: Some(row.updated_at.to_rfc3339()),
        preview_image_url: None,
    }
}

/// project 搜索结果（对齐 Paperclip project 分支：type=project、matchedFields=["project"]、
/// snippet 源为 description??name，href /company/projects/{id}）。
fn project_search_result(
    row: &ProjectSearchRow,
    normalized_query: &str,
    tokens: &[String],
) -> CompanySearchResult {
    let terms: Vec<String> = std::iter::once(normalized_query.to_string())
        .chain(tokens.iter().cloned())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let src = row
        .description
        .clone()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| row.name.clone());
    let snippets = build_issue_snippets(
        &[("description".to_string(), "Project".to_string(), src)],
        &terms,
    );
    let snippet = snippets.first().map(|s| s.text.clone());
    CompanySearchResult {
        id: row.id.to_string(),
        result_type: "project".to_string(),
        score: score_simple_row(
            &row.name,
            row.description.as_deref(),
            None,
            normalized_query,
            tokens,
        ),
        title: row.name.clone(),
        href: format!("/company/projects/{}", row.id),
        matched_fields: vec!["project".to_string()],
        source_label: snippets.first().map(|s| s.label.clone()),
        snippet,
        snippets,
        issue: None,
        artifact: None,
        updated_at: Some(row.updated_at.to_rfc3339()),
        preview_image_url: None,
    }
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
            let overlaps = ranges.iter().any(|(s, e)| next.0 < *e && next.1 > *s);
            if !overlaps {
                ranges.push(next);
            }
            from = start + normalized.chars().count();
        }
    }
    ranges.sort_by_key(|(s, _)| *s);
    ranges
}
