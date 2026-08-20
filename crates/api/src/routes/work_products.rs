//! Work Product routes —— 对应 Paperclip issue deliverable / work product 段落。
//!
//! - 所有读写 handler 走 actor / company access 检查（读用 read_only=true，写用 false，
//!   viewer 角色因 `role.is_read_only()` 在写操作上被拒）。
//! - ServiceError 直接透传给 AppError，保留 404/400/409 语义，不再统一塌缩为 500。
//! - 所有 mutation 写 activity log。

use crate::app_state::AppState;
use crate::errors::AppError;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, patch},
    Json, Router,
};
use serde_json::json;
use uuid::Uuid;

use models::issue_auxiliary::{CreateWorkProductInput, UpdateWorkProductInput, WorkProduct};
use services::auth::AuthorizationActor;

use crate::routes::{require_company_access, AccessMode};

/// Work Product 路由的访问语义表。handler 直接消费本表，测试也基于本表断言，
/// 避免「代码改了但权限测试还在测旧语义」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkProductOp {
    List,
    Create,
    Update,
    Delete,
}

impl WorkProductOp {
    pub(crate) const fn access(self) -> AccessMode {
        match self {
            Self::List => AccessMode::Read,
            Self::Create | Self::Update | Self::Delete => AccessMode::Write,
        }
    }
}

/// Helper: 通过 issue_id 查询 company_id
async fn get_company_id_for_issue(state: &AppState, issue_id: Uuid) -> Result<Uuid, AppError> {
    sqlx::query_scalar("SELECT company_id FROM issues WHERE id=$1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))
}

/// Helper: 通过 work product id 查询 (company_id, issue_id)
async fn get_scope_for_work_product(
    state: &AppState,
    work_product_id: Uuid,
) -> Result<(Uuid, Uuid), AppError> {
    sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT company_id, issue_id FROM issue_work_products WHERE id = $1",
    )
    .bind(work_product_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Work product not found".to_string()))
}

/// Paperclip's `resolveWorkProductCreatedByRunId` guard.  A work product may
/// only point at a real heartbeat run in the same company; an Agent may only
/// point at its authenticated run, and creation defaults to that run.
async fn resolve_created_by_run_id(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    requested: Option<Uuid>,
    assign_current_run: bool,
) -> Result<Option<Uuid>, AppError> {
    let (agent_id, actor_run_id) = match actor {
        AuthorizationActor::Agent {
            agent_id, run_id, ..
        } => (Some(*agent_id), *run_id),
        _ => (None, None),
    };
    if let (Some(requested), Some(actor_run_id)) = (requested, actor_run_id) {
        if requested != actor_run_id {
            return Err(AppError::Forbidden(
                "createdByRunId must match the authenticated agent run".into(),
            ));
        }
    }
    let selected = if assign_current_run {
        requested.or(actor_run_id)
    } else {
        requested
    };
    let Some(run_id) = selected else {
        return Ok(None);
    };
    let valid = match agent_id {
        Some(agent_id) => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT h.id FROM heartbeat_runs h
              JOIN agents a ON a.id = h.agent_id AND a.company_id = h.company_id
             WHERE h.id = $1 AND h.company_id = $2 AND h.agent_id = $3",
            )
            .bind(run_id)
            .bind(company_id)
            .bind(agent_id)
            .fetch_optional(&state.pool)
            .await?
        }
        None => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM heartbeat_runs WHERE id = $1 AND company_id = $2",
            )
            .bind(run_id)
            .bind(company_id)
            .fetch_optional(&state.pool)
            .await?
        }
    };
    if valid.is_none() {
        return Err(AppError::Forbidden(
            "createdByRunId is not valid for this work product actor".into(),
        ));
    }
    Ok(Some(run_id))
}

fn forbidden(_: StatusCode) -> AppError {
    AppError::Forbidden("No access to this company".to_string())
}

/// GET /issues/:id/work-products - List work products
async fn list_work_products(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<WorkProduct>>, AppError> {
    let company_id = get_company_id_for_issue(&state, id).await?;
    require_company_access(&actor, company_id, WorkProductOp::List.access()).map_err(forbidden)?;

    Ok(Json(
        state
            .work_product_service
            .list_work_products(id, company_id)
            .await?,
    ))
}

/// POST /issues/:id/work-products - Create work product
async fn create_work_product(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(mut input): Json<CreateWorkProductInput>,
) -> Result<(StatusCode, Json<WorkProduct>), AppError> {
    let company_id = get_company_id_for_issue(&state, id).await?;
    require_company_access(&actor, company_id, WorkProductOp::Create.access())
        .map_err(forbidden)?;
    input.created_by_run_id =
        resolve_created_by_run_id(&state, &actor, company_id, input.created_by_run_id, true)
            .await?;

    let work_product = state
        .work_product_service
        .create_work_product(id, company_id, input)
        .await?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "issue.work_product_created",
        &actor,
        "issue",
        id,
        json!({ "workProductId": work_product.id }),
    )
    .await;

    Ok((StatusCode::CREATED, Json(work_product)))
}

/// PATCH /work-products/:id - Update work product
async fn update_work_product(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(mut input): Json<UpdateWorkProductInput>,
) -> Result<Json<WorkProduct>, AppError> {
    let (company_id, issue_id) = get_scope_for_work_product(&state, id).await?;
    require_company_access(&actor, company_id, WorkProductOp::Update.access())
        .map_err(forbidden)?;
    input.created_by_run_id =
        resolve_created_by_run_id(&state, &actor, company_id, input.created_by_run_id, false)
            .await?;

    let work_product = state
        .work_product_service
        .update_work_product(id, company_id, input)
        .await?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "issue.work_product_updated",
        &actor,
        "issue",
        issue_id,
        json!({ "workProductId": id }),
    )
    .await;

    Ok(Json(work_product))
}

/// DELETE /work-products/:id - Delete work product
async fn delete_work_product(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let (company_id, issue_id) = get_scope_for_work_product(&state, id).await?;
    require_company_access(&actor, company_id, WorkProductOp::Delete.access())
        .map_err(forbidden)?;

    state
        .work_product_service
        .delete_work_product(id, company_id)
        .await?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "issue.work_product_deleted",
        &actor,
        "issue",
        issue_id,
        json!({ "workProductId": id }),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Create work product routes
pub fn work_product_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/issues/:id/work-products",
            get(list_work_products).post(create_work_product),
        )
        .route(
            "/work-products/:id",
            patch(update_work_product).delete(delete_work_product),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::access_test_support::{agent_of, anonymous, board_with_role};
    use services::auth::MembershipRole;

    const ALL_OPS: [WorkProductOp; 4] = [
        WorkProductOp::List,
        WorkProductOp::Create,
        WorkProductOp::Update,
        WorkProductOp::Delete,
    ];

    #[test]
    fn only_list_is_read_only() {
        assert_eq!(WorkProductOp::List.access(), AccessMode::Read);
        for op in [
            WorkProductOp::Create,
            WorkProductOp::Update,
            WorkProductOp::Delete,
        ] {
            assert_eq!(op.access(), AccessMode::Write, "{op:?} must be a write op");
        }
    }

    #[test]
    fn viewer_can_only_list_work_products() {
        let company = Uuid::new_v4();
        let viewer = board_with_role(company, MembershipRole::Viewer);
        for op in ALL_OPS {
            let allowed = require_company_access(&viewer, company, op.access()).is_ok();
            assert_eq!(
                allowed,
                op == WorkProductOp::List,
                "viewer access mismatch for {op:?}"
            );
        }
    }

    #[test]
    fn operator_owner_and_agent_can_mutate_work_products() {
        let company = Uuid::new_v4();
        for actor in [
            board_with_role(company, MembershipRole::Operator),
            board_with_role(company, MembershipRole::Owner),
            agent_of(company),
        ] {
            for op in ALL_OPS {
                assert!(
                    require_company_access(&actor, company, op.access()).is_ok(),
                    "{op:?} should be allowed"
                );
            }
        }
    }

    #[test]
    fn cross_company_and_anonymous_are_rejected() {
        let company = Uuid::new_v4();
        let other = Uuid::new_v4();
        for actor in [
            board_with_role(other, MembershipRole::Owner),
            agent_of(other),
            anonymous(),
        ] {
            for op in ALL_OPS {
                assert_eq!(
                    require_company_access(&actor, company, op.access()),
                    Err(StatusCode::FORBIDDEN),
                    "{op:?} must be forbidden"
                );
            }
        }
    }
}
