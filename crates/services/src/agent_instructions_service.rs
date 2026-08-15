/// Agent 指令服务
/// 
/// 管理 Agent 的指令模板、变量替换和版本控制

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum InstructionsError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("template not found: {0}")]
    TemplateNotFound(String),
    
    #[error("variable not found: {0}")]
    VariableNotFound(String),
}

pub type InstructionsResult<T> = Result<T, InstructionsError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionTemplate {
    pub id: Uuid,
    pub name: String,
    pub content: String,
    pub variables: Vec<String>,
    pub version: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Agent 指令服务
pub struct AgentInstructionsService {
    pool: PgPool,
}

impl AgentInstructionsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// 创建指令模板
    pub async fn create_template(
        &self,
        name: String,
        content: String,
        variables: Vec<String>,
    ) -> InstructionsResult<Uuid> {
        let id = Uuid::new_v4();
        
        sqlx::query!(
            "INSERT INTO instruction_templates (id, name, content, variables, version, created_at)
             VALUES ($1, $2, $3, $4, 1, NOW())",
            id,
            name,
            content,
            &variables
        )
        .execute(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    /// 获取指令模板
    pub async fn get_template(&self, id: Uuid) -> InstructionsResult<InstructionTemplate> {
        let row = sqlx::query!(
            "SELECT id, name, content, variables, version, created_at
             FROM instruction_templates
             WHERE id = $1",
            id
        )
        .fetch_one(&self.pool)
        .await?;
        
        Ok(InstructionTemplate {
            id: row.id,
            name: row.name,
            content: row.content,
            variables: row.variables,
            version: row.version,
            created_at: row.created_at,
        })
    }
    
    /// 替换指令变量
    pub fn replace_variables(
        &self,
        template: &str,
        variables: &HashMap<String, String>,
    ) -> String {
        let mut result = template.to_string();
        
        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }
        
        result
    }
    
    /// 更新指令模板（创建新版本）
    pub async fn update_template(
        &self,
        id: Uuid,
        content: String,
        variables: Vec<String>,
    ) -> InstructionsResult<i32> {
        let current = self.get_template(id).await?;
        let new_version = current.version + 1;
        
        sqlx::query!(
            "UPDATE instruction_templates
             SET content = $1, variables = $2, version = $3, updated_at = NOW()
             WHERE id = $4",
            content,
            &variables,
            new_version,
            id
        )
        .execute(&self.pool)
        .await?;
        
        Ok(new_version)
    }
    
    /// 列出所有模板
    pub async fn list_templates(&self) -> InstructionsResult<Vec<InstructionTemplate>> {
        let rows = sqlx::query!(
            "SELECT id, name, content, variables, version, created_at
             FROM instruction_templates
             ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| InstructionTemplate {
            id: row.id,
            name: row.name,
            content: row.content,
            variables: row.variables,
            version: row.version,
            created_at: row.created_at,
        }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_replace_variables() {
        let service = AgentInstructionsService {
            pool: sqlx::PgPool::connect_lazy("postgresql://test").unwrap(),
        };
        
        let template = "Hello {{name}}, your role is {{role}}";
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "Alice".to_string());
        variables.insert("role".to_string(), "Developer".to_string());
        
        let result = service.replace_variables(template, &variables);
        assert_eq!(result, "Hello Alice, your role is Developer");
    }
}
