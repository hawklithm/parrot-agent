//! `POST /companies/:companyId/onboarding-seed`.
//!
//! Contract source: Paperclip `server/src/routes/onboarding-seed.ts` (65 lines).
//! The route is `validate(applyOnboardingSeedSchema)` → `assertCompanyAccess` →
//! `onboardingSeedService(db).apply(companyId, req.body, getActorInfo(req))`, so
//! a body-shape failure is a `400` even for a caller who would also be refused,
//! because the validator is middleware that runs before the handler.
//!
//! The seed itself is customer free text and rides the JSON body only — it is
//! never read from the trusted `x-paperclip-cloud-*` envelope. Cloud treats any
//! 2xx as "the tenant holds this content" and writes the acknowledged revision
//! only afterwards, so this route answers 200 only once every part of the seed —
//! goal, agent, first task, seed record *and* audit entry — is durable. A
//! logging failure rolls the whole seed back and surfaces as a 5xx, which is the
//! recoverable outcome: Cloud's retry then finds no stored revision and
//! re-applies.

use axum::{
    extract::{rejection::JsonRejection, Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::json;
use services::auth::AuthorizationActor;
use services::onboarding_seed_service::{self, OnboardingSeedAuditActor, OnboardingSeedInput};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use crate::routes::{require_company_access, AccessMode};

/// `packages/shared/src/validators/onboarding-seed.ts` bounds. Zod measures
/// string length in UTF-16 code units, so these are counted the same way rather
/// than in `char`s.
const REVISION_MAX_LENGTH: usize = 128;
const MISSION_MAX_LENGTH: usize = 2000;
const AGENT_NAME_MAX_LENGTH: usize = 80;
const AGENT_ROLE_MAX_LENGTH: usize = 120;
const FIRST_TASK_TITLE_MAX_LENGTH: usize = 200;
const FIRST_TASK_DETAILS_MAX_LENGTH: usize = 2000;

/// Paperclip's Zod failure body (`server/src/middleware/error-handler.ts:129`).
fn validation_error(issues: serde_json::Value) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "Validation error", "details": issues })),
    )
        .into_response()
}

/// One Zod-shaped issue: `{code, message, path}`.
fn issue(code: &str, message: String, path: &[&str]) -> serde_json::Value {
    json!({ "code": code, "message": message, "path": path })
}

/// Zod's `.max(n)` / `.min(n)` bounds, counted in UTF-16 code units.
fn check_max(
    issues: &mut Vec<serde_json::Value>,
    value: &str,
    max: usize,
    path: &[&str],
) {
    let length = value.encode_utf16().count();
    if length > max {
        issues.push(issue(
            "too_big",
            format!("String must contain at most {max} character(s)"),
            path,
        ));
    }
}

fn check_min(
    issues: &mut Vec<serde_json::Value>,
    value: &str,
    min: usize,
    path: &[&str],
) {
    let length = value.encode_utf16().count();
    if length < min {
        issues.push(issue(
            "too_small",
            format!("String must contain at least {min} character(s)"),
            path,
        ));
    }
}

/// Reimplements `applyOnboardingSeedSchema.safeParse` for the body serde already
/// accepted, so bound violations report the same `400` shape as a shape failure.
fn validate_seed(seed: &OnboardingSeedInput) -> Result<(), Vec<serde_json::Value>> {
    let mut issues = Vec::new();

    check_min(&mut issues, &seed.revision, 1, &["revision"]);
    check_max(&mut issues, &seed.revision, REVISION_MAX_LENGTH, &["revision"]);

    if let Some(mission) = &seed.mission {
        check_max(&mut issues, mission, MISSION_MAX_LENGTH, &["mission"]);
    }

    if let Some(agent) = &seed.agent {
        check_min(&mut issues, &agent.name, 1, &["agent", "name"]);
        check_max(&mut issues, &agent.name, AGENT_NAME_MAX_LENGTH, &["agent", "name"]);
        if let Some(role) = &agent.role {
            check_max(&mut issues, role, AGENT_ROLE_MAX_LENGTH, &["agent", "role"]);
        }
    }

    if let Some(task) = &seed.first_task {
        check_min(&mut issues, &task.title, 1, &["firstTask", "title"]);
        check_max(
            &mut issues,
            &task.title,
            FIRST_TASK_TITLE_MAX_LENGTH,
            &["firstTask", "title"],
        );
        if let Some(details) = &task.details {
            check_max(
                &mut issues,
                details,
                FIRST_TASK_DETAILS_MAX_LENGTH,
                &["firstTask", "details"],
            );
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

/// The actor fields the audit entry needs, as Paperclip's `getActorInfo`
/// produces them.
fn audit_actor(actor: &AuthorizationActor) -> OnboardingSeedAuditActor {
    let (agent_id, run_id) = match actor {
        AuthorizationActor::Agent {
            agent_id, run_id, ..
        } => (Some(*agent_id), *run_id),
        _ => (None, None),
    };

    OnboardingSeedAuditActor {
        actor_type: actor.actor_type(),
        actor_id: actor.principal_id().unwrap_or_else(Uuid::nil),
        agent_id,
        run_id,
    }
}

/// `POST /companies/:companyId/onboarding-seed`.
pub async fn apply_onboarding_seed(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    payload: Result<Json<OnboardingSeedInput>, JsonRejection>,
) -> Response {
    // `validate(...)` runs as middleware, i.e. before `assertCompanyAccess`.
    let Json(seed) = match payload {
        Ok(seed) => seed,
        Err(rejection) => {
            return validation_error(json!([
                issue("invalid_type", rejection.body_text(), &[]),
            ]))
        }
    };
    if let Err(issues) = validate_seed(&seed) {
        return validation_error(json!(issues));
    }

    // `assertCompanyAccess(req, companyId)` — an unknown company and a caller
    // without access are both refused, so the response cannot be used to
    // enumerate company ids.
    if let Err(status) = require_company_access(&actor, company_id, AccessMode::Write) {
        return status.into_response();
    }

    let audit = audit_actor(&actor);

    match onboarding_seed_service::apply(&state.pool, company_id, &seed, Some(&audit)).await {
        Ok(applied) => (
            StatusCode::OK,
            Json(json!({
                "companyId": company_id,
                "revision": applied.revision,
                "applied": true,
                "changed": applied.changed,
                "goalId": applied.goal_id,
                "agentId": applied.agent_id,
                "issueId": applied.issue_id,
            })),
        )
            .into_response(),
        // Service errors keep their own HTTP meaning — a database failure here
        // surfaces as a 5xx, which is what makes the audit-failure rollback
        // observable to Cloud's retry.
        Err(error) => AppError::from(error).into_response(),
    }
}

pub fn onboarding_seed_routes() -> Router<AppState> {
    Router::new().route(
        "/companies/:companyId/onboarding-seed",
        post(apply_onboarding_seed),
    )
}
