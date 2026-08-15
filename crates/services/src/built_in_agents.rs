/// Built-in Agents Service
/// 
/// 内置Agent快速创建

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum BuiltInAgentsError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("agent not found: {0}")]
    AgentNotFound(String),
}

pub type BuiltInAgentsResult<T> = Result<T, BuiltInAgentsError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTemplate {
    pub id: String,
    pub name: String,
    pub instructions: String,
    pub tools: Vec<String>,
    pub config: serde_json::Value,
}

pub struct BuiltInAgentsService {
    pool: PgPool,
}

impl BuiltInAgentsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub fn get_templates() -> Vec<AgentTemplate> {
        vec![
            AgentTemplate {
                id: "task".to_string(),
                name: "Task At".to_string(),
                instructions: "Execute tasks as instructed".to_string(),
                tools: vec!["read".to_string(), "write".to_string()],
                config: serde_json::json!({}),
            },
            AgentTemplate {
                id: "scout".to_string(),
                name: "Scout Agent".to_string(),
                instructions: "Explore and analyze codebase".to_string(),
                tools: vec!["read".to_string(), "search".to_string()],
                config: serde_json::json!({"read_only": true}),
            },
        ]
    }
    
    pub async fn create_from_template(
        &self,
        template_id: &str,
        owner_id: Uuid,
    ) -> BuiltInAgentsResult<Uuid> {
        let templates = Self::get_templates();
        let template = templates.iter()
            .find(|t| t.id == template_id)
            .ok_or_else(|| BuiltInAgentsError::AgentNotFound(template_id.to_string()))?;
        
        let agent_id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO agents 
            (id, name, instructions, owner_id, config, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RE id
            "#
        )
        .bind(agent_id)
        .bind(&template.name)
        .bind(&template.instructions)
        .bind(owner_id)
        .bind(&template.config)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(agent_id)
    }
}
