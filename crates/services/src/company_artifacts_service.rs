/// Company Artifacts Service
/// 
/// Company 制品管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CompanyArtifactsError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("artifact not found: {0}")]
    NotFound(Uuid),
}

pub type CompanyArtifactsResult<T> = Result<T, CompanyArtifactsError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub artifact_type: ArtifactType,
    pub content_url: String,
    pub metadata: serde_json::Value,
    pub created_by: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtifactType {
    Document,
    Code,
    Data,
    Model,
    Config,
}

pub struct CompanyArtifactsService {
    pool: PgPool,
}

impl CompanyArtifactsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_artifact(
        &self,
        company_id: Uuid,
        name: String,
        artifact_type: ArtifactType,
        content_url: String,
        metadata: serde_json::Value,
        created_by: Uuid,
    ) -> CompanyArtifactsResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO company_artifacts 
            (id, company_id, name, artifact_type, content_url, metadata, created_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(company_id)
        .bind(&name)
        .bind(format!("{:?}", artifact_type))
        .bind(&content_url)
        .bind(&metadata)
        .bind(created_by)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_artifact(&self, id: Uuid) -> CompanyArtifactsResult<Artifact> {
        let row = sqlx::query(
            r#"
            SELECT id, company_id, name, artifact_type, content_url, metadata, created_by, created_at
            FROM company_artifacts
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(Artifact {
            id: row.get("id"),
            company_id: row.get("company_id"),
            name: row.get("name"),
            artifact_type: parse_type(row.get("artifact_type")),
            content_url: row.get("content_url"),
            metadata: row.get("metadata"),
            created_by: row.get("created_by"),
            created_at: row.get("created_at"),
        })
    }
    
    pub async fn list_artifacts(
        &self,
        company_id: Uuid,
        artifact_type: Option<ArtifactType>,
    ) -> CompanyArtifactsResult<Vec<Artifact>> {
        let mut query = "SELECT id, company_id, name, artifact_type, content_url, metadata, created_by, created_at FROM company_artifacts WHERE company_id = $1".to_string();
        
        if artifact_type.is_some() {
            query.push_str(" AND artifact_type = $2");
        }
        
        query.push_str(" ORDER BY created_at DESC");
        
        let rows = sqlx::query(&query)
            .bind(company_id)
            .fetch_all(&self.pool)
            .await?;
        
        let artifacts = rows.into_iter().map(|row| {
            Artifact {
                id: row.get("id"),
                company_id: row.get("company_id"),
                name: row.get("name"),
                artifact_type: parse_type(row.get("artifact_type")),
                content_url: row.get("content_url"),
                metadata: row.get("metadata"),
                created_by: row.get("created_by"),
                created_at: row.get("created_at"),
            }
        }).collect();
        
        Ok(artifacts)
    }
    
    pub async fn delete_artifact(&self, id: Uuid) -> CompanyArtifactsResult<()> {
        sqlx::query("DELETE FROM company_artifacts WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}

fn parse_type(s: &str) -> ArtifactType {
    match s {
        "Document" => ArtifactType::Document,
        "Code" => ArtifactType::Code,
        "Data" => ArtifactType::Data,
        "Model" => ArtifactType::Model,
        "Config" => ArtifactType::Config,
        _ => ArtifactType::Document,
    }
}
