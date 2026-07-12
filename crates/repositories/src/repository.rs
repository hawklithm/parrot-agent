//! 通用 Repository trait 与事务辅助
//!
//! 为所有 Repository 提供统一的构造入口、连接池访问与事务辅助方法，
//! 避免各 repo 重复实现样板代码。各业务 Repository 只需 `impl Repository`
//! （提供 `new` / `pool`）即可通过 [`RepositoryExt`] 免费获得 `with_transaction()`。

use async_trait::async_trait;
use sqlx::{PgPool, Transaction, Postgres};
use uuid::Uuid;

/// 所有 Repository 的基础 trait。
///
/// 提供统一的构造与连接池访问能力。
#[async_trait]
pub trait Repository: Send + Sync {
    /// 基于连接池构造 Repository 实例。
    fn new(pool: PgPool) -> Self
    where
        Self: Sized;

    /// 返回底层连接池引用。
    fn pool(&self) -> &PgPool;
}

/// 泛型 CRUD 子 trait。
///
/// 适用于以 UUID 为主键、具备映射的实体。具体 Repository 可选择性实现。
#[async_trait]
pub trait CrudOps<T>: Repository
where
    T: Send + Sync,
{
    /// 按主键查询。
    async fn find_by_id(&self, id: Uuid) -> Result<Option<T>, RepositoryError>;

    /// 创建实体。
    async fn create(&self, entity: T) -> Result<T, RepositoryError>;

    /// 全量更新实体。
    async fn update(&self, entity: T) -> Result<T, RepositoryError>;

    /// 按主键删除。
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;
}

/// 统一 Repository 错误类型。
///
/// 业务 Repository 可复用此类型或自行定义（需自行保证 `From<sqlx::Error>`）。
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Conflict: {0}")]
    Conflict(String),
}

/// 为所有 [`Repository`] 提供统一事务辅助方法。
///
/// 闭包接收 `&mut Transaction<Postgres>`，在事务上执行多条语句后返回结果。
/// 成功时自动提交，失败时自动回滚。错误类型 `E` 必须可由 [`sqlx::Error`] 转换。
#[async_trait]
pub trait RepositoryExt: Repository {
    /// 在事务中执行闭包。
    async fn with_transaction<F, Fut, T, E>(&self, f: F) -> Result<T, E>
    where
        E: From<sqlx::Error>,
        F: FnOnce(&mut Transaction<'_, Postgres>) -> Fut + Send,
        Fut: std::future::Future<Output = Result<T, E>> + Send,
        T: Send,
    {
        let mut tx = self.pool().begin().await?;
        let result = f(&mut tx).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// `with_transaction` 的别名，与任务拆解文档中的 `with_tx()` 命名保持一致。
    async fn with_tx<F, Fut, T, E>(&self, f: F) -> Result<T, E>
    where
        E: From<sqlx::Error>,
        F: FnOnce(&mut Transaction<'_, Postgres>) -> Fut + Send,
        Fut: std::future::Future<Output = Result<T, E>> + Send,
        T: Send,
    {
        self.with_transaction(f).await
    }
}

// 为所有实现了 Repository 的类型自动提供 RepositoryExt
impl<R: Repository + ?Sized> RepositoryExt for R {}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyRepo {
        pool: PgPool,
    }

    #[async_trait]
    impl Repository for DummyRepo {
        fn new(pool: PgPool) -> Self {
            Self { pool }
        }
        fn pool(&self) -> &PgPool {
            &self.pool
        }
    }

    #[test]
    fn test_repository_error_from_sqlx() {
        // 确保 sqlx::Error 可转换为 RepositoryError（编译期保证）
        fn assert_from<E>()
        where
            RepositoryError: From<E>,
        {
        }
        assert_from::<sqlx::Error>();
    }
}
