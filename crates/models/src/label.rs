use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Label 标签
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Label {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// CreateLabelInput 创建标签输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLabelInput {
    pub company_id: Uuid,
    pub name: String,
    pub color: Option<String>,
}
