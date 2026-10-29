//! HTTP surface for task-driven team orchestration.
//!
//! These routes expose the persisted orchestration instance for a root issue:
//! the read model for the task-detail UI, the CEO decision contract, the board
//! team-plan decision, deliverable submission and review, the CEO final report
//! and cancellation. All authorization decisions that depend on database state
//! (who the CEO is, who the task-scope superior is, whether an agent may submit)
//! are made by the service; this module only maps the HTTP actor into that call
//! and maps errors to status codes.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json, Router,
    routing::{get, post, put},
};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::ApiError;
use services::auth::AuthorizationActor;
use services::task_orchestration_service::{OrchestrationError, TaskOrchestrationService};

fn orchestration_service(state: &AppState) -> TaskOrchestrationService {
    TaskOrchestrationService::new(
        state.pool.clone(),
        state.heartbeat_service.clone(),
        state.agent_service.clone(),
        state.approval_service.clone(),
        state.issue_service.clone(),
    )
}

/// Publish a live event so the task-detail UI can refetch without polling.
async fn publish_orchestration_update(
    company_id: Uuid,
    issue_id: Uuid,
    row: &services::task_orchestration_service::OrchRow,
    action: &str,
) {
    services::live_events::publish_live_event(
        company_id,
        "task_orchestration.updated",
        json!({
            "issueId": issue_id,
            "orchestrationId": row.id,
            "stage": row.stage,
            "stageVersion": row.stage_version,
            "action": action,
        }),
    )
    .await;
}

fn map_error(error: OrchestrationError) -> ApiError {
    match error {
        OrchestrationError::NotOrchestrated { .. }
        | OrchestrationError::SubtaskNotFound(_)
        | OrchestrationError::InteractionNotFound(_)
        | OrchestrationError::PlanNotFound(_) => ApiError::NotFound(error.to_string()),
        OrchestrationError::CrossCompanyAccess
        | OrchestrationError::AgentNotInCompany(_)
        | OrchestrationError::AgentNotExecutable(_)
        | OrchestrationError::SelfReviewBlocked
        | OrchestrationError::NotDirectReviewer
        | OrchestrationError::NotSubtaskOwner => ApiError::Forbidden(error.to_string()),
        OrchestrationError::Conflict(_)
        | OrchestrationError::SubtaskAlreadyPlanned(_)
        | OrchestrationError::TerminalState => ApiError::Conflict(error.to_string()),
        OrchestrationError::InvalidTransition { .. }
        | OrchestrationError::UnansweredQuestionsRemain { .. }
        | OrchestrationError::RequiredSubtasksNotAccepted { .. }
        | OrchestrationError::DependenciesNotSatisfied { .. }
        | OrchestrationError::ReportingCycleDetected
        | OrchestrationError::DependencyCycleDetected
        | OrchestrationError::ContractInvalid { .. } => ApiError::Unprocessable(error.to_string()),
        OrchestrationError::Storage(_) | OrchestrationError::Service(_) => {
            tracing::error!(error = %error, "task orchestration failed");
            ApiError::InternalServerError(error.to_string())
        }
    }
}

fn assert_company(actor: &AuthorizationActor, company_id: Uuid, read_only: bool) -> Result<(), ApiError> {
    crate::routes::assert_company_access(actor, company_id, read_only)
        .map_err(|_| ApiError::Forbidden("Issue is outside the actor's company scope".into()))
}

fn agent_id(actor: &AuthorizationActor, company_id: Uuid) -> Result<Uuid, ApiError> {
    match actor {
        AuthorizationActor::Agent {
            agent_id,
            company_id: actor_company_id,
            ..
        } if *actor_company_id == company_id => Ok(*agent_id),
        AuthorizationActor::Agent { .. } => Err(ApiError::Forbidden(
            "Issue is outside the actor's company scope".into(),
        )),
        AuthorizationActor::Board { .. } => Err(ApiError::Forbidden(
            "This operation must be performed by the responsible agent".into(),
        )),
        AuthorizationActor::None => Err(ApiError::Unauthorized("Authentication required".into())),
    }
}

/// GET /companies/:companyId/issues/:issueId/orchestration
async fn get_orchestration(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    assert_company(&actor, company_id, true)?;
    let service = orchestration_service(&state);
    let view = service
        .view(company_id, issue_id)
        .await
        .map_err(map_error)?
        .ok_or_else(|| ApiError::NotFound("This issue is not under team orchestration".into()))?;
    Ok(Json(view))
}

/// POST /companies/:companyId/issues/:issueId/orchestration/decision
///
/// Body is the CEO decision contract. A `{ "contract": … }` envelope is
/// accepted as well so callers can attach extra metadata without changing the
/// contract shape.
async fn submit_decision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, ApiError> {
    assert_company(&actor, company_id, false)?;
    let author = agent_id(&actor, company_id)?;
    let contract = body.get("contract").cloned().unwrap_or(body);
    let service = orchestration_service(&state);
    let outcome = service
        .submit_decision(company_id, issue_id, author, &contract)
        .await
        .map_err(map_error)?;
    publish_orchestration_update(company_id, issue_id, &outcome.orchestration, "decision").await;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "planId": outcome.plan_id,
            "revision": outcome.revision,
            "approvalRequired": outcome.approval_required,
            "approvalIds": outcome.approval_ids,
            "subtaskIssueIds": outcome.subtask_issue_ids,
            "questionInteractionIds": outcome.question_interaction_ids,
            "stage": outcome.orchestration.stage,
            "stageVersion": outcome.orchestration.stage_version,
        })),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamPlanDecisionBody {
    approved: bool,
    #[serde(default)]
    note: Option<String>,
}

/// POST /companies/:companyId/issues/:issueId/orchestration/team-plan
async fn decide_team_plan(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<TeamPlanDecisionBody>,
) -> Result<impl IntoResponse, ApiError> {
    assert_company(&actor, company_id, false)?;
    crate::routes::assert_board(&actor)
        .map_err(|_| ApiError::Forbidden("Only board users can decide a team plan".into()))?;
    let service = orchestration_service(&state);
    let row = service
        .decide_team_plan(company_id, issue_id, body.approved, body.note.as_deref())
        .await
        .map_err(map_error)?;
    publish_orchestration_update(company_id, issue_id, &row, "team_plan").await;
    Ok(Json(json!({
        "stage": row.stage,
        "stageVersion": row.stage_version,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamAdjustmentBody {
    members: Vec<services::task_orchestration_service::TeamAdjustment>,
}

fn deserialize_budget_cents<'de, D>(deserializer: D) -> Result<Option<Option<i64>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<i64>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskBudgetBody {
    #[serde(default, deserialize_with = "deserialize_budget_cents")]
    budget_cents: Option<Option<i64>>,
}

/// PUT /companies/:companyId/issues/:issueId/orchestration/budget
///
/// Board may set, change, or remove the root task's lifetime cap. Replaying
/// skipped wakes is part of the service mutation, after the cap is committed.
async fn update_task_budget(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<TaskBudgetBody>,
) -> Result<impl IntoResponse, ApiError> {
    assert_company(&actor, company_id, false)?;
    crate::routes::assert_board(&actor)
        .map_err(|_| ApiError::Forbidden("Only board users can change a task budget".into()))?;
    let Some(budget_cents) = body.budget_cents else {
        return Err(ApiError::Unprocessable(
            "budgetCents must be supplied as a non-negative integer or null".into(),
        ));
    };
    let actor_user_id = actor
        .principal_id()
        .ok_or_else(|| ApiError::Forbidden("Board user identity is required".into()))?;
    let service = orchestration_service(&state);
    let result = service
        .update_task_budget(company_id, issue_id, actor_user_id, budget_cents)
        .await
        .map_err(map_error)?;
    Ok(Json(result))
}

/// POST /companies/:companyId/issues/:issueId/orchestration/team-members
///
/// Task-scope membership change after the plan was approved. `exit` ends task
/// membership only; the shared agent is untouched.
async fn adjust_team(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<TeamAdjustmentBody>,
) -> Result<impl IntoResponse, ApiError> {
    assert_company(&actor, company_id, false)?;
    let author = agent_id(&actor, company_id)?;
    let service = orchestration_service(&state);
    let plan = service
        .adjust_team(company_id, issue_id, author, body.members)
        .await
        .map_err(map_error)?;
    if let Ok(Some(row)) = service.find(company_id, issue_id).await {
        publish_orchestration_update(company_id, issue_id, &row, "team_members").await;
    }
    Ok(Json(plan))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubmitDeliverableBody {
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    evidence: Option<Value>,
}

/// POST /companies/:companyId/issues/:issueId/orchestration/subtasks/:subtaskIssueId/submit
async fn submit_deliverable(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id, subtask_issue_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(body): Json<SubmitDeliverableBody>,
) -> Result<impl IntoResponse, ApiError> {
    assert_company(&actor, company_id, false)?;
    let author = agent_id(&actor, company_id)?;
    let service = orchestration_service(&state);
    let (deliverable_id, version, row) = service
        .submit_deliverable(
            company_id,
            issue_id,
            subtask_issue_id,
            author,
            body.body.as_deref(),
            body.evidence.unwrap_or_else(|| json!({})),
        )
        .await
        .map_err(map_error)?;
    publish_orchestration_update(company_id, issue_id, &row, "submit_deliverable").await;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "deliverableId": deliverable_id,
            "version": version,
            "stage": row.stage,
            "stageVersion": row.stage_version,
        })),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewBody {
    accepted: bool,
    #[serde(default)]
    feedback: Option<String>,
}

/// POST /companies/:companyId/issues/:issueId/orchestration/subtasks/:subtaskIssueId/review
async fn review_subtask(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id, subtask_issue_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(body): Json<ReviewBody>,
) -> Result<impl IntoResponse, ApiError> {
    assert_company(&actor, company_id, false)?;
    let reviewer = agent_id(&actor, company_id)?;
    let service = orchestration_service(&state);
    let row = service
        .review_subtask(
            company_id,
            issue_id,
            subtask_issue_id,
            reviewer,
            body.accepted,
            body.feedback.as_deref(),
        )
        .await
        .map_err(map_error)?;
    publish_orchestration_update(company_id, issue_id, &row, "review").await;
    Ok(Json(json!({
        "stage": row.stage,
        "stageVersion": row.stage_version,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FinalReportBody {
    body: String,
    #[serde(default)]
    evidence: Option<Value>,
}

/// POST /companies/:companyId/issues/:issueId/orchestration/final-report
async fn record_final_report(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<FinalReportBody>,
) -> Result<impl IntoResponse, ApiError> {
    assert_company(&actor, company_id, false)?;
    let author = agent_id(&actor, company_id)?;
    let service = orchestration_service(&state);
    let (deliverable_id, row) = service
        .record_final_report(
            company_id,
            issue_id,
            author,
            &body.body,
            body.evidence.unwrap_or_else(|| json!({})),
        )
        .await
        .map_err(map_error)?;
    publish_orchestration_update(company_id, issue_id, &row, "final_report").await;
    Ok(Json(json!({
        "deliverableId": deliverable_id,
        "stage": row.stage,
        "stageVersion": row.stage_version,
        "completedAt": row.completed_at,
    })))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LifecycleBody {
    #[serde(default)]
    reason: Option<String>,
}

/// POST /companies/:companyId/issues/:issueId/orchestration/cancel
async fn cancel_orchestration(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<LifecycleBody>,
) -> Result<impl IntoResponse, ApiError> {
    assert_company(&actor, company_id, false)?;
    crate::routes::assert_board(&actor)
        .map_err(|_| ApiError::Forbidden("Only board users can cancel a task".into()))?;
    let service = orchestration_service(&state);
    let row = service
        .cancel(company_id, issue_id)
        .await
        .map_err(map_error)?;
    publish_orchestration_update(company_id, issue_id, &row, "cancel").await;
    if let Some(reason) = body.reason.as_deref() {
        let _ = sqlx::query(
            "UPDATE task_orchestrations SET error_reason = $3, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(row.id)
        .bind(company_id)
        .bind(format!("cancelled: {reason}"))
        .execute(&state.pool)
        .await;
    }
    Ok(Json(json!({
        "stage": row.stage,
        "stageVersion": row.stage_version,
    })))
}

/// POST /companies/:companyId/issues/:issueId/orchestration/retry
async fn retry_orchestration(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    assert_company(&actor, company_id, false)?;
    crate::routes::assert_board(&actor)
        .map_err(|_| ApiError::Forbidden("Only board users can retry a failed task".into()))?;
    let service = orchestration_service(&state);
    let row = service
        .retry(company_id, issue_id)
        .await
        .map_err(map_error)?;
    publish_orchestration_update(company_id, issue_id, &row, "retry").await;
    Ok(Json(json!({
        "stage": row.stage,
        "stageVersion": row.stage_version,
    })))
}

/// POST /companies/:companyId/issues/:issueId/orchestration/resume
async fn resume_orchestration(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    assert_company(&actor, company_id, false)?;
    crate::routes::assert_board(&actor)
        .map_err(|_| ApiError::Forbidden("Only board users can resume a task".into()))?;
    let service = orchestration_service(&state);
    let row = service
        .resume(company_id, issue_id)
        .await
        .map_err(map_error)?;
    publish_orchestration_update(company_id, issue_id, &row, "resume").await;
    Ok(Json(json!({
        "stage": row.stage,
        "stageVersion": row.stage_version,
    })))
}

pub fn task_orchestration_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:companyId/issues/:issueId/orchestration",
            get(get_orchestration),
        )
        .route(
            "/companies/:companyId/issues/:issueId/orchestration/budget",
            put(update_task_budget),
        )
        .route(
            "/companies/:companyId/issues/:issueId/orchestration/decision",
            post(submit_decision),
        )
        .route(
            "/companies/:companyId/issues/:issueId/orchestration/team-plan",
            post(decide_team_plan),
        )
        .route(
            "/companies/:companyId/issues/:issueId/orchestration/team-members",
            post(adjust_team),
        )
        .route(
            "/companies/:companyId/issues/:issueId/orchestration/subtasks/:subtaskIssueId/submit",
            post(submit_deliverable),
        )
        .route(
            "/companies/:companyId/issues/:issueId/orchestration/subtasks/:subtaskIssueId/review",
            post(review_subtask),
        )
        .route(
            "/companies/:companyId/issues/:issueId/orchestration/final-report",
            post(record_final_report),
        )
        .route(
            "/companies/:companyId/issues/:issueId/orchestration/cancel",
            post(cancel_orchestration),
        )
        .route(
            "/companies/:companyId/issues/:issueId/orchestration/retry",
            post(retry_orchestration),
        )
        .route(
            "/companies/:companyId/issues/:issueId/orchestration/resume",
            post(resume_orchestration),
        )
}

/// Record the orchestration event for a freshly created clarification
/// interaction. Returns silently when the issue is not orchestrated, so the
/// generic interaction endpoint keeps working for ordinary issues.
pub async fn note_interaction_created(
    service: &TaskOrchestrationService,
    company_id: Uuid,
    issue_id: Uuid,
    interaction_kind: &str,
    interaction_id: Uuid,
) {
    if interaction_kind != "ask_user_questions" {
        return;
    }
    if let Err(error) = service
        .record_clarification_request(company_id, issue_id, interaction_id)
        .await
    {
        match error {
            OrchestrationError::NotOrchestrated { .. } => {}
            other => tracing::warn!(
                %issue_id,
                %interaction_id,
                error = %other,
                "failed to record orchestration clarification request"
            ),
        }
    } else if let Ok(Some(row)) = service.find(company_id, issue_id).await {
        publish_orchestration_update(company_id, issue_id, &row, "clarification_request").await;
    }
}

/// Record the orchestration event for an answered clarification interaction.
pub async fn note_interaction_answered(
    service: &TaskOrchestrationService,
    company_id: Uuid,
    issue_id: Uuid,
    interaction_kind: &str,
    interaction_id: Uuid,
) {
    if interaction_kind != "ask_user_questions" {
        return;
    }
    if let Err(error) = service
        .record_clarification_answer(company_id, issue_id, interaction_id)
        .await
    {
        match error {
            OrchestrationError::NotOrchestrated { .. } => {}
            other => tracing::warn!(
                %issue_id,
                %interaction_id,
                error = %other,
                "failed to record orchestration clarification answer"
            ),
        }
    } else if let Ok(Some(row)) = service.find(company_id, issue_id).await {
        publish_orchestration_update(company_id, issue_id, &row, "clarification_answer").await;
    }
}
