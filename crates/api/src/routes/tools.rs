//! Tool access read endpoints.
//!
//! The tool-access persistence/service layer has not been migrated yet, but
//! Paperclip's UI expects these company-scoped read contracts to exist. Return
//! the same empty, typed envelopes until tool connections, profiles and
//! policies are backed by their repositories.

use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use serde_json::Value;
use uuid::Uuid;

use crate::app_state::AppState;

async fn list_connections(
    Path(_company_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
            'id', id, 'companyId', company_id, 'applicationId', application_id,\
            'name', name, 'uid', uid, 'connectionKind', connection_kind,\
            'ownership', ownership, 'transport', transport, 'authKind', auth_kind,\
            'status', status, 'transportConfig', transport_config,\
            'credentialSecretRefs', credential_secret_refs, 'enabled', enabled,\
            'createdByAgentId', created_by_agent_id, 'createdByUserId', created_by_user_id,\
            'createdAt', created_at, 'updatedAt', updated_at) ORDER BY name), '[]'::jsonb)\
         FROM tool_connections WHERE company_id = $1",
    ).bind(_company_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    (StatusCode::OK, Json(serde_json::json!({ "connections": rows })))
}

async fn list_policies(
    Path(_company_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
            'id', id, 'companyId', company_id, 'name', name, 'description', description,\
            'policyType', policy_type, 'priority', priority, 'enabled', enabled,\
            'selectors', selectors, 'conditions', conditions, 'config', config,\
            'createdByAgentId', created_by_agent_id, 'createdByUserId', created_by_user_id,\
            'createdAt', created_at, 'updatedAt', updated_at) ORDER BY priority, name), '[]'::jsonb)\
         FROM tool_policies WHERE company_id = $1",
    ).bind(_company_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    (StatusCode::OK, Json(serde_json::json!({ "policies": rows })))
}

async fn effective_profiles_for_agent(
    Path((_company_id, agent_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let profiles = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(to_jsonb(p) || jsonb_build_object('profileKey', p.profile_key) ORDER BY p.name), '[]'::jsonb)\
         FROM tool_profiles p JOIN tool_profile_bindings b ON b.profile_id = p.id\
         WHERE p.company_id = $1 AND b.target_type = 'agent' AND b.target_id = $2",
    ).bind(_company_id).bind(agent_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    let bindings = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(to_jsonb(b) ORDER BY b.created_at), '[]'::jsonb)\
         FROM tool_profile_bindings b WHERE b.company_id = $1 AND b.target_type = 'agent' AND b.target_id = $2",
    ).bind(_company_id).bind(agent_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    let entries = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
            'id', e.id, 'profileId', e.profile_id, 'selectorType', e.selector_type,\
            'selectorValue', e.selector_value, 'effect', e.effect, 'connectionId', e.connection_id,\
            'toolName', e.tool_name, 'createdAt', e.created_at, 'updatedAt', e.updated_at)\
            ORDER BY e.created_at), '[]'::jsonb)\
         FROM tool_profile_entries e WHERE e.profile_id IN (\
            SELECT b.profile_id FROM tool_profile_bindings b\
            WHERE b.company_id = $1 AND b.target_type = 'agent' AND b.target_id = $2)",
    ).bind(_company_id).bind(agent_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    let allowed_names = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(DISTINCT e.tool_name) FILTER (WHERE e.effect = 'allow' AND e.tool_name IS NOT NULL), '[]'::jsonb)\
         FROM tool_profile_entries e WHERE e.profile_id IN (\
            SELECT b.profile_id FROM tool_profile_bindings b\
            WHERE b.company_id = $1 AND b.target_type = 'agent' AND b.target_id = $2)",
    ).bind(_company_id).bind(agent_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    let installed_connections = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(DISTINCT jsonb_build_object(\
            'id', c.id, 'companyId', c.company_id, 'applicationId', c.application_id, 'name', c.name,\
            'uid', c.uid, 'connectionKind', c.connection_kind, 'ownership', c.ownership,\
            'transport', c.transport, 'authKind', c.auth_kind, 'status', c.status,\
            'transportConfig', c.transport_config, 'credentialSecretRefs', c.credential_secret_refs,\
            'enabled', c.enabled, 'createdAt', c.created_at, 'updatedAt', c.updated_at)), '[]'::jsonb)\
         FROM tool_connections c JOIN tool_profile_entries e ON e.connection_id = c.id\
         WHERE e.profile_id IN (SELECT b.profile_id FROM tool_profile_bindings b\
            WHERE b.company_id = $1 AND b.target_type = 'agent' AND b.target_id = $2)",
    ).bind(_company_id).bind(agent_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    (StatusCode::OK, Json(serde_json::json!({"agentId": agent_id, "profiles": profiles, "entries": entries, "bindings": bindings, "allowedTools": [], "allowedToolNames": allowed_names, "installedConnections": installed_connections})))
}

pub fn tool_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/tools/connections", get(list_connections))
        .route("/companies/:company_id/tools/policies", get(list_policies))
        .route(
            "/companies/:company_id/tools/profiles/effective/agents/:agent_id",
            get(effective_profiles_for_agent),
        )
}
