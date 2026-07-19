use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::SigningKey;
use pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryResponse {
    pub stack: DiscoveryStack,
    pub auth: DiscoveryAuth,
    pub transfer: Option<DiscoveryTransfer>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryStack {
    pub id: String,
    pub slug: Option<String>,
    pub company_id: String,
    pub origin: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryAuth {
    pub pkce: Option<DiscoveryPkce>,
    pub scopes: Option<Vec<String>>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryPkce {
    pub authorize_url: String,
    pub token_url: String,
    pub code_challenge_method: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryTransfer {
    pub supported_schema_major: Option<i32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRequest<'a> {
    pub grant_type: &'static str,
    pub code: &'a str,
    pub redirect_uri: &'a str,
    pub code_verifier: &'a str,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait CloudUpstreamService: Send + Sync {
    async fn list(&self, company_id: Uuid) -> ServiceResult<Vec<serde_json::Value>>;
    async fn start_connect(
        &self,
        company_id: Uuid,
        remote_url: &str,
        redirect_uri: &str,
    ) -> ServiceResult<serde_json::Value>;
    async fn finish_connect(
        &self,
        connection_id: Uuid,
        state: &str,
        code: &str,
    ) -> ServiceResult<serde_json::Value>;
    async fn preview(
        &self,
        connection_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<serde_json::Value>;
    async fn create_run(
        &self,
        connection_id: Uuid,
        company_id: Uuid,
        body: serde_json::Value,
    ) -> ServiceResult<serde_json::Value>;
    async fn read_run(&self, company_id: Uuid, run_id: Uuid) -> ServiceResult<serde_json::Value>;
    async fn cancel_run(&self, company_id: Uuid, run_id: Uuid) -> ServiceResult<()>;
    async fn activate_entity(
        &self,
        run_id: Uuid,
        connection_id: Uuid,
        entity_type: &str,
    ) -> ServiceResult<serde_json::Value>;
}

// ---------------------------------------------------------------------------
// Default implementation
// ---------------------------------------------------------------------------

pub struct DefaultCloudUpstreamService {
    pub pool: PgPool,
    pub client: reqwest::Client,
}

impl DefaultCloudUpstreamService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            client: reqwest::Client::new(),
        }
    }

    /// Fetch the Paperclip upstream discovery document from the remote.
    async fn fetch_discovery(&self, remote_url: &str) -> Result<DiscoveryResponse, ServiceError> {
        let parsed = reqwest::Url::parse(remote_url)
            .map_err(|_| ServiceError::BadRequest("invalid remote URL".into()))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| ServiceError::BadRequest("remote URL missing host".into()))?;
        if parsed.scheme() != "https" && host != "localhost" && host != "127.0.0.1" {
            return Err(ServiceError::BadRequest(
                "remote URL must be https (localhost exempt)".into(),
            ));
        }
        let mut url = parsed.clone();
        url.set_path("/.well-known/paperclip-upstream");
        let first_segment = parsed
            .path()
            .trim_matches('/')
            .split('/')
            .next()
            .filter(|s| !s.is_empty());
        if let Some(stack_id) = first_segment {
            url.set_query(Some(&format!("stackId={}", urlencoding::encode(stack_id))));
        }
        let resp = self
            .client
            .get(url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|_| bad_gateway("discovery request failed"))?
            .error_for_status()
            .map_err(|_| bad_gateway("discovery returned error"))?;
        resp.json()
            .await
            .map_err(|_| bad_gateway("invalid discovery JSON"))
    }

    /// Verify that cloud sync is enabled via instance settings.
    async fn ensure_cloud_sync(&self) -> Result<(), ServiceError> {
        // We access instance settings through a simple DB query for now.
        // In production, this should go through InstanceSettingsService.
        let enabled: Option<bool> = sqlx::query_scalar(
            "SELECT COALESCE((experimental->>'enableCloudSync')::bool, false) FROM instance_settings LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?
        .flatten();
        if enabled != Some(true) {
            return Err(ServiceError::NotFound("cloud sync is not enabled".into()));
        }
        Ok(())
    }
}

#[async_trait]
impl CloudUpstreamService for DefaultCloudUpstreamService {
    async fn list(&self, company_id: Uuid) -> ServiceResult<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT id, remote_url, status, token_status, updated_at \
             FROM cloud_upstream_connections WHERE company_id=$1 ORDER BY updated_at DESC",
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.get::<Uuid, _>("id"),
                    "remoteUrl": r.get::<String, _>("remote_url"),
                    "status": r.get::<String, _>("status"),
                    "tokenStatus": r.get::<String, _>("token_status"),
                    "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            })
            .collect())
    }

    async fn start_connect(
        &self,
        company_id: Uuid,
        remote_url: &str,
        redirect_uri: &str,
    ) -> ServiceResult<serde_json::Value> {
        self.ensure_cloud_sync().await?;
        let discovery = self.fetch_discovery(remote_url).await?;
        let pkce = discovery
            .auth
            .pkce
            .ok_or_else(|| ServiceError::BadRequest("remote does not support PKCE".into()))?;

        let id = Uuid::new_v4();
        let state_token = Uuid::new_v4().to_string();
        let verifier = URL_SAFE_NO_PAD.encode({
            let mut bytes = [0u8; 32];
            rand::RngCore::fill_bytes(&mut OsRng, &mut bytes);
            bytes
        });
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

        let signing_key = SigningKey::generate(&mut OsRng);
        let public_key = signing_key
            .verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .map_err(|_| ServiceError::Internal("key export failed".into()))?;
        let private_key = signing_key
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(|_| ServiceError::Internal("key export failed".into()))?
            .to_string();
        let fingerprint = format!(
            "sha256:{}",
            hex::encode(Sha256::digest(public_key.as_bytes()))
        );
        let source_instance_id = std::env::var("INSTANCE_ID").unwrap_or_else(|_| "local".into());
        let target_company_id = Uuid::parse_str(&discovery.stack.company_id)
            .map_err(|_| bad_gateway("invalid target company id"))?;
        let target_schema_major = discovery
            .transfer
            .as_ref()
            .and_then(|transfer| transfer.supported_schema_major);

        sqlx::query(
            "INSERT INTO cloud_upstream_connections \
             (id, company_id, remote_url, status, source_instance_id, \
              source_instance_fingerprint, source_public_key, private_key_pem, scopes, \
              target_stack_id, target_stack_slug, target_company_id, target_origin, \
              target_schema_major, pending_state, pending_code_verifier, \
              pending_redirect_uri, pending_token_url) \
             VALUES ($1,$2,$3,'pending',$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)",
        )
        .bind(id)
        .bind(company_id)
        .bind(remote_url)
        .bind(&source_instance_id)
        .bind(&fingerprint)
        .bind(&public_key)
        .bind(&private_key)
        .bind(discovery.auth.scopes.clone().unwrap_or_default())
        .bind(&discovery.stack.id)
        .bind(&discovery.stack.slug)
        .bind(target_company_id.to_string())
        .bind(&discovery.stack.origin)
        .bind(target_schema_major)
        .bind(&state_token)
        .bind(&verifier)
        .bind(redirect_uri)
        .bind(&pkce.token_url)
        .execute(&self.pool)
        .await
        .map_err(|_| ServiceError::Internal("failed to persist connection".into()))?;

        let authorization_url = format!(
            "{}?stackId={}&state={}&codeChallenge={}&codeChallengeMethod={}&redirectUri={}&sourceInstanceId={}&sourceInstanceFingerprint={}&sourcePublicKey={}&scopes={}",
            pkce.authorize_url,
            discovery.stack.id,
            state_token,
            challenge,
            pkce.code_challenge_method.unwrap_or_else(|| "S256".into()),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&source_instance_id),
            urlencoding::encode(&fingerprint),
            urlencoding::encode(&public_key),
            urlencoding::encode(&discovery.auth.scopes.unwrap_or_default().join(" "))
        );

        Ok(serde_json::json!({
            "connectionId": id,
            "status": "pending",
            "authorizationUrl": authorization_url,
        }))
    }

    async fn finish_connect(
        &self,
        connection_id: Uuid,
        returned_state: &str,
        code: &str,
    ) -> ServiceResult<serde_json::Value> {
        self.ensure_cloud_sync().await?;

        let pending: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT pending_state, pending_code_verifier, pending_redirect_uri, \
             pending_token_url FROM cloud_upstream_connections WHERE id = $1",
        )
        .bind(connection_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
        let Some((expected_state, verifier, redirect_uri, token_url)) = pending else {
            return Err(ServiceError::NotFound(format!(
                "connection {connection_id} not found"
            )));
        };
        if expected_state != returned_state {
            return Err(ServiceError::BadRequest("state mismatch".into()));
        }

        let token: serde_json::Value = self
            .client
            .post(&token_url)
            .json(&TokenRequest {
                grant_type: "authorization_code",
                code,
                redirect_uri: &redirect_uri,
                code_verifier: &verifier,
            })
            .send()
            .await
            .map_err(|_| bad_gateway("token exchange failed"))?
            .error_for_status()
            .map_err(|_| bad_gateway("token exchange returned error"))?
            .json()
            .await
            .map_err(|_| bad_gateway("invalid token response"))?;

        let access_token = token
            .get("accessToken")
            .or_else(|| token.get("access_token"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| bad_gateway("missing access_token in response"))?;

        sqlx::query(
            "UPDATE cloud_upstream_connections SET \
             status = 'connected', token_status = 'connected', \
             access_token = $2, token_id = $3, \
             token_expires_at = CASE WHEN $4::text IS NULL THEN NULL \
                                ELSE NOW() + ($4::text || ' seconds')::interval END, \
             pending_state = NULL, pending_code_verifier = NULL, \
             pending_redirect_uri = NULL, pending_token_url = NULL, \
             updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(connection_id)
        .bind(access_token)
        .bind(
            token
                .get("tokenId")
                .or_else(|| token.get("token_id"))
                .and_then(|v| v.as_str()),
        )
        .bind(
            token
                .get("expiresIn")
                .or_else(|| token.get("expires_in"))
                .and_then(|v| v.as_i64())
                .map(|v| v.to_string()),
        )
        .execute(&self.pool)
        .await
        .map_err(|_| ServiceError::Internal("failed to persist token".into()))?;

        Ok(serde_json::json!({
            "connectionId": connection_id,
            "status": "connected",
        }))
    }

    async fn preview(
        &self,
        connection_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<serde_json::Value> {
        self.ensure_cloud_sync().await?;
        let exists: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM cloud_upstream_connections \
             WHERE id = $1 AND company_id = $2 AND status = 'connected'",
        )
        .bind(connection_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
        if exists.is_none() {
            return Err(ServiceError::NotFound(format!(
                "connection {connection_id} not found or not connected"
            )));
        }
        Ok(serde_json::json!({
            "connectionId": connection_id,
            "companyId": company_id,
            "preview": { "ready": true },
        }))
    }

    async fn create_run(
        &self,
        connection_id: Uuid,
        company_id: Uuid,
        body: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        self.ensure_cloud_sync().await?;

        let connection: Option<(String, Option<String>, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT status, target_origin, target_company_id, access_token \
                 FROM cloud_upstream_connections WHERE id = $1 AND company_id = $2",
            )
            .bind(connection_id)
            .bind(company_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ServiceError::Internal(e.to_string()))?;
        let Some((connection_status, target_origin, target_company_id, access_token)) =
            connection
        else {
            return Err(ServiceError::NotFound(format!(
                "connection {connection_id} not found"
            )));
        };
        if connection_status != "connected" {
            return Err(ServiceError::Conflict("connection is not connected".into()));
        }

        // Check for running runs
        let running: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM cloud_upstream_runs \
             WHERE connection_id = $1 AND company_id = $2 AND status = 'running')",
        )
        .bind(connection_id)
        .bind(company_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
        if running {
            return Err(ServiceError::Conflict(
                "another push run is already in progress".into(),
            ));
        }

        let idempotency_key = Uuid::new_v4();
        let manifest_hash = hex::encode(Sha256::digest(
            serde_json::to_vec(&body).map_err(|e| ServiceError::Internal(e.to_string()))?,
        ));
        let run_id = Uuid::new_v4();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ServiceError::Internal(e.to_string()))?;
        sqlx::query(
            "INSERT INTO cloud_upstream_runs \
             (id, connection_id, company_id, status, active_step, progress_percent, \
              summary, idempotency_key, manifest_hash, target_url, dry_run) \
             VALUES ($1,$2,$3,'running','remote_create',10,$4,$5,$6,$7,$8)",
        )
        .bind(run_id)
        .bind(connection_id)
        .bind(company_id)
        .bind(&body)
        .bind(idempotency_key.to_string())
        .bind(&manifest_hash)
        .bind(&target_origin)
        .bind(body.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false))
        .execute(&mut *tx)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| ServiceError::Internal(e.to_string()))?;

        let Some(origin) = target_origin else {
            return Err(bad_gateway("connection has no target origin"));
        };
        let Some(target_company) = target_company_id else {
            return Err(bad_gateway("connection has no target company id"));
        };

        // Remote push
        let create_url = format!(
            "{}/api/companies/{}/upstream-imports/runs",
            origin.trim_end_matches('/'),
            target_company
        );
        let mut request = self
            .client
            .post(&create_url)
            .header("Idempotency-Key", idempotency_key.to_string())
            .json(&body);
        if let Some(token) = access_token.as_deref() {
            request = request.bearer_auth(token);
        }
        let remote_response = match request.send().await {
            Ok(r) => r,
            Err(_) => {
                let _ = sqlx::query(
                    "UPDATE cloud_upstream_runs SET status='failed', \
                     active_step='remote_create', updated_at=NOW(), completed_at=NOW() \
                     WHERE id=$1",
                )
                .bind(run_id)
                .execute(&self.pool)
                .await;
                return Err(bad_gateway("remote create request failed"));
            }
        };
        let remote_response = match remote_response.error_for_status() {
            Ok(r) => r,
            Err(_) => {
                let _ = sqlx::query(
                    "UPDATE cloud_upstream_runs SET status='failed', \
                     active_step='remote_create', updated_at=NOW(), completed_at=NOW() \
                     WHERE id=$1",
                )
                .bind(run_id)
                .execute(&self.pool)
                .await;
                return Err(bad_gateway("remote create returned error"));
            }
        };

        let remote = remote_response
            .json::<serde_json::Value>()
            .await
            .map_err(|_| bad_gateway("invalid remote response"))?;
        let remote_run_id = remote
            .get("remoteRunId")
            .or_else(|| remote.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| bad_gateway("missing remoteRunId in response"))?;

        let apply_url = format!(
            "{}/api/upstream-import-runs/{}/apply",
            origin.trim_end_matches('/'),
            remote_run_id
        );
        let mut apply = self
            .client
            .post(&apply_url)
            .header("Idempotency-Key", idempotency_key.to_string())
            .json(&serde_json::json!({"companyId": target_company}));
        if let Some(token) = access_token.as_deref() {
            apply = apply.bearer_auth(token);
        }
        match apply.send().await {
            Ok(r) if r.status().is_success() => {}
            _ => {
                let _ = sqlx::query(
                    "UPDATE cloud_upstream_runs SET status='failed', \
                     active_step='remote_apply', updated_at=NOW(), completed_at=NOW(), \
                     remote_run_id=$2 WHERE id=$1",
                )
                .bind(run_id)
                .bind(remote_run_id)
                .execute(&self.pool)
                .await;
                return Err(bad_gateway("remote apply failed"));
            }
        }

        sqlx::query(
            "UPDATE cloud_upstream_runs SET status='succeeded', \
             active_step='completed', progress_percent=100, remote_run_id=$2, \
             updated_at=NOW(), completed_at=NOW() WHERE id=$1",
        )
        .bind(run_id)
        .bind(remote_run_id)
        .execute(&self.pool)
        .await
        .map_err(|_| ServiceError::Internal("failed to update run status".into()))?;

        Ok(serde_json::json!({
            "connectionId": connection_id,
            "runId": run_id,
            "remoteRunId": remote_run_id,
            "status": "succeeded",
        }))
    }

    async fn read_run(&self, company_id: Uuid, run_id: Uuid) -> ServiceResult<serde_json::Value> {
        sqlx::query(
            "SELECT id, connection_id, status, progress_percent, summary, warnings, \
             conflicts, events FROM cloud_upstream_runs WHERE id=$1 AND company_id=$2",
        )
        .bind(run_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "connectionId": r.get::<Uuid, _>("connection_id"),
                "status": r.get::<String, _>("status"),
                "progressPercent": r.get::<i32, _>("progress_percent"),
                "summary": r.get::<serde_json::Value, _>("summary"),
                "warnings": r.get::<serde_json::Value, _>("warnings"),
                "conflicts": r.get::<serde_json::Value, _>("conflicts"),
                "events": r.get::<serde_json::Value, _>("events"),
            })
        })
        .ok_or_else(|| ServiceError::NotFound(format!("run {run_id}")))
    }

    async fn cancel_run(&self, company_id: Uuid, run_id: Uuid) -> ServiceResult<()> {
        // Try remote cancel first
        let remote_info: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT r.remote_run_id, c.target_origin, c.access_token \
             FROM cloud_upstream_runs r \
             JOIN cloud_upstream_connections c ON c.id = r.connection_id \
             WHERE r.id=$1 AND r.company_id=$2",
        )
        .bind(run_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?;

        if let Some((Some(remote_id), Some(origin), token)) = remote_info {
            let mut request = self.client.post(format!(
                "{}/api/upstream-import-runs/{}/cancel",
                origin.trim_end_matches('/'),
                remote_id
            ));
            if let Some(t) = token {
                request = request.bearer_auth(t);
            }
            let _ = request.send().await;
        }

        let result = sqlx::query(
            "UPDATE cloud_upstream_runs SET status='cancelled', \
             completed_at=NOW(), updated_at=NOW() WHERE id=$1 AND company_id=$2",
        )
        .bind(run_id)
        .bind(company_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(ServiceError::NotFound(format!("run {run_id}")));
        }
        Ok(())
    }

    async fn activate_entity(
        &self,
        run_id: Uuid,
        connection_id: Uuid,
        entity_type: &str,
    ) -> ServiceResult<serde_json::Value> {
        let report: Option<serde_json::Value> = sqlx::query_scalar(
            "UPDATE cloud_upstream_runs \
             SET report = jsonb_set(report, ARRAY['activationChecklist', $3], \
                'true'::jsonb, true), \
             updated_at = NOW() \
             WHERE id = $1 AND connection_id = $2 AND status = 'succeeded' \
             RETURNING report",
        )
        .bind(run_id)
        .bind(connection_id)
        .bind(entity_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
        let Some(report) = report else {
            return Err(ServiceError::NotFound(format!(
                "run {run_id} not found or not in succeeded state"
            )));
        };
        Ok(serde_json::json!({
            "connectionId": connection_id,
            "runId": run_id,
            "report": report,
        }))
    }
}

/// Convenience constructor for bad-gateway errors (maps to Internal with context).
fn bad_gateway(msg: &str) -> ServiceError {
    ServiceError::Internal(format!("bad gateway: {msg}"))
}
