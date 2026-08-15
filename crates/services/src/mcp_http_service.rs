use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum McpHttpServiceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("http error: {0}")]
    Http(String),
    #[error("configuration error: {0}")]
    Configuration(String),
}

pub type McpHttpResult<T> = Result<T, McpHttpServiceError>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct McpHttpEndpoint {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub headers: serde_json::Value,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

#[derive(Debug, Clone)]
pub struct RegisterEndpointRequest {
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub headers: serde_json::Value,
}

#[async_trait]
pub trait McpHttpService: Send + Sync {
    async fn register_endpoint(&self, req: RegisterEndpointRequest) -> McpHttpResult<Uuid>;
    async fn get_endpoint(&self, endpoint_id: Uuid) -> McpHttpResult<Option<McpHttpEndpoint>>;
    async fn list_endpoints(&self) -> McpHttpResult<Vec<McpHttpEndpoint>>;
    async fn disable_endpoint(&self, endpoint_id: Uuid) -> McpHttpResult<()>;
    async fn delete_endpoint(&self, endpoint_id: Uuid) -> McpHttpResult<()>;
}

pub struct McpHttpServiceImpl {
    pool: PgPool,
}

impl McpHttpServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl McpHttpService for McpHttpServiceImpl {
    async fn register_endpoint(&self, req: RegisterEndpointRequest) -> McpHttpResult<Uuid> {
        let endpoint_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO mcp_http_endpoints (id, name, url, method, headers, enabled, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#
        )
        .bind(endpoint_id)
        .bind(&req.name)
        .bind(&req.url)
        .bind(serde_json::to_value(&req.method).unwrap())
        .bind(&req.headers)
        .bind(true)
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(endpoint_id)
    }
    
    async fn get_endpoint(&self, endpoint_id: Uuid) -> McpHttpResult<Option<McpHttpEndpoint>> {
        let row = sqlx::query_as::<_, McpHttpEndpoint>(
            r#"
            SELECT id, name, url, method, headers, enabled, created_at
            FROM mcp_http_endpoints
            WHERE id = $1
            "#
        )
        .bind(endpoint_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    async fn list_endpoints(&self) -> McpHttpResult<Vec<McpHttpEndpoint>> {
        let rows = sqlx::query_as::<_, McpHttpEndpoint>(
            r#"
            SELECT id, name, url, method, headers, enabled, created_at
            FROM mcp_http_endpoints
            WHERE enabled = true
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    async fn disable_endpoint(&self, endpoint_id: Uuid) -> McpHttpResult<()> {
        sqlx::query(
            "UPDATE mcp_http_endpoints SET enabled = false WHERE id = $1"
        )
        .bind(endpoint_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    async fn delete_endpoint(&self, endpoint_id: Uuid) -> McpHttpResult<()> {
        sqlx::query("DELETE FROM mcp_http_endpoints WHERE id = $1")
            .bind(endpoint_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}
