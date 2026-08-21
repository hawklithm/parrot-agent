use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DecisionServiceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("decision not found: {0}")]
    NotFound(Uuid),
    #[error("invalid decision: {0}")]
    Invalid(String),
}

pub type DecisionResult<T> = Result<T, DecisionServiceError>;

/// Mirrors the `decisions` table in `migrations/00_init_schema_unified.sql`.
/// Status values: `open` | `decided` | `cancelled` | `expired`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Decision {
    pub id: Uuid,
    pub company_id: Uuid,
    pub bundle_id: Option<Uuid>,
    pub origin_agent_id: Uuid,
    pub origin_issue_id: Uuid,
    pub origin_run_id: Uuid,
    pub rule_key: Option<String>,
    pub title: String,
    pub body: String,
    pub options: serde_json::Value,
    pub inputs: Option<serde_json::Value>,
    pub status: DecisionStatus,
    pub execution_status: Option<String>,
    pub chosen_option_id: Option<String>,
    pub input_values: Option<serde_json::Value>,
    pub decided_by_user_id: Option<String>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub idempotency_key: Option<String>,
    pub signed_spec: String,
    pub target_snapshots: serde_json::Value,
    pub continuation_policy: String,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "lowercase")]
pub enum DecisionStatus {
    Open,
    Decided,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone)]
pub struct CreateDecisionRequest {
    pub company_id: Uuid,
    pub origin_agent_id: Uuid,
    pub origin_issue_id: Uuid,
    pub origin_run_id: Uuid,
    pub rule_key: Option<String>,
    pub title: String,
    pub body: String,
    pub options: serde_json::Value,
    pub inputs: Option<serde_json::Value>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub idempotency_key: Option<String>,
    pub continuation_policy: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Deterministic, dependency-free stand-in for Paperclip's `signDecisionSpec`.
/// Produces a stable signature over the canonical spec JSON so `signed_spec` is
/// never empty. Full cryptographic signing (HMAC/asymmetric) is a separate concern.
fn sign_spec(spec: &serde_json::Value) -> String {
    let mut hasher = DefaultHasher::new();
    spec.to_string().hash(&mut hasher);
    format!("sig1:{:016x}", hasher.finish())
}

fn select_columns() -> &'static str {
    "id, company_id, bundle_id, origin_agent_id, origin_issue_id, origin_run_id, \
     rule_key, title, body, options, inputs, status, execution_status, chosen_option_id, \
     input_values, decided_by_user_id, decided_at, expires_at, idempotency_key, signed_spec, \
     target_snapshots, continuation_policy, metadata, created_at, updated_at"
}

#[async_trait]
pub trait DecisionService: Send + Sync {
    async fn create_decision(&self, req: CreateDecisionRequest) -> DecisionResult<Decision>;
    async fn get_decision(&self, decision_id: Uuid) -> DecisionResult<Option<Decision>>;
    async fn make_decision(
        &self,
        decision_id: Uuid,
        option_id: String,
        decided_by_user_id: Option<String>,
    ) -> DecisionResult<()>;
    async fn cancel_decision(&self, decision_id: Uuid) -> DecisionResult<()>;
    async fn list_pending_decisions(&self, company_id: Uuid) -> DecisionResult<Vec<Decision>>;
}

pub struct DecisionServiceImpl {
    pool: PgPool,
}

impl DecisionServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DecisionService for DecisionServiceImpl {
    async fn create_decision(&self, req: CreateDecisionRequest) -> DecisionResult<Decision> {
        // Idempotency: a repeated create with the same company + key returns the
        // existing decision (Paperclip conflict-dedup semantics).
        if let Some(ref ik) = req.idempotency_key {
            let existing = sqlx::query_as::<_, Decision>(&format!(
                "SELECT {} FROM decisions WHERE company_id = $1 AND idempotency_key = $2",
                select_columns()
            ))
            .bind(req.company_id)
            .bind(ik)
            .fetch_optional(&self.pool)
            .await?;
            if let Some(d) = existing {
                return Ok(d);
            }
        }

        let decision_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let expires_at = req
            .expires_at
            .unwrap_or_else(|| now + chrono::Duration::days(7));
        let continuation_policy = req
            .continuation_policy
            .clone()
            .unwrap_or_else(|| "none".to_string());
        let options = req.options.clone();
        let metadata = req
            .metadata
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let signed_spec = sign_spec(&serde_json::json!({
            "id": decision_id,
            "options": options,
            "title": req.title,
            "body": req.body,
        }));

        let row = sqlx::query_as::<_, Decision>(&format!(
            "INSERT INTO decisions \
             (id, company_id, bundle_id, origin_agent_id, origin_issue_id, origin_run_id, \
              rule_key, title, body, options, inputs, status, expires_at, idempotency_key, \
              signed_spec, target_snapshots, continuation_policy, metadata) \
             VALUES ($1, $2, NULL, $3, $4, $5, $6, $7, $8, $9, $10, 'open', $11, $12, $13, '{{}}'::jsonb, $14, $15) \
             RETURNING {}",
            select_columns()
        ))
        .bind(decision_id)
        .bind(req.company_id)
        .bind(req.origin_agent_id)
        .bind(req.origin_issue_id)
        .bind(req.origin_run_id)
        .bind(&req.rule_key)
        .bind(&req.title)
        .bind(&req.body)
        .bind(&options)
        .bind(&req.inputs)
        .bind(expires_at)
        .bind(&req.idempotency_key)
        .bind(&signed_spec)
        .bind(&continuation_policy)
        .bind(&metadata)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    async fn get_decision(&self, decision_id: Uuid) -> DecisionResult<Option<Decision>> {
        let row = sqlx::query_as::<_, Decision>(&format!(
            "SELECT {} FROM decisions WHERE id = $1",
            select_columns()
        ))
        .bind(decision_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    async fn make_decision(
        &self,
        decision_id: Uuid,
        option_id: String,
        decided_by_user_id: Option<String>,
    ) -> DecisionResult<()> {
        let now = chrono::Utc::now();
        let result = sqlx::query(
            "UPDATE decisions \
             SET chosen_option_id = $1, decided_by_user_id = $2, decided_at = $3, \
                 status = 'decided', execution_status = 'pending', updated_at = $3 \
             WHERE id = $4 AND status = 'open'",
        )
        .bind(&option_id)
        .bind(&decided_by_user_id)
        .bind(now)
        .bind(decision_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(DecisionServiceError::NotFound(decision_id));
        }
        Ok(())
    }

    async fn cancel_decision(&self, decision_id: Uuid) -> DecisionResult<()> {
        let now = chrono::Utc::now();
        let result = sqlx::query(
            "UPDATE decisions SET status = 'cancelled', updated_at = $1 \
             WHERE id = $2 AND status = 'open'",
        )
        .bind(now)
        .bind(decision_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(DecisionServiceError::NotFound(decision_id));
        }
        Ok(())
    }

    async fn list_pending_decisions(&self, company_id: Uuid) -> DecisionResult<Vec<Decision>> {
        let rows = sqlx::query_as::<_, Decision>(&format!(
            "SELECT {} FROM decisions WHERE company_id = $1 AND status = 'open' \
             ORDER BY created_at DESC",
            select_columns()
        ))
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}
