/// GET /issues/:issue_id/relations
async fn get_issue_relations(
    State(state): State<AppState>,
    Path(issue_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let relation_service = services::issue_relation_service::IssueRelationService::new(state.pool.clone());
    
    let relations = relation_service
        .get_relation_summaries(issue_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    Ok(Json(serde_json::json!({
        "blockedBy": relations.blocked_by,
        "blocks": relations.blocks,
    })))
}

/// POST /issues/:issue_id/relations/blocked-by
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateBlockedByInput {
    blocked_by_issue_ids: Vec<Uuid>,
}

async fn update_blocked_by_relations(
    State(state): State<AppState>,
    Path(issue_id): Path<Uuid>,
    Json(input): Json<UpdateBlockedByInput>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Get issue to retrieve company_id
    let issue: Issue = sqlx::query_as(
        "SELECT * FROM issues WHERE id = $1"
    )
    .bind(issue_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| (StatusCode::NOT_FOUND, format!("Issue not found: {}", e)))?;
    
    let relation_service = services::issue_relation_service::IssueRelationService::new(state.pool.clone());
    
    relation_service
        .update_blocked_by_relations(
            issue.company_id,
            issue_id,
            input.blocked_by_issue_ids,
            None, // TODO: Get from auth context
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    Ok(StatusCode::OK)
}
