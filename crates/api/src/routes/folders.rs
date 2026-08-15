//! Folders 路由 —— 对齐 Paperclip `server/src/routes/folders.ts`。
//!
//! 端点：
//! - `GET    /companies/:company_id/folders?kind=routine|skill`：列表（含 itemCount）。
//! - `POST   /companies/:company_id/folders`：创建（`folder.created` 审计）。
//! - `POST   /companies/:company_id/folders/ensure-my`：创建个人 skill 目录（board，`folder.personal_ensured`）。
//! - `PATCH  /companies/:company_id/folders/:folder_id`：更新（`folder.updated`）。
//! - `POST   /companies/:company_id/folders/items/move`：移动 item 到目录（`folder.item_moved`）。
//! - `POST   /companies/:company_id/folders/:folder_id/move`：移动目录（`folder.moved`）。
//! - `DELETE /companies/:company_id/folders/:folder_id`：删除（`folder.deleted`）。
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{require_company_access, AccessMode};
use services::auth::AuthorizationActor;

/// 对齐 `folderSlugSchema`：仅小写字母/数字/单连字符。
fn valid_slug(slug: &str) -> bool {
    if slug.is_empty() || slug.len() > 120 {
        return false;
    }
    let bytes = slug.as_bytes();
    if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
        return false;
    }
    let mut prev_hyphen = false;
    for &b in &bytes[1..] {
        if b == b'-' {
            if prev_hyphen {
                return false;
            }
            prev_hyphen = true;
        } else if b.is_ascii_lowercase() || b.is_ascii_digit() {
            prev_hyphen = false;
        } else {
            return false;
        }
    }
    !prev_hyphen // 不能以连字符结尾
}

#[derive(Debug, Deserialize)]
struct ListFoldersQuery {
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateFolderRequest {
    kind: String,
    #[serde(rename = "parentId")]
    parent_id: Option<Uuid>,
    name: String,
    slug: Option<String>,
    color: Option<String>,
    position: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct UpdateFolderRequest {
    name: Option<String>,
    slug: Option<String>,
    color: Option<String>,
    position: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct MoveFolderRequest {
    #[serde(rename = "parentId")]
    parent_id: Option<Uuid>,
    position: i64,
}

#[derive(Debug, Deserialize)]
struct MoveFolderItemRequest {
    kind: String,
    #[serde(rename = "itemId")]
    item_id: Uuid,
    #[serde(rename = "folderId")]
    folder_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct EnsureMyFolderRequest {
    slug: Option<String>,
}

fn folder_json(
    row: &sqlx::postgres::PgRow,
    item_count: Option<i64>,
) -> serde_json::Value {
    use sqlx::Row;
    let mut v = json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "kind": row.get::<String, _>("kind"),
        "parentId": row.get::<Option<Uuid>, _>("parent_id"),
        "name": row.get::<String, _>("name"),
        "slug": row.get::<String, _>("slug"),
        "systemKey": row.get::<Option<String>, _>("system_key"),
        "path": row.get::<String, _>("path"),
        "depth": row.get::<i32, _>("depth"),
        "color": row.get::<Option<String>, _>("color"),
        "position": row.get::<i32, _>("position"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    });
    if let Some(count) = item_count {
        v["itemCount"] = json!(count);
    }
    v
}

fn build_path(_company_id: Uuid, kind: &str, slug: &str, parent: Option<&sqlx::postgres::PgRow>) -> String {
    match parent {
        Some(p) => {
            use sqlx::Row;
            format!("{}/{}", p.get::<String, _>("path"), slug)
        }
        None => format!("/{}/{}", kind, slug),
    }
}

/// GET /companies/:company_id/folders?kind=...
async fn list_folders(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ListFoldersQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let kind = match query.kind.as_deref() {
        Some("routine") => "routine",
        Some("skill") => "skill",
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT f.*, \
         (SELECT COUNT(*) FROM folder_items fi WHERE fi.folder_id = f.id) AS item_count \
         FROM folders f WHERE f.company_id = $1 AND f.kind = $2 ORDER BY f.position ASC, f.created_at ASC",
    )
    .bind(company_id)
    .bind(kind)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list folders: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let folders: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let count = r.get::<i64, _>("item_count");
            folder_json(r, Some(count))
        })
        .collect();

    let all_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM folders WHERE company_id = $1 AND kind = $2",
    )
    .bind(company_id)
    .bind(kind)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to count folders: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let unfiled_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM folder_items fi \
         JOIN folders f ON f.id = fi.folder_id \
         WHERE f.company_id = $1 AND f.kind = $2 AND fi.item_id NOT IN \
         (SELECT item_id FROM folder_items fi2 JOIN folders f2 ON f2.id = fi2.folder_id \
          WHERE f2.company_id = $1)",
    )
    .bind(company_id)
    .bind(kind)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    Ok(Json(json!({
        "kind": kind,
        "folders": folders,
        "allCount": all_count,
        "unfiledCount": unfiled_count,
    })))
}

/// POST /companies/:company_id/folders
async fn create_folder(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateFolderRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let kind = match request.kind.as_str() {
        "routine" => "routine",
        "skill" => "skill",
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let name = request.name.trim().to_string();
    if name.is_empty() || name.len() > 120 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let slug = request
        .slug
        .clone()
        .unwrap_or_else(|| slugify(&name));
    if !valid_slug(&slug) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let position = request.position.unwrap_or(0).max(0) as i32;
    let depth = 1;
    let path = build_path(company_id, kind, &slug, None);

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO folders (id, company_id, kind, parent_id, name, slug, color, path, depth, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(company_id)
    .bind(kind)
    .bind(request.parent_id)
    .bind(&name)
    .bind(&slug)
    .bind(&request.color)
    .bind(&path)
    .bind(depth)
    .bind(position)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create folder: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "folder.created",
        &actor,
        "folder",
        id,
        json!({ "kind": kind, "name": name, "path": path, "parentId": request.parent_id, "position": position }),
    )
    .await;

    let row = sqlx::query("SELECT * FROM folders WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reload folder: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(folder_json(&row, Some(0)))))
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_hyphen = false;
    for ch in name.to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            prev_hyphen = false;
        } else if !prev_hyphen && !out.is_empty() {
            out.push('-');
            prev_hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("folder");
    }
    out
}

/// POST /companies/:company_id/folders/ensure-my
async fn ensure_my_folder(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<EnsureMyFolderRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;

    let slug = request
        .slug
        .clone()
        .unwrap_or_else(|| format!("me-{}", user_id.to_string().get(0..8).unwrap_or("me")));
    if !valid_slug(&slug) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let system_key = format!("personal:{}:{}", user_id, slug);
    let path = format!("/skill/{}", slug);

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO folders (id, company_id, kind, name, slug, system_key, path, depth, position) \
         VALUES ($1, $2, 'skill', $3, $4, $5, $6, 1, 0) \
         ON CONFLICT (company_id, kind, slug) DO UPDATE SET updated_at = NOW() RETURNING id",
    )
    .bind(id)
    .bind(company_id)
    .bind(format!("Personal ({})", user_id))
    .bind(&slug)
    .bind(&system_key)
    .bind(&path)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to ensure personal folder: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "folder.personal_ensured",
        &actor,
        "folder",
        id,
        json!({ "path": path, "systemKey": system_key }),
    )
    .await;

    let row = sqlx::query("SELECT * FROM folders WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reload folder: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(folder_json(&row, None)))
}

/// PATCH /companies/:company_id/folders/:folder_id
async fn update_folder(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, folder_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateFolderRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if let Some(slug) = &request.slug {
        if !valid_slug(slug) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let row = sqlx::query(
        "UPDATE folders SET \
         name = COALESCE($3, name), \
         slug = COALESCE($4, slug), \
         color = COALESCE($5, color), \
         position = COALESCE($6, position), \
         updated_at = NOW() \
         WHERE id = $1 AND company_id = $2 RETURNING *",
    )
    .bind(folder_id)
    .bind(company_id)
    .bind(request.name.as_deref())
    .bind(request.slug.as_deref())
    .bind(request.color.as_deref())
    .bind(request.position.map(|p| p.max(0) as i32))
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update folder: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "folder.updated",
        &actor,
        "folder",
        folder_id,
        json!({ "name": request.name, "position": request.position }),
    )
    .await;

    Ok(Json(folder_json(&row, None)))
}

/// POST /companies/:company_id/folders/items/move
async fn move_folder_item(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<MoveFolderItemRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let kind = match request.kind.as_str() {
        "routine" => "routine",
        "skill" => "skill",
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    // 校验 item 归属本 company
    let owner_exists: bool = match kind {
        "routine" => {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM routines WHERE id = $1 AND company_id = $2)")
                .bind(request.item_id)
                .bind(company_id)
                .fetch_one(&state.pool)
                .await
                .unwrap_or(false)
        }
        _ => {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM company_skills WHERE id = $1 AND company_id = $2)")
                .bind(request.item_id)
                .bind(company_id)
                .fetch_one(&state.pool)
                .await
                .unwrap_or(false)
        }
    };
    if !owner_exists {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 先移除该 item 的所有目录归属，再（可选）挂到目标目录
    sqlx::query("DELETE FROM folder_items WHERE item_kind = $1 AND item_id = $2")
        .bind(kind)
        .bind(request.item_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to clear folder item: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if let Some(folder_id) = request.folder_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM folders WHERE id = $1 AND company_id = $2 AND kind = $3)")
            .bind(folder_id)
            .bind(company_id)
            .bind(kind)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(false);
        if !ok {
            return Err(StatusCode::BAD_REQUEST);
        }
        sqlx::query(
            "INSERT INTO folder_items (folder_id, item_kind, item_id) VALUES ($1, $2, $3) \
             ON CONFLICT (folder_id, item_kind, item_id) DO NOTHING",
        )
        .bind(folder_id)
        .bind(kind)
        .bind(request.item_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to move folder item: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "folder.item_moved",
        &actor,
        if kind == "routine" { "routine" } else { "company_skill" },
        request.item_id,
        json!({ "kind": kind, "folderId": request.folder_id }),
    )
    .await;

    Ok(Json(json!({
        "kind": kind,
        "itemId": request.item_id,
        "folderId": request.folder_id,
    })))
}

/// POST /companies/:company_id/folders/:folder_id/move
async fn move_folder(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, folder_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<MoveFolderRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let row = sqlx::query(
        "UPDATE folders SET parent_id = $3, position = $4, updated_at = NOW() \
         WHERE id = $1 AND company_id = $2 RETURNING *",
    )
    .bind(folder_id)
    .bind(company_id)
    .bind(request.parent_id)
    .bind(request.position.max(0) as i32)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to move folder: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "folder.moved",
        &actor,
        "folder",
        folder_id,
        json!({ "parentId": request.parent_id, "position": request.position }),
    )
    .await;

    Ok(Json(folder_json(&row, None)))
}

/// DELETE /companies/:company_id/folders/:folder_id
async fn delete_folder(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, folder_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let row = sqlx::query(
        "DELETE FROM folders WHERE id = $1 AND company_id = $2 RETURNING kind, name",
    )
    .bind(folder_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to delete folder: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };

    use sqlx::Row;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "folder.deleted",
        &actor,
        "folder",
        folder_id,
        json!({
            "kind": row.get::<String, _>("kind"),
            "name": row.get::<String, _>("name"),
        }),
    )
    .await;

    Ok(Json(json!({ "deleted": { "id": folder_id } })))
}

pub fn folder_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/folders", get(list_folders).post(create_folder))
        .route(
            "/companies/:company_id/folders/ensure-my",
            post(ensure_my_folder),
        )
        .route("/companies/:company_id/folders/items/move", post(move_folder_item))
        .route(
            "/companies/:company_id/folders/:folder_id",
            patch(update_folder).delete(delete_folder),
        )
        .route(
            "/companies/:company_id/folders/:folder_id/move",
            post(move_folder),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_validation_matches_paperclip() {
        assert!(valid_slug("my-folder"));
        assert!(valid_slug("a1-b2"));
        assert!(!valid_slug("My-Folder"));
        assert!(!valid_slug("folder--x"));
        assert!(!valid_slug("-folder"));
        assert!(!valid_slug("folder-"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("has space"));
    }

    #[test]
    fn slugify_produces_valid_slugs() {
        assert_eq!(slugify("My Folder"), "my-folder");
        assert_eq!(slugify("Hello World!"), "hello-world");
        assert_eq!(slugify("123"), "123");
        assert_eq!(slugify("!!!"), "folder");
    }
}
