//! Shared skill file inventory classification.
//!
//! Paperclip derives a skill's `fileInventory` from the files on disk and stores
//! `{path, kind}` on the skill plus `{path, kind, content}` on each version
//! (`classifyInventoryKind` / `serializeVersionFileInventory` in
//! `server/src/services/company-skills.ts`). Parrot keeps the bytes in
//! `skill_files`, so both projections are derived from that table and must agree
//! on the `kind` vocabulary — hence this single implementation, shared by the
//! repository read paths and the project-skill importer.

use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};

/// Portable (POSIX-style, `.`/`..`-resolved) form of a stored skill file path.
///
/// Mirrors Paperclip's `normalizePortablePath`: backslashes become separators,
/// a leading `./` or `/` is dropped, empty and `.` segments are discarded, and
/// `..` pops the previous segment.
pub fn normalize_portable_path(input: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    let unified = input.replace('\\', "/");
    for segment in unified.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// Inventory kind for a skill file path.
///
/// Order matters and follows Paperclip's `classifyInventoryKind` exactly:
/// `SKILL.md` wins over the `references/`-style prefixes, directory prefixes win
/// over extensions, and a Markdown suffix wins over the script/asset extension
/// lists.
pub fn classify_inventory_kind(relative_path: &str) -> &'static str {
    let normalized = normalize_portable_path(relative_path).to_lowercase();
    if normalized == "skill.md" || normalized.ends_with("/skill.md") {
        return "skill";
    }
    if normalized.starts_with("references/") {
        return "reference";
    }
    if normalized.starts_with("scripts/") {
        return "script";
    }
    if normalized.starts_with("assets/") {
        return "asset";
    }
    if normalized.ends_with(".md") {
        return "markdown";
    }
    let basename = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
    let extension = basename.rsplit_once('.').map(|(_, extension)| extension);
    match extension {
        Some("sh" | "js" | "mjs" | "cjs" | "ts" | "py" | "rb" | "bash") => "script",
        Some("png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "pdf") => "asset",
        _ => "other",
    }
}

/// `{path, kind}` inventory entries for a skill's stored files.
///
/// Paths are de-duplicated by their portable form so a stored `a//b.md` and
/// `a/b.md` cannot produce two entries that the UI would render as one.
pub fn skill_file_inventory(files: &[(String, String)]) -> Vec<Value> {
    let mut seen: Vec<String> = Vec::new();
    let mut inventory: Vec<Value> = Vec::new();
    for (path, _) in files {
        let portable = normalize_portable_path(path);
        if portable.is_empty() || seen.contains(&portable) {
            continue;
        }
        inventory.push(json!({ "path": portable, "kind": classify_inventory_kind(path) }));
        seen.push(portable);
    }
    inventory.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
    });
    inventory
}

/// Version inventory entries (`{path, kind, content}`) for a skill's stored
/// files. Content travels with the version so the diff/restore surfaces stay
/// readable after the files themselves change.
pub fn skill_file_version_inventory(files: &[(String, String)]) -> Vec<Value> {
    let mut seen: Vec<String> = Vec::new();
    let mut inventory: Vec<Value> = Vec::new();
    for (path, content) in files {
        let portable = normalize_portable_path(path);
        if portable.is_empty() || seen.contains(&portable) {
            continue;
        }
        inventory.push(json!({
            "path": portable,
            "kind": classify_inventory_kind(path),
            "content": content,
        }));
        seen.push(portable);
    }
    inventory.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
    });
    inventory
}

/// `skill_files` rows for a skill, as `(path, content)` pairs ordered by path.
pub async fn load_skill_files(
    pool: &PgPool,
    company_id: uuid::Uuid,
    skill_id: uuid::Uuid,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT sf.path, sf.content
        FROM skill_files sf
        JOIN company_skills cs ON cs.id = sf.skill_id AND cs.company_id = $1
        WHERE sf.skill_id = $2
        ORDER BY sf.path
        "#,
    )
    .bind(company_id)
    .bind(skill_id)
    .fetch_all(pool)
    .await
}

/// Transaction-scoped counterpart of [`load_skill_files`] for writers that
/// already hold the skill row lock.
pub async fn load_skill_files_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    company_id: uuid::Uuid,
    skill_id: uuid::Uuid,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT sf.path, sf.content
        FROM skill_files sf
        JOIN company_skills cs ON cs.id = sf.skill_id AND cs.company_id = $1
        WHERE sf.skill_id = $2
        ORDER BY sf.path
        "#,
    )
    .bind(company_id)
    .bind(skill_id)
    .fetch_all(&mut **tx)
    .await
}

/// Every stored file for every skill in a company, keyed by skill id.
///
/// The company-skill list projection needs all of them at once; classifying in
/// Rust (rather than in a SQL `CASE`) keeps the kind rules in one place.
pub async fn load_company_skill_files(
    pool: &PgPool,
    company_id: uuid::Uuid,
) -> Result<std::collections::HashMap<uuid::Uuid, Vec<(String, String)>>, sqlx::Error> {
    let rows: Vec<(uuid::Uuid, String, String)> = sqlx::query_as(
        r#"
        SELECT sf.skill_id, sf.path, sf.content
        FROM skill_files sf
        JOIN company_skills cs ON cs.id = sf.skill_id
        WHERE cs.company_id = $1
        ORDER BY sf.path
        "#,
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;

    let mut grouped: std::collections::HashMap<uuid::Uuid, Vec<(String, String)>> =
        std::collections::HashMap::new();
    for (skill_id, path, content) in rows {
        grouped.entry(skill_id).or_default().push((path, content));
    }
    Ok(grouped)
}

/// Rewrite a projected company skill's `fileInventory` from its stored files.
///
/// `skill_files` holds the bytes, so the inventory is derived from it rather
/// than read back from `company_skills.file_inventory` — that column is only a
/// snapshot the project importer happens to write, and it goes stale as soon as
/// files are edited or removed.
pub fn attach_file_inventory(row: &mut Value, files: &[(String, String)]) {
    if let Some(object) = row.as_object_mut() {
        object.insert(
            "fileInventory".to_string(),
            Value::Array(skill_file_inventory(files)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_path_resolves_segments() {
        assert_eq!(normalize_portable_path("./a/./b.md"), "a/b.md");
        assert_eq!(normalize_portable_path("\\a\\b.md"), "a/b.md");
        assert_eq!(normalize_portable_path("/a/../b.md"), "b.md");
        assert_eq!(normalize_portable_path("a/../../b.md"), "b.md");
        assert_eq!(normalize_portable_path(""), "");
    }

    #[test]
    fn kinds_follow_paperclip_rules() {
        assert_eq!(classify_inventory_kind("SKILL.md"), "skill");
        assert_eq!(classify_inventory_kind("nested/dir/Skill.MD"), "skill");
        assert_eq!(classify_inventory_kind("./references/faq.md"), "reference");
        assert_eq!(classify_inventory_kind("scripts/run.mjs"), "script");
        assert_eq!(classify_inventory_kind("assets/logo.webp"), "asset");
        assert_eq!(classify_inventory_kind("notes/guide.md"), "markdown");
        assert_eq!(classify_inventory_kind("tools/helper.rb"), "script");
        assert_eq!(classify_inventory_kind("media/clip.mp4"), "other");
        assert_eq!(classify_inventory_kind("README"), "other");
    }

    #[test]
    fn inventory_dedupes_portable_paths() {
        let files = vec![
            ("a//b.md".to_string(), "one".to_string()),
            ("./a/b.md".to_string(), "two".to_string()),
        ];
        let entries = skill_file_inventory(&files);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["path"], "a/b.md");
        assert_eq!(entries[0]["kind"], "markdown");
    }
}
