/// Company Search Service
/// 
/// Company搜索功能

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CompanySearchError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type CompanySearchResult<T> = Result<T, CompanySearchError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanySearchItem {
    pub company_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub member_count: i32,
    pub relevance_score: f64,
}

pub struct CompanySearchService {
    pool: PgPool,
}

impl CompanySearchService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    
    pub async fn search(
        &self,
        query: &str,
        limit: i64,
    ) -> CompanySearchResult<Vec<CompanySearchItem>> {
        let rows = sqlx::query(
            r#"
            SELECT c.id, c.name, c.description,
                   COUNT(cm.user_id) as member_count,
                   similarity(c.name, $1) as relevance_score
            FROM companies c
            LEFT JOIN company_members cm ON c.id = cm.company_id
            WHERE c.name ILIKE '%' || $1 || '%'
               OR c.description ILIKE '%' || $1 || '%'
            GROUP BY c.id
            ORDER BY relevance_score DESC
            LIMIT $2
            "#
        )
        .bind(query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        
        let results: Vec<CompanySearchItem> = rows.into_iter().map(|row| {
            CompanySearchItem {
                company_id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                member_count: row.get::<i64, _>("member_count") as i32,
                relevance_score: row.get::<f64, _>("relevance_score"),
            }
        }).collect();
        
        Ok(results)
    }
    
    pub async fn search_by_member(
        &self,
        user_id: Uuid,
    ) -> CompanySearchResult<Vec<Uuid>> {
        let rows = sqlx::query(
            "SELECT company_id FROM company_members WHERE user_id = $1"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|r| r.get("company_id")).collect())
    }
}
