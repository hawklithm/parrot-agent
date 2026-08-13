//! P1.4 Teams Catalog routes.
//!
//! 对齐 Paperclip `server/src/routes/teams-catalog.ts`：
//! - GET  /teams/catalog                                     列表（kind/category/q）
//! - GET  /teams/catalog/:catalog_id/files?path=             读取 catalog 文件
//! - GET  /teams/catalog/:catalog_id                         详情
//! - GET  /companies/:company_id/teams/catalog/installed     公司已安装
//! - POST /companies/:company_id/teams/catalog/:catalog_id/preview
//! - POST /companies/:company_id/teams/catalog/:catalog_id/install
//!
//! 安装权限复用 Paperclip 的 `agents:create` 语义：board 用户走 decide_access，
//! agent actor 需同公司且具备 agent:create 授权。
use crate::{app_state::AppState, errors::AppError};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use services::auth::{decide_access, AuthorizationAction, AuthorizationActor};
use services::teams_catalog_service::{InstallActor, TeamsCatalogError};
use uuid::Uuid;

pub fn teams_catalog_routes() -> Router<AppState> {
    Router::new()
        .route("/teams/catalog", get(list_catalog_teams))
        .route("/teams/catalog/:catalog_id/files", get(read_catalog_file))
        .route("/teams/catalog/:catalog_id", get(get_catalog_team))
        .route(
            "/companies/:company_id/teams/catalog/installed",
            get(list_installed_teams),
        )
        .route(
            "/companies/:company_id/teams/catalog/:catalog_id/preview",
            post(preview_catalog_team),
        )
        .route(
            "/companies/:company_id/teams/catalog/:catalog_id/install",
            post(install_catalog_team),
        )
}

#[derive(Debug, Deserialize)]
pub struct CatalogListQuery {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogFileQuery {
    #[serde(default)]
    pub path: Option<String>,
    /// 与 Paperclip 一致：允许用 ?ref= 覆盖 path 参数中的 catalog 引用
    #[serde(default)]
    pub r#ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogRefQuery {
    #[serde(default)]
    pub r#ref: Option<String>,
}

fn map_err(e: TeamsCatalogError) -> AppError {
    match e {
        TeamsCatalogError::NotFound(m) => AppError::NotFound(m),
        TeamsCatalogError::InvalidInput(m) => AppError::Validation(m),
        TeamsCatalogError::InvalidCatalog(m) => AppError::Validation(m),
        TeamsCatalogError::Conflict(m) => AppError::Conflict(m),
        TeamsCatalogError::Io(m) => AppError::InternalServerError(m),
        TeamsCatalogError::Database(e) => AppError::from(e),
    }
}

fn require_authenticated(actor: &AuthorizationActor) -> Result<(), AppError> {
    if actor.is_anonymous() {
        return Err(AppError::Unauthorized("authentication required".into()));
    }
    Ok(())
}

/// 安装权限：board → agent:create 决策；agent → 同公司 + agent:create 授权。
async fn assert_can_install(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> Result<(), AppError> {
    require_authenticated(actor)?;
    super::assert_company_access(actor, company_id, false)
        .map_err(|_| AppError::Forbidden("company access denied".into()))?;

    if actor.is_instance_admin() {
        return Ok(());
    }

    // Agent actor 必须属于本公司
    if actor.is_agent() && actor.company_id() != Some(company_id) {
        return Err(AppError::Forbidden(
            "Agent key cannot access another company".into(),
        ));
    }

    let action = AuthorizationAction::AgentCreate { company_id };
    if decide_access(&state.pool, actor, &action, Some(company_id)).await {
        return Ok(());
    }
    Err(AppError::Forbidden("Missing permission: agents:create".into()))
}

async fn list_catalog_teams(
    State(s): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Query(q): Query<CatalogListQuery>,
) -> Result<Json<Value>, AppError> {
    require_authenticated(&actor)?;
    s.teams_catalog_service
        .list_teams(q.kind.as_deref(), q.category.as_deref(), q.q.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

async fn get_catalog_team(
    State(s): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(catalog_id): Path<String>,
    Query(q): Query<CatalogRefQuery>,
) -> Result<Json<Value>, AppError> {
    require_authenticated(&actor)?;
    let catalog_ref = q.r#ref.unwrap_or(catalog_id);
    s.teams_catalog_service
        .get_team(&catalog_ref)
        .await
        .map(Json)
        .map_err(map_err)
}

async fn read_catalog_file(
    State(s): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(catalog_id): Path<String>,
    Query(q): Query<CatalogFileQuery>,
) -> Result<Json<Value>, AppError> {
    require_authenticated(&actor)?;
    let catalog_ref = q.r#ref.unwrap_or(catalog_id);
    let rel = q.path.unwrap_or_else(|| "TEAM.md".to_string());
    s.teams_catalog_service
        .read_team_file(&catalog_ref, &rel)
        .await
        .map(Json)
        .map_err(map_err)
}

async fn list_installed_teams(
    State(s): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_authenticated(&actor)?;
    super::assert_company_access(&actor, company_id, true)
        .map_err(|_| AppError::Forbidden("company access denied".into()))?;
    s.teams_catalog_service
        .list_installed(company_id)
        .await
        .map(Json)
        .map_err(map_err)
}

async fn preview_catalog_team(
    State(s): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, catalog_id)): Path<(Uuid, String)>,
    Query(q): Query<CatalogRefQuery>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, AppError> {
    require_authenticated(&actor)?;
    super::assert_company_access(&actor, company_id, true)
        .map_err(|_| AppError::Forbidden("company access denied".into()))?;
    let catalog_ref = q.r#ref.unwrap_or(catalog_id);
    let options = body.map(|Json(v)| v).unwrap_or(Value::Null);
    s.teams_catalog_service
        .preview_install(company_id, &catalog_ref, &options)
        .await
        .map(Json)
        .map_err(map_err)
}

async fn install_catalog_team(
    State(s): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, catalog_id)): Path<(Uuid, String)>,
    Query(q): Query<CatalogRefQuery>,
    body: Option<Json<Value>>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    assert_can_install(&s, &actor, company_id).await?;

    let catalog_ref = q.r#ref.unwrap_or(catalog_id);
    let options = body.map(|Json(v)| v).unwrap_or(Value::Null);
    let install_actor = InstallActor {
        actor_type: actor.actor_type().to_string(),
        user_id: if actor.is_board() {
            actor.principal_id()
        } else {
            None
        },
        agent_id: if actor.is_agent() {
            actor.principal_id()
        } else {
            None
        },
    };

    let result = s
        .teams_catalog_service
        .install(company_id, &catalog_ref, &options, install_actor)
        .await
        .map_err(map_err)?;

    let already = result
        .get("alreadyInstalled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let status = if already {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(result)))
}
