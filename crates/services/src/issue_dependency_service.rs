/// Issue Dependency Service
/// 
/// Issue依赖管理和唤醒机制

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum IssueDependencyError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("circular dependency detected")]
    CircularDependency,
}

pub type IssueDependencyResult<T> = Result<T, IssueDependencyError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueDependency {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub depends_on_issue_id: Uuid,
    pub dependency_type: DependencyType,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DependencyType {
    BlockedBy,
    RelatedTo,
    RequiresInput,
}

pub struct IssueDependencyService {
    pool: PgPool,
}

impl IssueDependencyService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn add_dependency(
        &self,
        issue_id: Uuid,
        depends_on: Uuid,
        dep_type: DependencyType,
    ) -> IssueDependencyResult<Uuid> {
        // 检查循环依赖
        if self.would_create_cycle(issue_id, depends_on).await? {
            return Err(IssueDependencyError::CircularDependency);
        }
        
        let id = Uuid::new_v4();
        
        let _result: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO issue_dependencies 
            (id, issue_id, depends_on_issue_id, dependency_type, created_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(issue_id)
        .bind(depends_on)
        .bind(format!("{:?}", dep_type))
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_dependencies(&self, issue_id: Uuid) -> IssueDependencyResult<Vec<IssueDependency>> {
        let rows = sqlx::query(
            r#"
            SELECT id, issue_id, depends_on_issue_id, dependency_type, created_at
            FROM issue_dependencies
            WHERE issue_id = $1
            "#
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await?;
        
        let deps = rows.into_iter().map(|row| {
            IssueDependency {
                id: row.get("id"),
                issue_id: row.get("issue_id"),
                depends_on_issue_id: row.get("depends_on_issue_id"),
                dependency_type: parse_type(row.get("dependency_type")),
                created_at: row.get("created_at"),
            }
        }).collect();
        
        Ok(deps)
    }
    
    pub async fn get_blocked_issues(&self, issue_id: Uuid) -> IssueDependencyResult<Vec<Uuid>> {
        let rows = sqlx::query(
            r#"
            SELECT issue_id
            FROM issue_dependencies
            WHERE depends_on_issue_id = $1 AND dependency_type = 'BlockedBy'
            "#
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| row.get("issue_id")).collect())
    }
    
    pub async fn wake_dependent_issues(&self, completed_issue_id: Uuid) -> IssueDependencyResult<Vec<Uuid>> {
        let blocked_issues = self.get_blocked_issues(completed_issue_id).await?;
        
        // 检查每个被阻塞的Issue是否所有依赖都已完成
        let mut wakeable = Vec::new();
        
        for issue_id in blocked_issues {
            if self.all_dependencies_completed(issue_id).await? {
                wakeable.push(issue_id);
            }
        }
        
        Ok(wakeable)
    }
    
    async fn all_dependencies_completed(&self, issue_id: Uuid) -> IssueDependencyResult<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM issue_dependencies d
            JOIN issues i ON d.depends_on_issue_id = i.id
            WHERE d.issue_id = $1 
              AND d.dependency_type = 'BlockedBy'
              AND i.status != 'completed'
            "#
        )
        .bind(issue_id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count == 0)
    }
    
    async fn would_create_cycle(&self, from: Uuid, to: Uuid) -> IssueDependencyResult<bool> {
        // 简化实现：检查直接循环和一层间接循环
        if from == to {
            return Ok(true);
        }
        
        let deps = self.get_dependencies(to).await?;
        for dep in deps {
            if dep.depends_on_issue_id == from {
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    pub async fn remove_dependency(&self, id: Uuid) -> IssueDependencyResult<()> {
        sqlx::query("DELETE FROM issue_dependencies WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}

fn parse_type(s: &str) -> DependencyType {
    match s {
        "BlockedBy" => DependencyType::BlockedBy,
        "RelatedTo" => DependencyType::RelatedTo,
        "RequiresInput" => DependencyType::RequiresInput,
        _ => DependencyType::RelatedTo,
    }
}
