/// Routable Blocked Service
/// 
/// 路由阻塞管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RoutableBlockedError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("route blocked: {0}")]
    RouteBlocked(String),
}

pub type RoutableBlockedResult<T> = Result<T, RoutableBlockedError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedRoute {
    pub id: Uuid,
    pub route_pattern: String,
    pub reason: String,
    pub blocked_by: Uuid,
    pub blocked_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BlockReason {
    Security,
    Maintenance,
    RateLimit,
    Policy,
    Emergency,
}

pub struct RoutableBlockedService {
    pool: PgPool,
}

impl RoutableBlockedService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn block_route(
        &self,
        route_pattern: String,
        reason: String,
        blocked_by: Uuid,
        ttl_seconds: Option<i64>,
    ) -> RoutableBlockedResult<Uuid> {
        let id = Uuid::new_v4();
        let expires_at = ttl_seconds.map(|ttl| {
            chrono::Utc::now() + chrono::Duration::seconds(ttl)
        });
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO blocked_routes 
            (id, route_pattern, reason, blocked_by, blocked_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(&route_pattern)
        .bind(&reason)
        .bind(blocked_by)
        .bind(chrono::Utc::now())
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn is_route_blocked(&self, route: &str) -> RoutableBlockedResult<bool> {
        // 清理过期的阻塞
        self.cleanup_expired().await?;
        
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM blocked_routes
            WHERE $1 ~ route_pattern
              AND (expires_at IS NULL OR expires_at > $2)
            "#
        )
        .bind(route)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count > 0)
    }
    
    pub async fn check_route(&self, route: &str) -> RoutableBlockedResult<()> {
        if self.is_route_blocked(route).await? {
            return Err(RoutableBlockedError::RouteBlocked(
                format!("Route {} is blocked", route)
            ));
        }
        Ok(())
    }
    
    pub async fn get_blocked_routes(&self) -> RoutableBlockedResult<Vec<BlockedRoute>> {
        self.cleanup_expired().await?;
        
        let rows = sqlx::query(
            r#"
            SELECT id, route_pattern, reason, blocked_by, blocked_at, expires_at
            FROM blocked_routes
            WHERE expires_at IS NULL OR expires_at > $1
            ORDER BY blocked_at DESC
            "#
        )
        .bind(chrono::Utc::now())
        .fetch_all(&self.pool)
        .await?;
        
        let routes = rows.into_iter().map(|row| {
            BlockedRoute {
                id: row.get("id"),
                route_pattern: row.get("route_pattern"),
                reason: row.get("reason"),
                blocked_by: row.get("blocked_by"),
                blocked_at: row.get("blocked_at"),
                expires_at: row.get("expires_at"),
            }
        }).collect();
        
        Ok(routes)
    }
    
    pub async fn unblock_route(&self, id: Uuid) -> RoutableBlockedResult<()> {
        sqlx::query("DELETE FROM blocked_routes WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    pub async fn unblock_by_pattern(&self, route_pattern: &str) -> RoutableBlockedResult<()> {
        sqlx::query("DELETE FROM blocked_routes WHERE route_pattern = $1")
            .bind(route_pattern)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    async fn cleanup_expired(&self) -> RoutableBlockedResult<()> {
        sqlx::query(
            r#"
            DELETE FROM blocked_routes
            WHERE expires_at IS NOT NULL AND expires_at <= $1
            "#
        )
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
