use crate::app_state::AppState;
use crate::errors::AppError;
use crate::routes::{require_company_access, AccessMode};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use services::auth::AuthorizationActor;
use std::collections::{HashMap, HashSet};
use std::path::{Path as FsPath, PathBuf};
use uuid::Uuid;

/// GET /api/skills/available
/// List all available skills (public access)
pub async fn list_available_skills(State(state): State<AppState>) -> Response {
    match state.skill_registry_service.list_available_skills().await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/skills/index
/// Get skill index with metadata (authenticated)
pub async fn get_skill_index(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if actor.is_anonymous() {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    match state.skill_registry_service.get_skill_index().await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/skills/:skillName
/// Get skill details with examples (authenticated)
pub async fn get_skill_details(
    Path(skill_name): Path<String>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if actor.is_anonymous() {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    match state
        .skill_registry_service
        .get_skill_details(&skill_name)
        .await
    {
        Ok(details) => (StatusCode::OK, Json(details)).into_response(),
        Err(e) => match e {
            services::errors::ServiceError::NotFound(_) => {
                (StatusCode::NOT_FOUND, e.to_string()).into_response()
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}

// ============================================================================
// P2: Skill 补齐 Handlers (SK1-SK38)
// ============================================================================

/// SK1: GET /skills/catalog
async fn get_skill_catalog(
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state
        .skill_registry_service
        .get_catalog()
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK2: GET /skills/catalog/:catalog_id
async fn get_skill_catalog_detail(
    State(state): State<AppState>,
    Path(catalog_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .skill_registry_service
        .get_catalog_detail(catalog_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK3: GET /skills/catalog/files
async fn get_skill_catalog_files(
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state
        .skill_registry_service
        .get_catalog_files()
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK4: GET /companies/:company_id/skills/categories
async fn list_skill_categories(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;
    state
        .skill_registry_service
        .get_categories(company_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK5: GET /companies/:company_id/skills/:skill_id
async fn get_company_skill(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;
    state
        .skill_registry_service
        .get_skill_by_id(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK6: GET /companies/:company_id/skills/:skill_id/fork-precheck
async fn fork_skill_precheck(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;
    state
        .skill_registry_service
        .fork_precheck(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK7: GET /companies/:company_id/skills/:skill_id/versions
async fn list_skill_versions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;
    state
        .skill_registry_service
        .list_skill_versions(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK8: GET /companies/:company_id/skills/:skill_id/versions/:version_id
async fn get_skill_version(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, skill_id, version_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;
    state
        .skill_registry_service
        .get_skill_version(company_id, skill_id, version_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK9-SK12: Test input management
async fn list_skill_test_inputs(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state
        .skill_registry_service
        .list_test_inputs(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn create_skill_test_input(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .skill_registry_service
        .create_test_input(company_id, skill_id, payload)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_skill_test_input(
    State(state): State<AppState>,
    Path((company_id, skill_id, input_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .skill_registry_service
        .update_test_input(company_id, skill_id, input_id, payload)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn delete_skill_test_input(
    State(state): State<AppState>,
    Path((company_id, skill_id, input_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state
        .skill_registry_service
        .delete_test_input(company_id, skill_id, input_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK13-SK16: Test run template management
async fn list_skill_test_run_templates(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state
        .skill_registry_service
        .list_test_run_templates(company_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn create_skill_test_run_template(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .skill_registry_service
        .create_test_run_template(company_id, payload)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_skill_test_run_template(
    State(state): State<AppState>,
    Path((company_id, template_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .skill_registry_service
        .update_test_run_template(company_id, template_id, payload)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn delete_skill_test_run_template(
    State(state): State<AppState>,
    Path((company_id, template_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state
        .skill_registry_service
        .delete_test_run_template(company_id, template_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK17-SK20: Test run management
async fn list_skill_test_runs(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state
        .skill_registry_service
        .list_test_runs(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn get_skill_test_run(
    State(state): State<AppState>,
    Path((company_id, skill_id, run_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .skill_registry_service
        .get_test_run(company_id, skill_id, run_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn cancel_skill_test_run(
    State(state): State<AppState>,
    Path((company_id, skill_id, run_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .skill_registry_service
        .cancel_test_run(company_id, skill_id, run_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn delete_skill_test_run(
    State(state): State<AppState>,
    Path((company_id, skill_id, run_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state
        .skill_registry_service
        .delete_test_run(company_id, skill_id, run_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK21: Star / SK22: Unstar
async fn star_company_skill(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;
    state
        .skill_registry_service
        .star_skill(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn unstar_company_skill(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;
    state
        .skill_registry_service
        .unstar_skill(company_id, skill_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// P1.3: 公司 Skill 策略网关。
///
/// 在 skill 变更类操作（import / install / fork）前统一评估：
/// - 平台安全层（受保护 skill）拒绝 → 403 Forbidden
/// - 公司策略层拒绝 → 403 Forbidden
async fn enforce_skill_policy(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    action: &str,
    source: &str,
    skill_key: &str,
) -> Result<(), AppError> {
    let role = actor_policy_role(actor, company_id);
    let agent_id = if actor.is_agent() {
        actor.principal_id()
    } else {
        None
    };
    let decision = state
        .skill_policy_service
        .evaluate(company_id, agent_id, &role, action, source, skill_key)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    if !decision.allowed {
        return Err(AppError::Forbidden(decision.reason));
    }
    Ok(())
}

/// 从 Actor 推导策略评估用的 role 字符串。
fn actor_policy_role(actor: &AuthorizationActor, company_id: Uuid) -> String {
    if actor.is_instance_admin() {
        return "instance_admin".to_string();
    }
    if actor.is_agent() {
        return "agent".to_string();
    }
    match actor.role_in(company_id) {
        Some(role) => format!("{:?}", role).to_ascii_lowercase(),
        None => "anonymous".to_string(),
    }
}

/// SK23: Fork
async fn fork_company_skill(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    enforce_skill_policy(
        &state,
        &actor,
        company_id,
        "fork",
        "company",
        &skill_id.to_string(),
    )
    .await?;
    state
        .skill_registry_service
        .fork_skill(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK24: Audit
async fn audit_company_skill(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .skill_registry_service
        .audit_skill(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK25: Install update
async fn install_skill_update(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .skill_registry_service
        .install_skill_update(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK26: Reset
async fn reset_company_skill(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .skill_registry_service
        .reset_skill(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK27: Update status
async fn get_skill_update_status(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .skill_registry_service
        .get_skill_update_status(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK28-SK31: Comments
async fn list_skill_comments(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state
        .skill_registry_service
        .list_skill_comments(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn add_skill_comment(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .skill_registry_service
        .add_skill_comment(company_id, skill_id, payload)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_skill_comment(
    State(state): State<AppState>,
    Path((company_id, skill_id, comment_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .skill_registry_service
        .update_skill_comment(company_id, skill_id, comment_id, payload)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn delete_skill_comment(
    State(state): State<AppState>,
    Path((company_id, skill_id, comment_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state
        .skill_registry_service
        .delete_skill_comment(company_id, skill_id, comment_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK32-SK34: Files
async fn list_skill_files(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state
        .skill_registry_service
        .list_skill_files(company_id, skill_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn update_skill_files(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .skill_registry_service
        .update_skill_files(company_id, skill_id, payload)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn delete_skill_files(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state
        .skill_registry_service
        .delete_skill_files(company_id, skill_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK35: Import
async fn import_company_skill(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;
    // P1.3: 导入前评估公司 skill 策略（受保护 skill 一律拒绝）
    let skill_key = payload
        .get("skillKey")
        .or_else(|| payload.get("key"))
        .or_else(|| payload.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    enforce_skill_policy(&state, &actor, company_id, "import", "import", &skill_key).await?;

    let result = state
        .skill_registry_service
        .import_skill(company_id, payload)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// SK39: Create a standalone (independent) company skill that persists as a
/// `company_skills` row. Distinct from import/fork/install: the caller supplies
/// the skill's own name/slug/description/category/version/tags/config (and
/// optional `files`), and the row is marked `is_paperclip_managed = false`.
async fn create_company_skill(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;
    let skill_key = payload
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    enforce_skill_policy(&state, &actor, company_id, "create", "company", &skill_key).await?;
    let result = state
        .skill_registry_service
        .create_company_skill(company_id, payload)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// SK36: Install catalog
async fn install_skill_catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;
    enforce_skill_policy(&state, &actor, company_id, "install", "catalog", "*").await?;
    state
        .skill_registry_service
        .install_catalog(company_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSkillScanRequest {
    project_ids: Option<Vec<Uuid>>,
    workspace_ids: Option<Vec<Uuid>>,
    mode: Option<String>,
    selection: Option<Vec<ProjectSkillScanSelection>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSkillScanSelection {
    workspace_id: Uuid,
    path: String,
    slug: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ProjectSkillScanWorkspace {
    project_id: Uuid,
    project_name: String,
    workspace_id: Uuid,
    workspace_name: String,
    workspace_cwd: Option<String>,
}

#[derive(Debug, Clone)]
struct ProjectSkillScanDirectory {
    skill_dir: PathBuf,
    directory_root: String,
    relative_path: String,
}

#[derive(Debug, Clone)]
struct ProjectSkillFile {
    path: String,
    content: String,
    mime_type: &'static str,
}

#[derive(Debug, Clone)]
struct DiscoveredProjectSkill {
    project_id: Uuid,
    project_name: String,
    workspace_id: Uuid,
    workspace_name: String,
    workspace_root: PathBuf,
    directory_root: String,
    relative_path: String,
    skill_dir: PathBuf,
    slug: String,
    name: String,
    description: Option<String>,
    markdown: String,
    files: Vec<ProjectSkillFile>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ExistingCompanySkill {
    id: Uuid,
    key: String,
    slug: String,
    source_locator: Option<String>,
    metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
struct SkillFileInventoryEntry {
    path: String,
    kind: &'static str,
}

/// SK37: Discover and optionally import project workspace skills.
///
/// This mirrors Paperclip's preview/import split. Filesystem reads happen
/// before the import transaction; all database writes for one import are
/// guarded by a company advisory lock and committed atomically.
async fn scan_skill_projects(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    payload: Option<Json<ProjectSkillScanRequest>>,
) -> Result<Json<Value>, AppError> {
    let request = payload.map(|Json(value)| value).unwrap_or_default();
    let mode = request.mode.as_deref().unwrap_or("import");
    if !matches!(mode, "preview" | "import") {
        return Err(AppError::BadRequest(
            "mode must be either 'preview' or 'import'".to_string(),
        ));
    }
    let access_mode = if mode == "import" {
        AccessMode::Write
    } else {
        AccessMode::Read
    };
    require_company_access(&actor, company_id, access_mode)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;

    let result = scan_project_skill_workspaces(&state.pool, company_id, &request, mode).await?;
    Ok(Json(result))
}

const PROJECT_SKILL_SCAN_ROOTS: &[&str] = &[
    "skills",
    ".agents/skills",
    ".agent/skills",
    ".claude/skills",
    ".codex/skills",
    ".cursor/skills",
    ".gemini/skills",
    ".opencode/skills",
    ".pi/skills",
    ".roo/skills",
    ".windsurf/skills",
];

const MAX_PROJECT_SKILL_FILE_BYTES: usize = 1_000_000;
const MAX_PROJECT_SKILL_FILES: usize = 256;

async fn scan_project_skill_workspaces(
    pool: &sqlx::PgPool,
    company_id: Uuid,
    request: &ProjectSkillScanRequest,
    mode: &str,
) -> Result<Value, AppError> {
    let workspaces: Vec<ProjectSkillScanWorkspace> = sqlx::query_as(
        r#"
        SELECT
            p.id AS project_id,
            p.name AS project_name,
            pw.id AS workspace_id,
            pw.name AS workspace_name,
            pw.config->>'cwd' AS workspace_cwd
        FROM project_workspaces pw
        JOIN projects p ON p.id = pw.project_id
        WHERE p.company_id = $1
        ORDER BY p.name, pw.is_primary DESC, pw.name, pw.id
        "#,
    )
    .bind(company_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    let project_filter: Option<HashSet<Uuid>> = request
        .project_ids
        .as_ref()
        .map(|ids| ids.iter().copied().collect());
    let workspace_filter: Option<HashSet<Uuid>> = request
        .workspace_ids
        .as_ref()
        .map(|ids| ids.iter().copied().collect());
    let selected = normalized_scan_selections(request.selection.as_deref().unwrap_or(&[]));
    let selective_import = mode == "import" && request.selection.is_some();
    let selected_workspace_ids: HashSet<Uuid> = selected.keys().map(|(id, _)| *id).collect();

    let mut skipped = Vec::new();
    let mut warnings = Vec::new();
    let mut discovered_skills = Vec::new();
    let mut rediscovered = HashSet::new();
    let mut scanned_project_ids = HashSet::new();
    let mut scanned_workspaces = 0_i64;
    let mut discovered_count = 0_i64;
    let mut total_filtered_projects = HashSet::new();
    let total_filtered_workspaces = workspaces
        .iter()
        .filter(|workspace| {
            project_filter
                .as_ref()
                .map(|ids| ids.contains(&workspace.project_id))
                .unwrap_or(true)
                && workspace_filter
                    .as_ref()
                    .map(|ids| ids.contains(&workspace.workspace_id))
                    .unwrap_or(true)
        })
        .count() as i64;

    for workspace in workspaces.into_iter().filter(|workspace| {
        project_filter
            .as_ref()
            .map(|ids| ids.contains(&workspace.project_id))
            .unwrap_or(true)
            && workspace_filter
                .as_ref()
                .map(|ids| ids.contains(&workspace.workspace_id))
                .unwrap_or(true)
    }) {
        total_filtered_projects.insert(workspace.project_id);
        if selective_import && !selected_workspace_ids.contains(&workspace.workspace_id) {
            continue;
        }
        let Some(cwd) = workspace
            .workspace_cwd
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        else {
            skipped.push(json!({
                "projectId": workspace.project_id,
                "projectName": workspace.project_name,
                "workspaceId": workspace.workspace_id,
                "workspaceName": workspace.workspace_name,
                "path": null,
                "reason": "No local workspace path is configured.",
            }));
            warnings.push(format!(
                "Skipped {} / {}: no local workspace path is configured.",
                workspace.project_name, workspace.workspace_name
            ));
            continue;
        };
        let Ok(workspace_root) = tokio::fs::canonicalize(cwd).await else {
            skipped.push(json!({
                "projectId": workspace.project_id,
                "projectName": workspace.project_name,
                "workspaceId": workspace.workspace_id,
                "workspaceName": workspace.workspace_name,
                "path": cwd,
                "reason": "Local workspace path is not available.",
            }));
            warnings.push(format!(
                "Skipped {} / {}: local workspace path is not available at {}.",
                workspace.project_name, workspace.workspace_name, cwd
            ));
            continue;
        };
        if !tokio::fs::metadata(&workspace_root)
            .await
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            continue;
        }

        scanned_workspaces += 1;
        scanned_project_ids.insert(workspace.project_id);
        let (directories, discovery_warnings) =
            discover_project_skill_directories(&workspace_root, &selected, workspace.workspace_id)
                .await;
        warnings.extend(discovery_warnings);
        for directory in directories {
            discovered_count += 1;
            let selection_key = (workspace.workspace_id, directory.relative_path.clone());
            let is_selected = !selective_import || selected.contains_key(&selection_key);
            if selective_import && !is_selected {
                continue;
            }
            if selected.contains_key(&selection_key) {
                rediscovered.insert(selection_key);
            }

            match read_project_skill(&workspace, &workspace_root, &directory).await {
                Ok(skill) => discovered_skills.push(skill),
                Err(reason) => {
                    let reason = format!("Project skill candidate could not be read: {reason}");
                    warnings.push(reason.clone());
                    skipped.push(json!({
                        "projectId": workspace.project_id,
                        "projectName": workspace.project_name,
                        "workspaceId": workspace.workspace_id,
                        "workspaceName": workspace.workspace_name,
                        "path": directory.relative_path,
                        "reason": reason,
                    }));
                }
            }
        }
    }

    let mut existing_skills = if mode == "import" {
        Vec::new()
    } else {
        load_existing_company_skills(pool, company_id).await?
    };
    let mut candidates = Vec::new();
    let mut conflicts = Vec::new();
    let mut imported = Vec::new();
    let mut updated = Vec::new();

    let mut transaction = if mode == "import" {
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
            .bind(format!("company-skill-project-scan:{company_id}"))
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
        existing_skills = load_existing_company_skills_tx(&mut tx, company_id).await?;
        Some(tx)
    } else {
        None
    };

    for skill in discovered_skills
        .iter()
        .filter(|skill| !skill.markdown.is_empty())
    {
        let requested_slug = selected
            .get(&(skill.workspace_id, skill.relative_path.clone()))
            .and_then(|slug| slug.clone());
        let slug = requested_slug.unwrap_or_else(|| skill.slug.clone());
        let source_locator = skill.skill_dir.to_string_lossy().to_string();
        let key = format!(
            "local/{}/{slug}",
            project_skill_source_hash(&source_locator)
        );
        let existing_by_source = existing_skills.iter().find(|existing| {
            existing.source_locator.as_deref() == Some(source_locator.as_str())
                || existing
                    .metadata
                    .get("sourceLocator")
                    .and_then(Value::as_str)
                    == Some(source_locator.as_str())
        });
        let existing_by_key_or_slug = existing_skills
            .iter()
            .find(|existing| existing.key == key || existing.slug == slug);
        let same_source = existing_by_source.is_some();

        if let Some(existing) = existing_by_source.or(existing_by_key_or_slug) {
            if !same_source {
                let reason = format!("Slug {} is already in use by {}.", slug, existing.key);
                conflicts.push(json!({
                    "slug": slug,
                    "key": key,
                    "projectId": skill.project_id,
                    "projectName": skill.project_name,
                    "workspaceId": skill.workspace_id,
                    "workspaceName": skill.workspace_name,
                    "path": skill.relative_path,
                    "existingSkillId": existing.id,
                    "existingSkillKey": existing.key,
                    "existingSourceLocator": existing.source_locator,
                    "reason": reason,
                }));
                candidates.push(project_skill_candidate(
                    skill,
                    &slug,
                    "conflict",
                    Some(existing.id),
                    Some(reason),
                ));
                continue;
            }
            let reason = "This skill is already installed from the same path.";
            candidates.push(project_skill_candidate(
                skill,
                &slug,
                "already_imported",
                Some(existing.id),
                Some(reason.to_string()),
            ));
            if mode == "preview" {
                continue;
            }
            let transaction_ref = transaction.as_mut().expect("import transaction");
            let persisted = persist_project_skill(
                transaction_ref,
                company_id,
                skill,
                &slug,
                &key,
                Some(existing.id),
            )
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
            updated.push(persisted);
            continue;
        }

        candidates.push(project_skill_candidate(skill, &slug, "new", None, None));
        if mode == "preview" {
            continue;
        }
        let transaction_ref = transaction.as_mut().expect("import transaction");
        let persisted =
            persist_project_skill(transaction_ref, company_id, skill, &slug, &key, None)
                .await
                .map_err(|e| AppError::InternalServerError(e.to_string()))?;
        let persisted_id = persisted
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| Uuid::parse_str(id).ok());
        imported.push(persisted);
        if let Some(id) = persisted_id {
            existing_skills.push(ExistingCompanySkill {
                id,
                key: key.clone(),
                slug: slug.clone(),
                source_locator: Some(skill.skill_dir.to_string_lossy().to_string()),
                metadata: project_skill_metadata(skill),
            });
        }
    }

    if let Some(tx) = transaction {
        tx.commit()
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    }

    if selective_import {
        for ((workspace_id, path), renamed_slug) in selected {
            if rediscovered.contains(&(workspace_id, path.clone())) {
                continue;
            }
            skipped.push(json!({
                "projectId": Value::Null,
                "projectName": Value::Null,
                "workspaceId": workspace_id,
                "workspaceName": Value::Null,
                "path": path,
                "reason": format!("The selected path was not rediscovered in the workspace scan{}.", if renamed_slug.is_some() { "" } else { "" }),
            }));
        }
    }

    Ok(json!({
        "companyId": company_id,
        "scanComplete": true,
        "projectsScanned": total_filtered_projects.len() as i64,
        "workspaceCount": total_filtered_workspaces,
        "scannedProjects": scanned_project_ids.len() as i64,
        "scannedWorkspaces": scanned_workspaces,
        "discovered": discovered_count,
        "imported": imported,
        "updated": updated,
        "skipped": skipped,
        "conflicts": conflicts,
        "candidates": candidates,
        "warnings": warnings,
    }))
}

fn normalized_scan_selections(
    selections: &[ProjectSkillScanSelection],
) -> HashMap<(Uuid, String), Option<String>> {
    selections
        .iter()
        .filter_map(|selection| {
            let path = normalize_project_skill_path(&selection.path)?;
            let slug = selection.slug.as_deref().map(normalize_skill_slug);
            if selection.slug.is_some()
                && slug
                    .as_deref()
                    .map(|value| value.is_empty())
                    .unwrap_or(true)
            {
                return None;
            }
            Some(((selection.workspace_id, path), slug))
        })
        .collect()
}

async fn discover_project_skill_directories(
    workspace_root: &FsPath,
    selected: &HashMap<(Uuid, String), Option<String>>,
    workspace_id: Uuid,
) -> (Vec<ProjectSkillScanDirectory>, Vec<String>) {
    let mut directories = HashMap::<String, ProjectSkillScanDirectory>::new();
    let mut warnings = Vec::new();
    let mut add_directory = |skill_dir: PathBuf, directory_root: String, relative_path: String| {
        let key = skill_dir.to_string_lossy().to_string();
        directories.entry(key).or_insert(ProjectSkillScanDirectory {
            skill_dir,
            directory_root,
            relative_path,
        });
    };

    let root_skill = workspace_root.join("SKILL.md");
    if is_regular_file(&root_skill).await {
        add_directory(
            workspace_root.to_path_buf(),
            ".".to_string(),
            ".".to_string(),
        );
    }

    for ((selected_workspace_id, relative_path), _) in selected {
        if *selected_workspace_id != workspace_id {
            continue;
        }
        let Some(normalized) = normalize_project_skill_path(relative_path) else {
            continue;
        };
        let candidate = workspace_root.join(normalized.replace('/', std::path::MAIN_SEPARATOR_STR));
        if path_contains_symlink(workspace_root, &candidate).await {
            warnings.push(format!(
                "Skipped symbolic link in selected project skill path: {}",
                candidate.display()
            ));
            continue;
        }
        let Some(canonical) = canonical_contained_path(workspace_root, &candidate).await else {
            continue;
        };
        let Some(directory) =
            skill_directory_from_path(workspace_root, canonical, &normalized).await
        else {
            continue;
        };
        add_directory(directory.0, directory.1, directory.2);
    }

    for relative_root in PROJECT_SKILL_SCAN_ROOTS {
        let root = workspace_root.join(relative_root.replace('/', std::path::MAIN_SEPARATOR_STR));
        let Ok(mut entries) = tokio::fs::read_dir(&root).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };
            if file_type.is_symlink() {
                warnings.push(format!(
                    "Skipped symbolic link in project skill root: {}",
                    entry.path().display()
                ));
                continue;
            }
            if !file_type.is_dir() {
                continue;
            }
            let path = entry.path();
            let Some(canonical) = canonical_contained_path(workspace_root, &path).await else {
                warnings.push(format!(
                    "Skipped project skill path outside workspace: {}",
                    path.display()
                ));
                continue;
            };
            let relative_path = canonical
                .strip_prefix(workspace_root)
                .ok()
                .map(|path| path.to_string_lossy().replace('\\', "/"));
            let Some(relative_path) = relative_path else {
                continue;
            };
            if is_regular_file(&canonical.join("SKILL.md")).await {
                add_directory(canonical, (*relative_root).to_string(), relative_path);
            }
        }
    }

    let mut result: Vec<_> = directories.into_values().collect();
    result.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    (result, warnings)
}

async fn skill_directory_from_path(
    workspace_root: &FsPath,
    canonical: PathBuf,
    normalized: &str,
) -> Option<(PathBuf, String, String)> {
    let metadata = tokio::fs::metadata(&canonical).await.ok()?;
    let directory = if metadata.is_file() && normalized.eq_ignore_ascii_case("skill.md") {
        canonical.parent()?.to_path_buf()
    } else if metadata.is_dir() {
        canonical
    } else {
        return None;
    };
    if !is_regular_file(&directory.join("SKILL.md")).await {
        return None;
    }
    let relative = directory
        .strip_prefix(workspace_root)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    let relative = if relative.is_empty() {
        ".".to_string()
    } else {
        relative
    };
    let root = FsPath::new(&relative)
        .parent()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| ".".to_string());
    Some((directory, root, relative))
}

async fn read_project_skill(
    workspace: &ProjectSkillScanWorkspace,
    workspace_root: &FsPath,
    directory: &ProjectSkillScanDirectory,
) -> Result<DiscoveredProjectSkill, String> {
    let markdown = tokio::fs::read_to_string(directory.skill_dir.join("SKILL.md"))
        .await
        .map_err(|error| error.to_string())?;
    let (frontmatter, _) = parse_skill_frontmatter(&markdown);
    let fallback = directory
        .skill_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("skill");
    let slug = frontmatter
        .get("slug")
        .or_else(|| frontmatter.get("name"))
        .map(|value| normalize_skill_slug(value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| normalize_skill_slug(fallback));
    let name = frontmatter
        .get("name")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string());
    let description = frontmatter
        .get("description")
        .cloned()
        .filter(|value| !value.trim().is_empty());
    let files = collect_project_skill_files(workspace_root, &directory.skill_dir).await?;
    Ok(DiscoveredProjectSkill {
        project_id: workspace.project_id,
        project_name: workspace.project_name.clone(),
        workspace_id: workspace.workspace_id,
        workspace_name: workspace.workspace_name.clone(),
        workspace_root: workspace_root.to_path_buf(),
        directory_root: directory.directory_root.clone(),
        relative_path: directory.relative_path.clone(),
        skill_dir: directory.skill_dir.clone(),
        slug,
        name,
        description,
        markdown,
        files,
    })
}

async fn collect_project_skill_files(
    workspace_root: &FsPath,
    skill_dir: &FsPath,
) -> Result<Vec<ProjectSkillFile>, String> {
    let skill_metadata = tokio::fs::symlink_metadata(skill_dir)
        .await
        .map_err(|error| error.to_string())?;
    if skill_metadata.file_type().is_symlink() {
        return Err(format!("symbolic link found at {}", skill_dir.display()));
    }
    let mut files = Vec::new();
    let is_workspace_root = skill_dir == workspace_root;
    let mut stack = if is_workspace_root {
        let mut roots = Vec::new();
        for relative_dir in ["references", "scripts", "assets"] {
            let path = skill_dir.join(relative_dir);
            let Ok(metadata) = tokio::fs::symlink_metadata(&path).await else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                return Err(format!("symbolic link found at {}", path.display()));
            }
            if metadata.is_dir()
            {
                roots.push(path);
            }
        }
        let root_skill = skill_dir.join("SKILL.md");
        files.push(read_project_skill_file(workspace_root, skill_dir, &root_skill).await?);
        roots
    } else {
        vec![skill_dir.to_path_buf()]
    };
    while let Some(directory) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&directory)
            .await
            .map_err(|error| error.to_string())?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| error.to_string())?
        {
            let file_type = entry.file_type().await.map_err(|error| error.to_string())?;
            let path = entry.path();
            if file_type.is_symlink() {
                return Err(format!("symbolic link found at {}", path.display()));
            }
            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name != ".git" && name != "node_modules" {
                    stack.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let Some(canonical) = canonical_contained_path(workspace_root, &path).await else {
                return Err(format!(
                    "path resolves outside workspace: {}",
                    path.display()
                ));
            };
            let bytes = tokio::fs::read(&canonical)
                .await
                .map_err(|error| error.to_string())?;
            if bytes.len() > MAX_PROJECT_SKILL_FILE_BYTES {
                return Err(format!("file exceeds {MAX_PROJECT_SKILL_FILE_BYTES} bytes"));
            }
            let Ok(content) = String::from_utf8(bytes) else {
                return Err(format!("file is not UTF-8: {}", path.display()));
            };
            let relative = canonical
                .strip_prefix(skill_dir)
                .map_err(|_| "file is outside skill directory".to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            files.push(ProjectSkillFile {
                mime_type: mime_type_for_path(&relative),
                path: relative,
                content,
            });
            if files.len() > MAX_PROJECT_SKILL_FILES {
                return Err(format!(
                    "skill contains more than {MAX_PROJECT_SKILL_FILES} files"
                ));
            }
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

async fn read_project_skill_file(
    workspace_root: &FsPath,
    skill_dir: &FsPath,
    path: &FsPath,
) -> Result<ProjectSkillFile, String> {
    let metadata = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err(format!("symbolic link found at {}", path.display()));
    }
    let Some(canonical) = canonical_contained_path(workspace_root, path).await else {
        return Err(format!("path resolves outside workspace: {}", path.display()));
    };
    let bytes = tokio::fs::read(&canonical)
        .await
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_PROJECT_SKILL_FILE_BYTES {
        return Err(format!(
            "file exceeds {MAX_PROJECT_SKILL_FILE_BYTES} bytes"
        ));
    }
    let Ok(content) = String::from_utf8(bytes) else {
        return Err(format!("file is not UTF-8: {}", path.display()));
    };
    let relative = canonical
        .strip_prefix(skill_dir)
        .map_err(|_| "file is outside skill directory".to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(ProjectSkillFile {
        mime_type: mime_type_for_path(&relative),
        path: relative,
        content,
    })
}

async fn load_existing_company_skills(
    pool: &sqlx::PgPool,
    company_id: Uuid,
) -> Result<Vec<ExistingCompanySkill>, AppError> {
    sqlx::query_as(
        "SELECT id, key, slug, source_locator, metadata FROM company_skills WHERE company_id = $1",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn load_existing_company_skills_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    company_id: Uuid,
) -> Result<Vec<ExistingCompanySkill>, AppError> {
    sqlx::query_as(
        "SELECT id, key, slug, source_locator, metadata FROM company_skills WHERE company_id = $1 FOR UPDATE",
    )
    .bind(company_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn persist_project_skill(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    company_id: Uuid,
    skill: &DiscoveredProjectSkill,
    slug: &str,
    key: &str,
    existing_id: Option<Uuid>,
) -> Result<Value, sqlx::Error> {
    let metadata = project_skill_metadata(skill);
    let file_inventory: Vec<SkillFileInventoryEntry> = skill
        .files
        .iter()
        .map(|file| SkillFileInventoryEntry {
            path: file.path.clone(),
            kind: inventory_kind(&file.path),
        })
        .collect();
    let file_inventory_json = serde_json::to_value(&file_inventory).unwrap_or_else(|_| json!([]));
    let skill_id = if let Some(existing_id) = existing_id {
        sqlx::query_scalar::<_, Uuid>(
            r#"
            UPDATE company_skills
            SET key = $3, slug = $4, name = $5, description = $6, markdown = $7,
                source_type = 'local_path', source_locator = $8, source_ref = NULL,
                trust_level = $9, compatibility = 'compatible', file_inventory = $10,
                categories = '[]'::jsonb, sharing_scope = 'company', metadata = $11,
                status = 'active', is_paperclip_managed = false, updated_at = NOW()
            WHERE id = $2 AND company_id = $1
            RETURNING id
            "#,
        )
        .bind(company_id)
        .bind(existing_id)
        .bind(key)
        .bind(slug)
        .bind(&skill.name)
        .bind(skill.description.as_deref().unwrap_or(""))
        .bind(&skill.markdown)
        .bind(skill.skill_dir.to_string_lossy().to_string())
        .bind(trust_level(&file_inventory))
        .bind(&file_inventory_json)
        .bind(&metadata)
        .fetch_one(&mut **tx)
        .await?
    } else {
        sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO company_skills
                (company_id, key, slug, name, description, markdown, source_type,
                 source_locator, source_ref, trust_level, compatibility, file_inventory,
                 categories, sharing_scope, metadata, status, is_paperclip_managed,
                 version, tags, config)
            VALUES ($1, $2, $3, $4, $5, $6, 'local_path', $7, NULL, $8, 'compatible',
                    $9, '[]'::jsonb, 'company', $10, 'active', false, '1.0.0', '[]'::jsonb, '{}')
            RETURNING id
            "#,
        )
        .bind(company_id)
        .bind(key)
        .bind(slug)
        .bind(&skill.name)
        .bind(skill.description.as_deref().unwrap_or(""))
        .bind(&skill.markdown)
        .bind(skill.skill_dir.to_string_lossy().to_string())
        .bind(trust_level(&file_inventory))
        .bind(&file_inventory_json)
        .bind(&metadata)
        .fetch_one(&mut **tx)
        .await?
    };

    sqlx::query("DELETE FROM skill_files WHERE company_id = $1 AND skill_id = $2")
        .bind(company_id)
        .bind(skill_id)
        .execute(&mut **tx)
        .await?;
    for file in &skill.files {
        sqlx::query(
            r#"
            INSERT INTO skill_files (company_id, skill_id, path, content, mime_type, size_bytes)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (skill_id, path) DO UPDATE SET
                content = EXCLUDED.content,
                mime_type = EXCLUDED.mime_type,
                size_bytes = EXCLUDED.size_bytes,
                updated_at = NOW()
            "#,
        )
        .bind(company_id)
        .bind(skill_id)
        .bind(&file.path)
        .bind(&file.content)
        .bind(file.mime_type)
        .bind(file.content.len() as i64)
        .execute(&mut **tx)
        .await?;
    }

    sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
            'id', cs.id, 'companyId', cs.company_id, 'key', cs.key, 'slug', cs.slug,
            'name', cs.name, 'description', NULLIF(cs.description, ''), 'markdown', cs.markdown,
            'sourceType', cs.source_type, 'sourceLocator', cs.source_locator, 'sourceRef', cs.source_ref,
            'trustLevel', cs.trust_level, 'compatibility', cs.compatibility,
            'fileInventory', cs.file_inventory, 'iconUrl', NULL, 'color', NULL, 'tagline', NULL,
            'authorName', NULL, 'homepageUrl', NULL, 'categories', cs.categories,
            'sharingScope', cs.sharing_scope, 'publicShareToken', NULL,
            'forkedFromSkillId', NULL, 'forkedFromCompanyId', NULL, 'starCount', 0,
            'installCount', cs.install_count, 'forkCount', 0, 'currentVersionId', NULL,
            'metadata', cs.metadata, 'createdAt', cs.created_at, 'updatedAt', cs.updated_at
        )
        FROM company_skills cs
        WHERE cs.id = $1 AND cs.company_id = $2
        "#,
    )
    .bind(skill_id)
    .bind(company_id)
    .fetch_one(&mut **tx)
    .await
}

fn project_skill_candidate(
    skill: &DiscoveredProjectSkill,
    slug: &str,
    status: &str,
    existing_skill_id: Option<Uuid>,
    reason: Option<String>,
) -> Value {
    let mut candidate = json!({
        "slug": slug,
        "name": skill.name,
        "description": skill.description,
        "workspaceId": skill.workspace_id,
        "workspaceName": skill.workspace_name,
        "projectId": skill.project_id,
        "projectName": skill.project_name,
        "directoryRoot": skill.directory_root,
        "relativePath": skill.relative_path,
        "status": status,
    });
    if let Some(id) = existing_skill_id {
        candidate["existingSkillId"] = json!(id);
    }
    if let Some(reason) = reason {
        candidate["reason"] = json!(reason);
    }
    candidate
}

fn project_skill_metadata(skill: &DiscoveredProjectSkill) -> Value {
    json!({
        "sourceKind": "project_scan",
        "sourceLocator": skill.skill_dir,
        "projectId": skill.project_id,
        "projectName": skill.project_name,
        "workspaceId": skill.workspace_id,
        "workspaceName": skill.workspace_name,
        "workspaceCwd": skill.workspace_root,
        "relativePath": skill.relative_path,
        "directoryRoot": skill.directory_root,
    })
}

fn parse_skill_frontmatter(markdown: &str) -> (HashMap<String, String>, String) {
    let normalized = markdown.replace("\r\n", "\n");
    let mut fields = HashMap::new();
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return (fields, normalized);
    };
    let Some(end) = rest.find("\n---") else {
        return (fields, normalized);
    };
    let (frontmatter, body) = rest.split_at(end);
    for line in frontmatter.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        fields.insert(key.trim().to_string(), unquote_skill_value(value));
    }
    (
        fields,
        body.trim_start_matches('\n')
            .trim_start_matches("---")
            .trim_start_matches('\n')
            .to_string(),
    )
}

fn unquote_skill_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn normalize_project_skill_path(value: &str) -> Option<String> {
    let normalized = value.trim().replace('\\', "/");
    if normalized == "." {
        return Some(normalized);
    }
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.starts_with("//")
        || normalized.chars().nth(1) == Some(':')
    {
        return None;
    }
    let mut segments = Vec::new();
    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains('\0') {
            return None;
        }
        segments.push(segment);
    }
    let path = segments.join("/");
    if path.eq_ignore_ascii_case("skill.md") {
        Some(".".to_string())
    } else {
        Some(path)
    }
}

fn normalize_skill_slug(value: &str) -> String {
    let mut output = String::new();
    let mut previous_dash = false;
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash && !output.is_empty() {
            output.push('-');
            previous_dash = true;
        }
    }
    output.trim_end_matches('-').chars().take(255).collect()
}

async fn canonical_contained_path(root: &FsPath, candidate: &FsPath) -> Option<PathBuf> {
    let canonical_root = tokio::fs::canonicalize(root).await.ok()?;
    let canonical_candidate = tokio::fs::canonicalize(candidate).await.ok()?;
    canonical_candidate
        .starts_with(&canonical_root)
        .then_some(canonical_candidate)
}

async fn is_regular_file(path: &FsPath) -> bool {
    tokio::fs::symlink_metadata(path)
        .await
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
}

fn mime_type_for_path(path: &str) -> &'static str {
    match FsPath::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("md") => "text/markdown",
        Some("json") => "application/json",
        Some("yaml") | Some("yml") => "application/yaml",
        Some("toml") => "application/toml",
        Some("txt") => "text/plain",
        _ => "text/plain",
    }
}

fn inventory_kind(path: &str) -> &'static str {
    let normalized = path.to_ascii_lowercase();
    if normalized == "skill.md" {
        "skill"
    } else if normalized.starts_with("references/") {
        "reference"
    } else if normalized.starts_with("assets/") {
        "asset"
    } else if normalized.ends_with(".md") {
        "markdown"
    } else if normalized.ends_with(".sh")
        || normalized.ends_with(".py")
        || normalized.ends_with(".js")
        || normalized.ends_with(".ts")
    {
        "script"
    } else {
        "other"
    }
}

fn project_skill_source_hash(source_locator: &str) -> String {
    let digest = hex::encode(Sha256::digest(source_locator.as_bytes()));
    digest[..10].to_string()
}

async fn path_contains_symlink(root: &FsPath, candidate: &FsPath) -> bool {
    let Ok(relative) = candidate.strip_prefix(root) else {
        return true;
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let Ok(metadata) = tokio::fs::symlink_metadata(&current).await else {
            return false;
        };
        if metadata.file_type().is_symlink() {
            return true;
        }
    }
    false
}

fn trust_level(files: &[SkillFileInventoryEntry]) -> &'static str {
    if files.iter().any(|file| file.kind == "script") {
        "scripts_executables"
    } else if files
        .iter()
        .any(|file| file.kind == "other" || file.kind == "asset")
    {
        "assets"
    } else {
        "markdown_only"
    }
}

/// SK38: Delete skill
async fn delete_company_skill(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state
        .skill_registry_service
        .delete_skill(company_id, skill_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK39: List all skills for a company
async fn list_company_skills(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| AppError::Forbidden("Skills company access denied".to_string()))?;
    state
        .skill_registry_service
        .list_company_skills(company_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// Router setup for skills endpoints
pub fn skill_routes() -> Router<AppState> {
    axum::Router::new()
        .route(
            "/skills/available",
            axum::routing::get(list_available_skills),
        )
        .route("/skills/index", axum::routing::get(get_skill_index))
        .route("/skills/:skillName", axum::routing::get(get_skill_details))
        // --- P2: SK routes ---
        .route("/skills/catalog", get(get_skill_catalog))
        .route("/skills/catalog/:catalog_id", get(get_skill_catalog_detail))
        .route("/skills/catalog/files", get(get_skill_catalog_files))
        .route(
            "/companies/:company_id/skills",
            get(list_company_skills).post(create_company_skill),
        )
        .route(
            "/companies/:company_id/skills/categories",
            get(list_skill_categories),
        )
        .route(
            "/companies/:company_id/skills/:skill_id",
            get(get_company_skill).delete(delete_company_skill),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/fork-precheck",
            get(fork_skill_precheck),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/versions",
            get(list_skill_versions),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/versions/:version_id",
            get(get_skill_version),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/test-inputs",
            get(list_skill_test_inputs).post(create_skill_test_input),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/test-inputs/:input_id",
            patch(update_skill_test_input).delete(delete_skill_test_input),
        )
        .route(
            "/companies/:company_id/skill-test-run-templates",
            get(list_skill_test_run_templates).post(create_skill_test_run_template),
        )
        .route(
            "/companies/:company_id/skill-test-run-templates/:template_id",
            patch(update_skill_test_run_template).delete(delete_skill_test_run_template),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/test-runs",
            get(list_skill_test_runs),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/test-runs/:run_id",
            get(get_skill_test_run).delete(delete_skill_test_run),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/test-runs/:run_id/cancel",
            post(cancel_skill_test_run),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/star",
            post(star_company_skill).delete(unstar_company_skill),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/fork",
            post(fork_company_skill),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/audit",
            post(audit_company_skill),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/install-update",
            post(install_skill_update),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/reset",
            post(reset_company_skill),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/update-status",
            get(get_skill_update_status),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/comments",
            get(list_skill_comments).post(add_skill_comment),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/comments/:comment_id",
            patch(update_skill_comment).delete(delete_skill_comment),
        )
        .route(
            "/companies/:company_id/skills/:skill_id/files",
            get(list_skill_files)
                .patch(update_skill_files)
                .delete(delete_skill_files),
        )
        .route(
            "/companies/:company_id/skills/import",
            post(import_company_skill),
        )
        .route(
            "/companies/:company_id/skills/install-catalog",
            post(install_skill_catalog),
        )
        .route(
            "/companies/:company_id/skills/scan-projects",
            post(scan_skill_projects),
        )
}
