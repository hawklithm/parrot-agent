/// GitHub Fetch Service
/// 
/// GitHub数据获取

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum GitHubFetchError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("API error: {0}")]
    ApiError(String),
}

pub type GitHubFetchResult<T> = Result<T, GitHubFetchError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRepository {
    pub id: Uuid,
    pub github_id: i64,
    pub name: String,
    pub full_name: String,
    pub owner: String,
    pub url: String,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

pub struct GitHubFetchService {
    pool: PgPool,
}

impl GitHubFetchService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn fetch_repository(
        &self,
        owner: &str,
        repo: &str,
    ) -> GitHubFetchResult<GitHubRepository> {
        // 简化实现：实际应调用GitHub API
        let id = Uuid::new_v4();
        let github_id = 12345;
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO github_repositories 
            (id, github_id, name, full_name, owner, url, fetched_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(github_id)
        .bind(repo)
        .bind(format!("{}/{}", owner, repo))
        .bind(owner)
        .bind(format!("https://github.com/{}/{}", owner, repo))
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(GitHubRepository {
            id,
            github_id,
            name: repo.to_string(),
            full_name: format!("{}/{}", owner, repo),
            owner: owner.to_string(),
            url: format!("https://github.com/{}/{}", owner, repo),
            fetched_at: chrono::Utc::now(),
        })
    }
    
    pub async fn get_cached_repository(
        &self,
        owner: &str,
        repo: &str,
    ) -> GitHubFetchResult<Option<GitHubRepository>> {
        let row = sqlx::query(
            r#"
            SELECT id, github_id, name, full_name, owner, url, fetched_at
            FROM github_repositories
            WHERE owner = $1 AND name = $2
            ORDER BY fetched_at DESC
            LIMIT 1
            "#
        )
        .bind(owner)
        .bind(repo)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| GitHubRepository {
            id: r.get("id"),
            github_id: r.get("github_id"),
            name: r.get("name"),
            full_name: r.get("full_name"),
            owner: r.get("owner"),
            url: r.get("url"),
            fetched_at: r.get("fetched_at"),
        }))
    }
}
