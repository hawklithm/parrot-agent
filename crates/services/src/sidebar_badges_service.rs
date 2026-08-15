/// Sidebar Badges Service
/// 
/// 侧边栏徽章管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SidebarBadgesError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type SidebarBadgesResult<T> = Result<T, SidebarBadgesError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Badge {
    pub id: Uuid,
    pub user_id: Uuid,
    pub badge_type: String,
    pub label: String,
    pub count: i32,
    pub color: String,
}

pub struct SidebarBadgesService {
    pool: PgPool,
}

impl SidebarBadgesService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn get_user_badges(&self, user_id: Uuid) -> SidebarBadgesResult<Vec<Badge>> {
        let rows = sqlx::query(
            r#"
            SELECT id, user_id, badge_type, label, count, color
            FROM sidebar_badges
            WHERE user_id = $1
            ORDER BY badge_type
            "#
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        
        let badges = rows.into_iter().map(|row| {
            Badge {
                id: row.get("id"),
                user_id: row.get("user_id"),
                badge_type: row.get("badge_type"),
                label: row.get("label"),
                count: row.get("count"),
                color: row.get("color"),
            }
        }).collect();
        
        Ok(badges)
    }
    
    pub async fn update_badge_count(
        &self,
        user_id: Uuid,
        badge_type: &str,
        count: i32,
    ) -> SidebarBadgesResult<()> {
        sqlx::query(
            r#"
            INSERT INTO sidebar_badges (id, user_id, badge_type, label, count, color)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id, badge_type)
            DO UPDATE SET count = $5
            "#
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(badge_type)
        .bind(badge_type)
        .bind(count)
        .bind("blue")
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
