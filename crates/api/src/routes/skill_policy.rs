//! Company Skill Policy routes.
//!
//! 对齐 paperclip `company-skill-policy.ts`：GET/DELETE 当前 policy，
//! POST 设置（带版本与校验），POST /simulate 预览评估结果。
use crate::{app_state::AppState, errors::AppError};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

pub fn skill_policy_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:company_id/skill-policy",
            get(get_skill_policy).delete(delete_skill_policy),
        )
        .route("/companies/:company_id/skill-policy", post(set_skill_policy))
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

async fn get_skill_policy(
    State(s): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
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
    Path(company_id): Path<Uuid>,
    Json(policy): Json<Value>,
) -> Result<Json<Value>, AppError> {
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
    Path(company_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    s.skill_policy_service
        .delete_policy(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn simulate_skill_policy(
    State(s): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(req): Json<SimulateRequest>,
) -> Result<Json<Value>, AppError> {
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
