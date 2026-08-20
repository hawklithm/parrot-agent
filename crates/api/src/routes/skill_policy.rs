//! Company Skill Policy routes.
//!
//! 对齐 paperclip `company-skill-policy.ts`：GET/DELETE 当前 policy，
//! POST 设置（带版本与校验），POST /simulate 预览评估结果。
use crate::{app_state::AppState, errors::AppError};
use axum::{
    extract::{Extension, Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use services::auth::{AuthorizationAction, AuthorizationActor};
use uuid::Uuid;

pub fn skill_policy_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:company_id/skill-policy",
            get(get_skill_policy).delete(delete_skill_policy),
        )
        .route(
            "/companies/:company_id/skill-policy",
            post(set_skill_policy).put(set_skill_policy),
        )
        .route(
            "/companies/:company_id/skill-policy/simulate",
            post(simulate_skill_policy),
        )
}

#[derive(Deserialize)]
struct SimulateRequest {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    skill: Option<String>,
}

async fn assert_skill_policy_access(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    write: bool,
) -> Result<(), AppError> {
    if actor.is_anonymous() {
        return Err(AppError::Unauthorized("Authentication required".into()));
    }
    let action = if write {
        AuthorizationAction::CompanyUpdate { company_id }
    } else {
        AuthorizationAction::CompanyRead { company_id }
    };
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        actor,
        &action,
        Some(company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(if write {
            "Insufficient permissions: company skill policy administration required"
        } else {
            "Insufficient permissions: company membership required"
        }
        .into()));
    }
    Ok(())
}

async fn get_skill_policy(
    State(s): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    assert_skill_policy_access(&s, &actor, company_id, false).await?;
    let policy = s
        .skill_policy_service
        .get_policy(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(policy.unwrap_or_else(|| {
        // 无 policy → 默认开放
        serde_json::json!({ "companyId": company_id, "policy": null, "version": 0 })
    })))
}

async fn set_skill_policy(
    State(s): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(mut policy): Json<Value>,
) -> Result<Json<Value>, AppError> {
    assert_skill_policy_access(&s, &actor, company_id, true).await?;
    if let Some(expected) = policy.get("expectedRevision").and_then(|v| v.as_i64()) {
        if let Some(current) = s
            .skill_policy_service
            .get_policy(company_id)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?
        {
            let current_version = current.get("version").and_then(|v| v.as_i64()).unwrap_or(0);
            if current_version != expected {
                return Err(AppError::Conflict(format!(
                    "Skill policy revision conflict: expected {}, current {}",
                    expected, current_version
                )));
            }
        }
        if let Some(object) = policy.as_object_mut() {
            object.remove("expectedRevision");
        }
    }
    if let Some(inner) = policy.get("policy").cloned() {
        policy = inner;
    }
    let result = s
        .skill_policy_service
        .set_policy(company_id, policy)
        .await
        .map_err(|e| match e {
            services::skill_policy_service::SkillPolicyError::InvalidPolicy(msg) => {
                AppError::BadRequest(msg)
            }
            other => AppError::InternalServerError(other.to_string()),
        })?;
    Ok(Json(result))
}

async fn delete_skill_policy(
    State(s): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    assert_skill_policy_access(&s, &actor, company_id, true).await?;
    s.skill_policy_service
        .delete_policy(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn simulate_skill_policy(
    State(s): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(req): Json<SimulateRequest>,
) -> Result<Json<Value>, AppError> {
    assert_skill_policy_access(&s, &actor, company_id, false).await?;
    let role = req.role.unwrap_or_else(|| "member".to_string());
    let action = req.action.unwrap_or_else(|| "execute".to_string());
    let source = req.source.unwrap_or_else(|| "user".to_string());
    let skill = req.skill.unwrap_or_else(|| "custom".to_string());

    let result = s
        .skill_policy_service
        .simulate(company_id, None, &role, &action, &source, &skill)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(result))
}
