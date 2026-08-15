use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DocumentAnnotationError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("annotation not found: {0}")]
    NotFound(Uuid),
}

pub type AnnotationResult<T> = Result<T, DocumentAnnotationError>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentAnnotation {
    pub id: Uuid,
    pub document_id: Uuid,
    pub user_id: Uuid,
    pub annotation_type: AnnotationType,
    pub content: String,
    pub position: AnnotationPosition,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum AnnotationType {
    Comment,
    Highlight,
    Note,
    Suggestion,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "jsonb")]
pub struct AnnotationPosition {
    pub start_offset: i32,
    pub end_offset: i32,
    pub section: Option<String>,
}

#[async_trait]
pub trait DocumentAnnotationService: Send + Sync {
    async fn create_annotation(
        &self,
        document_id: Uuid,
        user_id: Uuid,
        annotation_type: AnnotationType,
        content: String,
        position: AnnotationPosition,
    ) -> AnnotationResult<DocumentAnnotation>;
    
    async fn get_annotation(&self, annotation_id: Uuid) -> AnnotationResult<Option<DocumentAnnotation>>;
    async fn list_annotations(&self, document_id: Uuid) -> AnnotationResult<Vec<DocumentAnnotation>>;
    async fn delete_annotation(&self, annotation_id: Uuid) -> AnnotationResult<()>;
    async fn update_annotation(&self, annotation_id: Uuid, content: String) -> AnnotationResult<()>;
}

pub struct DocumentAnnotationServiceImpl {
    pool: PgPool,
}

impl DocumentAnnotationServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DocumentAnnotationService for DocumentAnnotationServiceImpl {
    async fn create_annotation(
        &self,
        document_id: Uuid,
        user_id: Uuid,
        annotation_type: AnnotationType,
        content: String,
        position: AnnotationPosition,
    ) -> AnnotationResult<DocumentAnnotation> {
        let annotation_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO document_annotations (id, document_id, user_id, annotation_type, content, position, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#
        )
        .bind(annotation_id)
        .bind(document_id)
        .bind(user_id)
        .bind(serde_json::to_value(&annotation_type).unwrap())
        .bind(&content)
        .bind(serde_json::to_value(&position).unwrap())
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(DocumentAnnotation {
            id: annotation_id,
            document_id,
            user_id,
            annotation_type,
            content,
            position,
            created_at: now,
        })
    }
    
    async fn get_annotation(&self, annotation_id: Uuid) -> AnnotationResult<Option<DocumentAnnotation>> {
        let row = sqlx::query_as::<_, DocumentAnnotation>(
            r#"
            SELECT id, document_id, user_id, annotation_type, content, position, created_at
            FROM document_annotations
            WHERE id = $1
            "#
        )
        .bind(annotation_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    async fn list_annotations(&self, document_id: Uuid) -> AnnotationResult<Vec<DocumentAnnotation>> {
        let rows = sqlx::query_as::<_, DocumentAnnotation>(
            r#"
            SELECT id, document_id, user_id, annotation_type, content, position, created_at
            FROM document_annotations
            WHERE document_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(document_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    async fn delete_annotation(&self, annotation_id: Uuid) -> AnnotationResult<()> {
        sqlx::query("DELETE FROM document_annotations WHERE id = $1")
            .bind(annotation_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    async fn update_annotation(&self, annotation_id: Uuid, content: String) -> AnnotationResult<()> {
        sqlx::query(
            "UPDATE document_annotations SET content = $1 WHERE id = $2"
        )
        .bind(&content)
        .bind(annotation_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
