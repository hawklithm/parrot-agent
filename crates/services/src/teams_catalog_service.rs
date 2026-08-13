//! P1.4 Teams Catalog
//!
//! 对齐 Paperclip `server/src/services/teams-catalog.ts` 的能力子集：
//! - 从文件系统扫描 team catalog（`<root>/<kind>/<category>/<slug>/TEAM.md`）
//! - 列表 / 详情 / catalog 文件读取（带路径穿越防护）
//! - 公司已安装 team 查询
//! - preview：预览将创建的 Agent、reportsTo 映射、命名冲突、所需 skill、权限影响
//! - install：单事务导入，失败整体回滚，重复安装幂等，manager remapping
//!
//! 与 Paperclip 的差异：Parrot 侧不依赖 company-portability（当前仍是 stub），
//! 而是直接把 catalog 中的 Agent 定义落到 `agents` 表，并用
//! `company_team_installs` 记录安装来源，保证幂等与可追溯。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TeamsCatalogError {
    #[error("catalog team not found: {0}")]
    NotFound(String),
    #[error("invalid catalog: {0}")]
    InvalidCatalog(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("io error: {0}")]
    Io(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub type TeamsCatalogResult<T> = Result<T, TeamsCatalogError>;

/// catalog 中的单个 Agent 定义（来自 `agents/<slug>/AGENTS.md` frontmatter）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogTeamAgent {
    pub slug: String,
    pub name: String,
    pub title: Option<String>,
    pub role: String,
    pub reports_to: Option<String>,
    pub skills: Vec<String>,
    pub path: String,
}

/// catalog 中的一个文件条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogTeamFile {
    pub path: String,
    pub kind: String,
    pub size_bytes: u64,
    pub sha256: String,
}

/// 一个 catalog team
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogTeam {
    pub id: String,
    pub key: String,
    pub kind: String,
    pub category: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub entrypoint: String,
    pub schema: String,
    pub default_install: bool,
    pub recommended_for_company_types: Vec<String>,
    pub tags: Vec<String>,
    pub required_skills: Vec<String>,
    pub manager_slug: Option<String>,
    pub agents: Vec<CatalogTeamAgent>,
    pub files: Vec<CatalogTeamFile>,
    pub content_hash: String,
}

/// 安装动作的发起者（用于审计）
#[derive(Debug, Clone, Default)]
pub struct InstallActor {
    pub actor_type: String,
    pub user_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
}

#[async_trait]
pub trait TeamsCatalogService: Send + Sync {
    /// 列出 catalog 中的 team，可按 kind/category/关键词过滤
    async fn list_teams(
        &self,
        kind: Option<&str>,
        category: Option<&str>,
        q: Option<&str>,
    ) -> TeamsCatalogResult<Value>;

    /// 获取单个 team 详情
    async fn get_team(&self, catalog_ref: &str) -> TeamsCatalogResult<Value>;

    /// 读取 team 目录下的某个文件（默认 TEAM.md）
    async fn read_team_file(&self, catalog_ref: &str, rel_path: &str)
        -> TeamsCatalogResult<Value>;

    /// 公司已安装的 catalog team
    async fn list_installed(&self, company_id: Uuid) -> TeamsCatalogResult<Value>;

    /// 预览安装：不写库
    async fn preview_install(
        &self,
        company_id: Uuid,
        catalog_ref: &str,
        options: &Value,
    ) -> TeamsCatalogResult<Value>;

    /// 执行安装：单事务，失败回滚
    async fn install(
        &self,
        company_id: Uuid,
        catalog_ref: &str,
        options: &Value,
        actor: InstallActor,
    ) -> TeamsCatalogResult<Value>;
}

// ---------------------------------------------------------------------------
// 纯函数区（可单测）
// ---------------------------------------------------------------------------

/// 极简 frontmatter 解析：支持 `key: value` 与 `- item` 列表，够覆盖 catalog 格式。
///
/// 返回 (frontmatter map, body)。无 frontmatter 时 map 为空。
pub fn parse_frontmatter(input: &str) -> (BTreeMap<String, Value>, String) {
    let normalized = input.replace("\r\n", "\n");
    let mut map: BTreeMap<String, Value> = BTreeMap::new();

    let Some(rest) = normalized.strip_prefix("---\n") else {
        return (map, normalized);
    };
    let Some(end) = rest.find("\n---") else {
        return (map, normalized);
    };
    let (front, body) = rest.split_at(end);
    let body = body
        .trim_start_matches('\n')
        .trim_start_matches("---")
        .trim_start_matches('\n')
        .to_string();

    let mut current_list_key: Option<String> = None;
    for line in front.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() || trimmed.trim_start().starts_with('#') {
            continue;
        }
        let indented_item = trimmed.trim_start().starts_with("- ");
        if indented_item {
            if let Some(key) = current_list_key.clone() {
                let item = trimmed.trim_start().trim_start_matches("- ").trim();
                let entry = map.entry(key).or_insert_with(|| Value::Array(vec![]));
                if let Some(arr) = entry.as_array_mut() {
                    arr.push(Value::String(unquote(item)));
                }
            }
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = value.trim();
        if value.is_empty() {
            // 后续行是列表项
            map.insert(key.clone(), Value::Array(vec![]));
            current_list_key = Some(key);
        } else {
            current_list_key = None;
            map.insert(key, scalar_value(value));
        }
    }

    (map, body)
}

fn unquote(raw: &str) -> String {
    let s = raw.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        return s[1..s.len() - 1].to_string();
    }
    s.to_string()
}

fn scalar_value(raw: &str) -> Value {
    let s = unquote(raw);
    match s.as_str() {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::String(s),
    }
}

fn front_str(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn front_list(map: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    map.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// catalog id：`parrot:<kind>:<category>:<slug>`；同时接受 key 形式作为引用。
pub fn catalog_id(kind: &str, category: &str, slug: &str) -> String {
    format!("parrot:{kind}:{category}:{slug}")
}

/// catalog key：`parrot/<kind>/<category>/<slug>`
pub fn catalog_key(kind: &str, category: &str, slug: &str) -> String {
    format!("parrot/{kind}/{category}/{slug}")
}

/// 判断引用（id / key / slug）是否命中该 team。
pub fn matches_ref(team_id: &str, team_key: &str, slug: &str, catalog_ref: &str) -> bool {
    let r = catalog_ref.trim();
    r == team_id || r == team_key || r == slug
}

/// 相对路径安全校验（拒绝绝对路径、`..`、空字节）。
pub fn is_safe_catalog_path(rel: &str) -> bool {
    if rel.is_empty() || rel.contains('\0') {
        return false;
    }
    let p = Path::new(rel);
    if p.is_absolute() {
        return false;
    }
    p.components().all(|c| matches!(c, Component::Normal(_)))
}

/// 文件 kind 推断，用于前端图标/语法高亮。
pub fn file_kind(rel: &str) -> &'static str {
    let lower = rel.to_ascii_lowercase();
    if lower.ends_with("team.md") {
        "team"
    } else if lower.ends_with("agents.md") {
        "agent"
    } else if lower.ends_with("project.md") {
        "project"
    } else if lower.ends_with("task.md") {
        "task"
    } else if lower.ends_with("skill.md") {
        "skill"
    } else if lower.ends_with("readme.md") {
        "readme"
    } else if lower.ends_with(".md") {
        "markdown"
    } else if lower.ends_with(".sh") || lower.ends_with(".py") || lower.ends_with(".js") {
        "script"
    } else {
        "other"
    }
}

/// catalog role → Parrot AgentRole 字符串（DB enum 值为 snake_case）。
pub fn map_agent_role(raw: &str) -> &'static str {
    let r = raw.trim().to_ascii_lowercase();
    if r.contains("ceo") || r.contains("chief-executive") {
        "ceo"
    } else if r.contains("vp") || r.contains("vice-president") {
        "vp"
    } else if r.contains("manager") || r.contains("cto") || r.contains("lead") {
        "manager"
    } else if r.contains("research") || r.contains("analyst") {
        "researcher"
    } else {
        "general"
    }
}

/// 命名冲突处理策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionStrategy {
    /// 已存在同名则跳过创建
    Skip,
    /// 已存在同名则追加序号
    Rename,
    /// 已存在同名则整单失败
    Fail,
}

impl CollisionStrategy {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.unwrap_or("skip").trim().to_ascii_lowercase().as_str() {
            "rename" => Self::Rename,
            "fail" | "error" => Self::Fail,
            _ => Self::Skip,
        }
    }
}

/// 冲突决策结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameResolution {
    /// 直接使用该名字创建
    Create(String),
    /// 跳过创建
    Skip,
    /// 整单失败
    Fail(String),
}

/// 纯函数：根据现有名字集合与策略决定最终名字。
pub fn resolve_name_collision(
    existing: &HashSet<String>,
    proposed: &str,
    strategy: CollisionStrategy,
) -> NameResolution {
    if !existing.contains(proposed) {
        return NameResolution::Create(proposed.to_string());
    }
    match strategy {
        CollisionStrategy::Skip => NameResolution::Skip,
        CollisionStrategy::Fail => NameResolution::Fail(format!(
            "agent name '{proposed}' already exists in company"
        )),
        CollisionStrategy::Rename => {
            for n in 2..1000 {
                let candidate = format!("{proposed} ({n})");
                if !existing.contains(&candidate) {
                    return NameResolution::Create(candidate);
                }
            }
            NameResolution::Fail(format!("cannot find free name for '{proposed}'"))
        }
    }
}

/// 纯函数：把 catalog 内部的 reportsTo slug 映射为真实 agent id。
///
/// - slug 命中同批创建的 agent → 该 agent 的 id
/// - 无 reportsTo（根节点）或 slug 未命中 → 落到 target_manager（可为 None）
pub fn resolve_reports_to(
    reports_to_slug: Option<&str>,
    created: &HashMap<String, Uuid>,
    target_manager: Option<Uuid>,
) -> Option<Uuid> {
    match reports_to_slug {
        Some(slug) => created.get(slug).copied().or(target_manager),
        None => target_manager,
    }
}

/// 纯函数：检测 catalog 内部 reportsTo 是否存在环或指向不存在的 slug。
pub fn validate_reports_to_graph(agents: &[CatalogTeamAgent]) -> Result<(), String> {
    let slugs: HashSet<&str> = agents.iter().map(|a| a.slug.as_str()).collect();
    for a in agents {
        if let Some(parent) = a.reports_to.as_deref() {
            if parent == a.slug {
                return Err(format!("agent '{}' reports to itself", a.slug));
            }
            if !slugs.contains(parent) {
                // 指向 catalog 外部：视为根节点，交由 targetManager 接管，不算错误
                continue;
            }
        }
    }
    // 环检测
    let parent_of: HashMap<&str, &str> = agents
        .iter()
        .filter_map(|a| a.reports_to.as_deref().map(|p| (a.slug.as_str(), p)))
        .collect();
    for a in agents {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut cursor = a.slug.as_str();
        while let Some(next) = parent_of.get(cursor) {
            if !seen.insert(cursor) {
                return Err(format!("reportsTo cycle detected at '{}'", a.slug));
            }
            if *next == a.slug {
                return Err(format!("reportsTo cycle detected at '{}'", a.slug));
            }
            cursor = next;
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

// ---------------------------------------------------------------------------
// 文件系统 catalog 加载
// ---------------------------------------------------------------------------

/// 解析 catalog 根目录：优先 env `TEAMS_CATALOG_DIR`，否则按候选路径探测。
pub fn resolve_catalog_root() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TEAMS_CATALOG_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
        return None;
    }
    let candidates = [
        "teams-catalog/catalog",
        "../teams-catalog/catalog",
        "../../teams-catalog/catalog",
        "parrot-agent/teams-catalog/catalog",
    ];
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_dir())
        .or_else(|| {
            std::env::current_dir().ok().and_then(|cwd| {
                candidates
                    .iter()
                    .map(|c| cwd.join(c))
                    .find(|p| p.is_dir())
            })
        })
}

fn collect_files(root: &Path) -> Vec<CatalogTeamFile> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            out.push(CatalogTeamFile {
                kind: file_kind(&rel).to_string(),
                size_bytes: bytes.len() as u64,
                sha256: sha256_hex(&bytes),
                path: rel,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn load_team_from_dir(kind: &str, category: &str, dir: &Path) -> Option<CatalogTeam> {
    let team_md = dir.join("TEAM.md");
    let raw = std::fs::read_to_string(&team_md).ok()?;
    let (front, _body) = parse_frontmatter(&raw);

    let slug = front_str(&front, "slug")
        .or_else(|| dir.file_name().map(|s| s.to_string_lossy().to_string()))?;
    let name = front_str(&front, "name").unwrap_or_else(|| slug.clone());
    let description = front_str(&front, "description").unwrap_or_default();
    let manager_slug = front_str(&front, "manager").map(|m| {
        // `agents/ceo/AGENTS.md` → `ceo`
        m.split('/')
            .nth(1)
            .map(|s| s.to_string())
            .unwrap_or_else(|| m.clone())
    });

    // 扫描 agents 子目录
    let mut agents = Vec::new();
    let agents_dir = dir.join("agents");
    if agents_dir.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&agents_dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        entries.sort();
        for agent_dir in entries {
            let md = agent_dir.join("AGENTS.md");
            let Ok(agent_raw) = std::fs::read_to_string(&md) else {
                continue;
            };
            let (af, _) = parse_frontmatter(&agent_raw);
            let agent_slug = front_str(&af, "slug").unwrap_or_else(|| {
                agent_dir
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
            agents.push(CatalogTeamAgent {
                name: front_str(&af, "name").unwrap_or_else(|| agent_slug.clone()),
                title: front_str(&af, "title"),
                role: front_str(&af, "role").unwrap_or_else(|| "general".to_string()),
                reports_to: front_str(&af, "reportsTo"),
                skills: front_list(&af, "skills"),
                path: format!("agents/{agent_slug}/AGENTS.md"),
                slug: agent_slug,
            });
        }
    }

    let files = collect_files(dir);
    let mut hasher = Sha256::new();
    for f in &files {
        hasher.update(f.path.as_bytes());
        hasher.update(f.sha256.as_bytes());
    }
    let content_hash = hex::encode(hasher.finalize());

    Some(CatalogTeam {
        id: catalog_id(kind, category, &slug),
        key: catalog_key(kind, category, &slug),
        kind: kind.to_string(),
        category: category.to_string(),
        name,
        description,
        path: dir.to_string_lossy().to_string(),
        entrypoint: "TEAM.md".to_string(),
        schema: front_str(&front, "schema").unwrap_or_else(|| "agentcompanies/v1".to_string()),
        default_install: front
            .get("defaultInstall")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        recommended_for_company_types: front_list(&front, "recommendedForCompanyTypes"),
        tags: front_list(&front, "tags"),
        required_skills: front_list(&front, "requiredSkills"),
        manager_slug,
        agents,
        files,
        content_hash,
        slug,
    })
}

/// 扫描整个 catalog 根目录。
pub fn scan_catalog(root: &Path) -> Vec<CatalogTeam> {
    let mut teams = Vec::new();
    let Ok(kinds) = std::fs::read_dir(root) else {
        return teams;
    };
    for kind_entry in kinds.flatten() {
        let kind_path = kind_entry.path();
        if !kind_path.is_dir() {
            continue;
        }
        let kind = kind_entry.file_name().to_string_lossy().to_string();
        let Ok(categories) = std::fs::read_dir(&kind_path) else {
            continue;
        };
        for cat_entry in categories.flatten() {
            let cat_path = cat_entry.path();
            if !cat_path.is_dir() {
                continue;
            }
            let category = cat_entry.file_name().to_string_lossy().to_string();
            let Ok(slugs) = std::fs::read_dir(&cat_path) else {
                continue;
            };
            for slug_entry in slugs.flatten() {
                let slug_path = slug_entry.path();
                if !slug_path.is_dir() {
                    continue;
                }
                if let Some(team) = load_team_from_dir(&kind, &category, &slug_path) {
                    teams.push(team);
                }
            }
        }
    }
    teams.sort_by(|a, b| a.id.cmp(&b.id));
    teams
}

// ---------------------------------------------------------------------------
// Service 实现
// ---------------------------------------------------------------------------

pub struct DefaultTeamsCatalogService {
    pool: PgPool,
}

impl DefaultTeamsCatalogService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn teams(&self) -> TeamsCatalogResult<Vec<CatalogTeam>> {
        match resolve_catalog_root() {
            Some(root) => Ok(scan_catalog(&root)),
            // catalog 未部署时返回空列表，而不是 500
            None => Ok(Vec::new()),
        }
    }

    fn find_team(&self, catalog_ref: &str) -> TeamsCatalogResult<CatalogTeam> {
        self.teams()?
            .into_iter()
            .find(|t| matches_ref(&t.id, &t.key, &t.slug, catalog_ref))
            .ok_or_else(|| TeamsCatalogError::NotFound(catalog_ref.to_string()))
    }

    async fn existing_agent_names(&self, company_id: Uuid) -> TeamsCatalogResult<HashSet<String>> {
        let rows = sqlx::query("SELECT name FROM agents WHERE company_id = $1")
            .bind(company_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| r.get::<String, _>("name"))
            .collect())
    }
}

#[async_trait]
impl TeamsCatalogService for DefaultTeamsCatalogService {
    async fn list_teams(
        &self,
        kind: Option<&str>,
        category: Option<&str>,
        q: Option<&str>,
    ) -> TeamsCatalogResult<Value> {
        let needle = q.map(|s| s.trim().to_ascii_lowercase()).unwrap_or_default();
        let teams: Vec<CatalogTeam> = self
            .teams()?
            .into_iter()
            .filter(|t| kind.map(|k| t.kind == k).unwrap_or(true))
            .filter(|t| category.map(|c| t.category == c).unwrap_or(true))
            .filter(|t| {
                if needle.is_empty() {
                    return true;
                }
                t.name.to_ascii_lowercase().contains(&needle)
                    || t.slug.to_ascii_lowercase().contains(&needle)
                    || t.description.to_ascii_lowercase().contains(&needle)
                    || t.tags.iter().any(|g| g.to_ascii_lowercase().contains(&needle))
            })
            .collect();

        Ok(json!({
            "teams": teams,
            "total": teams.len(),
            "catalogAvailable": resolve_catalog_root().is_some(),
        }))
    }

    async fn get_team(&self, catalog_ref: &str) -> TeamsCatalogResult<Value> {
        let team = self.find_team(catalog_ref)?;
        Ok(serde_json::to_value(team).unwrap_or(Value::Null))
    }

    async fn read_team_file(
        &self,
        catalog_ref: &str,
        rel_path: &str,
    ) -> TeamsCatalogResult<Value> {
        let team = self.find_team(catalog_ref)?;
        let rel = if rel_path.trim().is_empty() {
            "TEAM.md"
        } else {
            rel_path.trim()
        };
        if !is_safe_catalog_path(rel) {
            return Err(TeamsCatalogError::InvalidInput(format!(
                "unsafe catalog path: {rel}"
            )));
        }
        let base = PathBuf::from(&team.path);
        let full = base.join(rel);
        // 二次校验：规范化后仍需位于 team 目录内
        let canonical_base = base.canonicalize().map_err(|e| TeamsCatalogError::Io(e.to_string()))?;
        let canonical_full = full
            .canonicalize()
            .map_err(|_| TeamsCatalogError::NotFound(rel.to_string()))?;
        if !canonical_full.starts_with(&canonical_base) {
            return Err(TeamsCatalogError::InvalidInput(
                "path escapes catalog team directory".to_string(),
            ));
        }
        let content = std::fs::read_to_string(&canonical_full)
            .map_err(|_| TeamsCatalogError::NotFound(rel.to_string()))?;

        Ok(json!({
            "catalogTeamId": team.id,
            "path": rel,
            "kind": file_kind(rel),
            "markdown": rel.to_ascii_lowercase().ends_with(".md"),
            "content": content,
        }))
    }

    async fn list_installed(&self, company_id: Uuid) -> TeamsCatalogResult<Value> {
        let rows = sqlx::query(
            r#"
            SELECT catalog_id, catalog_key, content_hash, agent_ids, agent_count,
                   created_at, updated_at
            FROM company_team_installs
            WHERE company_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;

        let catalog = self.teams()?;
        let items: Vec<Value> = rows
            .into_iter()
            .map(|r| {
                let catalog_id: String = r.get("catalog_id");
                let installed_hash: String = r.get("content_hash");
                let current = catalog.iter().find(|t| t.id == catalog_id);
                json!({
                    "catalogId": catalog_id,
                    "catalogKey": r.get::<Option<String>, _>("catalog_key"),
                    "present": current.is_some(),
                    "currentContentHash": current.map(|t| t.content_hash.clone()),
                    "installedContentHash": installed_hash,
                    "outOfDate": current
                        .map(|t| t.content_hash != installed_hash)
                        .unwrap_or(false),
                    "agentCount": r.get::<i32, _>("agent_count"),
                    "agentIds": r.get::<Vec<Uuid>, _>("agent_ids"),
                    "installedAt": r.get::<chrono::DateTime<Utc>, _>("created_at"),
                    "updatedAt": r.get::<chrono::DateTime<Utc>, _>("updated_at"),
                })
            })
            .collect();

        Ok(json!({ "installed": items, "total": items.len() }))
    }

    async fn preview_install(
        &self,
        company_id: Uuid,
        catalog_ref: &str,
        options: &Value,
    ) -> TeamsCatalogResult<Value> {
        let team = self.find_team(catalog_ref)?;
        validate_reports_to_graph(&team.agents)
            .map_err(TeamsCatalogError::InvalidCatalog)?;

        let strategy = CollisionStrategy::parse(
            options
                .get("collisionStrategy")
                .and_then(|v| v.as_str()),
        );
        let name_overrides: HashMap<String, String> = options
            .get("nameOverrides")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let target_manager_id = options
            .get("targetManagerAgentId")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        // 目标 manager 必须属于本公司
        let mut manager_warning: Option<String> = None;
        if let Some(mid) = target_manager_id {
            let row = sqlx::query("SELECT company_id FROM agents WHERE id = $1")
                .bind(mid)
                .fetch_optional(&self.pool)
                .await?;
            match row {
                Some(r) if r.get::<Uuid, _>("company_id") == company_id => {}
                Some(_) => {
                    manager_warning =
                        Some("targetManagerAgentId belongs to another company".to_string())
                }
                None => manager_warning = Some("targetManagerAgentId not found".to_string()),
            }
        }

        let mut existing = self.existing_agent_names(company_id).await?;
        let mut planned = Vec::new();
        let mut conflicts = Vec::new();
        let mut blocking_error: Option<String> = None;

        for agent in &team.agents {
            let proposed = name_overrides
                .get(&agent.slug)
                .cloned()
                .unwrap_or_else(|| agent.name.clone());
            match resolve_name_collision(&existing, &proposed, strategy) {
                NameResolution::Create(final_name) => {
                    if final_name != proposed {
                        conflicts.push(json!({
                            "slug": agent.slug,
                            "proposedName": proposed,
                            "resolution": "rename",
                            "finalName": final_name,
                        }));
                    }
                    existing.insert(final_name.clone());
                    planned.push(json!({
                        "slug": agent.slug,
                        "name": final_name,
                        "role": map_agent_role(&agent.role),
                        "catalogRole": agent.role,
                        "reportsToSlug": agent.reports_to,
                        "skills": agent.skills,
                        "action": "create",
                    }));
                }
                NameResolution::Skip => {
                    conflicts.push(json!({
                        "slug": agent.slug,
                        "proposedName": proposed,
                        "resolution": "skip",
                    }));
                    planned.push(json!({
                        "slug": agent.slug,
                        "name": proposed,
                        "action": "skip",
                    }));
                }
                NameResolution::Fail(msg) => {
                    conflicts.push(json!({
                        "slug": agent.slug,
                        "proposedName": proposed,
                        "resolution": "fail",
                        "reason": msg.clone(),
                    }));
                    blocking_error = Some(msg);
                }
            }
        }

        // 已安装状态
        let installed = sqlx::query(
            "SELECT content_hash FROM company_team_installs WHERE company_id = $1 AND catalog_id = $2",
        )
        .bind(company_id)
        .bind(&team.id)
        .fetch_optional(&self.pool)
        .await?;

        let mut warnings: Vec<String> = Vec::new();
        if let Some(w) = manager_warning {
            warnings.push(w);
        }
        if installed.is_some() {
            warnings.push("team already installed for this company".to_string());
        }
        if target_manager_id.is_none() {
            warnings.push("no targetManagerAgentId provided: root agents will have no manager".to_string());
        }

        Ok(json!({
            "team": team,
            "plannedAgents": planned,
            "conflicts": conflicts,
            "requiredSkills": team.required_skills,
            "permissionImpact": {
                "requiredPermission": "agents:create",
                "willCreateAgents": planned.iter().filter(|p| p["action"] == "create").count(),
            },
            "alreadyInstalled": installed.is_some(),
            "warnings": warnings,
            "errors": blocking_error.map(|e| vec![e]).unwrap_or_default(),
            "installable": true,
        }))
    }

    async fn install(
        &self,
        company_id: Uuid,
        catalog_ref: &str,
        options: &Value,
        actor: InstallActor,
    ) -> TeamsCatalogResult<Value> {
        let team = self.find_team(catalog_ref)?;
        validate_reports_to_graph(&team.agents)
            .map_err(TeamsCatalogError::InvalidCatalog)?;

        let force = options
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let strategy = CollisionStrategy::parse(
            options.get("collisionStrategy").and_then(|v| v.as_str()),
        );
        let name_overrides: HashMap<String, String> = options
            .get("nameOverrides")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let target_manager_id = options
            .get("targetManagerAgentId")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        let mut tx = self.pool.begin().await?;

        // 幂等：已安装且未 force → 直接返回既有记录，不做任何变更
        let existing_install = sqlx::query(
            r#"
            SELECT catalog_id, content_hash, agent_ids, agent_count, created_at
            FROM company_team_installs
            WHERE company_id = $1 AND catalog_id = $2
            "#,
        )
        .bind(company_id)
        .bind(&team.id)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(row) = &existing_install {
            if !force {
                let result = json!({
                    "team": team,
                    "alreadyInstalled": true,
                    "createdAgents": [],
                    "skippedAgents": [],
                    "agentIds": row.get::<Vec<Uuid>, _>("agent_ids"),
                    "agentCount": row.get::<i32, _>("agent_count"),
                    "installedAt": row.get::<chrono::DateTime<Utc>, _>("created_at"),
                    "warnings": ["team already installed; pass force=true to reinstall"],
                });
                tx.rollback().await?;
                return Ok(result);
            }
        }

        // 目标 manager 的 company boundary 校验
        if let Some(mid) = target_manager_id {
            let row = sqlx::query("SELECT company_id FROM agents WHERE id = $1")
                .bind(mid)
                .fetch_optional(&mut *tx)
                .await?;
            match row {
                Some(r) if r.get::<Uuid, _>("company_id") == company_id => {}
                Some(_) => {
                    tx.rollback().await?;
                    return Err(TeamsCatalogError::InvalidInput(
                        "targetManagerAgentId belongs to another company".to_string(),
                    ));
                }
                None => {
                    tx.rollback().await?;
                    return Err(TeamsCatalogError::InvalidInput(
                        "targetManagerAgentId not found".to_string(),
                    ));
                }
            }
        }

        // 现有名字集合
        let name_rows = sqlx::query("SELECT name FROM agents WHERE company_id = $1")
            .bind(company_id)
            .fetch_all(&mut *tx)
            .await?;
        let mut existing_names: HashSet<String> = name_rows
            .into_iter()
            .map(|r| r.get::<String, _>("name"))
            .collect();

        let mut created: HashMap<String, Uuid> = HashMap::new();
        let mut created_records: Vec<Value> = Vec::new();
        let mut skipped: Vec<Value> = Vec::new();

        // 第一阶段：创建 Agent（reports_to 先留空，避免顺序依赖）
        for agent in &team.agents {
            let proposed = name_overrides
                .get(&agent.slug)
                .cloned()
                .unwrap_or_else(|| agent.name.clone());
            let final_name = match resolve_name_collision(&existing_names, &proposed, strategy) {
                NameResolution::Create(n) => n,
                NameResolution::Skip => {
                    skipped.push(json!({ "slug": agent.slug, "name": proposed, "reason": "name exists" }));
                    continue;
                }
                NameResolution::Fail(msg) => {
                    // 整单回滚
                    tx.rollback().await?;
                    return Err(TeamsCatalogError::Conflict(msg));
                }
            };

            let agent_id = Uuid::new_v4();
            let role = map_agent_role(&agent.role);
            let metadata = json!({
                "catalogTeamId": team.id,
                "catalogAgentSlug": agent.slug,
                "catalogTitle": agent.title,
                "catalogSkills": agent.skills,
            });

            sqlx::query(
                r#"
                INSERT INTO agents (
                    id, company_id, name, role, status, adapter_type,
                    adapter_config, runtime_config, permissions, metadata,
                    budget_monthly_cents, reports_to, created_at, updated_at
                )
                VALUES ($1, $2, $3, $4::agent_role, 'active'::agent_status, 'claude_code',
                        '{}'::jsonb, '{}'::jsonb, $5::jsonb, $6::jsonb,
                        0, NULL, NOW(), NOW())
                "#,
            )
            .bind(agent_id)
            .bind(company_id)
            .bind(&final_name)
            .bind(role)
            .bind(json!({
                "can_create_agents": role == "ceo",
                "can_create_skills": true,
                "trust_preset": "standard",
                "authorization_policy": "manual"
            }))
            .bind(&metadata)
            .execute(&mut *tx)
            .await?;

            existing_names.insert(final_name.clone());
            created.insert(agent.slug.clone(), agent_id);
            created_records.push(json!({
                "id": agent_id,
                "slug": agent.slug,
                "name": final_name,
                "role": role,
            }));
        }

        // 第二阶段：回填 reports_to（catalog 内部 slug → 新 id；根节点 → targetManager）
        for agent in &team.agents {
            let Some(agent_id) = created.get(&agent.slug) else {
                continue;
            };
            let parent = resolve_reports_to(agent.reports_to.as_deref(), &created, target_manager_id);
            if let Some(parent_id) = parent {
                if parent_id == *agent_id {
                    continue;
                }
                sqlx::query("UPDATE agents SET reports_to = $1, updated_at = NOW() WHERE id = $2")
                    .bind(parent_id)
                    .bind(agent_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        // 记录安装（重装时更新）
        let agent_ids: Vec<Uuid> = created.values().copied().collect();
        sqlx::query(
            r#"
            INSERT INTO company_team_installs (
                id, company_id, catalog_id, catalog_key, content_hash,
                agent_ids, agent_count, installed_by_user_id, installed_by_agent_id,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())
            ON CONFLICT (company_id, catalog_id) DO UPDATE
            SET content_hash = EXCLUDED.content_hash,
                agent_ids = EXCLUDED.agent_ids,
                agent_count = EXCLUDED.agent_count,
                updated_at = NOW()
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(&team.id)
        .bind(&team.key)
        .bind(&team.content_hash)
        .bind(&agent_ids)
        .bind(agent_ids.len() as i32)
        .bind(actor.user_id)
        .bind(actor.agent_id)
        .execute(&mut *tx)
        .await?;

        // 审计（同事务，失败一并回滚）
        let _ = sqlx::query(
            r#"
            INSERT INTO activity_logs (
                id, company_id, event_type, actor_type, actor_id,
                resource_type, resource_id, metadata, created_at
            )
            VALUES ($1, $2, 'team_catalog_install', $3, $4, 'team', $5, $6, NOW())
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(if actor.actor_type.is_empty() {
            "user".to_string()
        } else {
            actor.actor_type.clone()
        })
        .bind(actor.user_id.or(actor.agent_id))
        .bind(&team.id)
        .bind(json!({
            "catalogKey": team.key,
            "contentHash": team.content_hash,
            "createdAgents": created_records.len(),
            "skippedAgents": skipped.len(),
            "reinstall": existing_install.is_some(),
        }))
        .execute(&mut *tx)
        .await;

        tx.commit().await?;

        Ok(json!({
            "team": team,
            "alreadyInstalled": false,
            "reinstalled": existing_install.is_some(),
            "createdAgents": created_records,
            "skippedAgents": skipped,
            "agentIds": agent_ids,
            "agentCount": agent_ids.len(),
            "targetManagerAgentId": target_manager_id,
            "requiredSkills": team.required_skills,
        }))
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(slug: &str, reports_to: Option<&str>) -> CatalogTeamAgent {
        CatalogTeamAgent {
            slug: slug.to_string(),
            name: slug.to_uppercase(),
            title: None,
            role: "general".to_string(),
            reports_to: reports_to.map(|s| s.to_string()),
            skills: vec![],
            path: format!("agents/{slug}/AGENTS.md"),
        }
    }

    #[test]
    fn frontmatter_parses_scalars_lists_and_body() {
        let md = "---\nname: CTO\nslug: cto\ndefaultInstall: true\nskills:\n  - a\n  - b\n---\n\n# Body\ntext";
        let (front, body) = parse_frontmatter(md);
        assert_eq!(front.get("name").unwrap().as_str(), Some("CTO"));
        assert_eq!(front.get("defaultInstall").unwrap().as_bool(), Some(true));
        assert_eq!(
            front.get("skills").unwrap().as_array().unwrap().len(),
            2
        );
        assert!(body.starts_with("# Body"));
    }

    #[test]
    fn frontmatter_absent_returns_full_body() {
        let (front, body) = parse_frontmatter("# just markdown");
        assert!(front.is_empty());
        assert_eq!(body, "# just markdown");
    }

    #[test]
    fn safe_catalog_path_blocks_traversal() {
        assert!(is_safe_catalog_path("TEAM.md"));
        assert!(is_safe_catalog_path("agents/ceo/AGENTS.md"));
        assert!(!is_safe_catalog_path("../secret"));
        assert!(!is_safe_catalog_path("/etc/passwd"));
        assert!(!is_safe_catalog_path(""));
        assert!(!is_safe_catalog_path("a\0b"));
    }

    #[test]
    fn role_mapping_covers_known_titles() {
        assert_eq!(map_agent_role("ceo"), "ceo");
        assert_eq!(map_agent_role("engineering-manager"), "manager");
        assert_eq!(map_agent_role("cto"), "manager");
        assert_eq!(map_agent_role("vp-sales"), "vp");
        assert_eq!(map_agent_role("research-analyst"), "researcher");
        assert_eq!(map_agent_role("qa"), "general");
    }

    #[test]
    fn collision_skip_and_rename_and_fail() {
        let mut existing = HashSet::new();
        existing.insert("CEO".to_string());

        assert_eq!(
            resolve_name_collision(&existing, "CTO", CollisionStrategy::Skip),
            NameResolution::Create("CTO".to_string())
        );
        assert_eq!(
            resolve_name_collision(&existing, "CEO", CollisionStrategy::Skip),
            NameResolution::Skip
        );
        assert_eq!(
            resolve_name_collision(&existing, "CEO", CollisionStrategy::Rename),
            NameResolution::Create("CEO (2)".to_string())
        );
        assert!(matches!(
            resolve_name_collision(&existing, "CEO", CollisionStrategy::Fail),
            NameResolution::Fail(_)
        ));
    }

    #[test]
    fn reports_to_maps_internal_slug_then_falls_back_to_manager() {
        let mut created = HashMap::new();
        let ceo = Uuid::new_v4();
        let manager = Uuid::new_v4();
        created.insert("ceo".to_string(), ceo);

        // 内部 slug 命中
        assert_eq!(resolve_reports_to(Some("ceo"), &created, Some(manager)), Some(ceo));
        // 未命中 → 落到 targetManager
        assert_eq!(resolve_reports_to(Some("ghost"), &created, Some(manager)), Some(manager));
        // 根节点 → targetManager
        assert_eq!(resolve_reports_to(None, &created, Some(manager)), Some(manager));
        // 无 targetManager → None
        assert_eq!(resolve_reports_to(None, &created, None), None);
    }

    #[test]
    fn reports_to_graph_rejects_self_and_cycle() {
        assert!(validate_reports_to_graph(&[agent("ceo", None), agent("cto", Some("ceo"))]).is_ok());
        assert!(validate_reports_to_graph(&[agent("ceo", Some("ceo"))]).is_err());
        assert!(
            validate_reports_to_graph(&[agent("a", Some("b")), agent("b", Some("a"))]).is_err()
        );
        // 指向 catalog 外部 slug 视为根节点，不报错
        assert!(validate_reports_to_graph(&[agent("a", Some("outside"))]).is_ok());
    }

    #[test]
    fn collision_strategy_parses_defaults_to_skip() {
        assert_eq!(CollisionStrategy::parse(None), CollisionStrategy::Skip);
        assert_eq!(CollisionStrategy::parse(Some("rename")), CollisionStrategy::Rename);
        assert_eq!(CollisionStrategy::parse(Some("fail")), CollisionStrategy::Fail);
        assert_eq!(CollisionStrategy::parse(Some("weird")), CollisionStrategy::Skip);
    }

    #[test]
    fn ref_matching_accepts_id_key_and_slug() {
        let id = catalog_id("bundled", "company-defaults", "core-exec-team");
        let key = catalog_key("bundled", "company-defaults", "core-exec-team");
        assert!(matches_ref(&id, &key, "core-exec-team", &id));
        assert!(matches_ref(&id, &key, "core-exec-team", &key));
        assert!(matches_ref(&id, &key, "core-exec-team", "core-exec-team"));
        assert!(!matches_ref(&id, &key, "core-exec-team", "nope"));
    }

    #[test]
    fn scan_catalog_reads_team_and_agents() {
        let dir = tempfile::tempdir().unwrap();
        let team_dir = dir
            .path()
            .join("bundled")
            .join("company-defaults")
            .join("core-exec-team");
        std::fs::create_dir_all(team_dir.join("agents").join("ceo")).unwrap();
        std::fs::create_dir_all(team_dir.join("agents").join("cto")).unwrap();
        std::fs::write(
            team_dir.join("TEAM.md"),
            "---\nname: Core Exec Team\nslug: core-exec-team\nmanager: agents/ceo/AGENTS.md\ndefaultInstall: true\ntags:\n  - default\nrequiredSkills:\n  - task-planning\n---\n\nbody",
        )
        .unwrap();
        std::fs::write(
            team_dir.join("agents").join("ceo").join("AGENTS.md"),
            "---\nname: CEO\nslug: ceo\nrole: ceo\n---\n\nbody",
        )
        .unwrap();
        std::fs::write(
            team_dir.join("agents").join("cto").join("AGENTS.md"),
            "---\nname: CTO\nslug: cto\nrole: engineering-manager\nreportsTo: ceo\n---\n\nbody",
        )
        .unwrap();

        let teams = scan_catalog(dir.path());
        assert_eq!(teams.len(), 1);
        let team = &teams[0];
        assert_eq!(team.slug, "core-exec-team");
        assert_eq!(team.kind, "bundled");
        assert_eq!(team.category, "company-defaults");
        assert!(team.default_install);
        assert_eq!(team.manager_slug.as_deref(), Some("ceo"));
        assert_eq!(team.required_skills, vec!["task-planning".to_string()]);
        assert_eq!(team.agents.len(), 2);
        let cto = team.agents.iter().find(|a| a.slug == "cto").unwrap();
        assert_eq!(cto.reports_to.as_deref(), Some("ceo"));
        assert!(!team.content_hash.is_empty());
        assert!(team.files.iter().any(|f| f.path == "TEAM.md" && f.kind == "team"));
    }
}
