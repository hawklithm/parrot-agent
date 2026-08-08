use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Issue plan decomposition status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssuePlanDecompositionStatus {
    InFlight,
    Completed,
    Cancelled,
}

impl std::fmt::Display for IssuePlanDecompositionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InFlight => write!(f, "in_flight"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Issue plan decomposition record
/// Tracks the process of breaking down an accepted plan into child issues
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuePlanDecomposition {
    pub id: Uuid,
    pub company_id: Uuid,
    pub source_issue_id: Uuid,
    pub accepted_plan_revision_id: Uuid,
    pub accepted_interaction_id: Option<Uuid>,
    #[sqlx(try_from = "String")]
    pub status: IssuePlanDecompositionStatus,
    pub request_fingerprint: String,
    pub requested_child_count: i32,
    pub requested_children: serde_json::Value, // Array of child issue inputs
    pub child_issue_ids: serde_json::Value,    // Array of created child issue UUIDs
    pub owner_agent_id: Option<Uuid>,
    pub owner_user_id: Option<Uuid>,
    pub owner_run_id: Option<Uuid>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating/updating a plan decomposition
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedPlanDecompositionInput {
    pub accepted_plan_revision_id: Uuid,
    pub children: Vec<serde_json::Value>, // Array of CreateIssueInput-like objects
}

/// Result of a plan decomposition operation
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedPlanDecompositionResult {
    pub decomposition: IssuePlanDecomposition,
    pub child_issue_ids: Vec<Uuid>,
    pub newly_created_child_issue_ids: Vec<Uuid>,
}

// sqlx type conversion for status
impl sqlx::Type<sqlx::Postgres> for IssuePlanDecompositionStatus {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<sqlx::Postgres>>::type_info()
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for IssuePlanDecompositionStatus {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let s = <String as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        match s.as_str() {
            "in_flight" => Ok(Self::InFlight),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("Invalid IssuePlanDecompositionStatus: {}", s).into()),
        }
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for IssuePlanDecompositionStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> sqlx::encode::IsNull {
        let s = self.to_string();
        <String as sqlx::Encode<sqlx::Postgres>>::encode(s, buf)
    }
}

impl std::convert::TryFrom<String> for IssuePlanDecompositionStatus {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "in_flight" => Ok(Self::InFlight),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("Invalid IssuePlanDecompositionStatus: {}", value)),
        }
    }
}
