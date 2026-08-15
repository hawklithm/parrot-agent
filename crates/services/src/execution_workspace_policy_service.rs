/// Execution Workspace Policy Service
/// 
/// Workspace 执行策略管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ExecutionWorkspacePolicyError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("policy violation: {0}")]
    PolicyViolation(String),
}

pub type ExecutionWorkspacePolicyResult<T> = Result<T, ExecutionWorkspacePolicyError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspacePolicy {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub policy_type: PolicyType,
    pub rules: serde_json::Value,
    pub enforcement_level: EnforcementLevel,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicyType {
    FileAccess,
    NetworkAccess,
    ResourceLimits,
    AllowedTools,
    AllowedCommands,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EnforcementLevel {
    Audit,      // 记录但不阻止
    Warn,       // 警告但不阻止
    Block,      // 阻止执行
}

pub struct ExecutionWorkspacePolicyService {
    pool: PgPool,
}

impl ExecutionWorkspacePolicyService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_policy(
        &self,
        workspace_id: Uuid,
        policy_type: PolicyType,
        rules: serde_json::Value,
        enforcement_level: EnforcementLevel,
    ) -> ExecutionWorkspacePolicyResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO workspace_policies 
            (id, workspace_id, policy_type, rules, enforcement_level, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(workspace_id)
        .bind(format!("{:?}", policy_type))
        .bind(&rules)
        .bind(format!("{:?}", enforcement_level))
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_policies(&self, workspace_id: Uuid) -> ExecutionWorkspacePolicyResult<Vec<WorkspacePolicy>> {
        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, policy_type, rules, enforcement_level, created_at
            FROM workspace_policies
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;
        
        let policies = rows.into_iter().map(|row| {
            WorkspacePolicy {
                id: row.get("id"),
                workspace_id: row.get("workspace_id"),
                policy_type: parse_policy_type(row.get("policy_type")),
                rules: row.get("rules"),
                enforcement_level: parse_enforcement(row.get("enforcement_level")),
                created_at: row.get("created_at"),
            }
        }).collect();
        
        Ok(policies)
    }
    
    pub async fn check_file_access(
        &self,
        workspace_id: Uuid,
        file_path: &str,
        _operation: &str,
    ) -> ExecutionWorkspacePolicyResult<()> {
        let policies = self.get_policies(workspace_id).await?;
        
        for policy in policies {
            if policy.policy_type == PolicyType::FileAccess {
                if policy.enforcement_level == EnforcementLevel::Block {
                    // 检查规则
                    if let Some(blocked_patterns) = policy.rules.get("blocked_patterns") {
                        if let Some(patterns) = blocked_patterns.as_array() {
                            for pattern in patterns {
                                if let Some(p) = pattern.as_str() {
                                    if file_path.contains(p) {
                                        return Err(ExecutionWorkspacePolicyError::PolicyViolation(
                                            format!("File access to {} is blocked by policy", file_path)
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    pub async fn check_tool_access(
        &self,
        workspace_id: Uuid,
        tool_name: &str,
    ) -> ExecutionWorkspacePolicyResult<()> {
        let policies = self.get_policies(workspace_id).await?;
        
        for policy in policies {
            if policy.policy_type == PolicyType::AllowedTools {
                if policy.enforcement_level == EnforcementLevel::Block {
                    if let Some(allowed_tools) = policy.rules.get("allowed") {
                        if let Some(tools) = allowed_tools.as_array() {
                            let tool_names: Vec<String> = tools.iter()
                                .filter_map(|t| t.as_str())
                                .map(|s| s.to_string())
                                .collect();
                            
                            if !tool_names.contains(&tool_name.to_string()) {
                                return Err(ExecutionWorkspacePolicyError::PolicyViolation(
                                    format!("Tool {} is not allowed in this workspace", tool_name)
                                ));
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    pub async fn delete_policy(&self, id: Uuid) -> ExecutionWorkspacePolicyResult<()> {
        sqlx::query("DELETE FROM workspace_policies WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}

fn parse_policy_type(s: &str) -> PolicyType {
    match s {
        "FileAccess" => PolicyType::FileAccess,
        "NetworkAccess" => PolicyType::NetworkAccess,
        "ResourceLimits" => PolicyType::ResourceLimits,
        "AllowedTools" => PolicyType::AllowedTools,
        "AllowedCommands" => PolicyType::AllowedCommands,
        _ => PolicyType::FileAccess,
    }
}

fn parse_enforcement(s: &str) -> EnforcementLevel {
    match s {
        "Audit" => EnforcementLevel::Audit,
        "Warn" => EnforcementLevel::Warn,
        "Block" => EnforcementLevel::Block,
        _ => EnforcementLevel::Audit,
    }
}
