//! Company-secret & secret-provider routes — SE5, SE14-SE20.
//!
//! 对应 FEATURE_GAP_TASKS.md §4.5 Secrets/Providers (SE5, SE14-SE20) 及
//! API_GAP_TASKS.md §16 密钥/提供方。
//!
//! 路由路径与 Paperclip `server/src/routes/secrets.ts` 对齐：
//!   GET    /companies/:companyId/secret-providers            (SE5)
//!   GET    /companies/:companyId/secrets                     (list)
//!   POST   /companies/:companyId/secrets                     (SE14)
//!   GET    /secrets/:id                                      (SE15)
//!   PATCH  /secrets/:id                                      (SE16)
//!   DELETE /secrets/:id                                      (SE17)
//!   POST   /secrets/:id/rotate                               (SE18)
//!   GET    /secrets/:id/usage                                (SE19)
//!   GET    /secrets/:id/access-events                        (SE20)
//!
//! 响应 shape 与 Paperclip `CompanySecret` / `SecretProviderDescriptor` 对齐
//! (camelCase)。本模块直接查 `company_secrets` 表，避免改动既有
//! `models::CompanySecret`（其字段不全且被别处引用）。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::app_state::AppState;

pub fn secret_routes() -> Router<AppState> {
    Router::new()
        // SE5: provider registry (static descriptor list, like Paperclip listSecretProviders)
        .route(
            "/companies/:company_id/secret-providers",
            get(list_secret_providers),
        )
        // Company-scoped secret list + create
        .route(
            "/companies/:company_id/secrets",
            get(list_company_secrets).post(create_company_secret),
        )
        // Secret-scoped CRUD + actions
        .route(
            "/secrets/:id",
            get(get_secret).patch(update_secret).delete(delete_secret),
        )
        .route("/secrets/:id/rotate", post(rotate_secret))
        .route("/secrets/:id/usage", get(get_secret_usage))
        .route("/secrets/:id/access-events", get(get_secret_access_events))
}

/// The canonical provider descriptor list — mirrors Paperclip
/// `listSecretProviders()` (provider-registry.ts). `local_encrypted` is always
/// available; the cloud providers are advertised but marked `configured: false`
/// unless a matching `company_secret_provider_configs` row exists.
const KNOWN_PROVIDERS: &[(&str, &str)] = &[
    ("local_encrypted", "Local encrypted store"),
    ("aws_secrets_manager", "AWS Secrets Manager"),
    ("gcp_secret_manager", "Google Cloud Secret Manager"),
    ("vault", "HashiCorp Vault"),
];

/// SE5: GET /companies/:company_id/secret-providers
async fn list_secret_providers(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<Value>>, SecretError> {
    let pool = &state.pool;
    // Determine which providers have a configured config row for this company.
    let configured: Vec<String> = sqlx::query_scalar(
        r#"SELECT DISTINCT provider::text FROM company_secret_provider_configs
            WHERE company_id = $1 AND status = 'active'"#,
    )
    .bind(company_id)
    .fetch_all(pool)
    .await
    .map_err(|e| SecretError::Database(e.to_string()))?;

    let descriptors: Vec<Value> = KNOWN_PROVIDERS
        .iter()
        .map(|(id, label)| {
            let is_cloud = *id != "local_encrypted";
            json!({
                "id": id,
                "label": label,
                "requiresExternalRef": is_cloud,
                "supportsManagedValues": !is_cloud,
                "supportsExternalReferences": is_cloud,
                "configured": *id == "local_encrypted" || configured.iter().any(|c| c == id),
            })
        })
        .collect();

    Ok(Json(descriptors))
}

const SECRET_SELECT: &str = r#"SELECT id, company_id, scope, owner_user_id, user_secret_definition_id,
       key, name, provider, status, managed_mode, external_ref, provider_config_id,
       provider_metadata, latest_version, description, last_resolved_at, last_rotated_at,
       deleted_at, created_by_agent_id, created_by_user_id, created_at, updated_at
  FROM company_secrets"#;

/// Map `managed_mode` DB value to the Paperclip wire value.
/// DB default is `paperclip_managed`; `external` -> `external_reference`.
fn managed_mode_to_wire(v: &str) -> &str {
    match v {
        "external" => "external_reference",
        other => other,
    }
}

/// Serialize a `company_secrets` row to the Paperclip `CompanySecret` shape.
fn secret_to_json(r: &sqlx::postgres::PgRow) -> Value {
    let provider: Option<String> = r.try_get("provider").unwrap_or(None);
    let managed_mode: String = r.try_get::<String, _>("managed_mode").unwrap_or_default();
    json!({
        "id": r.get::<Uuid, _>("id"),
        "companyId": r.get::<Uuid, _>("company_id"),
        "scope": r.get::<String, _>("scope"),
        "ownerUserId": r.get::<Option<String>, _>("owner_user_id"),
        "userSecretDefinitionId": r.get::<Option<Uuid>, _>("user_secret_definition_id"),
        "key": r.get::<String, _>("key"),
        "name": r.get::<String, _>("name"),
        "provider": provider.unwrap_or_else(|| "local_encrypted".to_string()),
        "status": r.get::<String, _>("status"),
        "managedMode": managed_mode_to_wire(&managed_mode),
        "externalRef": r.get::<Option<String>, _>("external_ref"),
        "providerConfigId": r.get::<Option<Uuid>, _>("provider_config_id"),
        "providerMetadata": r.get::<Option<Value>, _>("provider_metadata"),
        "latestVersion": r.get::<i32, _>("latest_version"),
        "description": r.get::<Option<String>, _>("description"),
        "lastResolvedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_resolved_at"),
        "lastRotatedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_rotated_at"),
        "deletedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("deleted_at"),
        "createdByAgentId": r.get::<Option<Uuid>, _>("created_by_agent_id"),
        "createdByUserId": r.get::<Option<String>, _>("created_by_user_id"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

/// GET /companies/:company_id/secrets
async fn list_company_secrets(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<Value>>, SecretError> {
    let pool = &state.pool;
    let rows = sqlx::query(&format!(
        "{} WHERE company_id = $1 AND scope = 'company' AND deleted_at IS NULL ORDER BY created_at DESC",
        SECRET_SELECT
    ))
    .bind(company_id)
    .fetch_all(pool)
    .await
    .map_err(|e| SecretError::Database(e.to_string()))?;

    Ok(Json(rows.iter().map(secret_to_json).collect()))
}

/// POST /companies/:companyId/secrets body — mirrors `createSecretSchema`.
#[derive(Debug, Deserialize)]
struct CreateSecretBody {
    name: String,
    key: Option<String>,
    provider: Option<String>,
    provider_config_id: Option<Uuid>,
    managed_mode: Option<String>,
    value: Option<String>,
    description: Option<String>,
    external_ref: Option<String>,
    provider_metadata: Option<Value>,
    #[allow(dead_code)]
    provider_version_ref: Option<String>,
}

/// SE14: POST /companies/:company_id/secrets
async fn create_company_secret(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<CreateSecretBody>,
) -> Result<(StatusCode, Json<Value>), SecretError> {
    let pool = &state.pool;

    let managed_mode = body.managed_mode.as_deref().unwrap_or("paperclip_managed");
    let is_external = managed_mode == "external" || managed_mode == "external_reference";

    // Validate per createSecretSchema superRefine.
    if is_external && body.external_ref.as_deref().map_or(true, |s| s.trim().is_empty()) {
        return Err(SecretError::BadRequest(
            "External reference secrets require externalRef".to_string(),
        ));
    }
    if !is_external {
        if body
            .external_ref
            .as_deref()
            .map_or(false, |s| !s.trim().is_empty())
        {
            return Err(SecretError::BadRequest(
                "Managed secrets cannot set externalRef".to_string(),
            ));
        }
        if body.value.as_deref().map_or(true, |s| s.trim().is_empty()) {
            return Err(SecretError::BadRequest(
                "Managed secrets require value".to_string(),
            ));
        }
    }

    let key = body.key.unwrap_or_else(|| body.name.replace(' ', "_"));
    let provider = body.provider.unwrap_or_else(|| "local_encrypted".to_string());
    let db_managed_mode = if is_external { "external" } else { "paperclip_managed" };

    let row = sqlx::query(
        r#"INSERT INTO company_secrets
             (company_id, scope, key, name, provider, status, managed_mode,
              external_ref, provider_config_id, provider_metadata, description,
              created_by_user_id)
           VALUES ($1, 'company', $2, $3, $4, 'active', $5, $6, $7, $8, $9, $10)
           RETURNING id, company_id, scope, owner_user_id, user_secret_definition_id,
                     key, name, provider, status, managed_mode, external_ref, provider_config_id,
                     provider_metadata, latest_version, description, last_resolved_at, last_rotated_at,
                     deleted_at, created_by_agent_id, created_by_user_id, created_at, updated_at"#,
    )
    .bind(company_id)
    .bind(&key)
    .bind(&body.name)
    .bind(&provider)
    .bind(db_managed_mode)
    .bind(body.external_ref.as_deref())
    .bind(body.provider_config_id)
    .bind(&body.provider_metadata)
    .bind(body.description.as_deref())
    .bind("board")
    .fetch_one(pool)
    .await
    .map_err(|e| SecretError::Database(e.to_string()))?;

    let secret = secret_to_json(&row);

    // Record the initial version row for managed secrets (value material).
    // NOTE: the plaintext `value` is NOT persisted in plaintext here; a real
    // implementation would encrypt it via the configured provider. We store a
    // redacted material envelope so the version audit row exists.
    if !is_external {
        if let Some(id) = secret.get("id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) {
            let _ = sqlx::query(
                r#"INSERT INTO company_secret_versions (secret_id, version, material, value_sha256, status)
                   VALUES ($1, 1, $2, $3, 'current')"#,
            )
            .bind(id)
            .bind(json!({"redacted": true}))
            .bind(
                body.value
                    .as_deref()
                    .map(|s| sha256_hex(s.as_bytes()))
                    .unwrap_or_default(),
            )
            .execute(pool)
            .await;
        }
    }

    Ok((StatusCode::CREATED, Json(secret)))
}

/// SE15: GET /secrets/:id
async fn get_secret(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, SecretError> {
    let pool = &state.pool;
    let row = sqlx::query(&format!("{} WHERE id = $1", SECRET_SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| SecretError::Database(e.to_string()))?;
    match row {
        Some(r) => Ok(Json(secret_to_json(&r))),
        None => Err(SecretError::NotFound),
    }
}

/// PATCH /secrets/:id body — mirrors `updateSecretSchema`.
#[derive(Debug, Default, Deserialize)]
struct UpdateSecretBody {
    name: Option<String>,
    key: Option<String>,
    status: Option<String>,
    provider_config_id: Option<Option<Uuid>>,
    description: Option<Option<String>>,
    external_ref: Option<Option<String>>,
    provider_metadata: Option<Option<Value>>,
}

/// SE16: PATCH /secrets/:id
async fn update_secret(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateSecretBody>,
) -> Result<Json<Value>, SecretError> {
    let pool = &state.pool;
    let row = sqlx::query(
        r#"UPDATE company_secrets
              SET updated_at = NOW(),
                  name = COALESCE($2, name),
                  key = COALESCE($3, key),
                  status = COALESCE($4, status),
                  provider_config_id = CASE WHEN $5::bool IS TRUE THEN $6 ELSE provider_config_id END,
                  description = CASE WHEN $7::bool IS TRUE THEN $8 ELSE description END,
                  external_ref = CASE WHEN $9::bool IS TRUE THEN $10 ELSE external_ref END,
                  provider_metadata = CASE WHEN $11::bool IS TRUE THEN $12 ELSE provider_metadata END
            WHERE id = $1 AND deleted_at IS NULL
          RETURNING id, company_id, scope, owner_user_id, user_secret_definition_id,
                    key, name, provider, status, managed_mode, external_ref, provider_config_id,
                    provider_metadata, latest_version, description, last_resolved_at, last_rotated_at,
                    deleted_at, created_by_agent_id, created_by_user_id, created_at, updated_at"#,
    )
    .bind(id)
    .bind(body.name)
    .bind(body.key)
    .bind(body.status)
    .bind(body.provider_config_id.is_some())
    .bind(body.provider_config_id.unwrap_or(None))
    .bind(body.description.is_some())
    .bind(body.description.unwrap_or(None))
    .bind(body.external_ref.is_some())
    .bind(body.external_ref.unwrap_or(None))
    .bind(body.provider_metadata.is_some())
    .bind(body.provider_metadata.unwrap_or(None))
    .fetch_optional(pool)
    .await
    .map_err(|e| SecretError::Database(e.to_string()))?;
    match row {
        Some(r) => Ok(Json(secret_to_json(&r))),
        None => Err(SecretError::NotFound),
    }
}

/// SE17: DELETE /secrets/:id (soft-delete: set status='deleted', deleted_at=NOW())
async fn delete_secret(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, SecretError> {
    let pool = &state.pool;
    let res = sqlx::query(
        r#"UPDATE company_secrets
              SET status = 'deleted', deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL"#,
    )
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| SecretError::Database(e.to_string()))?;
    if res.rows_affected() == 0 {
        Err(SecretError::NotFound)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

/// POST /secrets/:id/rotate body — mirrors `rotateSecretSchema`.
#[derive(Debug, Default, Deserialize)]
struct RotateSecretBody {
    value: Option<String>,
    external_ref: Option<String>,
    provider_version_ref: Option<String>,
    provider_config_id: Option<Uuid>,
}

/// SE18: POST /secrets/:id/rotate
async fn rotate_secret(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RotateSecretBody>,
) -> Result<Json<Value>, SecretError> {
    let pool = &state.pool;

    // Validate rotation input (requireSecretRotationInput).
    let has_input = body.value.as_deref().map_or(false, |s| !s.trim().is_empty())
        || body.external_ref.as_deref().map_or(false, |s| !s.trim().is_empty())
        || body.provider_version_ref.is_some()
        || body.provider_config_id.is_some();
    if !has_input {
        return Err(SecretError::BadRequest(
            "Secret rotation requires value, externalRef, providerVersionRef, or providerConfigId"
                .to_string(),
        ));
    }

    // Fetch existing (must be company-scoped, non-deleted).
    let existing = sqlx::query(&format!("{} WHERE id = $1", SECRET_SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| SecretError::Database(e.to_string()))?;
    let existing = existing.ok_or(SecretError::NotFound)?;
    let scope: String = existing.get("scope");
    if scope != "company" {
        return Err(SecretError::NotFound);
    }
    let deleted: Option<chrono::DateTime<chrono::Utc>> = existing.get("deleted_at");
    if deleted.is_some() {
        return Err(SecretError::NotFound);
    }
    let next_version: i32 = existing.get::<i32, _>("latest_version") + 1;

    // Insert the new version row (current), retiring the prior current.
    let _ = sqlx::query(
        r#"UPDATE company_secret_versions SET status = 'superseded', revoked_at = NOW()
            WHERE secret_id = $1 AND status = 'current'"#,
    )
    .bind(id)
    .execute(pool)
    .await;
    let material = body.value.as_ref().map(|_| json!({"redacted": true})).unwrap_or(json!({}));
    let value_sha = body
        .value
        .as_deref()
        .map(|s| sha256_hex(s.as_bytes()))
        .unwrap_or_else(|| sha256_hex(format!("{:?}", body).as_bytes()));
    let _ = sqlx::query(
        r#"INSERT INTO company_secret_versions
             (secret_id, version, material, value_sha256, provider_version_ref, status)
           VALUES ($1, $2, $3, $4, $5, 'current')"#,
    )
    .bind(id)
    .bind(next_version)
    .bind(&material)
    .bind(&value_sha)
    .bind(body.provider_version_ref.as_deref())
    .execute(pool)
    .await;

    // Bump latest_version + last_rotated_at, optionally update external_ref/provider_config_id.
    let row = sqlx::query(
        r#"UPDATE company_secrets
              SET latest_version = $2,
                  last_rotated_at = NOW(),
                  updated_at = NOW(),
                  external_ref = COALESCE($3, external_ref),
                  provider_config_id = COALESCE($4, provider_config_id)
            WHERE id = $1
          RETURNING id, company_id, scope, owner_user_id, user_secret_definition_id,
                    key, name, provider, status, managed_mode, external_ref, provider_config_id,
                    provider_metadata, latest_version, description, last_resolved_at, last_rotated_at,
                    deleted_at, created_by_agent_id, created_by_user_id, created_at, updated_at"#,
    )
    .bind(id)
    .bind(next_version)
    .bind(body.external_ref.as_deref())
    .bind(body.provider_config_id)
    .fetch_one(pool)
    .await
    .map_err(|e| SecretError::Database(e.to_string()))?;

    Ok(Json(secret_to_json(&row)))
}

/// SE19: GET /secrets/:id/usage
///
/// Returns the binding references for this secret (Paperclip `listBindingReferences`).
async fn get_secret_usage(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, SecretError> {
    let pool = &state.pool;
    let rows = sqlx::query(
        r#"SELECT id, company_id, secret_id, target_type, target_id, config_path,
                  version_selector, required, label, created_at, updated_at
             FROM company_secret_bindings
            WHERE secret_id = $1
            ORDER BY created_at DESC"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| SecretError::Database(e.to_string()))?;

    let bindings: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "companyId": r.get::<Uuid, _>("company_id"),
                "secretId": r.get::<Uuid, _>("secret_id"),
                "targetType": r.get::<String, _>("target_type"),
                "targetId": r.get::<String, _>("target_id"),
                "configPath": r.get::<String, _>("config_path"),
                "versionSelector": r.get::<String, _>("version_selector"),
                "required": r.get::<bool, _>("required"),
                "label": r.get::<Option<String>, _>("label"),
                "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            })
        })
        .collect();

    Ok(Json(json!({ "secretId": id, "bindings": bindings })))
}

/// SE20: GET /secrets/:id/access-events
async fn get_secret_access_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Value>>, SecretError> {
    let pool = &state.pool;
    let rows = sqlx::query(
        r#"SELECT id, company_id, secret_id, version, provider, actor_type, actor_id,
                  consumer_type, consumer_id, config_path, secret_scope,
                  responsible_user_id, credential_owner_user_id,
                  credential_subject_type, credential_subject_id,
                  issue_id, heartbeat_run_id, plugin_id, outcome, created_at
             FROM secret_access_events
            WHERE secret_id = $1
            ORDER BY created_at DESC"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| SecretError::Database(e.to_string()))?;

    let events: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "companyId": r.get::<Uuid, _>("company_id"),
                "secretId": r.get::<Option<Uuid>, _>("secret_id"),
                "version": r.get::<Option<i32>, _>("version"),
                "provider": r.get::<String, _>("provider"),
                "actorType": r.get::<String, _>("actor_type"),
                "actorId": r.get::<Option<String>, _>("actor_id"),
                "consumerType": r.get::<String, _>("consumer_type"),
                "consumerId": r.get::<String, _>("consumer_id"),
                "configPath": r.get::<Option<String>, _>("config_path"),
                "secretScope": r.get::<String, _>("secret_scope"),
                "responsibleUserId": r.get::<Option<String>, _>("responsible_user_id"),
                "credentialOwnerUserId": r.get::<Option<String>, _>("credential_owner_user_id"),
                "credentialSubjectType": r.get::<Option<String>, _>("credential_subject_type"),
                "credentialSubjectId": r.get::<Option<String>, _>("credential_subject_id"),
                "issueId": r.get::<Option<Uuid>, _>("issue_id"),
                "heartbeatRunId": r.get::<Option<Uuid>, _>("heartbeat_run_id"),
                "pluginId": r.get::<Option<Uuid>, _>("plugin_id"),
                "outcome": r.get::<String, _>("outcome"),
                "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();

    Ok(Json(events))
}

#[derive(Debug)]
pub enum SecretError {
    NotFound,
    BadRequest(String),
    Database(String),
}

impl IntoResponse for SecretError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match self {
            SecretError::NotFound => (StatusCode::NOT_FOUND, "Secret not found".to_string()),
            SecretError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            SecretError::Database(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

/// Minimal SHA-256 hex digest (no external dep).
fn sha256_hex(bytes: &[u8]) -> String {
    use std::collections::VecDeque;
    // State constants (FIPS 180-4).
    const H0: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h = H0;
    // Padding.
    let bit_len = (bytes.len() as u64) * 8;
    let mut msg: VecDeque<u8> = bytes.iter().copied().collect();
    msg.push_back(0x80);
    while msg.len() % 64 != 56 {
        msg.push_back(0);
    }
    msg.extend(&bit_len.to_be_bytes());

    let chunks: Vec<Vec<u8>> = msg
        .as_slices()
        .0
        .chunks(64)
        .map(|c| c.to_vec())
        .collect();
    let chunks = if chunks.is_empty() {
        // msg may be stored contiguously; fallback
        let v: Vec<u8> = msg.into_iter().collect();
        v.chunks(64).map(|c| c.to_vec()).collect()
    } else {
        chunks
    };

    for chunk in chunks {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter().flat_map(|x| x.to_be_bytes()).map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_router_constructs() {
        let _ = secret_routes();
    }

    #[test]
    fn sha256_known_vector() {
        // "abc" -> ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
