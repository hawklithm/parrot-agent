use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json, Router,
    routing::{get, post},
};
use serde_json::json;
use uuid::Uuid;

use crate::{app_state::AppState, errors::ApiError};
use crate::routes::log_activity;
use models;
use services::auth::AuthorizationActor;
use services::{CrossIssueInfluenceKind, CrossIssueInfluenceLimitService,
    DefaultCrossIssueInfluenceLimitService, HeartbeatWakeupOptions, InfluenceLimitError,
    ObserveCrossIssueInfluenceInput};

fn map_interaction_service_error(error: String) -> ApiError {
    let lower = error.to_ascii_lowercase();
    if lower.contains("already resolved")
        || lower.contains("not pending")
        || lower.contains("idempotency key conflicts")
    {
        return ApiError::Conflict(error);
    }
    if lower.contains("interaction not found") || lower.contains("not found") {
        return ApiError::NotFound(error);
    }
    if lower.contains("resolver")
        || lower.contains("authorized human")
        || lower.contains("human-only")
        || lower.contains("not resolvable")
        || lower.contains("other than its creator")
    {
        return ApiError::Forbidden(error);
    }
    if lower.contains("only ")
        || lower.contains("unsupported")
        || lower.contains("unknown item")
        || lower.contains("unknown option")
        || lower.contains("unknown question")
        || lower.contains("duplicate item")
        || lower.contains("duplicate answer")
        || lower.contains("duplicate selected")
        || lower.contains("invalid item")
        || lower.contains("missing an items")
        || lower.contains("missing a questions")
        || lower.contains("requires an answer")
        || lower.contains("invokable agent")
        || lower.contains("only allows one answer")
        || lower.contains("options must be selected")
        || lower.contains("options may be selected")
        || lower.contains("reason is required")
    {
        return ApiError::Unprocessable(error);
    }
    ApiError::InternalServerError(error)
}

async fn assert_agent_run(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    agent_id: Uuid,
) -> Result<Option<Uuid>, ApiError> {
    let AuthorizationActor::Agent { run_id, company_id: actor_company_id, .. } = actor else {
        return Ok(None);
    };
    if *actor_company_id != company_id {
        return Err(ApiError::Forbidden("Issue is outside the actor's company scope".into()));
    }
    let Some(run_id) = run_id else {
        return Err(ApiError::Unprocessable(
            "A valid authenticated agent run is required for issue-thread interactions".into(),
        ));
    };
    let exists: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM heartbeat_runs WHERE id = $1 AND company_id = $2 AND agent_id = $3",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
    exists.ok_or_else(|| {
        ApiError::Unprocessable("The authenticated agent run is not valid for this company and agent".into())
    }).map(Some)
}

fn actor_label(actor: &AuthorizationActor) -> &'static str {
    match actor {
        AuthorizationActor::Board { .. } => "user",
        AuthorizationActor::Agent { .. } => "agent",
        AuthorizationActor::None => "system",
    }
}

async fn guard_cross_issue_resolution(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    issue_id: Uuid,
) -> Result<(), ApiError> {
    let AuthorizationActor::Agent {
        agent_id,
        run_id: Some(run_id),
        company_id: actor_company_id,
        ..
    } = actor
    else {
        return Ok(());
    };
    if *actor_company_id != company_id {
        return Err(ApiError::Forbidden("Missing company scope".into()));
    }
    let guard = DefaultCrossIssueInfluenceLimitService::new().with_pool(state.pool.clone());
    match guard
        .observe_influence(ObserveCrossIssueInfluenceInput {
            heartbeat_run_id: *run_id,
            company_id,
            agent_id: *agent_id,
            source_issue_id: issue_id,
            target_issue_id: issue_id,
            influence_kind: CrossIssueInfluenceKind::InteractionResolution,
            actor_label: None,
            assignee_label: None,
            issue_identifier: None,
        })
        .await
    {
        Ok(_) => Ok(()),
        Err(InfluenceLimitError::LimitExceeded { current, cap }) => Err(
            ApiError::TooManyRequests(format!(
                "Cross-issue influence limit exceeded: {current}/{cap}"
            )),
        ),
        Err(InfluenceLimitError::RunNotFound(_)
        | InfluenceLimitError::RunContextRequired) => Err(ApiError::Forbidden(
            "Run context required for cross-issue interaction resolution".into(),
        )),
        Err(InfluenceLimitError::DatabaseError(error)) => {
            Err(ApiError::InternalServerError(error))
        }
    }
}

/// POST /issues/:issue_id/interactions - Create a thread interaction
pub async fn create_interaction(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
    Json(mut input): Json<models::CreateThreadInteractionInput>,
) -> Result<impl IntoResponse, ApiError> {
    // Get issue's company_id and assert write access
    let company_id: Option<Uuid> = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
    
    let Some(company_id) = company_id else {
        return Err(ApiError::NotFound(format!("Issue not found: {}", issue_id)));
    };
    
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| ApiError::Forbidden("Issue is outside the actor's company scope".into()))?;

    if let AuthorizationActor::Agent { agent_id, run_id, .. } = &actor {
        let authenticated_run = assert_agent_run(&state, &actor, company_id, *agent_id).await?;
        input.source_run_id = authenticated_run.or(*run_id);
    }

    // Load issue directly from DB
    let issue: models::Issue = sqlx::query_as("SELECT * FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Issue not found".to_string()))?;

    // Determine creator
    let creator = match &actor {
        AuthorizationActor::Board { user_id, .. } => {
            services::InteractionCreator {
                agent_id: None,
                user_id: Some(*user_id),
            }
        },
        AuthorizationActor::Agent { agent_id, .. } => {
            services::InteractionCreator {
                agent_id: Some(*agent_id),
                user_id: None,
            }
        },
        AuthorizationActor::None => {
            return Err(ApiError::Unauthorized("Authentication required".to_string()));
        },
    };

    // Create interaction
    let service = services::issue_thread_interaction_service::IssueThreadInteractionService::new(state.pool.clone());
    let interaction = service.create(&issue, input, creator).await
        .map_err(map_interaction_service_error)?;

    log_activity(
        &state.pool,
        company_id,
        "issue.thread_interaction_created",
        &actor,
        "issue",
        issue_id,
        json!({
            "interactionId": interaction.id,
            "interactionKind": interaction.kind,
            "interactionStatus": interaction.status,
            "requestedResolverPolicy": interaction.requested_resolver_policy,
            "effectiveResolverPolicy": interaction.effective_resolver_policy,
            "resolverPolicyProvenance": interaction.resolver_policy_provenance,
            "effectiveResolverPolicySource": interaction.effective_resolver_policy_source,
        }),
    ).await;

    if let Some(addressee_agent_id) = interaction.addressee_agent_id {
        let _ = state.heartbeat_service.wakeup_with_options(
            addressee_agent_id,
            issue_id,
            company_id,
            HeartbeatWakeupOptions {
                source: Some("automation".into()),
                trigger_detail: Some("system".into()),
                reason: Some("interaction_pending".into()),
                requested_by_actor_type: Some(actor_label(&actor).into()),
                requested_by_actor_id: actor.principal_id(),
                idempotency_key: Some(format!("interaction-pending:{}", interaction.id)),
                payload: Some(json!({
                    "issueId": issue_id,
                    "interactionId": interaction.id,
                    "interactionKind": interaction.kind,
                    "mutation": "interaction",
                })),
                context_snapshot: Some(json!({
                    "issueId": issue_id,
                    "taskId": issue_id,
                    "interactionId": interaction.id,
                    "interactionKind": interaction.kind,
                    "wakeReason": "interaction_pending",
                    "source": "issue.interaction.created",
                })),
                ..Default::default()
            },
        ).await;
    }

    Ok((StatusCode::CREATED, Json(interaction)))
}

/// GET /issues/:issue_id/interactions - List interactions for an issue
pub async fn list_interactions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // Get issue's company_id and assert access
    let company_id: Option<Uuid> = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
    
    let Some(company_id) = company_id else {
        return Err(ApiError::NotFound(format!("Issue not found: {}", issue_id)));
    };
    
    crate::routes::assert_company_access(&actor, company_id, true)
        .map_err(|_| ApiError::Forbidden("Issue is outside the actor's company scope".into()))?;

    // List interactions
    let service = services::issue_thread_interaction_service::IssueThreadInteractionService::new(state.pool.clone());
    let interactions = service.list_for_issue(issue_id).await
        .map_err(map_interaction_service_error)?;

    Ok(Json(interactions))
}

/// POST /issues/:issue_id/interactions/:interaction_id/accept - Accept an interaction
pub async fn accept_interaction(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, interaction_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<models::AcceptThreadInteractionInput>,
) -> Result<impl IntoResponse, ApiError> {
    // Get issue's company_id and assert access
    let company_id: Option<Uuid> = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
    
    let Some(company_id) = company_id else {
        return Err(ApiError::NotFound(format!("Issue not found: {}", issue_id)));
    };
    
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| ApiError::Forbidden("Issue is outside the actor's company scope".into()))?;

    guard_cross_issue_resolution(&state, &actor, company_id, issue_id).await?;

    // Load issue directly from DB
    let issue: models::Issue = sqlx::query_as("SELECT * FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Issue not found".to_string()))?;

    // Determine resolver
    let resolver = match &actor {
        AuthorizationActor::Board { user_id, .. } => {
            services::InteractionResolver {
                resolver_type: "user".to_string(),
                resolver_id: user_id.to_string(),
                run_id: None,
            }
        },
        AuthorizationActor::Agent { agent_id, run_id, .. } => {
            services::InteractionResolver {
                resolver_type: "agent".to_string(),
                resolver_id: agent_id.to_string(),
                run_id: *run_id,
            }
        },
        AuthorizationActor::None => {
            return Err(ApiError::Unauthorized("Authentication required".to_string()));
        },
    };

    // Accept interaction
    let service = services::issue_thread_interaction_service::IssueThreadInteractionService::new(state.pool.clone());
    let result = service.accept_interaction(&issue, interaction_id, input, resolver).await
        .map_err(map_interaction_service_error)?;

    Ok(Json(result))
}

/// POST /issues/:issue_id/interactions/:interaction_id/reject - Reject an interaction
pub async fn reject_interaction(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, interaction_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<models::RejectThreadInteractionInput>,
) -> Result<impl IntoResponse, ApiError> {
    // Get issue's company_id and assert access
    let company_id: Option<Uuid> = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
    
    let Some(company_id) = company_id else {
        return Err(ApiError::NotFound(format!("Issue not found: {}", issue_id)));
    };
    
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| ApiError::Forbidden("Issue is outside the actor's company scope".into()))?;

    guard_cross_issue_resolution(&state, &actor, company_id, issue_id).await?;

    // Load issue directly from DB
    let issue: models::Issue = sqlx::query_as("SELECT * FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Issue not found".to_string()))?;

    let resolver = match &actor {
        AuthorizationActor::Board { user_id, .. } => {
            services::InteractionResolver {
                resolver_type: "user".to_string(),
                resolver_id: user_id.to_string(),
                run_id: None,
            }
        },
        AuthorizationActor::Agent { agent_id, run_id, .. } => {
            services::InteractionResolver {
                resolver_type: "agent".to_string(),
                resolver_id: agent_id.to_string(),
                run_id: *run_id,
            }
        },
        AuthorizationActor::None => {
            return Err(ApiError::Unauthorized("Authentication required".to_string()));
        },
    };

    // Reject interaction
    let service = services::issue_thread_interaction_service::IssueThreadInteractionService::new(state.pool.clone());
    let interaction = service.reject_interaction(&issue, interaction_id, input, resolver).await
        .map_err(map_interaction_service_error)?;

    Ok(Json(interaction))
}

/// GET /issues/:issue_id/interactions/:interaction_id - Get a single interaction
pub async fn get_interaction(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, interaction_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    // Get issue's company_id and assert access
    let company_id: Option<Uuid> = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
    
    let Some(company_id) = company_id else {
        return Err(ApiError::NotFound(format!("Issue not found: {}", issue_id)));
    };
    
    crate::routes::assert_company_access(&actor, company_id, true)
        .map_err(|_| ApiError::Forbidden("Issue is outside the actor's company scope".into()))?;

    // Get interaction
    let service = services::issue_thread_interaction_service::IssueThreadInteractionService::new(state.pool.clone());
    let interaction = service.get_by_id(interaction_id).await
        .map_err(map_interaction_service_error)?
        .ok_or_else(|| ApiError::NotFound("Interaction not found".to_string()))?;

    // Verify interaction belongs to the issue
    if interaction.issue_id != issue_id {
        return Err(ApiError::NotFound("Interaction not found".to_string()));
    }

    Ok(Json(interaction))
}

/// POST /issues/:issue_id/interactions/:interaction_id/answer - Answer ask_user_questions interaction
pub async fn answer_questions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, interaction_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<models::AnswerQuestionsInput>,
) -> Result<impl IntoResponse, ApiError> {
    // Get issue's company_id and assert access
    let company_id: Option<Uuid> = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
    
    let Some(company_id) = company_id else {
        return Err(ApiError::NotFound(format!("Issue not found: {}", issue_id)));
    };
    
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| ApiError::Forbidden("Issue is outside the actor's company scope".into()))?;

    guard_cross_issue_resolution(&state, &actor, company_id, issue_id).await?;
    // Determine resolver
    let resolver = match &actor {
        AuthorizationActor::Board { user_id, .. } => {
            services::InteractionResolver {
                resolver_type: "user".to_string(),
                resolver_id: user_id.to_string(),
                run_id: None,
            }
        },
        AuthorizationActor::Agent { agent_id, run_id, .. } => {
            services::InteractionResolver {
                resolver_type: "agent".to_string(),
                resolver_id: agent_id.to_string(),
                run_id: *run_id,
            }
        },
        AuthorizationActor::None => {
            return Err(ApiError::Unauthorized("Authentication required".to_string()));
        },
    };

    // Answer questions
    let service = services::issue_thread_interaction_service::IssueThreadInteractionService::new(state.pool.clone());
    let interaction = service.answer_questions(issue_id, interaction_id, input, resolver).await
        .map_err(map_interaction_service_error)?;

    Ok(Json(interaction))
}

/// POST /issues/:issue_id/interactions/:interaction_id/cancel - Cancel ask_user_questions interaction
pub async fn cancel_questions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, interaction_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<models::CancelQuestionsInput>,
) -> Result<impl IntoResponse, ApiError> {
    // Get issue's company_id and assert access
    let company_id: Option<Uuid> = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
    
    let Some(company_id) = company_id else {
        return Err(ApiError::NotFound(format!("Issue not found: {}", issue_id)));
    };
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| ApiError::Forbidden("Issue is outside the actor's company scope".into()))?;

    crate::routes::assert_board(&actor)
        .map_err(|_| ApiError::Forbidden("Only Board users can cancel question interactions".into()))?;

    // Determine resolver
    let resolver = match &actor {
        AuthorizationActor::Board { user_id, .. } => {
            services::InteractionResolver {
                resolver_type: "user".to_string(),
                resolver_id: user_id.to_string(),
                run_id: None,
            }
        },
        AuthorizationActor::Agent { agent_id, run_id, .. } => {
            services::InteractionResolver {
                resolver_type: "agent".to_string(),
                resolver_id: agent_id.to_string(),
                run_id: *run_id,
            }
        },
        AuthorizationActor::None => {
            return Err(ApiError::Unauthorized("Authentication required".to_string()));
        },
    };

    // Cancel questions
    let service = services::issue_thread_interaction_service::IssueThreadInteractionService::new(state.pool.clone());
    let interaction = service.cancel_questions(issue_id, interaction_id, input, resolver).await
        .map_err(map_interaction_service_error)?;

    Ok(Json(interaction))
}

/// POST /issues/:issue_id/interactions/:interaction_id/withdraw - Withdraw a thread interaction
pub async fn withdraw_interaction(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, interaction_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<models::WithdrawInteractionInput>,
) -> Result<impl IntoResponse, ApiError> {
    // Get issue's company_id and assert access
    let company_id: Option<Uuid> = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let Some(company_id) = company_id else {
        return Err(ApiError::NotFound(format!("Issue not found: {}", issue_id)));
    };

    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| ApiError::Forbidden("Issue is outside the actor's company scope".into()))?;

    guard_cross_issue_resolution(&state, &actor, company_id, issue_id).await?;

    // Determine resolver
    let resolver = match &actor {
        AuthorizationActor::Board { user_id, .. } => {
            services::InteractionResolver {
                resolver_type: "user".to_string(),
                resolver_id: user_id.to_string(),
                run_id: None,
            }
        },
        AuthorizationActor::Agent { agent_id, run_id, .. } => {
            services::InteractionResolver {
                resolver_type: "agent".to_string(),
                resolver_id: agent_id.to_string(),
                run_id: *run_id,
            }
        },
        AuthorizationActor::None => {
            return Err(ApiError::Unauthorized("Authentication required".to_string()));
        },
    };

    let service = services::issue_thread_interaction_service::IssueThreadInteractionService::new(state.pool.clone());
    let interaction = service.withdraw_interaction(issue_id, interaction_id, input, resolver).await
        .map_err(map_interaction_service_error)?;

    // Log activity
    log_activity(
        &state.pool,
        company_id,
        "issue.thread_interaction_withdrawn",
        &actor,
        "issue_thread_interaction",
        interaction_id,
        serde_json::json!({
            "issueId": issue_id,
            "interactionId": interaction_id,
            "kind": interaction.kind,
            "source": "issue.interaction.withdraw",
        }),
    )
    .await;

    Ok(Json(interaction))
}

/// POST /issues/:issue_id/interactions/:interaction_id/verdicts - Submit item verdicts
pub async fn submit_item_verdicts(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, interaction_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<models::SubmitItemVerdictsInput>,
) -> Result<impl IntoResponse, ApiError> {
    // Get issue's company_id and assert access
    let company_id: Option<Uuid> = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let Some(company_id) = company_id else {
        return Err(ApiError::NotFound(format!("Issue not found: {}", issue_id)));
    };

    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| ApiError::Forbidden("Issue is outside the actor's company scope".into()))?;

    guard_cross_issue_resolution(&state, &actor, company_id, issue_id).await?;

    // Determine resolver
    let resolver = match &actor {
        AuthorizationActor::Board { user_id, .. } => {
            services::InteractionResolver {
                resolver_type: "user".to_string(),
                resolver_id: user_id.to_string(),
                run_id: None,
            }
        },
        AuthorizationActor::Agent { agent_id, run_id, .. } => {
            services::InteractionResolver {
                resolver_type: "agent".to_string(),
                resolver_id: agent_id.to_string(),
                run_id: *run_id,
            }
        },
        AuthorizationActor::None => {
            return Err(ApiError::Unauthorized("Authentication required".to_string()));
        },
    };

    let service = services::issue_thread_interaction_service::IssueThreadInteractionService::new(state.pool.clone());
    let verdict_count = input.verdicts.len();
    let interaction = service.submit_item_verdicts(issue_id, interaction_id, input, resolver).await
        .map_err(map_interaction_service_error)?;

    // Log activity
    log_activity(
        &state.pool,
        company_id,
        "issue.thread_interaction_item_verdicts_submitted",
        &actor,
        "issue_thread_interaction",
        interaction_id,
        serde_json::json!({
            "issueId": issue_id,
            "interactionId": interaction_id,
            "verdictCount": verdict_count,
            "source": "issue.interaction.verdicts",
        }),
    )
    .await;

    Ok(Json(interaction))
}

pub fn interaction_routes() -> Router<AppState> {
    Router::new()
        .route("/issues/:id/interactions", post(create_interaction))
        .route("/issues/:id/interactions", get(list_interactions))
        .route("/issues/:id/interactions/:interaction_id", get(get_interaction))
        .route("/issues/:id/interactions/:interaction_id/accept", post(accept_interaction))
        .route("/issues/:id/interactions/:interaction_id/reject", post(reject_interaction))
        .route("/issues/:id/interactions/:interaction_id/respond", post(answer_questions))
        .route("/issues/:id/interactions/:interaction_id/answer", post(answer_questions))
        .route("/issues/:id/interactions/:interaction_id/cancel", post(cancel_questions))
        .route("/issues/:id/interactions/:interaction_id/withdraw", post(withdraw_interaction))
        .route("/issues/:id/interactions/:interaction_id/verdicts", post(submit_item_verdicts))
}
