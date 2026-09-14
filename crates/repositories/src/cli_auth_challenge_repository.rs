//! CLI 认证挑战 Repository（对齐 Paperclip 设备授权流程）。
//!
//! 服务层是唯一消费方，因此这里只保留具体实现，不再保留 trait 抽象。

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::models::auth_keys::CliAuthChallenge;
use crate::RepositoryError;

/// `cli_auth_challenges` 的显式列清单。
///
/// 刻意不使用 `SELECT *`：迁移新增列时，显式清单仍按字段名解码，
/// 不会因列数量或顺序变化破坏 `FromRow`。
pub const CLI_AUTH_CHALLENGE_COLUMNS: &str = "id, secret_hash, command, client_name, requested_access, \
     requested_company_id, pending_key_hash, pending_key_name, approved_by_user_id, \
     board_api_key_id, approved_at, cancelled_at, expires_at, created_at, updated_at";

/// PostgreSQL 实现。
pub struct PgCliAuthChallengeRepository {
    pool: PgPool,
}

impl PgCliAuthChallengeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 插入挑战记录，并返回落库后的整行。
    pub async fn create(
        &self,
        challenge: &CliAuthChallenge,
    ) -> Result<CliAuthChallenge, RepositoryError> {
        let row = sqlx::query_as::<_, CliAuthChallenge>(&format!(
            r#"INSERT INTO cli_auth_challenges (
                   id, secret_hash, command, client_name, requested_access, requested_company_id,
                   pending_key_hash, pending_key_name, approved_by_user_id, board_api_key_id,
                   approved_at, cancelled_at, expires_at, created_at, updated_at
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
               RETURNING {CLI_AUTH_CHALLENGE_COLUMNS}"#
        ))
        .bind(challenge.id)
        .bind(&challenge.secret_hash)
        .bind(&challenge.command)
        .bind(&challenge.client_name)
        .bind(&challenge.requested_access)
        .bind(challenge.requested_company_id)
        .bind(&challenge.pending_key_hash)
        .bind(&challenge.pending_key_name)
        .bind(challenge.approved_by_user_id)
        .bind(challenge.board_api_key_id)
        .bind(challenge.approved_at)
        .bind(challenge.cancelled_at)
        .bind(challenge.expires_at)
        .bind(challenge.created_at)
        .bind(challenge.updated_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// 按 ID 查询。
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<CliAuthChallenge>, RepositoryError> {
        let row = sqlx::query_as::<_, CliAuthChallenge>(&format!(
            "SELECT {CLI_AUTH_CHALLENGE_COLUMNS} FROM cli_auth_challenges WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// 在调用方事务内按 ID 加行锁查询（`FOR UPDATE`），用于串行化批准流程。
    ///
    /// 必须在事务中调用；`conn` 由 `&mut *tx` 或 `&mut tx` 取得。
    pub async fn find_by_id_for_update(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<Option<CliAuthChallenge>, RepositoryError> {
        let row = sqlx::query_as::<_, CliAuthChallenge>(&format!(
            "SELECT {CLI_AUTH_CHALLENGE_COLUMNS} FROM cli_auth_challenges WHERE id = $1 FOR UPDATE"
        ))
        .bind(id)
        .fetch_optional(conn)
        .await?;
        Ok(row)
    }

    /// 标记批准：写入批准人、关联的 Board API Key 与批准时间。
    pub async fn mark_approved(
        conn: &mut PgConnection,
        id: Uuid,
        approved_by_user_id: Uuid,
        board_api_key_id: Uuid,
        approved_at: DateTime<Utc>,
    ) -> Result<CliAuthChallenge, RepositoryError> {
        let row = sqlx::query_as::<_, CliAuthChallenge>(&format!(
            r#"UPDATE cli_auth_challenges
               SET approved_by_user_id = $2,
                   board_api_key_id = $3,
                   approved_at = $4,
                   updated_at = NOW()
               WHERE id = $1
               RETURNING {CLI_AUTH_CHALLENGE_COLUMNS}"#
        ))
        .bind(id)
        .bind(approved_by_user_id)
        .bind(board_api_key_id)
        .bind(approved_at)
        .fetch_one(conn)
        .await?;
        Ok(row)
    }

    /// 标记取消。
    pub async fn mark_cancelled(
        conn: &mut PgConnection,
        id: Uuid,
        cancelled_at: DateTime<Utc>,
    ) -> Result<CliAuthChallenge, RepositoryError> {
        let row = sqlx::query_as::<_, CliAuthChallenge>(&format!(
            r#"UPDATE cli_auth_challenges
               SET cancelled_at = $2, updated_at = NOW()
               WHERE id = $1
               RETURNING {CLI_AUTH_CHALLENGE_COLUMNS}"#
        ))
        .bind(id)
        .bind(cancelled_at)
        .fetch_one(conn)
        .await?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SELECT *` 会在迁移新增列时破坏解码；列清单必须保持显式且覆盖全部模型字段。
    #[test]
    fn column_list_is_explicit_and_complete() {
        assert!(!CLI_AUTH_CHALLENGE_COLUMNS.contains('*'));
        let columns: Vec<&str> = CLI_AUTH_CHALLENGE_COLUMNS
            .split(',')
            .map(str::trim)
            .collect();
        assert_eq!(columns.len(), 15);
        assert_eq!(columns[0], "id");
        assert_eq!(columns[columns.len() - 1], "updated_at");
    }
}
