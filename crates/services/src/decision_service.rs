use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Decision {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub decision_type: String,
    pub context: serde_json::Value,
    pub options: serde_json::Value,
    pub selected_option: Option<Uuid>,
    pub status: DecisionStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DecisionOption {
    pub id: Uuid,
    pub label: String,
    pub description: Option<String>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum DecisionStatus {
    Pending,
    Decided,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct CreateDecisionRequest {
    pub agent_id: Uuid,
    pub decision_type: String,
    pub context: serde_json::Value,
    pub options: Vec<DecisionOption>,
}

#[async_trait]
pub trait DecisionService: Send + Sync {
    async fn create_decision(&self, req: CreateDecisionRequest) -> DecisionResult<Decision>;
    async fn get_decision(&self, decision_id: Uuid) -> DecisionResult<Option<Decision>>;
    async fn make_decision(&self, decision_id: Uuid, option_id: Uuid) -> DecisionResult<()>;
    async fn cancel_decision(&self, decision_id: Uuid) -> DecisionResult<()>;
    async fn list_pending_decisions(&self, agent_id: Uuid) -> DecisionResult<Vec<Decision>>;
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
        let decision_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            INSERT Isions (id, agent_id, decision_type, context, options, status, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#
        )
        .bind(decision_id)
        .bind(req.agent_id)
        .bind(&req.decision_type)
        .bind(&req.context)
        .bind(serde_json::to_value(&req.options).unwrap())
        .bind(serde_json::to_value(&DecisionStatus::Pending).unwrap())
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(Decision {
            id: decision_id,
            agent_id: req.agent_id,
            decision_type: req.decision_type,
            context: req.context,
            options: serde_json::to_value(&req.options).unwrap(),
            selected_option: None,
            status: DecisionStatus::Pending,
            created_at: now,
            decided_at: None,
        })
    }
    
    async fn get_decision(&self, decision_id: Uuid) -> DecisionResult<Option<Decision>> {
        let row = sqlx::query_as::<_, Decision>(
            r#"
            SELECT id, agent_id, decision_type, context, options, selected_option, status, created_at, decided_at
            FROM decisions
            WHERE id = $1
            "#
        )
        .bind(decision_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    async fn make_decision(&self, decision_id: Uuid, option_id: Uuid) -> DecisionResult<()> {
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            UPDATE decisions
            SET selected_option = $1, status = $2, decided_at = $3
            WHERE id = $4 AND status = 'pending'
            "#
        )
        .bind(option_id)
        .bind(serde_json::to_value(&DecisionStatus::Decided).unwrap())
        .bind(now)
        .bind(decision_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    async fn cancel_decision(&self, decision_id: Uuid) -> DecisionResult<()> {
        sqlx::query(
            r#"
            UPDATE decisions
            SET status = $1
            WHERE id = $2 AND status = 'pending'
            "#
        )
        .bind(serde_json::to_value(&DecisionStatus::Cancelled).unwrap())
        .bind(decision_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    async fn list_pending_decisions(&self, agent_id: Uuid) -> DecisionResult<Vec<Decision>> {
        let rows = sqlx::query_as::<_, Decision>(
            r#"
            SELECT id, agent_id, decision_type, context, options, selected_option, status, created_at, decided_at
            FROM decisions
            WHERE agent_id = $1 AND status = 'pending'
            ORDER BY created_at DESC
            "#
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
}
