/// Environment Custom Images Service
/// 
/// 自定义镜像管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CustomImagesError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type CustomImagesResult<T> = Result<T, CustomImagesError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomImage {
    pub id: Uuid,
    pub name: String,
    pub registry: String,
    pub tag: String,
    pub digest: Option<String>,
    pub created_by: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct EnvironmentCustomImagesService {
    pool: PgPool,
}

impl EnvironmentCustomImagesService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn register_image(
        &self,
        name: String,
        registry: String,
        tag: String,
        digest: Option<String>,
        created_by: Uuid,
    ) -> CustomImagesResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO custom_images 
            (id, name, registry, tag, digest, created_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(&name)
        .bind(&registry)
        .bind(&tag)
        .bind(&digest)
        .bind(created_by)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_image(&self, id: Uuid) -> CustomImagesResult<Option<CustomImage>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, registry, tag, digest, created_by, created_at
            FROM custom_images
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| CustomImage {
            id: r.get("id"),
            name: r.get("name"),
            registry: r.get("registry"),
            tag: r.get("tag"),
            digest: r.get("digest"),
            created_by: r.get("created_by"),
            created_at: r.get("created_at"),
        }))
    }
    
    pub async fn list_images(&self, created_by: Option<Uuid>) -> CustomImagesResult<Vec<CustomImage>> {
        let mut query = "SELECT id, name, registry, tag, digest, created_by, created_at FROM custom_images".to_string();
        
        if created_by.is_some() {
            query.push_str(" WHERE created_by = $1");
        }
        
        query.push_str(" ORDER BY created_at DESC");
        
        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await?;
        
        let images = rows.into_iter().map(|row| {
            CustomImage {
                id: row.get("id"),
                name: row.get("name"),
                registry: row.get("registry"),
                tag: row.get("tag"),
                digest: row.get("digest"),
                created_by: row.get("created_by"),
                created_at: row.get("created_at"),
            }
        }).collect();
        
        Ok(images)
    }
}
